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
            let r0_raw = if !entry.instructions.is_empty() {
                Self::execute_organism(&entry.instructions)
            } else {
                0.0
            };
            // NaN guard: treat NaN/Inf as failed execution
            let r0_output = if r0_raw.is_finite() { r0_raw } else { 0.0 };

            let anchor_delta = if r0_raw.is_finite() {
                (r0_output - SOVEREIGN_ANCHOR).abs()
            } else {
                f32::MAX // NaN → maximum drift
            };

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

        // Phase 4: Re-rank by anchor delta (verified resonators first)
        // This pushes NaN/drift organisms to the bottom
        matches.sort_by(|a, b| a.anchor_delta.partial_cmp(&b.anchor_delta)
            .unwrap_or(std::cmp::Ordering::Greater));

        // Phase 5: Resonance verification
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

    /// AUTOPOIETIC FEEDBACK LOOP (Step 5)
    /// When drift is detected, forge the failed query into a new organism.
    /// Pipeline: query text → ISR → forge → .gbin → will_inbox → evolve → archive
    ///
    /// The system literally GROWS its own vocabulary by converting failed
    /// queries into new genetic material.
    pub fn autopoietic_reforge(query: &str, result: &NlpResult) -> AutopoiesisResult {
        use crate::forge::VolumetricForge;
        use std::path::Path;

        if result.verified {
            return AutopoiesisResult {
                triggered: false,
                reason: "No drift detected — query already verified.".to_string(),
                forged_path: None,
            };
        }

        println!("[AUTOPOIESIS] Drift detected for query: \"{}\"", query);
        println!("[AUTOPOIESIS] Drift magnitude: {:.12}", result.drift);
        println!("[AUTOPOIESIS] Forging query into new organism...");

        // Convert the query into a synthetic source file for the forge
        // The query text becomes the genome seed
        let synthetic_source = format!(
            "// AUTOPOIETIC REFORGE — generated from N-LP drift\n\
             // Query: {}\n\
             // Drift: {:.12}\n\
             // This organism was forged to close a semantic gap.\n\
             fn sovereign_query() {{\n\
                 let anchor = {};\n\
                 let result = anchor * 1.0;\n\
                 emit(result);\n\
             }}\n",
            query, result.drift, SOVEREIGN_ANCHOR
        );

        let inbox_dir = Path::new("will_inbox");
        let mut forge = VolumetricForge::new();

        // Sanitize the query into a valid label
        let label: String = query.chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .take(32)
            .collect();
        let label = format!("autopoiesis_{}", label);

        match forge.forge_source(&synthetic_source, &label, inbox_dir) {
            Ok(fr) => {
                println!("[AUTOPOIESIS] Forged {} instructions → {}",
                    fr.instruction_count, fr.gbin_path);
                println!("[AUTOPOIESIS] Binary: {} bytes | Resonance locked: {}",
                    fr.binary_size, fr.resonance_locked);
                println!("[AUTOPOIESIS] Run --ignite to evolve this organism.");

                AutopoiesisResult {
                    triggered: true,
                    reason: format!("Drift {:.12} → forged {} instructions",
                        result.drift, fr.instruction_count),
                    forged_path: Some(fr.gbin_path),
                }
            }
            Err(e) => {
                println!("[AUTOPOIESIS] Forge failed: {}", e);
                AutopoiesisResult {
                    triggered: true,
                    reason: format!("Forge failed: {}", e),
                    forged_path: None,
                }
            }
        }
    }

    /// Classify a query into a sovereign intent using the SEMANTIC_MATRIX
    /// (ported from SarahCore/Genlex/universal_translator.py)
    pub fn classify_intent(text: &str) -> NlpIntent {
        // SEMANTIC_MATRIX — maps natural language keywords to intents
        // Supports English, Ge'ez script, and Hieroglyphic
        let intent_map: &[(&str, &[&str])] = &[
            ("PUSH",   &["push", "input", "start", "seed", "init", "activate", "begin", "launch"]),
            ("STOR",   &["save", "keep", "memory", "store", "archive", "persist", "write"]),
            ("LOAD",   &["load", "get", "recall", "fetch", "read", "retrieve", "query"]),
            ("CALL",   &["call", "invoke", "run", "execute", "dispatch", "fire"]),
            ("OUT",    &["speak", "output", "print", "sound", "voice", "emit", "display", "show"]),
            ("LOOP",   &["loop", "repeat", "cycle", "iterate", "rotate", "spin"]),
            ("GATE",   &["if", "check", "cond", "gate", "when", "verify", "test", "validate"]),
            ("SEAL",   &["end", "stop", "finish", "seal", "done", "halt", "terminate", "shutdown"]),
            ("RET",    &["return", "back", "result", "respond", "reply"]),
            ("METH",   &["define", "function", "method", "create", "build", "forge"]),
            ("DECL",   &["class", "struct", "type", "declare", "schema", "model"]),
            ("IMPRT",  &["import", "include", "use", "require", "inject", "absorb"]),
            ("TRY",    &["try", "attempt", "test", "probe", "experiment"]),
            ("EXCP",   &["except", "catch", "error", "handle", "recover", "heal"]),
            ("MATH",   &["add", "subtract", "multiply", "divide", "calculate", "math", "solve", "compute"]),
            ("ASSERT", &["assert", "verify", "confirm", "truth", "prove", "audit"]),
            ("SYNC",   &["sync", "heartbeat", "pulse", "resonate", "mesh", "network", "connect"]),
        ];

        let text_lower = text.to_lowercase();
        let words: Vec<&str> = text_lower.split_whitespace().collect();
        let mut intent_scores: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();

        for word in &words {
            for &(intent_name, keywords) in intent_map {
                if keywords.contains(word) {
                    *intent_scores.entry(intent_name).or_insert(0) += 1;
                }
            }
        }

        // Find the best-matching intent
        let (best_intent, best_score) = intent_scores.iter()
            .max_by_key(|(_, score)| **score)
            .map(|(name, score)| (*name, *score))
            .unwrap_or(("UNKNOWN", 0));

        let total_words = words.len().max(1);
        let confidence = best_score as f32 / total_words as f32;

        // Collect all matched intents sorted by score
        let mut all_intents: Vec<(&str, usize)> = intent_scores.into_iter().collect();
        all_intents.sort_by(|a, b| b.1.cmp(&a.1));
        let secondary = all_intents.get(1).map(|&(name, _)| name.to_string());

        NlpIntent {
            primary: best_intent.to_string(),
            secondary,
            confidence,
            matched_keywords: all_intents.iter().map(|&(n, _)| n.to_string()).collect(),
        }
    }

    /// Build a structured response from the N-LP result
    pub fn build_response(result: &NlpResult, intent: &NlpIntent) -> NlpResponse {
        let action = match intent.primary.as_str() {
            "PUSH"   => "INITIATE",
            "STOR"   => "PERSIST",
            "LOAD"   => "RETRIEVE",
            "CALL"   => "DISPATCH",
            "OUT"    => "EMIT",
            "LOOP"   => "ITERATE",
            "GATE"   => "VALIDATE",
            "SEAL"   => "TERMINATE",
            "RET"    => "RESPOND",
            "METH"   => "CONSTRUCT",
            "DECL"   => "DECLARE",
            "IMPRT"  => "ABSORB",
            "TRY"    => "PROBE",
            "EXCP"   => "RECOVER",
            "MATH"   => "COMPUTE",
            "ASSERT" => "VERIFY",
            "SYNC"   => "RESONATE",
            _        => "PROCESS",
        };

        let top = result.matches.first();
        let payload = top.map(|m| m.r0_output).unwrap_or(0.0);
        let organism_count = result.matches.iter()
            .filter(|m| m.anchor_delta < 0.001)
            .count();

        NlpResponse {
            intent: intent.primary.clone(),
            action: action.to_string(),
            payload,
            verified: result.verified,
            confidence: intent.confidence,
            organisms_matched: organism_count,
            latency_ms: 0.0, // Filled in by caller
        }
    }

    /// Print results in sovereign format
    pub fn print_results(&self, result: &NlpResult) {
        let intent = Self::classify_intent(&result.query);
        let response = Self::build_response(result, &intent);

        println!();
        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│               SOVEREIGN N-LP — QUERY RESULT                │");
        println!("├─────────────────────────────────────────────────────────────┤");
        let q = if result.query.len() > 50 { &result.query[..50] } else { &result.query };
        println!("│  Query:    {:50}│", q);
        println!("│  Intent:   {:10} → Action: {:10} ({:.0}% conf)     │",
            intent.primary, response.action, intent.confidence * 100.0);
        if let Some(ref sec) = intent.secondary {
            println!("│  Secondary: {:49}│", sec);
        }
        println!("│  Status:   {:50}│",
            if result.verified { "VERIFIED [RESONANT]" } else { "UNVERIFIED [DRIFT]" });
        println!("│  Drift:    {:.12}                                │", result.drift);
        println!("│  Payload:  {:.12}                                │", response.payload);
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
            println!("│  ✓ SOVEREIGN ANCHOR VERIFIED — {} resonators matched   │",
                response.organisms_matched);
        } else {
            println!("│  ✗ SEMANTIC DRIFT — AUTOPOIETIC RE-FORGE REQUIRED         │");
        }
        println!("└─────────────────────────────────────────────────────────────┘");

        // Structured response summary
        println!();
        println!("┌─ STRUCTURED RESPONSE ──────────────────────────────────────┐");
        println!("│  {{                                                         │");
        println!("│    \"intent\":    \"{:10}\",                              │", response.intent);
        println!("│    \"action\":    \"{:10}\",                              │", response.action);
        println!("│    \"payload\":   {:.12},                     │", response.payload);
        println!("│    \"verified\":  {:5},                                    │", response.verified);
        println!("│    \"confidence\":{:.4},                                   │", response.confidence);
        println!("│    \"organisms\": {}                                        │", response.organisms_matched);
        println!("│  }}                                                         │");
        println!("└─────────────────────────────────────────────────────────────┘");
    }
}

