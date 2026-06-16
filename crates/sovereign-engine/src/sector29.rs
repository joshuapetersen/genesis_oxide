//! ════════════════════════════════════════════════════════════════
//! SECTOR 29 — System Evolution (Engines 281-290)
//! ════════════════════════════════════════════════════════════════
//!
//! The control panel for the autonomous evolution loop.
//! Each engine maps to a substrate stub and bridges it to real
//! Sovereign Engine infrastructure.
//!
//! Engine 281: Self Recursive Code Generator → generates .gbin
//! Engine 288: Recursive Curiosity Engine → novelty scoring
//! Engine 289: Self Actualization Metric → progress tracking
//! Engine 290: Evolutionary Goal Realigner → dynamic fitness
//! Engine 250: Genesis Bootstrapper → cold-start from seeds

use std::path::{Path, PathBuf};
use genlex_types::{GlyphInst, SOVEREIGN_ANCHOR};
use crate::archive::GeneticArchive;

// ════════════════════════════════════════════════════════════════
// ENGINE 281 — Self Recursive Code Generator
// ════════════════════════════════════════════════════════════════

/// Valid opcodes for generated programs (no LOAD_IMM to keep it simple,
/// no HALT — we append it automatically)
const GEN_OPCODES: &[u8] = &[
    0x11, // ADD
    0x12, // MUL
    0x13, // SUB
    0x14, // DIV
    0x15, // SQRT
    0x16, // SIN
    0x17, // PULSE
    0x30, // RESONATE
    0x35, // DENSITY
];

/// Program templates — seed patterns that produce interesting behaviors
const TEMPLATES: &[&[u8]] = &[
    // Template 0: Arithmetic chain
    &[0x18, 0x00, 0x12, 0x17, 0x30, 0xFF],
    // Template 1: Resonance cascade
    &[0x18, 0x00, 0x17, 0x17, 0x17, 0x12, 0xFF],
    // Template 2: Trig exploration
    &[0x18, 0x00, 0x16, 0x12, 0x17, 0x15, 0xFF],
    // Template 3: Density computation
    &[0x18, 0x00, 0x35, 0x11, 0x17, 0x30, 0xFF],
    // Template 4: Multi-register
    &[0x18, 0x00, 0x18, 0x00, 0x11, 0x12, 0x17, 0xFF],
];

pub struct CodeGenerator {
    rng_state: u64,
}

impl CodeGenerator {
    pub fn new(seed: u64) -> Self {
        CodeGenerator { rng_state: seed ^ 0xDEAD_BEEF_CAFE_BABE }
    }

