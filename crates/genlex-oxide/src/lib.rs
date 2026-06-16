//! Genesis Oxide - Glyph VM & Kernel Compiler (Tier A)
//! Auto-generated. DO NOT EDIT.

use genlex_types::{GlyphInst, GlyphOp, GbinHeader, SOVEREIGN_ANCHOR, LATTICE_DIMS};

/// A compiled Genlex program
pub struct GlyphProgram {
    pub instructions: Vec<GlyphInst>,
    pub header: GbinHeader,
    pub registers: [f32; 16],
}

impl GlyphProgram {
    /// Load from .gbin bytes
    pub fn from_gbin(data: &[u8]) -> Result<Self, String> {
        if data.len() < 16 {
            return Err("File too small for GBIN header".into());
        }
        let header = unsafe {
            std::ptr::read_unaligned(data.as_ptr() as *const GbinHeader)
        };
        if !header.is_valid() {
            return Err(format!("Invalid GBIN: magic={:?} v={}", header.magic, header.version));
        }
        let payload = &data[16..data.len().saturating_sub(32)];
        let mut instructions = Vec::with_capacity(header.num_instructions as usize);
        let mut i = 0;
        while i + 3 < payload.len() {
            instructions.push(GlyphInst::from_bytes([
                payload[i], payload[i+1], payload[i+2], payload[i+3]
            ]));
            i += 4;
        }
        Ok(GlyphProgram { instructions, header, registers: [0.0_f32; 16] })
    }

    /// Execute on CPU (with cycle limit to prevent infinite loops)
    pub fn execute_cpu(&mut self) -> f32 {
        let mut pc: usize = 0;
        let mut flag: bool = false;
        let mut cycles: u32 = 0;
        const MAX_CYCLES: u32 = 10_000;

        while pc < self.instructions.len() {
            cycles += 1;
            if cycles > MAX_CYCLES { break; }

            let inst = self.instructions[pc];
            let a = (inst.reg_a as usize) & 0x0F;  // clamp to 0-15
            let b = (inst.reg_b as usize) & 0x0F;

            match GlyphOp::decode(inst.opcode) {
                Some(GlyphOp::Add) => { self.registers[a] += self.registers[b]; }
                Some(GlyphOp::Sub) => { self.registers[a] -= self.registers[b]; }
                Some(GlyphOp::Mul) => { self.registers[a] *= self.registers[b]; }
                Some(GlyphOp::Div) => {
                    if self.registers[b] != 0.0 { self.registers[a] /= self.registers[b]; }
                }
                Some(GlyphOp::Sqrt) => { self.registers[a] = self.registers[a].sqrt(); }
                Some(GlyphOp::Sin) => { self.registers[a] = self.registers[a].sin(); }
                Some(GlyphOp::Pulse) => { self.registers[a] *= SOVEREIGN_ANCHOR; }
                Some(GlyphOp::Mov) => { self.registers[a] = self.registers[b]; }
                Some(GlyphOp::CmpGt) => { flag = self.registers[a] > self.registers[b]; }
                Some(GlyphOp::CmpEq) => { flag = self.registers[a] == self.registers[b]; }
                Some(GlyphOp::Jump) => { pc = a; continue; }
                Some(GlyphOp::JumpIf) => { if flag { pc = a; continue; } }
                Some(GlyphOp::LoadConst) => { self.registers[a] = inst.reg_b as f32 * 0.01; }
                Some(GlyphOp::LoadImm) => {
                    if pc + 1 < self.instructions.len() {
                        let next = self.instructions[pc + 1];
                        let val = f32::from_le_bytes(next.to_bytes());
                        self.registers[a] = val;
                        pc += 1;
                    }
                }
                Some(GlyphOp::LoadMem) => { self.registers[a] = self.registers[b]; }
                Some(GlyphOp::StoreMem) => { self.registers[b] = self.registers[a]; }
                Some(GlyphOp::Resonate) => {
                    let mag: f32 = (0..16).map(|i| self.registers[i] * self.registers[i])
                        .sum::<f32>().sqrt();
                    self.registers[a] = mag * SOVEREIGN_ANCHOR;
                }
                Some(GlyphOp::Embed) => {
                    let val = self.registers[a];
                    for d in 0..std::cmp::min(16, LATTICE_DIMS) {
                        self.registers[d] =
                            ((val * (d as f32 + 1.0) * SOVEREIGN_ANCHOR).sin()) * 0.5 + 0.5;
                    }
                }
                Some(GlyphOp::ThreadId) => { self.registers[a] = 0.0; }
                Some(GlyphOp::StoreOut) => { /* GPU output buffer */ }
                Some(GlyphOp::Density) => {
                    let sum: f32 = (0..16).map(|i| self.registers[i].abs()).sum();
                    self.registers[a] = sum / 16.0;
                }
                Some(GlyphOp::Halt) => break,
                Some(GlyphOp::Nop) | None => {}
            }
            pc += 1;
        }
        self.registers[0]
    }

    /// Register file access
    pub fn registers(&self) -> &[f32; 16] { &self.registers }
}

/// GPU kernel: resonance search (N memories x DIMS dimensions)
pub fn resonance_search_kernel(
    memories: &[f32], query: &[f32], scores: &mut [f32], n: usize, dims: usize,
) {
    for i in 0..n {
        let mut dot = 0.0_f32;
        let mut mag_m = 0.0_f32;
        let mut mag_q = 0.0_f32;
        for d in 0..dims {
            let m = memories[i * dims + d];
            let q = query[d];
            dot += m * q;
            mag_m += m * m;
            mag_q += q * q;
        }
        let denom = mag_m.sqrt() * mag_q.sqrt();
        scores[i] = if denom > 0.0 { (dot / denom) * SOVEREIGN_ANCHOR } else { 0.0 };
    }
}

/// GPU kernel: 57D fractal lattice embedding
pub fn lattice_embed_kernel(input: &[u8], output: &mut [f32], dims: usize) {
    for d in 0..dims {
        let mut hash = SOVEREIGN_ANCHOR;
        for (i, &byte) in input.iter().enumerate() {
            let x = (byte as f32 + 1.0) * ((d as f32 + 1.0) * 0.618033988);
            hash += x.sin() * (1.0 / (i as f32 + 1.0));
        }
        output[d] = hash.sin() * 0.5 + 0.5;
    }
}
