//! ════════════════════════════════════════════════════════════════
//! SOVEREIGN N-LP — Natural Language Processor
//! ════════════════════════════════════════════════════════════════
//!
//! Not an LLM. A Deterministic Semantic Mapping engine.
//!
//! Pipeline:
//!   1. Natural language → GeometricTokenizer → 57D vector
//!   2. 57D vector → Archive nearest-neighbor search → top-K organisms
//!   3. Top-K organisms → GPU execution → r0 register states
//!   4. r0 states → Resonance verification → deterministic output
//!
//! The system doesn't "predict" the next token. It EXECUTES
//! the nearest verified logic program from the genetic archive.

use crate::archive::{GeneticArchive, ArchiveEntry};
use crate::vault::SovereignVault;
use genlex_oxide::GlyphProgram;
use genlex_types::{SOVEREIGN_ANCHOR, LATTICE_DIMS, GlyphInst};

/// Result of an N-LP query
#[derive(Debug, Clone)]
pub struct NlpResult {
    /// The original query text
    pub query: String,
    /// The 57D embedding of the query
    pub query_vector: [f32; LATTICE_DIMS],
    /// Top-K matched organisms with execution results
    pub matches: Vec<NlpMatch>,
    /// Whether the top result passed resonance verification
    pub verified: bool,
    /// Semantic drift: distance from anchor in the top result
    pub drift: f32,
}

/// A single matched organism and its execution result
#[derive(Debug, Clone)]
pub struct NlpMatch {
    /// Index in the genetic archive
    pub archive_index: usize,
    /// Cosine similarity (resonance) to the query vector
    pub resonance: f32,
    /// The r0 register after execution
    pub r0_output: f32,
    /// Delta from SOVEREIGN_ANCHOR
    pub anchor_delta: f32,
    /// Number of instructions in the organism
    pub instruction_count: usize,
    /// The organism's fitness from evolution
    pub evolved_fitness: f32,
    /// Generation the organism was evolved in
    pub generation: u32,
}

/// The Sovereign N-LP Engine
pub struct SovereignNlp {
    /// Archive vectors flattened: [entry0_dim0, entry0_dim1, ..., entry0_dim56, entry1_dim0, ...]
    archive_vectors: Vec<f32>,
    /// Number of entries in the archive
    num_entries: usize,
    /// Reference to archive entries (instructions + metadata)
    entries: Vec<ArchiveEntry>,
}

impl SovereignNlp {
    /// Initialize from a genetic archive
    pub fn from_archive(ga: &GeneticArchive) -> Self {
        let num_entries = ga.entries.len();
        let mut archive_vectors = Vec::with_capacity(num_entries * LATTICE_DIMS);

        for entry in &ga.entries {
            for d in 0..LATTICE_DIMS {
                archive_vectors.push(entry.vector[d] as f32);
            }
        }

        println!("[N-LP] Initialized with {} organisms in 57D search space", num_entries);

        SovereignNlp {
            archive_vectors,
            num_entries,
            entries: ga.entries.clone(),
        }
    }

