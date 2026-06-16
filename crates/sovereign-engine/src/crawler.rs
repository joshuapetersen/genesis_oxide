//! ════════════════════════════════════════════════════════════════
//! SOVEREIGN CRAWLER — Recursive Directory-to-Archive Pipeline
//! ════════════════════════════════════════════════════════════════
//!
//! Walks an entire directory tree, filters by extension and size,
//! deduplicates by SHA256 fingerprint, forges each file into .gbin,
//! and feeds the will_inbox for ignition.
//!
//! Usage: sovereign --forge-dir "C:\" --recursive
//!                  --extensions "py,rs,js,cpp"
//!                  --max-size 1048576
//!                  --dedup

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::ace::AceTokenNexus;
use crate::forge::VolumetricForge;

/// Default source code extensions to process
const DEFAULT_EXTENSIONS: &[&str] = &[
    "py", "rs", "js", "ts", "jsx", "tsx",
    "cpp", "c", "h", "hpp", "cc", "cxx",
    "cs", "java", "go", "rb", "php", "swift", "kt",
    "toml", "json", "yaml", "yml", "xml",
    "html", "css", "scss", "sql",
    "sh", "bat", "ps1", "cmd",
    "md", "txt", "cfg", "ini", "conf",
    "proto", "asm", "s", "wasm",
    "r", "jl", "lua", "pl", "ex", "exs",
    "zig", "nim", "v", "d",
];

/// Directories to always skip
const SKIP_DIRS: &[&str] = &[
    ".git", "node_modules", "__pycache__", ".vs", ".vscode",
    "target", "build", "dist", "bin", "obj",
    "$Recycle.Bin", "System Volume Information",
    "Windows", "ProgramData",
    ".gemini", ".cache", ".npm", ".cargo",
    "AppData",
];

/// Default max file size: 1MB
const DEFAULT_MAX_SIZE: u64 = 1_048_576;

/// Crawl statistics
#[derive(Debug, Clone)]
pub struct CrawlStats {
    pub files_found: usize,
    pub files_processed: usize,
    pub files_skipped_extension: usize,
    pub files_skipped_size: usize,
    pub files_skipped_read_error: usize,
    pub files_skipped_dedup: usize,
    pub files_forged: usize,
    pub files_forge_failed: usize,
    pub dirs_scanned: usize,
    pub dirs_skipped: usize,
    pub total_bytes_read: u64,
    pub total_forge_bytes: u64,
    pub elapsed_ms: f64,
}

impl CrawlStats {
    fn new() -> Self {
        CrawlStats {
            files_found: 0,
            files_processed: 0,
            files_skipped_extension: 0,
            files_skipped_size: 0,
            files_skipped_read_error: 0,
            files_skipped_dedup: 0,
            files_forged: 0,
            files_forge_failed: 0,
            dirs_scanned: 0,
            dirs_skipped: 0,
            total_bytes_read: 0,
            total_forge_bytes: 0,
            elapsed_ms: 0.0,
        }
    }
}

/// Configuration for the crawler
pub struct CrawlConfig {
    /// Root directory to crawl
    pub root: PathBuf,
    /// File extensions to include (lowercase, no dot)
    pub extensions: HashSet<String>,
    /// Maximum file size in bytes
    pub max_size: u64,
    /// Enable deduplication by content hash
    pub dedup: bool,
    /// Output directory for forged .gbin files
    pub output_dir: PathBuf,
    /// Progress report interval (every N files)
    pub report_interval: usize,
    /// Dry run — count files without forging
    pub dry_run: bool,
}

impl CrawlConfig {
    pub fn new(root: &str) -> Self {
        let exts: HashSet<String> = DEFAULT_EXTENSIONS.iter()
            .map(|s| s.to_string())
            .collect();

        CrawlConfig {
            root: PathBuf::from(root),
            extensions: exts,
            max_size: DEFAULT_MAX_SIZE,
            dedup: true,
            output_dir: PathBuf::from("will_inbox"),
            report_interval: 1000,
            dry_run: false,
        }
    }

    pub fn with_extensions(mut self, ext_str: &str) -> Self {
        self.extensions = ext_str.split(',')
            .map(|s| s.trim().to_lowercase().trim_start_matches('.').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        self
    }

    pub fn with_max_size(mut self, size: u64) -> Self {
        self.max_size = size;
        self
    }

    pub fn with_output(mut self, dir: &str) -> Self {
        self.output_dir = PathBuf::from(dir);
        self
    }

    pub fn with_dry_run(mut self, dry: bool) -> Self {
        self.dry_run = dry;
        self
    }
}

/// The Sovereign Crawler
pub struct SovereignCrawler {
    config: CrawlConfig,
    seen_fingerprints: HashSet<u64>,
    forge: VolumetricForge,
    stats: CrawlStats,
}

impl SovereignCrawler {
    pub fn new(config: CrawlConfig) -> Self {
        SovereignCrawler {
            config,
            seen_fingerprints: HashSet::with_capacity(100_000),
            forge: VolumetricForge::new(),
            stats: CrawlStats::new(),
        }
    }

