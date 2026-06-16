//! ════════════════════════════════════════════════════════════════
//! SOVEREIGN AUDITOR — The Engine Inspects Itself
//! ════════════════════════════════════════════════════════════════
//!
//! Walks a directory tree, extracts the purpose of each file from
//! its first comment/docstring, encodes it into a 57D vector via
//! GeometricTokenizer, and uses GPU similarity search to find:
//!   - Duplicate files (distance < 0.1)
//!   - Clusters of related files
//!   - Orphans (no close neighbors)
//!   - Stub files (minimal logic)
//!
//! Usage:
//!   sovereign --audit <directory> [--depth <N>]

use std::path::{Path, PathBuf};
use std::time::Instant;
use genlex_types::SOVEREIGN_ANCHOR;

/// Maximum files to audit in a single pass
const MAX_FILES: usize = 50_000;

/// File classification
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileClass {
    /// Stub — printline only, no real logic
    Stub,
    /// Logic — has real computation
    Logic,
    /// Wired — connects to other modules
    Wired,
    /// Data — binary/config file
    Data,
}

/// A single audited file
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub path: PathBuf,
    pub class: FileClass,
    pub lines: usize,
    pub purpose: String,
    pub vector: [f64; 57],
    pub signature: u64,
}

/// Audit results
pub struct AuditReport {
    pub entries: Vec<AuditEntry>,
    pub duplicates: Vec<(usize, usize, f64)>,  // (i, j, distance)
    pub clusters: Vec<Vec<usize>>,
    pub orphans: Vec<usize>,
    pub stubs: Vec<usize>,
    pub scan_time_ms: u128,
    pub encode_time_ms: u128,
}

/// The Sovereign Auditor
pub struct SovereignAuditor {
    pub entries: Vec<AuditEntry>,
}

impl SovereignAuditor {
    pub fn new() -> Self {
        SovereignAuditor { entries: Vec::new() }
    }

    /// Scan a directory tree and classify files
    pub fn scan(&mut self, root: &Path, max_depth: usize) -> Result<usize, String> {
        println!("[AUDITOR] Scanning: {}", root.display());
        let t_start = Instant::now();

        self.walk_dir(root, 0, max_depth)?;

        let scan_ms = t_start.elapsed().as_millis();
        println!("[AUDITOR] Found {} files in {} ms", self.entries.len(), scan_ms);
        Ok(self.entries.len())
    }

    fn walk_dir(&mut self, dir: &Path, depth: usize, max_depth: usize) -> Result<(), String> {
        if depth > max_depth || self.entries.len() >= MAX_FILES { return Ok(()); }

        let entries = std::fs::read_dir(dir)
            .map_err(|e| format!("Read dir {}: {}", dir.display(), e))?;

        for entry in entries {
            if self.entries.len() >= MAX_FILES { break; }
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            let name = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Skip hidden, target, node_modules, __pycache__
            if name.starts_with('.') || name == "target" || name == "node_modules"
                || name == "__pycache__" || name == ".git" {
                continue;
            }

            if path.is_dir() {
                self.walk_dir(&path, depth + 1, max_depth)?;
            } else if is_source_file(&name) {
                if let Some(entry) = self.audit_file(&path) {
                    self.entries.push(entry);
                }
            }
        }

        Ok(())
    }

    /// Audit a single source file
    fn audit_file(&self, path: &Path) -> Option<AuditEntry> {
        let content = std::fs::read_to_string(path).ok()?;
        let lines = content.lines().count();

        // Extract purpose from first comment/docstring
        let purpose = extract_purpose(&content);

        // Classify
        let class = classify_file(&content, lines);

        // Encode purpose string into 57D vector
        let vector = encode_text_to_57d(&purpose);

        // Generate signature
        let signature = compute_signature(&vector);

        Some(AuditEntry {
            path: path.to_path_buf(),
            class,
            lines,
            purpose,
            vector,
            signature,
        })
    }

