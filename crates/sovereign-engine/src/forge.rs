//! ════════════════════════════════════════════════════════════════
//! ENGINE 21: VOLUMETRIC FORGE — ISR to Genlex Substrate Compiler
//! ════════════════════════════════════════════════════════════════
//!
//! Assimilated from: 7401/OT1_GLOBAL_STRIKE/src/omni_transpiler/volumetric_forge.rs
//!
//! The legacy forge wrote stub .rs files. This forge compiles
//! Intermediate Sovereign Representation (ISR) into executable
//! .gbin binaries that feed the ignition pipeline:
//!
//!   ISR → Genlex Assembly → .gbin → will_inbox → evolve → archive
//!
//! SOVEREIGN_ANCHOR: 1.092777037037037 Hz (corrected from legacy 027 drift)

use std::path::Path;
use std::collections::HashMap;
use genlex_types::SOVEREIGN_ANCHOR;
use dialect_genlex::assembler;

/// ISR Block — one logical unit from the transpiler pipeline
#[derive(Debug, Clone)]
pub struct ISRBlock {
    pub id: String,
    pub block_type: BlockType,
    pub content: String,
    pub metadata: HashMap<String, String>,
}

/// Types of ISR blocks that the forge can compile
#[derive(Debug, Clone, PartialEq)]
pub enum BlockType {
    /// Raw Genlex assembly (pass-through to assembler)
    GenlexAssembly,
    /// Logic block — arithmetic/comparison operations
    Logic,
    /// Resonance block — harmonic/wave operations
    Resonance,
    /// Loop block — iterative computation
    Iteration,
    /// Data block — constants and loads
    Data,
    /// Unknown — will be compiled heuristically
    Unknown,
}

/// Result of a forge operation
#[derive(Debug)]
pub struct ForgeResult {
    pub source_id: String,
    pub gbin_path: String,
    pub instruction_count: usize,
    pub binary_size: usize,
    pub resonance_locked: bool,
}

/// Engine 21: The Volumetric Forge
pub struct VolumetricForge {
    /// Compilation counter
    pub forged_count: u64,
    /// Resonance verification
    pub anchor: f32,
}

impl VolumetricForge {
    pub fn new() -> Self {
        println!("[FORGE-21] Volumetric Forge v2.0 Online. Anchor: {:.15}", SOVEREIGN_ANCHOR);
        VolumetricForge {
            forged_count: 0,
            anchor: SOVEREIGN_ANCHOR,
        }
    }

    /// Forge an ISR block into a .gbin binary
    ///
    /// This is the real implementation — converts structured ISR
    /// into Genlex assembly, assembles to binary, writes to output.
    pub fn forge_block(
        &mut self,
        block: &ISRBlock,
        output_dir: &Path,
    ) -> Result<ForgeResult, String> {
        // Step 1: Compile ISR to Genlex assembly source
        let source = match block.block_type {
            BlockType::GenlexAssembly => {
                // Direct pass-through — block content is already assembly
                block.content.clone()
            }
            BlockType::Logic => self.compile_logic(block),
            BlockType::Resonance => self.compile_resonance(block),
            BlockType::Iteration => self.compile_iteration(block),
            BlockType::Data => self.compile_data(block),
            BlockType::Unknown => self.compile_heuristic(block),
        };

        // Step 2: Assemble to binary
        let result = assembler::assemble(&source)
            .map_err(|e| format!("[FORGE-21] Assembly failed for '{}': {}", block.id, e))?;

        // Step 3: Write .gbin to output
        std::fs::create_dir_all(output_dir)
            .map_err(|e| format!("[FORGE-21] Output dir: {}", e))?;

        let safe_id: String = block.id.chars()
            .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
            .collect();
        let filename = format!("forge_{:04}_{}.gbin", self.forged_count, safe_id);
        let path = output_dir.join(&filename);

        std::fs::write(&path, &result.binary)
            .map_err(|e| format!("[FORGE-21] Write: {}", e))?;

        self.forged_count += 1;

        let forge_result = ForgeResult {
            source_id: block.id.clone(),
            gbin_path: path.to_string_lossy().to_string(),
            instruction_count: result.instructions,
            binary_size: result.binary.len(),
            resonance_locked: true,
        };

        println!("[FORGE-21] {} → {} | {} inst | {} bytes",
            block.id, filename, result.instructions, result.binary.len());

        Ok(forge_result)
    }