    /// Process a natural language query
    pub fn query(&self, text: &str, top_k: usize) -> NlpResult {
        // Phase 1: Geometric Tokenization — text → 57D vector
        let query_vector = SovereignVault::encode_query(text);

        // Phase 2: Archive search — find nearest organisms in 57D space
        //          Score = cosine_similarity × log10(fitness + 1)
        //          This ensures high-fitness resonators rank above weak ones
        let mut scored: Vec<(usize, f32)> = (0..self.num_entries)
            .map(|i| {
                let base = i * LATTICE_DIMS;
                // Cosine similarity (both vectors are normalized)
                let mut dot = 0.0f32;
                let mut mag_a = 0.0f32;
                let mut mag_b = 0.0f32;
                for d in 0..LATTICE_DIMS {
                    let a = self.archive_vectors[base + d];
                    let b = query_vector[d];
                    dot += a * b;
                    mag_a += a * a;
                    mag_b += b * b;
                }
                let denom = (mag_a.sqrt() * mag_b.sqrt()).max(1e-12);
                let cosine = dot / denom;

                // Fitness weighting: log10(fitness + 1) as multiplier
                let fitness = self.entries[i].fitness.max(0.0);
                let fitness_weight = (fitness + 1.0).log10().max(1.0);
                let weighted_score = cosine * fitness_weight;

                (i, weighted_score)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        // Phase 3: Execute top-K organisms on CPU
        let mut matches = Vec::with_capacity(top_k);
        for (idx, resonance) in &scored {
            let entry = &self.entries[*idx];
            let r0_output = if !entry.instructions.is_empty() {
                Self::execute_organism(&entry.instructions)
            } else {
                0.0
            };

            let anchor_delta = (r0_output - SOVEREIGN_ANCHOR).abs();

            matches.push(NlpMatch {
                archive_index: *idx,
                resonance: *resonance,
                r0_output,
                anchor_delta,
                instruction_count: entry.instructions.len(),
                evolved_fitness: entry.fitness,
                generation: entry.generation,
            });
        }

        // Phase 4: Resonance verification
        let verified = matches.first()
            .map(|m| m.anchor_delta < 0.001)
            .unwrap_or(false);
        let drift = matches.first()
            .map(|m| m.anchor_delta)
            .unwrap_or(f32::MAX);

        NlpResult {
            query: text.to_string(),
            query_vector,
            matches,
            verified,
            drift,
        }
    }

    /// Execute a single organism's instruction set, return r0
    fn execute_organism(instructions: &[GlyphInst]) -> f32 {
        // Build a minimal .gbin in memory
        let mut data: Vec<u8> = Vec::with_capacity(16 + instructions.len() * 4 + 32);
        // Header: GBIN magic, version=1, num_instructions, flags=0
        data.extend_from_slice(b"GBIN");
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&(instructions.len() as u32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        // Instructions
        for inst in instructions {
            data.push(inst.opcode);
            data.push(inst.reg_a);
            data.push(inst.reg_b);
            data.push(inst.flags);
        }
        // Footer: 32 bytes of zeros (hash placeholder)
        data.extend_from_slice(&[0u8; 32]);

        match GlyphProgram::from_gbin(&data) {
            Ok(mut prog) => prog.execute_cpu(),
            Err(_) => 0.0,
        }
    }

    /// Print results in sovereign format
    pub fn print_results(&self, result: &NlpResult) {
        println!();
        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│               SOVEREIGN N-LP — QUERY RESULT                │");
        println!("├─────────────────────────────────────────────────────────────┤");
        let q = if result.query.len() > 50 { &result.query[..50] } else { &result.query };
        println!("│  Query:    {:50}│", q);
        println!("│  Status:   {}                                           │",
            if result.verified { "VERIFIED [RESONANT]" } else { "UNVERIFIED [DRIFT]" });
        println!("│  Drift:    {:.12}                                │", result.drift);
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│  {:>4} {:>10} {:>12} {:>10} {:>6} {:>6} │",
            "Rank", "Resonance", "r0 Output", "Δ Anchor", "Insts", "Gen");
        println!("├─────────────────────────────────────────────────────────────┤");

        for (i, m) in result.matches.iter().enumerate() {
            let marker = if m.anchor_delta < 0.001 { "●" } else { "○" };
            println!("│  {:>3}. {:>9.6} {:>12.6} {:>10.8} {:>5} {:>5} {} │",
                i + 1, m.resonance, m.r0_output, m.anchor_delta,
                m.instruction_count, m.generation, marker);
        }

        println!("├─────────────────────────────────────────────────────────────┤");
        if result.verified {
            println!("│  ✓ TOP RESULT PASSED SOVEREIGN ANCHOR VERIFICATION        │");
        } else {
            println!("│  ✗ SEMANTIC DRIFT DETECTED — TOKENIZER RE-ALIGNMENT       │");
            println!("│    NEEDED                                                  │");
        }
        println!("└─────────────────────────────────────────────────────────────┘");
    }
}