    /// Find duplicates, clusters, and orphans using brute-force distance
    pub fn analyze(&self) -> AuditReport {
        let t_start = Instant::now();
        let n = self.entries.len();

        println!("[AUDITOR] Analyzing {} files for duplicates and clusters...", n);

        // Compute pairwise distances (brute force for now, GPU later)
        let mut duplicates: Vec<(usize, usize, f64)> = Vec::new();
        let mut neighbor_count = vec![0u32; n];
        let distance_threshold = 0.15; // Very close = duplicate
        let cluster_threshold = 0.40;  // Reasonably close = cluster

        // Build adjacency for clustering
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); n];

        for i in 0..n {
            for j in (i + 1)..n {
                let dist = euclidean_distance(&self.entries[i].vector, &self.entries[j].vector);

                if dist < distance_threshold {
                    duplicates.push((i, j, dist));
                }
                if dist < cluster_threshold {
                    adjacency[i].push(j);
                    adjacency[j].push(i);
                    neighbor_count[i] += 1;
                    neighbor_count[j] += 1;
                }
            }
        }

        // Find clusters using simple connected components
        let mut visited = vec![false; n];
        let mut clusters: Vec<Vec<usize>> = Vec::new();

        for i in 0..n {
            if visited[i] || adjacency[i].is_empty() { continue; }
            let mut cluster = Vec::new();
            let mut stack = vec![i];
            while let Some(node) = stack.pop() {
                if visited[node] { continue; }
                visited[node] = true;
                cluster.push(node);
                for &neighbor in &adjacency[node] {
                    if !visited[neighbor] {
                        stack.push(neighbor);
                    }
                }
            }
            if cluster.len() > 1 {
                clusters.push(cluster);
            }
        }

        // Sort clusters by size descending
        clusters.sort_by(|a, b| b.len().cmp(&a.len()));

        // Find orphans (no neighbors at all)
        let orphans: Vec<usize> = (0..n)
            .filter(|&i| neighbor_count[i] == 0)
            .collect();

        // Find stubs
        let stubs: Vec<usize> = (0..n)
            .filter(|&i| self.entries[i].class == FileClass::Stub)
            .collect();

        let encode_ms = t_start.elapsed().as_millis();

        AuditReport {
            entries: self.entries.clone(),
            duplicates,
            clusters,
            orphans,
            stubs,
            scan_time_ms: 0,
            encode_time_ms: encode_ms,
        }
    }
}

