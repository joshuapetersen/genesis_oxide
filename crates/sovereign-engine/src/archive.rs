//! ════════════════════════════════════════════════════════════════
//! GENETIC ARCHIVE — Self-Improving Memory Loop
//! ════════════════════════════════════════════════════════════════
//!
//! Encodes evolved organisms into 57D LatticeNode vectors and
//! serializes them into binary-compatible vault format.
//!
//! The loop:
//!   Evolve → Archive → Search → Seed → Evolve → ...
//!
//! File format: identical to brain_scar_vault.dat
//!   Each entry = 464 bytes: 57 f64 doubles + 1 u64 signature
//!   Compatible with Python BrainScar_Bridge.py LatticeNode

use std::path::{Path, PathBuf};
use std::io::Write;
use genlex_types::{GlyphInst, SOVEREIGN_ANCHOR};

/// Archive constants (matches Python BrainScar_Bridge.py)
const LATTICE_DIMS: usize = 57;
const LATTICE_NODE_SIZE: usize = 464; // 57 f64 + 1 u64 = 58 * 8
const PHI: f64 = 1.618033988749895;
const HEARTBEAT: f64 = SOVEREIGN_ANCHOR as f64;
const SUPER_SYMMETRY: f64 = 1.1;

/// Minimum fitness threshold to qualify for archival
pub const ARCHIVE_THRESHOLD: f32 = 1.0;
/// Maximum archive entries (prevent unbounded growth)
pub const MAX_ARCHIVE_ENTRIES: usize = 101_000;

/// A single archived organism
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    /// 57D vector encoding of the program
    pub vector: [f64; LATTICE_DIMS],
    /// Cryptographic signature (hash of instruction bytes)
    pub signature: u64,
    /// Fitness at time of archival
    pub fitness: f32,
    /// Generation it was evolved in
    pub generation: u32,
    /// Number of directed evolution cycles applied
    pub cycles: u32,
    /// Number of instructions in the program
    pub num_instructions: u32,
    /// The actual program instructions
    pub instructions: Vec<GlyphInst>,
}

/// The Genetic Archive
pub struct GeneticArchive {
    pub archive_path: PathBuf,
    pub meta_path: PathBuf,
    pub instr_path: PathBuf,
    pub entries: Vec<ArchiveEntry>,
}

impl GeneticArchive {
    /// Create or load an archive at the given path
    pub fn open(archive_path: &Path) -> Self {
        let meta_path = archive_path.with_extension("meta.json");
        let instr_path = archive_path.with_extension("instructions");
        let mut archive = GeneticArchive {
            archive_path: archive_path.to_path_buf(),
            meta_path,
            instr_path,
            entries: Vec::new(),
        };

        // Load existing entries if file exists
        if archive_path.exists() {
            match archive.load_entries() {
                Ok(n) => println!("[ARCHIVE] Loaded {} existing entries from {}",
                    n, archive_path.display()),
                Err(e) => eprintln!("[ARCHIVE] Warning: could not load {}: {}",
                    archive_path.display(), e),
            }
        } else {
            println!("[ARCHIVE] New archive: {}", archive_path.display());
        }

        archive
    }

    /// Load existing archive entries from the binary file
    fn load_entries(&mut self) -> Result<usize, String> {
        let data = std::fs::read(&self.archive_path)
            .map_err(|e| format!("Read: {}", e))?;

        let num_entries = data.len() / LATTICE_NODE_SIZE;
        self.entries.clear();

        for i in 0..num_entries {
            let base = i * LATTICE_NODE_SIZE;
            if base + LATTICE_NODE_SIZE > data.len() { break; }

            let mut vector = [0.0f64; LATTICE_DIMS];
            for d in 0..LATTICE_DIMS {
                let offset = base + d * 8;
                vector[d] = f64::from_le_bytes([
                    data[offset], data[offset+1], data[offset+2], data[offset+3],
                    data[offset+4], data[offset+5], data[offset+6], data[offset+7],
                ]);
            }

            let sig_offset = base + LATTICE_DIMS * 8;
            let signature = u64::from_le_bytes([
                data[sig_offset], data[sig_offset+1], data[sig_offset+2], data[sig_offset+3],
                data[sig_offset+4], data[sig_offset+5], data[sig_offset+6], data[sig_offset+7],
            ]);

            self.entries.push(ArchiveEntry {
                vector,
                signature,
                fitness: 0.0, // Will be loaded from metadata if available
                generation: 0,
                cycles: 0,
                num_instructions: 0,
                instructions: Vec::new(),
            });
        }

        // Try to load metadata sidecar
        self.load_metadata();

        // Load instructions sidecar
        self.load_instructions();

        Ok(self.entries.len())
    }