    /// Run the full crawl
    pub fn crawl(&mut self) -> CrawlStats {
        let start = Instant::now();

        println!();
        println!("════════════════════════════════════════════════════════════════");
        println!("  SOVEREIGN CRAWLER — TOTAL SYSTEM ASSIMILATION");
        println!("  Root:       {}", self.config.root.display());
        println!("  Extensions: {} types", self.config.extensions.len());
        println!("  Max size:   {} bytes", self.config.max_size);
        println!("  Dedup:      {}", self.config.dedup);
        println!("  Output:     {}", self.config.output_dir.display());
        println!("  Dry run:    {}", self.config.dry_run);
        println!("════════════════════════════════════════════════════════════════");
        println!();

        // Ensure output directory exists
        if !self.config.dry_run {
            if let Err(e) = fs::create_dir_all(&self.config.output_dir) {
                eprintln!("[CRAWLER] Failed to create output dir: {}", e);
                return self.stats.clone();
            }
        }

        // Start recursive walk
        let root = self.config.root.clone();
        self.walk_dir(&root);

        self.stats.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        self.print_final_report();
        self.stats.clone()
    }

    /// Recursive directory walk
    fn walk_dir(&mut self, dir: &Path) {
        // Check if we should skip this directory
        if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
            if SKIP_DIRS.iter().any(|skip| name.eq_ignore_ascii_case(skip)) {
                self.stats.dirs_skipped += 1;
                return;
            }
        }

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => {
                // Permission denied or other error — skip silently
                self.stats.dirs_skipped += 1;
                return;
            }
        };

        self.stats.dirs_scanned += 1;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();

            if path.is_dir() {
                self.walk_dir(&path);
            } else if path.is_file() {
                self.stats.files_found += 1;
                self.process_file(&path);

                // Progress report
                if self.stats.files_found % self.config.report_interval == 0 {
                    self.print_progress();
                }
            }
        }
    }

    /// Process a single file
    fn process_file(&mut self, path: &Path) {
        // Check extension
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if !self.config.extensions.contains(&ext) {
            self.stats.files_skipped_extension += 1;
            return;
        }

        // Check file size
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => {
                self.stats.files_skipped_read_error += 1;
                return;
            }
        };

        if metadata.len() > self.config.max_size {
            self.stats.files_skipped_size += 1;
            return;
        }

        if metadata.len() == 0 {
            self.stats.files_skipped_size += 1;
            return;
        }

        // Read file content
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => {
                // Binary file or encoding error — skip
                self.stats.files_skipped_read_error += 1;
                return;
            }
        };

        self.stats.total_bytes_read += content.len() as u64;

        // Dedup check
        if self.config.dedup {
            let fingerprint = AceTokenNexus::fingerprint(&content);
            if !self.seen_fingerprints.insert(fingerprint) {
                self.stats.files_skipped_dedup += 1;
                return;
            }
        }

        self.stats.files_processed += 1;

        // Dry run — count but don't forge
        if self.config.dry_run {
            return;
        }

        // Forge the file
        let label: String = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
            .take(48)
            .collect();

        match self.forge.forge_source(&content, &label, &self.config.output_dir) {
            Ok(fr) => {
                self.stats.files_forged += 1;
                self.stats.total_forge_bytes += fr.binary_size as u64;
            }
            Err(_) => {
                self.stats.files_forge_failed += 1;
            }
        }
    }

    /// Print progress report
    fn print_progress(&self) {
        let rate = if self.stats.files_found > 0 {
            self.stats.files_forged as f64 / self.stats.files_found as f64 * 100.0
        } else {
            0.0
        };

        println!("[CRAWLER] {:>8} found | {:>7} forged ({:.1}%) | {:>6} dirs | {:>7} dedup | {:>7} skip-ext | {} MB read",
            self.stats.files_found,
            self.stats.files_forged,
            rate,
            self.stats.dirs_scanned,
            self.stats.files_skipped_dedup,
            self.stats.files_skipped_extension,
            self.stats.total_bytes_read / 1_048_576,
        );
    }

    /// Print final report
    fn print_final_report(&self) {
        let elapsed_secs = self.stats.elapsed_ms / 1000.0;
        let files_per_sec = if elapsed_secs > 0.0 {
            self.stats.files_found as f64 / elapsed_secs
        } else {
            0.0
        };

        println!();
        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│           SOVEREIGN CRAWLER — FINAL REPORT                 │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Directories scanned:    {:>10}                        │", self.stats.dirs_scanned);
        println!("│  Directories skipped:    {:>10}                        │", self.stats.dirs_skipped);
        println!("│  Files found:            {:>10}                        │", self.stats.files_found);
        println!("│  Files processed:        {:>10}                        │", self.stats.files_processed);
        println!("│  Files forged:           {:>10}                        │", self.stats.files_forged);
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Skipped (wrong ext):    {:>10}                        │", self.stats.files_skipped_extension);
        println!("│  Skipped (too large):    {:>10}                        │", self.stats.files_skipped_size);
        println!("│  Skipped (read error):   {:>10}                        │", self.stats.files_skipped_read_error);
        println!("│  Skipped (duplicate):    {:>10}                        │", self.stats.files_skipped_dedup);
        println!("│  Forge failures:         {:>10}                        │", self.stats.files_forge_failed);
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  Total bytes read:       {:>10} MB                     │", self.stats.total_bytes_read / 1_048_576);
        println!("│  Total forge output:     {:>10} KB                     │", self.stats.total_forge_bytes / 1024);
        println!("│  Elapsed:                {:>10.1}s                      │", elapsed_secs);
        println!("│  Speed:                  {:>10.0} files/sec             │", files_per_sec);
        println!("├─────────────────────────────────────────────────────────────┤");
        if self.config.dry_run {
            println!("│  MODE: DRY RUN — no files were forged                     │");
        } else {
            println!("│  Output: {}  │", format!("{:<47}", self.config.output_dir.display()));
            println!("│  Run: sovereign --ignite to evolve forged organisms       │");
        }
        println!("└─────────────────────────────────────────────────────────────┘");
    }
}