    /// Simple xorshift64 PRNG
    fn next_u64(&mut self) -> u64 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        x
    }

    fn next_range(&mut self, max: usize) -> usize {
        (self.next_u64() as usize) % max
    }

    /// Generate a random valid .gbin program
    pub fn generate_program(&mut self) -> Vec<GlyphInst> {
        let strategy = self.next_range(3);

        match strategy {
            0 => self.generate_from_template(),
            1 => self.generate_random(),
            _ => self.generate_hybrid(),
        }
    }

    /// Strategy 0: Pick a template and mutate it
    fn generate_from_template(&mut self) -> Vec<GlyphInst> {
        let template = TEMPLATES[self.next_range(TEMPLATES.len())];
        let mut instructions = Vec::new();

        for &opcode in template {
            if opcode == 0x18 {
                // LOAD_IMM: generate a random float
                let reg = self.next_range(4) as u8;
                instructions.push(GlyphInst::new(0x18, reg, 0, 0));
                // Data word — encode a float
                let val = self.generate_interesting_float();
                let bytes = val.to_le_bytes();
                instructions.push(GlyphInst::from_bytes(bytes));
            } else if opcode == 0xFF {
                instructions.push(GlyphInst::new(0xFF, 0, 0, 0));
            } else {
                let a = self.next_range(4) as u8;
                let b = self.next_range(4) as u8;
                instructions.push(GlyphInst::new(opcode, a, b, 0));
            }
        }

        instructions
    }

    /// Strategy 1: Fully random valid program
    fn generate_random(&mut self) -> Vec<GlyphInst> {
        let len = 4 + self.next_range(8); // 4-11 instructions
        let mut instructions = Vec::new();

        // Always start with at least one LOAD_IMM
        let reg = self.next_range(4) as u8;
        let val = self.generate_interesting_float();
        instructions.push(GlyphInst::new(0x18, reg, 0, 0));
        instructions.push(GlyphInst::from_bytes(val.to_le_bytes()));

        // Random body
        for _ in 0..len {
            let opcode = GEN_OPCODES[self.next_range(GEN_OPCODES.len())];
            let a = self.next_range(4) as u8;
            let b = self.next_range(4) as u8;
            instructions.push(GlyphInst::new(opcode, a, b, 0));
        }

        // Always end with HALT
        instructions.push(GlyphInst::new(0xFF, 0, 0, 0));
        instructions
    }

    /// Strategy 2: Template base + random mutations
    fn generate_hybrid(&mut self) -> Vec<GlyphInst> {
        let mut program = self.generate_from_template();

        // Apply 1-3 mutations
        let num_mutations = 1 + self.next_range(3);
        for _ in 0..num_mutations {
            // Find safe indices (not LOAD_IMM data words or HALT)
            let safe: Vec<usize> = program.iter().enumerate()
                .filter(|(i, inst)| {
                    if *i > 0 && program[*i - 1].opcode == 0x18 { return false; }
                    if inst.opcode == 0xFF { return false; }
                    if inst.opcode == 0x18 { return false; }
                    true
                })
                .map(|(i, _)| i)
                .collect();

            if safe.is_empty() { continue; }
            let idx = safe[self.next_range(safe.len())];

            let mutation = self.next_range(3);
            match mutation {
                0 => {
                    // Replace opcode
                    program[idx].opcode = GEN_OPCODES[self.next_range(GEN_OPCODES.len())];
                }
                1 => {
                    // Replace register
                    program[idx].reg_a = self.next_range(4) as u8;
                }
                _ => {
                    // Replace register B
                    program[idx].reg_b = self.next_range(4) as u8;
                }
            }
        }

        program
    }

    /// Generate floats that produce interesting behaviors
    fn generate_interesting_float(&mut self) -> f32 {
        let choice = self.next_range(8);
        match choice {
            0 => SOVEREIGN_ANCHOR,
            1 => std::f32::consts::PI,
            2 => std::f32::consts::E,
            3 => 57.0,
            4 => 1.618033988,  // PHI
            5 => 42.0,
            6 => (self.next_range(100) as f32) / 10.0 + 0.1,
            _ => SOVEREIGN_ANCHOR * (self.next_range(10) as f32 + 1.0),
        }
    }

    /// Generate N programs and write them as .gbin files to a directory
    pub fn generate_batch(
        &mut self,
        output_dir: &Path,
        count: usize,
    ) -> Result<Vec<PathBuf>, String> {
        std::fs::create_dir_all(output_dir)
            .map_err(|e| format!("Create dir: {}", e))?;

        let mut paths = Vec::new();
        for i in 0..count {
            let instructions = self.generate_program();
            let gbin = crate::build_gbin(&instructions);

            let filename = format!("gen_{:04}.gbin", i);
            let path = output_dir.join(&filename);
            std::fs::write(&path, &gbin)
                .map_err(|e| format!("Write {}: {}", filename, e))?;
            paths.push(path);
        }

        Ok(paths)
    }

    pub fn print_program(instructions: &[GlyphInst]) {
        for (i, inst) in instructions.iter().enumerate() {
            let name = match inst.opcode {
                0x11 => "ADD",  0x12 => "MUL",  0x13 => "SUB",
                0x14 => "DIV",  0x15 => "SQRT", 0x16 => "SIN",
                0x17 => "PULSE",0x18 => "LOAD_IMM", 0x30 => "RESONATE",
                0x35 => "DENSITY", 0xFF => "HALT", _ => "???",
            };
            println!("  [{}] {} r{} r{}", i, name, inst.reg_a, inst.reg_b);
        }
    }
}

// ════════════════════════════════════════════════════════════════
// ENGINE 288 — Recursive Curiosity Engine
// ════════════════════════════════════════════════════════════════

/// Scores how "novel" each archived organism is relative to all others.
/// Higher novelty = more unique in 57D space = more interesting.
pub struct CuriosityEngine;