impl AuditReport {
    pub fn print_summary(&self) {
        println!();
        println!("════════════════════════════════════════════════════════════════");
        println!("  SOVEREIGN AUDITOR — SYSTEM REPORT");
        println!("════════════════════════════════════════════════════════════════");
        println!();

        // Classification breakdown
        let stubs = self.entries.iter().filter(|e| e.class == FileClass::Stub).count();
        let logic = self.entries.iter().filter(|e| e.class == FileClass::Logic).count();
        let wired = self.entries.iter().filter(|e| e.class == FileClass::Wired).count();
        let data = self.entries.iter().filter(|e| e.class == FileClass::Data).count();

        println!("  FILES SCANNED : {}", self.entries.len());
        println!("  ├─ Stubs      : {} ({:.1}%)", stubs, stubs as f64 / self.entries.len().max(1) as f64 * 100.0);
        println!("  ├─ Logic      : {} ({:.1}%)", logic, logic as f64 / self.entries.len().max(1) as f64 * 100.0);
        println!("  ├─ Wired      : {} ({:.1}%)", wired, wired as f64 / self.entries.len().max(1) as f64 * 100.0);
        println!("  └─ Data       : {}", data);
        println!();

        // Duplicates
        println!("  DUPLICATES    : {}", self.duplicates.len());
        for (i, (a, b, dist)) in self.duplicates.iter().enumerate().take(10) {
            let name_a = self.entries[*a].path.file_name()
                .map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let name_b = self.entries[*b].path.file_name()
                .map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            println!("    {}. {} ↔ {} (dist={:.4})", i + 1, name_a, name_b, dist);
        }
        if self.duplicates.len() > 10 {
            println!("    ... and {} more", self.duplicates.len() - 10);
        }
        println!();

        // Clusters
        println!("  CLUSTERS      : {} (groups of related files)", self.clusters.len());
        for (i, cluster) in self.clusters.iter().enumerate().take(5) {
            println!("    Cluster #{} ({} files):", i + 1, cluster.len());
            for &idx in cluster.iter().take(5) {
                let name = self.entries[idx].path.file_name()
                    .map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                println!("      - {} ({:?}, {} lines)", name, self.entries[idx].class, self.entries[idx].lines);
            }
            if cluster.len() > 5 {
                println!("      ... and {} more", cluster.len() - 5);
            }
        }
        if self.clusters.len() > 5 {
            println!("    ... and {} more clusters", self.clusters.len() - 5);
        }
        println!();

        // Orphans
        println!("  ORPHANS       : {} (no close neighbors in 57D space)", self.orphans.len());
        for &idx in self.orphans.iter().take(5) {
            let name = self.entries[idx].path.file_name()
                .map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            println!("    - {} ({} lines): \"{}\"",
                name, self.entries[idx].lines,
                &self.entries[idx].purpose[..self.entries[idx].purpose.len().min(50)]);
        }
        if self.orphans.len() > 5 {
            println!("    ... and {} more", self.orphans.len() - 5);
        }
        println!();

        // Stubs
        println!("  STUBS         : {} (minimal logic, printline only)", self.stubs.len());
        println!();

        println!("  Analysis time : {} ms", self.encode_time_ms);
        println!("════════════════════════════════════════════════════════════════");
    }

    /// Print detailed cluster analysis as connectable wires
    pub fn print_wiring_opportunities(&self) {
        println!();
        println!("┌─────────────────────────────────────────────────────┐");
        println!("│         WIRING OPPORTUNITIES                        │");
        println!("├─────────────────────────────────────────────────────┤");

        for (i, cluster) in self.clusters.iter().enumerate().take(10) {
            let has_logic = cluster.iter().any(|&idx| self.entries[idx].class == FileClass::Logic);
            let has_stubs = cluster.iter().any(|&idx| self.entries[idx].class == FileClass::Stub);

            if has_logic && has_stubs {
                println!("│  Cluster #{}: WIREABLE — {} stubs can connect to logic │", i + 1, 
                    cluster.iter().filter(|&&idx| self.entries[idx].class == FileClass::Stub).count());
                for &idx in cluster.iter().take(3) {
                    let name = self.entries[idx].path.file_name()
                        .map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    println!("│    {:?}: {:<38}│", self.entries[idx].class, name);
                }
            }
        }

        println!("└─────────────────────────────────────────────────────┘");
    }
}

// ════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ════════════════════════════════════════════════════════════════

fn is_source_file(name: &str) -> bool {
    name.ends_with(".rs") || name.ends_with(".py") || name.ends_with(".glx")
        || name.ends_with(".toml") || name.ends_with(".ptx")
}