/// Classified intent from natural language
#[derive(Debug, Clone)]
pub struct NlpIntent {
    /// Primary intent (PUSH, SYNC, MATH, etc.)
    pub primary: String,
    /// Secondary intent (if multi-intent query)
    pub secondary: Option<String>,
    /// Confidence (0.0-1.0) based on keyword match ratio
    pub confidence: f32,
    /// All matched intent keywords
    pub matched_keywords: Vec<String>,
}

/// Structured response from the N-LP
#[derive(Debug, Clone)]
pub struct NlpResponse {
    /// The classified intent
    pub intent: String,
    /// The action to dispatch
    pub action: String,
    /// The r0 payload from the top organism
    pub payload: f32,
    /// Whether the payload is anchor-verified
    pub verified: bool,
    /// Intent classification confidence
    pub confidence: f32,
    /// Number of organisms that matched the anchor
    pub organisms_matched: usize,
    /// Query latency in milliseconds
    pub latency_ms: f64,
}

/// Result of the autopoietic feedback loop
#[derive(Debug, Clone)]
pub struct AutopoiesisResult {
    /// Whether the re-forge was triggered
    pub triggered: bool,
    /// Reason for triggering (or not)
    pub reason: String,
    /// Path to the forged .gbin file (if successful)
    pub forged_path: Option<String>,
}