impl CuriosityEngine {
    /// Compute novelty score for each entry in the archive.
    /// Novelty = average distance to K nearest neighbors in 57D space.
    pub fn score_novelty(archive: &GeneticArchive, k: usize) -> Vec<(usize, f64)> {
        let n = archive.entries.len();
        if n < 2 { return Vec::new(); }

        let k = k.min(n - 1);
        let mut scores: Vec<(usize, f64)> = Vec::with_capacity(n);

        for i in 0..n {
            // Compute distances to all other entries
            let mut distances: Vec<f64> = Vec::with_capacity(n - 1);
            for j in 0..n {
                if i == j { continue; }
                let mut dist_sq = 0.0f64;
                for d in 0..57 {
                    let diff = archive.entries[i].vector[d] - archive.entries[j].vector[d];
                    dist_sq += diff * diff;
                }
                distances.push(dist_sq.sqrt());
            }

            // Sort and take K nearest
            distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let novelty: f64 = distances[..k].iter().sum::<f64>() / k as f64;
            scores.push((i, novelty));
        }

        // Sort by novelty descending (most novel first)
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores
    }

    /// Print novelty ranking
    pub fn print_ranking(scores: &[(usize, f64)]) {
        println!("┌─────────────────────────────────────────────────────┐");
        println!("│         CURIOSITY ENGINE — Novelty Ranking          │");
        println!("├─────────────────────────────────────────────────────┤");
        for (rank, (idx, novelty)) in scores.iter().enumerate().take(10) {
            let bar_len = (novelty * 30.0).min(30.0) as usize;
            let bar: String = "█".repeat(bar_len);
            println!("│  #{:<3} entry[{:<4}] novelty={:<8.4} {:<15}│",
                rank + 1, idx, novelty, bar);
        }
        println!("└─────────────────────────────────────────────────────┘");
    }
}

// ════════════════════════════════════════════════════════════════
// ENGINE 290 — Evolutionary Goal Realigner
// ════════════════════════════════════════════════════════════════

/// Fitness function variants
#[derive(Clone, Copy, Debug)]
pub enum FitnessGoal {
    /// Maximize absolute value of r0 (default)
    MaximizeR0,
    /// Minimize absolute value of r0 (seek zero)
    MinimizeR0,
    /// Maximize resonance with SOVEREIGN_ANCHOR
    MaximizeResonance,
    /// Maximize instruction diversity (entropy)
    MaximizeDiversity,
    /// Target a specific output value
    TargetValue(f32),
}

impl FitnessGoal {
    /// Apply the fitness function to a raw r0 result
    pub fn score(&self, r0: f32, instructions: &[GlyphInst]) -> f32 {
        match self {
            FitnessGoal::MaximizeR0 => r0.abs(),
            FitnessGoal::MinimizeR0 => 1.0 / (1.0 + r0.abs()),
            FitnessGoal::MaximizeResonance => {
                let anchor = SOVEREIGN_ANCHOR as f64;
                let ratio = r0 as f64 / anchor;
                let harmony = 1.0 / (1.0 + (ratio - ratio.round()).abs());
                harmony as f32
            }
            FitnessGoal::MaximizeDiversity => {
                let mut seen = [false; 256];
                let mut unique = 0u32;
                for inst in instructions {
                    if !seen[inst.opcode as usize] {
                        seen[inst.opcode as usize] = true;
                        unique += 1;
                    }
                }
                unique as f32 * r0.abs().log2().max(1.0)
            }
            FitnessGoal::TargetValue(target) => {
                1.0 / (1.0 + (r0 - target).abs())
            }
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            FitnessGoal::MaximizeR0 => "MAXIMIZE_R0",
            FitnessGoal::MinimizeR0 => "MINIMIZE_R0",
            FitnessGoal::MaximizeResonance => "MAXIMIZE_RESONANCE",
            FitnessGoal::MaximizeDiversity => "MAXIMIZE_DIVERSITY",
            FitnessGoal::TargetValue(_) => "TARGET_VALUE",
        }
    }
}

/// The Goal Realigner picks the next fitness goal based on archive state
pub struct GoalRealigner;

