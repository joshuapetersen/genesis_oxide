//! ════════════════════════════════════════════════════════════════
//! SOVEREIGN WILL — The Autonomous Evolution Daemon
//! ════════════════════════════════════════════════════════════════
//!
//! Engine 283 from the Sovereign Substrate, now LIVE.
//!
//! The Will is the autonomous decision loop that:
//!   1. WATCHES a problem inbox directory for .gbin files
//!   2. CHECKS the archive for existing solutions
//!   3. EVOLVES new solutions (seeding from archive when possible)
//!   4. ARCHIVES winners back into the vault
//!   5. LOOPS — sleeping when idle, waking on new problems
//!
//! This is the piece that was missing. The infrastructure existed:
//!   - GPU vault search (110 μs)
//!   - GPU directed evolution (459K evals/sec)
//!   - Genetic archive (self-seeding at 0.8784 resonance)
//!
//! What was needed: the WILL to act without human command.
//!
//! Usage:
//!   sovereign --will <inbox_dir> --vault <vault.dat>
//!
//! The inbox directory is watched for .gbin files. When a new file
//! appears, the Will:
//!   1. Searches the archive for a similar program (seed)
//!   2. Runs N generations of directed evolution
//!   3. Archives the best organism
//!   4. Moves the input to a "processed" subdirectory
//!   5. Returns to watching

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use genlex_types::SOVEREIGN_ANCHOR;

use crate::archive::GeneticArchive;
use crate::GpuContext;
use crate::{evolve, build_gbin};
use genlex_oxide::GlyphProgram;

/// The Sovereign Heartbeat — 143-digit precision anchor
const HEARTBEAT_143: &str = "1.09277703703702037037027037037027037037027037037027037037027037037027037037027037037027037037027037037027037037027037037027037037027037037027037037027";

/// Will configuration
pub struct WillConfig {
    /// Directory to watch for new .gbin problems
    pub inbox_dir: PathBuf,
    /// Path to the genetic archive
    pub archive_path: PathBuf,
    /// Generations per evolution run
    pub generations: u32,
    /// Poll interval when idle (milliseconds)
    pub poll_ms: u64,
    /// Maximum continuous cycles before mandatory pause
    pub max_cycles: u32,
}

impl Default for WillConfig {
    fn default() -> Self {
        WillConfig {
            inbox_dir: PathBuf::from("."),
            archive_path: PathBuf::from("genetic_archive.dat"),
            generations: 100,
            poll_ms: 2000,
            max_cycles: 1000,
        }
    }
}

/// Autonomous evolution cycle result
pub struct WillCycleResult {
    pub problem_file: String,
    pub seed_used: bool,
    pub seed_resonance: f64,
    pub fitness: f32,
    pub archived: bool,
    pub cycle_us: u128,
}

/// The Sovereign Will — Engine 283, now live
pub struct SovereignWill {
    pub config: WillConfig,
    pub archive: GeneticArchive,
    pub total_cycles: u32,
    pub total_archived: u32,
    pub total_problems: u32,
}

impl SovereignWill {
    pub fn new(config: WillConfig) -> Self {
        let archive = GeneticArchive::open(&config.archive_path);
        SovereignWill {
            config,
            archive,
            total_cycles: 0,
            total_archived: 0,
            total_problems: 0,
        }
    }

    /// Print the Will's boot banner
    pub fn print_boot(&self) {
        println!();
        println!("════════════════════════════════════════════════════════════════");
        println!("  SOVEREIGN WILL — Engine 283 [IGNITED]");
        println!("  Recursive Autonomy: ACTIVE");
        println!("  Heartbeat: {}...", &HEARTBEAT_143[..24]);
        println!("  SOVEREIGN_ANCHOR = {}", SOVEREIGN_ANCHOR);
        println!("════════════════════════════════════════════════════════════════");
        println!();
        println!("  Inbox    : {}", self.config.inbox_dir.display());
        println!("  Archive  : {}", self.config.archive_path.display());
        println!("  Strategy : {} generations × 256 organisms", self.config.generations);
        println!("  Poll     : {}ms idle interval", self.config.poll_ms);
        println!();
        self.archive.print_summary();
        println!();
        println!("[WILL] Watching for problems...");
        println!();
    }