/// Extract the purpose/intent from first comment or docstring
fn extract_purpose(content: &str) -> String {
    let mut purpose = String::new();

    for line in content.lines().take(15) {
        let trimmed = line.trim();
        // Rust doc comments
        if trimmed.starts_with("//!") || trimmed.starts_with("///") {
            let text = trimmed.trim_start_matches("//!").trim_start_matches("///").trim();
            if !text.is_empty() {
                if !purpose.is_empty() { purpose.push(' '); }
                purpose.push_str(text);
            }
        }
        // Rust line comments
        else if trimmed.starts_with("//") {
            let text = trimmed.trim_start_matches("//").trim();
            if !text.is_empty() && text.len() > 5 {
                if !purpose.is_empty() { purpose.push(' '); }
                purpose.push_str(text);
            }
        }
        // Python docstrings
        else if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            let text = trimmed.trim_matches('"').trim_matches('\'').trim();
            if !text.is_empty() {
                if !purpose.is_empty() { purpose.push(' '); }
                purpose.push_str(text);
            }
        }
        // Python comments
        else if trimmed.starts_with('#') {
            let text = trimmed.trim_start_matches('#').trim();
            if !text.is_empty() && text.len() > 5 {
                if !purpose.is_empty() { purpose.push(' '); }
                purpose.push_str(text);
            }
        }

        if purpose.len() > 200 { break; }
    }

    if purpose.is_empty() {
        // Fallback: use filename
        "unknown purpose".to_string()
    } else {
        purpose
    }
}

/// Classify a file based on content analysis
fn classify_file(content: &str, lines: usize) -> FileClass {
    // Data/config files
    if content.starts_with('[') || content.starts_with('{') {
        return FileClass::Data;
    }

    let has_println_only = content.contains("println!") || content.contains("print(");
    let has_imports = content.contains("use ") || content.contains("import ");
    let has_real_logic = content.contains("fn ") && content.contains("return ")
        || content.contains("def ") && (content.contains("return ") || content.contains("yield "));
    let has_structs = content.contains("struct ") || content.contains("class ");
    let has_external_calls = content.contains("std::fs::") || content.contains("cudarc::")
        || content.contains("subprocess") || content.contains("firebase");

    // Wired = connects to external systems
    if has_external_calls && has_real_logic {
        return FileClass::Wired;
    }

    // Logic = has real computation
    if has_real_logic && lines > 30 {
        return FileClass::Logic;
    }

    // Stub = small, mostly println
    if lines < 50 && has_println_only && !has_real_logic {
        return FileClass::Stub;
    }

    // Default based on size
    if lines > 100 {
        FileClass::Logic
    } else {
        FileClass::Stub
    }
}

/// Encode a text string into a 57D vector using the GeometricTokenizer pattern
/// (same algorithm as BrainScar_Bridge.py and archive.rs)
fn encode_text_to_57d(text: &str) -> [f64; 57] {
    let mut vector = [0.0f64; 57];
    let bytes = text.as_bytes();
    if bytes.is_empty() { return vector; }

    let phi: f64 = 1.618033988749895;
    let anchor: f64 = SOVEREIGN_ANCHOR as f64;

    for (i, &byte) in bytes.iter().enumerate() {
        let seed = byte as f64 / 255.0;
        let dim = i % 57;
        let angle = seed * std::f64::consts::PI * 2.0 * phi * (dim as f64 + 1.0);
        vector[dim] += angle.sin() * anchor;
        vector[(dim + 7) % 57] += angle.cos() * phi;
        vector[(dim + 13) % 57] += seed * anchor * phi;
    }

    // Normalize to unit hypersphere
    let norm: f64 = vector.iter().map(|v| v * v).sum::<f64>().sqrt();
    if norm > 1e-10 {
        for v in &mut vector {
            *v /= norm;
        }
    }

    vector
}

/// Compute signature hash from vector
fn compute_signature(vector: &[f64; 57]) -> u64 {
    let mut hash: u64 = 0xCAFE_BABE_DEAD_BEEF;
    for (i, &v) in vector.iter().enumerate() {
        let bits = v.to_bits();
        hash ^= bits.wrapping_mul(0x517cc1b727220a95);
        hash = hash.rotate_left((i as u32) % 64);
    }
    hash
}

/// Euclidean distance between two 57D vectors
fn euclidean_distance(a: &[f64; 57], b: &[f64; 57]) -> f64 {
    let mut sum = 0.0f64;
    for i in 0..57 {
        let diff = a[i] - b[i];
        sum += diff * diff;
    }
    sum.sqrt()
}