    /// Encode a GlyphInst program into a 57D vector
    ///
    /// Uses the same GeometricTokenizer as BrainScar_Bridge.py:
    /// - Treats instruction bytes as "characters"
    /// - xyz[0..27]: sin(seed + i * PHI + position)
    /// - einstein[27..39]: cos(seed + i * PHI)
    /// - polarity[39..51]: sin(seed * PHI + i)
    /// - phi[51..56]: PHI^(-i)
    /// - architect_anchor[56]: SUPER_SYMMETRY accumulator
    pub fn encode_organism(instructions: &[GlyphInst]) -> [f64; LATTICE_DIMS] {
        let mut bundle = [0.0f64; LATTICE_DIMS];

        // Convert instructions to byte stream
        let bytes: Vec<u8> = instructions.iter()
            .flat_map(|inst| inst.to_bytes())
            .collect();

        for (pos, &byte) in bytes.iter().enumerate() {
            let seed = byte as f64 * HEARTBEAT;

            // xyz[0..27]: sin(seed + i * PHI + pos)
            for i in 0..27 {
                bundle[i] += (seed + i as f64 * PHI + pos as f64).sin();
            }
            // einstein[27..39]: cos(seed + i * PHI)
            for i in 0..12 {
                bundle[27 + i] += (seed + i as f64 * PHI).cos();
            }
            // polarity[39..51]: sin(seed * PHI + i)
            for i in 0..12 {
                bundle[39 + i] += (seed * PHI + i as f64).sin();
            }
            // phi[51..56]: PHI^(-i)
            for i in 0..5 {
                bundle[51 + i] += PHI.powi(-(i as i32));
            }
            // architect_anchor[56]
            bundle[56] += SUPER_SYMMETRY;
        }

        // Normalize onto 57D hypersphere
        let mag: f64 = bundle.iter().map(|x| x * x).sum::<f64>().sqrt();
        if mag > 0.0 {
            for v in bundle.iter_mut() {
                *v /= mag;
            }
        }

        bundle
    }

    /// Compute a signature hash from instruction bytes
    fn compute_signature(instructions: &[GlyphInst]) -> u64 {
        let mut hash: u64 = 0xcafe_babe_dead_beef;
        for inst in instructions {
            let bytes = inst.to_bytes();
            let word = u32::from_le_bytes(bytes) as u64;
            hash = hash.wrapping_mul(6364136223846793005).wrapping_add(word);
            hash ^= hash >> 33;
        }
        hash
    }

    /// Serialize a 57D vector + signature into a 464-byte LatticeNode
    fn write_lattice_node(vector: &[f64; LATTICE_DIMS], signature: u64) -> [u8; LATTICE_NODE_SIZE] {
        let mut node = [0u8; LATTICE_NODE_SIZE];

        // Write 57 f64 doubles
        for (i, &val) in vector.iter().enumerate() {
            let bytes = val.to_le_bytes();
            node[i * 8..i * 8 + 8].copy_from_slice(&bytes);
        }

        // Write u64 signature at offset 456
        let sig_bytes = signature.to_le_bytes();
        node[LATTICE_DIMS * 8..LATTICE_DIMS * 8 + 8].copy_from_slice(&sig_bytes);

        node
    }