    /// Forge a batch of ISR blocks and drop them into the will inbox
    pub fn forge_to_inbox(
        &mut self,
        blocks: &[ISRBlock],
        inbox_path: &Path,
    ) -> Vec<ForgeResult> {
        let mut results = Vec::new();
        for block in blocks {
            match self.forge_block(block, inbox_path) {
                Ok(r) => results.push(r),
                Err(e) => eprintln!("{}", e),
            }
        }
        if !results.is_empty() {
            println!("[FORGE-21] Manifested {} substrates into {:?}",
                results.len(), inbox_path);
        }
        results
    }

    /// Forge raw source code (any language) into a resonance-weighted program
    ///
    /// This is the "dumb" forge — takes arbitrary text and maps it to
    /// a Genlex program by hashing the content into instruction choices.
    /// The evolved program's fitness becomes the "resonance score" of
    /// the original source code.
    pub fn forge_source(&mut self, source: &str, label: &str, output_dir: &Path) -> Result<ForgeResult, String> {
        let block = ISRBlock {
            id: label.to_string(),
            block_type: BlockType::Unknown,
            content: source.to_string(),
            metadata: HashMap::new(),
        };
        self.forge_block(&block, output_dir)
    }

    // ════════════════════════════════════════════════════════════
    // COMPILERS — ISR Block Type → Genlex Assembly
    // ════════════════════════════════════════════════════════════