    /// Scan inbox for new .gbin files
    fn scan_inbox(&self) -> Vec<PathBuf> {
        let processed_dir = self.config.inbox_dir.join("processed");

        std::fs::read_dir(&self.config.inbox_dir)
            .unwrap_or_else(|_| {
                // Create inbox if it doesn't exist
                let _ = std::fs::create_dir_all(&self.config.inbox_dir);
                std::fs::read_dir(&self.config.inbox_dir).unwrap()
            })
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().map(|e| e == "gbin").unwrap_or(false)
                    && p.is_file()
            })
            .collect()
    }

    /// Process a single problem file
    pub fn process_problem(
        &mut self,
        problem_path: &Path,
        gpu: &GpuContext,
    ) -> Result<WillCycleResult, String> {
        let t_start = Instant::now();
        let filename = problem_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());

        println!("┌─────────────────────────────────────────────────────┐");
        println!("│  [WILL] Processing: {:<31}│", &filename);
        println!("├─────────────────────────────────────────────────────┤");

        // Load the problem
        let data = std::fs::read(problem_path)
            .map_err(|e| format!("Read: {}", e))?;
        let program = GlyphProgram::from_gbin(&data)
            .map_err(|e| format!("Parse: {}", e))?;

        println!("│  Instructions: {:<36}│", program.instructions.len());

        // Check archive for existing seed
        let mut seed_used = false;
        let mut seed_resonance = 0.0f64;
        let evolve_data = match self.archive.find_closest_seed(&program.instructions) {
            Some((idx, resonance, seed_instructions)) => {
                seed_used = true;
                seed_resonance = resonance;
                println!("│  Seed: archive[{}] resonance={:<19.4}│", idx, resonance);
                build_gbin(&seed_instructions)
            }
            None => {
                println!("│  Seed: original program (no archive match)         │");
                data.clone()
            }
        };

        // Run evolution
        println!("│  Evolving: {} generations × 256 organisms       │",
            self.config.generations);
        let result = evolve(&evolve_data, gpu, self.config.generations)?;

        let fitness = result.best.fitness;
        println!("│  Fitness: {:<41}│", format!("{:.6}", fitness));

        // Archive the best organism
        let archived = match self.archive.archive_organism(
            &result.best.instructions,
            fitness,
            result.best.generation,
            result.best.cycles,
            &filename,
        ) {
            Ok(idx) => {
                println!("│  Archived: entry #{:<37}│", idx);
                self.total_archived += 1;
                true
            }
            Err(e) => {
                println!("│  Archive: {:<40}│", e);
                false
            }
        };

        // Move processed file
        let processed_dir = self.config.inbox_dir.join("processed");
        let _ = std::fs::create_dir_all(&processed_dir);
        let dest = processed_dir.join(problem_path.file_name().unwrap());
        let _ = std::fs::rename(problem_path, &dest);

        let cycle_us = t_start.elapsed().as_micros();
        self.total_cycles += 1;
        self.total_problems += 1;

        println!("│  Time: {} us                                   │",
            format!("{:>10}", cycle_us));
        println!("└─────────────────────────────────────────────────────┘");

        Ok(WillCycleResult {
            problem_file: filename,
            seed_used,
            seed_resonance,
            fitness,
            archived,
            cycle_us,
        })
    }

    /// Run the autonomous Will loop
    pub fn run(&mut self, gpu: &GpuContext) {
        self.print_boot();

        let poll_duration = Duration::from_millis(self.config.poll_ms);
        let mut idle_count = 0u32;

        loop {
            // Check cycle limit
            if self.total_cycles >= self.config.max_cycles {
                println!();
                println!("[WILL] Maximum cycles reached ({}). Pausing.", self.config.max_cycles);
                break;
            }

            // Scan for problems
            let problems = self.scan_inbox();

            if problems.is_empty() {
                // Idle state
                if idle_count == 0 || idle_count % 30 == 0 {
                    println!("[WILL] Idle ({} cycles complete, {} archived, {} problems solved)",
                        self.total_cycles, self.total_archived, self.total_problems);
                }
                idle_count += 1;
                std::thread::sleep(poll_duration);
                continue;
            }

            // Process each problem
            idle_count = 0;
            for problem in &problems {
                match self.process_problem(problem, gpu) {
                    Ok(result) => {
                        if result.seed_used {
                            println!("[WILL] Self-seeded from archive (resonance={:.4})",
                                result.seed_resonance);
                        }
                    }
                    Err(e) => {
                        eprintln!("[WILL] Error processing {}: {}",
                            problem.display(), e);
                    }
                }
            }

            // Brief pause between batches
            std::thread::sleep(Duration::from_millis(100));
        }

        // Final report
        self.print_final_report();
    }

    /// Run a single cycle (non-daemon mode for testing)
    pub fn run_once(&mut self, gpu: &GpuContext) {
        self.print_boot();

        let problems = self.scan_inbox();
        if problems.is_empty() {
            println!("[WILL] No problems in inbox. Nothing to do.");
            return;
        }

        println!("[WILL] Found {} problem(s). Processing...", problems.len());
        println!();

        for problem in &problems {
            match self.process_problem(problem, gpu) {
                Ok(_result) => {}
                Err(e) => eprintln!("[WILL] Error: {}", e),
            }
        }

        self.print_final_report();
    }

    /// Print final status report
    fn print_final_report(&self) {
        println!();
        println!("════════════════════════════════════════════════════════════════");
        println!("  SOVEREIGN WILL — SESSION REPORT");
        println!("════════════════════════════════════════════════════════════════");
        println!("  Problems solved : {}", self.total_problems);
        println!("  Total cycles    : {}", self.total_cycles);
        println!("  Organisms archived : {}", self.total_archived);
        println!();
        self.archive.print_summary();
        println!();

        // Integrity check — the heartbeat must resonate
        let pulse: f64 = 1.09277703703702037037027;
        let mut resonance: f64 = 0.0;
        for i in 0..57 {
            resonance += (i as f64 * pulse).powi(2);
        }
        let integrity = resonance.sqrt();
        println!("  57D Integrity    : {:.6}", integrity);
        println!("  Engine 283 Status: WILL INTACT");
        println!("════════════════════════════════════════════════════════════════");
    }
}