    /// Archive a successful organism
    pub fn archive_organism(
        &mut self,
        instructions: &[GlyphInst],
        fitness: f32,
        generation: u32,
        cycles: u32,
        source_name: &str,
    ) -> Result<usize, String> {
        // Check limits
        if self.entries.len() >= MAX_ARCHIVE_ENTRIES {
            return Err(format!("Archive full ({} entries)", MAX_ARCHIVE_ENTRIES));
        }

        // Check fitness threshold
        if fitness < ARCHIVE_THRESHOLD {
            return Err(format!("Fitness {:.4} below threshold {:.4}",
                fitness, ARCHIVE_THRESHOLD));
        }

        // Encode to 57D
        let vector = Self::encode_organism(instructions);
        let signature = Self::compute_signature(instructions);

        // Check for duplicates (same signature = same program)
        if self.entries.iter().any(|e| e.signature == signature) {
            return Err("Duplicate organism (already archived)".into());
        }

        let entry = ArchiveEntry {
            vector,
            signature,
            fitness,
            generation,
            cycles,
            num_instructions: instructions.len() as u32,
            instructions: instructions.to_vec(),
        };

        // Append binary LatticeNode to file
        let node_bytes = Self::write_lattice_node(&vector, signature);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.archive_path)
            .map_err(|e| format!("Open archive: {}", e))?;
        file.write_all(&node_bytes)
            .map_err(|e| format!("Write archive: {}", e))?;

        let index = self.entries.len();
        self.entries.push(entry);

        // Update metadata sidecar
        self.save_metadata(source_name);

        // Save instructions sidecar
        self.save_instructions();