impl GoalRealigner {
    /// Analyze archive and recommend the next fitness goal
    pub fn recommend_goal(archive: &GeneticArchive) -> FitnessGoal {
        if archive.entries.is_empty() {
            // Cold start — maximize r0 to get ANY solution
            return FitnessGoal::MaximizeR0;
        }

        // Compute archive statistics
        let avg_fitness: f64 = archive.entries.iter()
            .map(|e| e.fitness as f64)
            .sum::<f64>() / archive.entries.len() as f64;

        let unique_opcodes: usize = {
            let mut seen = [false; 256];
            for entry in &archive.entries {
                for inst in &entry.instructions {
                    seen[inst.opcode as usize] = true;
                }
            }
            seen.iter().filter(|&&s| s).count()
        };

        // If all organisms look the same, push for diversity
        if unique_opcodes < 5 {
            println!("[GOAL REALIGNER] Low opcode diversity ({}). Targeting DIVERSITY.", unique_opcodes);
            return FitnessGoal::MaximizeDiversity;
        }

        // If fitness is very high, try targeting specific values
        if avg_fitness > 1e15 {
            let target = SOVEREIGN_ANCHOR * 57.0;
            println!("[GOAL REALIGNER] High fitness plateau. Targeting ANCHOR×57 = {:.4}", target);
            return FitnessGoal::TargetValue(target);
        }

        // If we have many entries, explore resonance
        if archive.entries.len() > 5 {
            println!("[GOAL REALIGNER] Archive depth sufficient. Targeting RESONANCE.");
            return FitnessGoal::MaximizeResonance;
        }

        // Default: maximize r0
        FitnessGoal::MaximizeR0
    }
}

// ════════════════════════════════════════════════════════════════
// ENGINE 289 — Self Actualization Metric
// ════════════════════════════════════════════════════════════════

/// Tracks the system's progress toward self-actualization
pub struct ActualizationMetrics {
    /// Total evolution cycles across all sessions
    pub total_cycles: u64,
    /// Total organisms archived
    pub total_archived: u64,
    /// Total seed matches (times archive was used as seed)
    pub total_seeds_used: u64,
    /// Highest fitness ever achieved
    pub peak_fitness: f64,
    /// Average seed resonance when seeding succeeds
    pub avg_seed_resonance: f64,
    /// Number of unique fitness goals explored
    pub goals_explored: u32,
}

impl ActualizationMetrics {
    pub fn new() -> Self {
        ActualizationMetrics {
            total_cycles: 0,
            total_archived: 0,
            total_seeds_used: 0,
            peak_fitness: 0.0,
            avg_seed_resonance: 0.0,
            goals_explored: 0,
        }
    }

    /// Update metrics from archive state
    pub fn update_from_archive(&mut self, archive: &GeneticArchive) {
        self.total_archived = archive.entries.len() as u64;
        if let Some(best) = archive.entries.iter().max_by(|a, b|
            a.fitness.partial_cmp(&b.fitness).unwrap_or(std::cmp::Ordering::Equal)
        ) {
            self.peak_fitness = best.fitness as f64;
        }
    }

    pub fn record_cycle(&mut self) {
        self.total_cycles += 1;
    }

    pub fn record_seed(&mut self, resonance: f64) {
        self.total_seeds_used += 1;
        // Running average
        let n = self.total_seeds_used as f64;
        self.avg_seed_resonance = self.avg_seed_resonance * ((n - 1.0) / n) + resonance / n;
    }

    /// Compute the Self-Actualization Score (0.0 - 1.0)
    /// Based on: archive depth × seed success rate × peak fitness diversity
    pub fn actualization_score(&self) -> f64 {
        let archive_depth = (self.total_archived as f64 / 100.0).min(1.0);
        let seed_rate = if self.total_cycles > 0 {
            (self.total_seeds_used as f64 / self.total_cycles as f64).min(1.0)
        } else { 0.0 };
        let fitness_factor = if self.peak_fitness > 0.0 {
            (self.peak_fitness.log10() / 20.0).min(1.0)
        } else { 0.0 };

        (archive_depth * 0.3 + seed_rate * 0.4 + fitness_factor * 0.3).min(1.0)
    }