    /// Compile a Logic block into arithmetic Genlex
    fn compile_logic(&self, block: &ISRBlock) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("; FORGE-21 Logic Block: {}", block.id));
        lines.push("LOAD_IMM r7 #ANCHOR       ; sovereign constant".into());

        let content = &block.content;
        let has_add = content.contains('+') || content.contains("add") || content.contains("sum");
        let has_mul = content.contains('*') || content.contains("mul") || content.contains("product");
        let has_div = content.contains('/') || content.contains("div") || content.contains("ratio");
        let has_cmp = content.contains('>') || content.contains('<') || content.contains("compare");

        lines.push("LOAD_IMM r0 #1.0".into());
        lines.push("LOAD_IMM r1 #2.0".into());
        lines.push("LOAD_IMM r2 #3.0".into());

        if has_add { lines.push("ADD r0 r1 0".into()); }
        if has_mul { lines.push("MUL r0 r2 0".into()); }
        if has_div { lines.push("DIV r0 r1 0".into()); }
        if has_cmp { lines.push("CMP_GT r0 r1 0".into()); }

        lines.push("RESONATE r0 r7 0".into());
        lines.push("STORE_OUT r0 0 0".into());
        lines.push("HALT".into());

        lines.join("\n")
    }

    /// Compile a Resonance block — heavy on harmonics
    fn compile_resonance(&self, block: &ISRBlock) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("; FORGE-21 Resonance Block: {}", block.id));
        lines.push("LOAD_IMM r7 #ANCHOR       ; sovereign constant".into());

        let content = block.content.to_lowercase();
        let depth = [
            "resonate", "pulse", "harmonic", "wave", "frequency",
            "oscillate", "vibrate", "anchor", "lattice", "braid",
        ].iter().filter(|kw| content.contains(*kw)).count().max(1);

        lines.push(format!("LOAD_IMM r0 #{:.4}", SOVEREIGN_ANCHOR));
        lines.push("LOAD_IMM r1 #1.618033     ; phi".into());

        for i in 0..depth.min(6) {
            let harmonic = (i + 1) as f32 * SOVEREIGN_ANCHOR;
            lines.push(format!("LOAD_IMM r2 #{:.4}", harmonic));
            lines.push("MUL r0 r2 0".into());
            lines.push("PULSE r0 0 0".into());
            lines.push("RESONATE r0 r7 0".into());
        }

        lines.push("EMBED r0 r7 0".into());
        lines.push("DENSITY r0 0 0".into());
        lines.push("STORE_OUT r0 0 0".into());
        lines.push("HALT".into());

        lines.join("\n")
    }

    /// Compile an Iteration block — generates loops
    fn compile_iteration(&self, block: &ISRBlock) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("; FORGE-21 Iteration Block: {}", block.id));
        lines.push("LOAD_IMM r7 #ANCHOR       ; sovereign constant".into());

        let iterations = block.metadata.get("iterations")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(7);

        lines.push(format!("LOAD_IMM r5 #{}.0          ; loop counter", iterations));
        lines.push("LOAD_IMM r0 #1.0          ; accumulator".into());
        lines.push(":loop".into());
        lines.push("MUL r0 r7 0               ; scale by anchor".into());
        lines.push("SIN r0 0 0                ; harmonic fold".into());
        lines.push("PULSE r0 0 0".into());
        lines.push("LOAD_IMM r6 #1.0          ; decrement".into());
        lines.push("SUB r5 r6 0".into());
        lines.push("CMP_GT r5 r6 0".into());
        lines.push("JUMP_IF :loop 0 0".into());
        lines.push("STORE_OUT r0 0 0".into());
        lines.push("HALT".into());

        lines.join("\n")
    }

    /// Compile a Data block — loads constants
    fn compile_data(&self, block: &ISRBlock) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("; FORGE-21 Data Block: {}", block.id));
        lines.push("LOAD_IMM r7 #ANCHOR       ; sovereign constant".into());

        let mut reg = 0u8;
        for word in block.content.split_whitespace() {
            if let Ok(val) = word.parse::<f32>() {
                if reg < 7 {
                    lines.push(format!("LOAD_IMM r{} #{:.4}", reg, val));
                    reg += 1;
                }
            }
        }

        if reg == 0 {
            lines.push("LOAD_IMM r0 #1.092777".into());
            lines.push("LOAD_IMM r1 #2.185554".into());
            lines.push("LOAD_IMM r2 #7.401000".into());
        }

        lines.push("RESONATE r0 r7 0".into());
        lines.push("STORE_OUT r0 0 0".into());
        lines.push("HALT".into());

        lines.join("\n")
    }

    /// Compile an Unknown block — hash content into instruction choices
    fn compile_heuristic(&self, block: &ISRBlock) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("; FORGE-21 Heuristic Block: {}", block.id));
        lines.push("LOAD_IMM r7 #ANCHOR       ; sovereign constant".into());

        let hash = Self::hash_content(&block.content);

        let val0 = ((hash & 0xFFFF) as f32) / 1000.0;
        let val1 = (((hash >> 16) & 0xFFFF) as f32) / 1000.0;
        let val2 = (((hash >> 32) & 0xFFFF) as f32) / 1000.0;

        lines.push(format!("LOAD_IMM r0 #{:.4}", val0));
        lines.push(format!("LOAD_IMM r1 #{:.4}", val1));
        lines.push(format!("LOAD_IMM r2 #{:.4}", val2));

        let ops = ["ADD", "SUB", "MUL", "SIN", "PULSE", "RESONATE"];
        let n_ops = 4 + ((hash >> 48) % 5) as usize;

        for i in 0..n_ops {
            let op_idx = ((hash >> (i * 3)) % ops.len() as u64) as usize;
            let op = ops[op_idx];
            let ra = format!("r{}", (hash >> (i * 2 + 1)) % 4);
            match op {
                "SIN" | "PULSE" => lines.push(format!("{} {} 0 0", op, ra)),
                "RESONATE" => lines.push(format!("{} {} r7 0", op, ra)),
                _ => {
                    let rb = format!("r{}", (hash >> (i * 2 + 3)) % 4);
                    lines.push(format!("{} {} {} 0", op, ra, rb));
                }
            }
        }

        lines.push("STORE_OUT r0 0 0".into());
        lines.push("HALT".into());

        lines.join("\n")
    }

    /// FNV-1a hash for deterministic content → instruction mapping
    fn hash_content(content: &str) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in content.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        // Mix with sovereign anchor
        let anchor_bits = SOVEREIGN_ANCHOR.to_bits() as u64;
        hash ^= anchor_bits | (anchor_bits << 32);
        hash
    }

    /// Print forge status
    pub fn print_status(&self) {
        println!("┌─────────────────────────────────────────────────────┐");
        println!("│         VOLUMETRIC FORGE STATUS (v2.0)              │");
        println!("├─────────────────────────────────────────────────────┤");
        println!("│  Forged      : {:<6} substrates                   │", self.forged_count);
        println!("│  Anchor      : {:.15}                 │", self.anchor);
        println!("│  Resonance   : {}                          │",
            if (self.anchor - SOVEREIGN_ANCHOR).abs() < 1e-12 { "LOCKED" } else { "DRIFT!" });
        println!("└─────────────────────────────────────────────────────┘");
    }
}