        Ok(index)
    }

    /// Search the archive for the closest match to a program
    /// Returns (index, resonance_score, archived_instructions)
    pub fn find_closest_seed(
        &self,
        instructions: &[GlyphInst],
    ) -> Option<(usize, f64, Vec<GlyphInst>)> {
        if self.entries.is_empty() {
            return None;
        }

        let query = Self::encode_organism(instructions);

        let mut best_idx = 0;
        let mut best_resonance = 0.0f64;

        for (i, entry) in self.entries.iter().enumerate() {
            let mut dist_sq = 0.0f64;
            for d in 0..LATTICE_DIMS {
                let diff = query[d] - entry.vector[d];
                dist_sq += diff * diff;
            }
            let resonance = 1.0 / (1.0 + dist_sq.sqrt());

            if resonance > best_resonance {
                best_resonance = resonance;
                best_idx = i;
            }
        }

        if best_resonance > 0.5 && !self.entries[best_idx].instructions.is_empty() {
            Some((
                best_idx,
                best_resonance,
                self.entries[best_idx].instructions.clone(),
            ))
        } else {
            None
        }
    }

    /// Get all archive vectors as f32 for GPU upload (union with vault)
    pub fn as_f32_vectors(&self) -> Vec<f32> {
        let mut result = Vec::with_capacity(self.entries.len() * LATTICE_DIMS);
        for entry in &self.entries {
            for &val in &entry.vector {
                result.push(val as f32);
            }
        }
        result
    }

    /// Number of archived entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Save metadata sidecar JSON
    fn save_metadata(&self, source_name: &str) {
        let entries_json: Vec<String> = self.entries.iter().enumerate().map(|(i, e)| {
            format!(
                r#"    {{
      "index": {},
      "fitness": {},
      "generation": {},
      "cycles": {},
      "num_instructions": {},
      "signature": "0x{:016X}",
      "source": "{}"
    }}"#,
                i, e.fitness, e.generation, e.cycles,
                e.num_instructions, e.signature, source_name
            )
        }).collect();

        let json = format!("{{\n  \"archive_version\": 1,\n  \"sovereign_anchor\": {},\n  \"total_entries\": {},\n  \"entries\": [\n{}\n  ]\n}}\n",
            SOVEREIGN_ANCHOR,
            self.entries.len(),
            entries_json.join(",\n")
        );

        if let Err(e) = std::fs::write(&self.meta_path, json) {
            eprintln!("[ARCHIVE] Warning: metadata write failed: {}", e);
        }
    }

    /// Load metadata from sidecar (populates fitness/generation/cycles)
    fn load_metadata(&mut self) {
        if !self.meta_path.exists() { return; }

        let content = match std::fs::read_to_string(&self.meta_path) {
            Ok(c) => c,
            Err(_) => return,
        };

        // Simple JSON parsing for fitness, generation, cycles fields
        // (avoiding serde dependency)
        for entry in &mut self.entries {
            let sig_str = format!("0x{:016X}", entry.signature);
            if let Some(pos) = content.find(&sig_str) {
                // Extract fitness
                if let Some(f_start) = content[..pos].rfind("\"fitness\":") {
                    let f_slice = &content[f_start + 10..pos];
                    if let Some(comma) = f_slice.find(',') {
                        if let Ok(f) = f_slice[..comma].trim().parse::<f32>() {
                            entry.fitness = f;
                        }
                    }
                }
            }
        }
    }

    /// Save instruction bytes to sidecar binary
    /// Format: [num_entries: u32] then for each entry:
    ///   [num_instructions: u32] [instruction_bytes: u8 × num_inst × 4]
    fn save_instructions(&self) {
        let mut data = Vec::new();
        data.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());

        for entry in &self.entries {
            data.extend_from_slice(&(entry.instructions.len() as u32).to_le_bytes());
            for inst in &entry.instructions {
                data.extend_from_slice(&inst.to_bytes());
            }
        }

        if let Err(e) = std::fs::write(&self.instr_path, &data) {
            eprintln!("[ARCHIVE] Warning: instruction sidecar write failed: {}", e);
        }
    }

    /// Load instruction bytes from sidecar binary
    fn load_instructions(&mut self) {
        if !self.instr_path.exists() { return; }

        let data = match std::fs::read(&self.instr_path) {
            Ok(d) => d,
            Err(_) => return,
        };

        if data.len() < 4 { return; }
        let num_entries = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let mut offset = 4usize;

        for i in 0..num_entries.min(self.entries.len()) {
            if offset + 4 > data.len() { break; }
            let num_inst = u32::from_le_bytes([
                data[offset], data[offset+1], data[offset+2], data[offset+3]
            ]) as usize;
            offset += 4;

            let mut instructions = Vec::with_capacity(num_inst);
            for _ in 0..num_inst {
                if offset + 4 > data.len() { break; }
                let bytes = [data[offset], data[offset+1], data[offset+2], data[offset+3]];
                instructions.push(GlyphInst::from_bytes(bytes));
                offset += 4;
            }

            self.entries[i].instructions = instructions;
            self.entries[i].num_instructions = num_inst as u32;
        }
    }

    /// Print archive summary
    pub fn print_summary(&self) {
        println!("┌─────────────────────────────────────────────────────┐");
        println!("│              GENETIC ARCHIVE STATUS                 │");
        println!("├─────────────────────────────────────────────────────┤");
        println!("│  Archive : {:<40}│", self.archive_path.display());
        println!("│  Entries : {:<40}│", self.entries.len());
        let with_instr = self.entries.iter().filter(|e| !e.instructions.is_empty()).count();
        println!("│  Seedable: {:<40}│", format!("{}/{} (have instructions)", with_instr, self.entries.len()));
        if let Some(best) = self.entries.iter().max_by(|a, b|
            a.fitness.partial_cmp(&b.fitness).unwrap_or(std::cmp::Ordering::Equal)
        ) {
            println!("│  Best    : fitness={:<20} gen={:<6} │",
                format!("{:.4}", best.fitness), best.generation);
        }
        let total_bytes = self.entries.len() * LATTICE_NODE_SIZE;
        println!("│  Size    : {} bytes ({} nodes × 464)         │",
            format!("{:>7}", total_bytes),
            self.entries.len());
        println!("└─────────────────────────────────────────────────────┘");
    }
}