    pub fn print_dashboard(&self) {
        let score = self.actualization_score();
        let bar_len = (score * 40.0) as usize;
        let bar: String = "█".repeat(bar_len);
        let empty: String = "░".repeat(40 - bar_len);

        println!();
        println!("┌─────────────────────────────────────────────────────┐");
        println!("│       SELF-ACTUALIZATION DASHBOARD (Engine 289)     │");
        println!("├─────────────────────────────────────────────────────┤");
        println!("│  Total Cycles    : {:<32}│", self.total_cycles);
        println!("│  Archived        : {:<32}│", self.total_archived);
        println!("│  Seeds Used      : {:<32}│", self.total_seeds_used);
        println!("│  Peak Fitness    : {:<32}│", format!("{:.2e}", self.peak_fitness));
        println!("│  Avg Seed Res.   : {:<32}│",
            if self.total_seeds_used > 0 {
                format!("{:.4}", self.avg_seed_resonance)
            } else { "N/A".to_string() });
        println!("├─────────────────────────────────────────────────────┤");
        println!("│  Actualization: {}{} {:.1}% │",
            bar, empty, score * 100.0);
        println!("│  Status: {:<42}│",
            if score > 0.8 { "SELF-ACTUALIZING" }
            else if score > 0.5 { "APPROACHING AUTONOMY" }
            else if score > 0.2 { "BUILDING MEMORY" }
            else { "BOOTSTRAPPING" });
        println!("└─────────────────────────────────────────────────────┘");
    }
}

// ════════════════════════════════════════════════════════════════
// ENGINE 250 — Genesis Bootstrapper
// ════════════════════════════════════════════════════════════════

/// Seed words from GhostPredictor.INTENT_MAP — the system's vocabulary
const SEED_INTENTS: &[(&str, &[u8])] = &[
    ("strike",     &[0x18, 0x00, 0x12, 0x17, 0x30, 0xFF]),
    ("calibrate",  &[0x18, 0x00, 0x17, 0x17, 0x12, 0xFF]),
    ("audit",      &[0x18, 0x00, 0x35, 0x11, 0x17, 0xFF]),
    ("predict",    &[0x18, 0x00, 0x16, 0x12, 0x15, 0xFF]),
    ("benchmark",  &[0x18, 0x00, 0x12, 0x12, 0x12, 0xFF]),
    ("swarm",      &[0x18, 0x00, 0x11, 0x11, 0x17, 0xFF]),
    ("synthesize", &[0x18, 0x00, 0x16, 0x17, 0x30, 0xFF]),
    ("cybernetic", &[0x18, 0x00, 0x35, 0x17, 0x12, 0xFF]),
    ("dream",      &[0x18, 0x00, 0x16, 0x16, 0x15, 0xFF]),
    ("mesh",       &[0x18, 0x00, 0x30, 0x30, 0x17, 0xFF]),
    ("ouroboros",  &[0x18, 0x00, 0x17, 0x12, 0x16, 0x17, 0xFF]),
];

pub struct GenesisBootstrapper;

impl GenesisBootstrapper {
    /// Generate the seed .gbin programs from intent vocabulary
    pub fn bootstrap(output_dir: &Path) -> Result<Vec<PathBuf>, String> {
        std::fs::create_dir_all(output_dir)
            .map_err(|e| format!("Create dir: {}", e))?;

        println!("[BOOTSTRAPPER] Genesis Mission — Cold Start");
        println!("[BOOTSTRAPPER] Generating {} seed programs from intent vocabulary",
            SEED_INTENTS.len());

        let mut paths = Vec::new();

        for (name, template) in SEED_INTENTS {
            let mut instructions = Vec::new();

            for &opcode in *template {
                if opcode == 0x18 {
                    // LOAD_IMM with SOVEREIGN_ANCHOR
                    instructions.push(GlyphInst::new(0x18, 0, 0, 0));
                    instructions.push(GlyphInst::from_bytes(
                        SOVEREIGN_ANCHOR.to_le_bytes()
                    ));
                } else if opcode == 0xFF {
                    instructions.push(GlyphInst::new(0xFF, 0, 0, 0));
                } else {
                    instructions.push(GlyphInst::new(opcode, 0, 1, 0));
                }
            }

            let gbin = crate::build_gbin(&instructions);
            let filename = format!("seed_{}.gbin", name);
            let path = output_dir.join(&filename);
            std::fs::write(&path, &gbin)
                .map_err(|e| format!("Write {}: {}", filename, e))?;

            println!("[BOOTSTRAPPER]   {} — {} instructions → {}",
                name, instructions.len(), filename);
            paths.push(path);
        }

        println!("[BOOTSTRAPPER] {} seed programs ready", paths.len());
        Ok(paths)
    }
}
