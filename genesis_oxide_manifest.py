"""
GENESIS OXIDE — Manifest Generator v2
=======================================
One-pass crate compiler. Generates the entire Genesis Oxide project
from the opcode table. Zero manual fixes. Same pattern as the
SARAH_ENGINES manifest_generator.py — define templates, loop, write.

Run:  python genesis_oxide_manifest.py
"""
import os
import hashlib

PROJECT_ROOT = r'C:\GenesisOS_Core\genesis_oxide'
SOVEREIGN_ANCHOR = 1.092777037037037

# ════════════════════════════════════════════════════════════════
# OPCODE TABLE — the single source of truth
# ════════════════════════════════════════════════════════════════
OPCODES = [
    (0x00, 'NOP',        'No operation'),
    (0x10, 'LOAD_CONST', 'Load constant from table'),
    (0x11, 'ADD',        'Float addition: rA += rB'),
    (0x12, 'MUL',        'Float multiply: rA *= rB'),
    (0x13, 'SUB',        'Float subtract: rA -= rB'),
    (0x14, 'DIV',        'Float divide: rA /= rB'),
    (0x15, 'SQRT',       'Square root: rA = sqrt(rA)'),
    (0x16, 'SIN',        'Sine: rA = sin(rA)'),
    (0x17, 'PULSE',      'Resonance pulse: rA *= SOVEREIGN_ANCHOR'),
    (0x18, 'LOAD_IMM',   'Load 32-bit float immediate (2 slots)'),
    (0x20, 'CMP_GT',     'Compare greater-than: flag = rA > rB'),
    (0x21, 'CMP_EQ',     'Compare equal: flag = rA == rB'),
    (0x22, 'JUMP',       'Unconditional jump to address'),
    (0x23, 'JUMP_IF',    'Conditional jump if flag set'),
    (0x24, 'MOV',        'Move: rA = rB'),
    (0x25, 'LOAD_MEM',   'Load from memory'),
    (0x26, 'STORE_MEM',  'Store to memory'),
    (0x30, 'RESONATE',   'Heartbeat-modulated magnitude'),
    (0x31, 'EMBED',      'Lattice embedding (57D fractal hash)'),
    (0x33, 'THREAD_ID',  'Get CUDA thread index'),
    (0x34, 'STORE_OUT',  'Store result to output buffer'),
    (0x35, 'DENSITY',    'Compute density metric'),
    (0xFF, 'HALT',       'Terminate execution'),
]

# LLVM lowering map for dialect tier
LLVM_MAP = {
    'ADD': 'fadd f32', 'SUB': 'fsub f32', 'MUL': 'fmul f32', 'DIV': 'fdiv f32',
    'SQRT': 'llvm.sqrt.f32', 'SIN': 'llvm.sin.f32',
    'CMP_GT': 'fcmp ogt', 'CMP_EQ': 'fcmp oeq',
    'JUMP': 'br label', 'JUMP_IF': 'br i1 %flag, label',
    'MOV': 'bitcast f32', 'LOAD_MEM': 'load f32, ptr', 'STORE_MEM': 'store f32, ptr',
    'LOAD_CONST': 'load f32, ptr @const_table', 'LOAD_IMM': 'f32 <imm>',
    'PULSE': 'fmul f32 %val, 0x3F8BE01E', 'NOP': '; nop', 'HALT': 'ret void',
    'RESONATE': 'call @llvm.sqrt.f32 + fmul ANCHOR',
    'EMBED': 'call @llvm.sin.f32 (fractal loop)',
    'THREAD_ID': 'call @llvm.nvvm.read.ptx.sreg.tid.x',
    'STORE_OUT': 'store f32, ptr @output', 'DENSITY': 'loop fadd + fdiv',
}

def rust_name(s):
    """LOAD_CONST -> LoadConst"""
    return ''.join(w.capitalize() for w in s.split('_'))

def write(path, content):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)
    rel = os.path.relpath(path, PROJECT_ROOT)
    print(f'  [GEN] {rel}')

# ════════════════════════════════════════════════════════════════
# FILE GENERATORS — each function writes one compilable file
# ════════════════════════════════════════════════════════════════

def gen_workspace():
    write(os.path.join(PROJECT_ROOT, 'Cargo.toml'), """[workspace]
resolver = "2"
members = [
    "crates/genlex-types",
    "crates/genlex-oxide",
    "crates/dialect-genlex",
    "crates/genesis-runtime",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
authors = ["Joshua Petersen <joshuapetersen119@gmail.com>"]
license = "Apache-2.0"
""")


def gen_types():
    # Build enum variants
    variants = ''
    decode = ''
    encode = ''
    for code, name, desc in OPCODES:
        rn = rust_name(name)
        variants += f'    /// 0x{code:02X}: {desc}\n    {rn},\n'
        decode += f'            0x{code:02X} => Some(GlyphOp::{rn}),\n'
        encode += f'            GlyphOp::{rn} => 0x{code:02X},\n'

    src = f'''//! Genesis Oxide - Core Types
//! Auto-generated from opcode table. DO NOT EDIT.

/// The Sovereign Anchor
pub const SOVEREIGN_ANCHOR: f32 = {SOVEREIGN_ANCHOR}_f32;

/// Embedding dimensions
pub const LATTICE_DIMS: usize = 57;

/// A single Genlex instruction (4 bytes packed)
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct GlyphInst {{
    pub opcode: u8,
    pub reg_a: u8,
    pub reg_b: u8,
    pub flags: u8,
}}

impl GlyphInst {{
    pub const fn new(opcode: u8, a: u8, b: u8, flags: u8) -> Self {{
        GlyphInst {{ opcode, reg_a: a, reg_b: b, flags }}
    }}
    pub fn from_bytes(b: [u8; 4]) -> Self {{
        GlyphInst {{ opcode: b[0], reg_a: b[1], reg_b: b[2], flags: b[3] }}
    }}
    pub fn to_bytes(self) -> [u8; 4] {{
        [self.opcode, self.reg_a, self.reg_b, self.flags]
    }}
}}

/// Decoded opcode enum
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GlyphOp {{
{variants}}}

impl GlyphOp {{
    pub fn decode(opcode: u8) -> Option<Self> {{
        match opcode {{
{decode}            _ => None,
        }}
    }}
    pub fn encode(self) -> u8 {{
        match self {{
{encode}        }}
    }}
}}

/// GBIN file header (16 bytes)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct GbinHeader {{
    pub magic: [u8; 4],
    pub version: u32,
    pub num_instructions: u32,
    pub exec_flags: u32,
}}

impl GbinHeader {{
    pub const MAGIC: [u8; 4] = *b"GBIN";
    pub fn is_valid(&self) -> bool {{
        self.magic == Self::MAGIC && self.version == 1
    }}
    pub fn is_gpu(&self) -> bool {{
        self.exec_flags == 0 || self.exec_flags == 1
    }}
}}

pub mod constants {{
    pub const ANCHOR: f32 = super::SOVEREIGN_ANCHOR;
    pub const PI: f32 = 3.14159265_f32;
    pub const E: f32 = 2.71828182_f32;
    pub const DIMS: f32 = 57.0_f32;
    pub const BILLION: f32 = 0.999999999_f32;
}}
'''
    write(os.path.join(PROJECT_ROOT, 'crates', 'genlex-types', 'Cargo.toml'), """[package]
name = "genlex-types"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
""")
    write(os.path.join(PROJECT_ROOT, 'crates', 'genlex-types', 'src', 'lib.rs'), src)


def gen_oxide():
    write(os.path.join(PROJECT_ROOT, 'crates', 'genlex-oxide', 'Cargo.toml'), """[package]
name = "genlex-oxide"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
genlex-types = { path = "../genlex-types" }
""")

    # The VM lib.rs — written as a raw string, no .format() needed
    src = r'''//! Genesis Oxide - Glyph VM & Kernel Compiler (Tier A)
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

    /// Execute on CPU
    pub fn execute_cpu(&mut self) -> f32 {
        let mut pc: usize = 0;
        let mut flag: bool = false;

        while pc < self.instructions.len() {
            let inst = self.instructions[pc];
            let a = inst.reg_a as usize;
            let b = inst.reg_b as usize;

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
'''
    write(os.path.join(PROJECT_ROOT, 'crates', 'genlex-oxide', 'src', 'lib.rs'), src)


def gen_dialect():
    # Pliron ops — generated from table
    ops = ''
    lowering = ''
    for code, name, desc in OPCODES:
        rn = rust_name(name)
        llvm = LLVM_MAP.get(name, f'; TODO: lower {name}')
        ops += f'    /// {desc}\n'
        ops += f'    pub struct {rn}Op;\n'
        ops += f'    impl {rn}Op {{\n'
        ops += f'        pub const OPCODE: u8 = 0x{code:02X};\n'
        ops += f'        pub const NAME: &\'static str = "{name}";\n'
        ops += f'    }}\n\n'
        lowering += f'        // {name} (0x{code:02X}) -> {llvm}\n'

    src = f'''//! Genesis Oxide - Pliron IR Dialect (Tier B)
//! Auto-generated. DO NOT EDIT.

/// Dialect identifier
pub const DIALECT_NAME: &str = "genlex";
pub const DIALECT_VERSION: u32 = 1;

/// Pliron operation definitions (one per glyph opcode)
pub mod ops {{
{ops}}}

/// Type system
pub mod types {{
    /// Genlex register: always f32
    pub struct GenlexRegister;
    /// Genlex memory: indexed f32 array
    pub struct GenlexMemory;
    /// Genlex flag: boolean comparison result
    pub struct GenlexFlag;
}}

/// Lowering: Genlex IR -> LLVM IR
pub mod lowering {{
    /// Each glyph opcode maps to LLVM instructions:
    pub fn lower_to_llvm() {{
{lowering}    }}
}}
'''
    write(os.path.join(PROJECT_ROOT, 'crates', 'dialect-genlex', 'Cargo.toml'), """[package]
name = "dialect-genlex"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
genlex-types = { path = "../genlex-types" }
""")
    write(os.path.join(PROJECT_ROOT, 'crates', 'dialect-genlex', 'src', 'lib.rs'), src)


def gen_runtime():
    write(os.path.join(PROJECT_ROOT, 'crates', 'genesis-runtime', 'Cargo.toml'), """[package]
name = "genesis-runtime"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
genlex-types = { path = "../genlex-types" }
genlex-oxide = { path = "../genlex-oxide" }
""")

    # main.rs — raw string, no format escaping issues
    src = r'''//! Genesis Oxide - Runtime
//! Auto-generated. DO NOT EDIT.

use genlex_oxide::GlyphProgram;

fn main() {
    println!("=== GENESIS OXIDE v0.1.0 ===");
    println!("SOVEREIGN_ANCHOR = {}", genlex_types::SOVEREIGN_ANCHOR);
    println!();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: genesis-runtime <program.gbin>");
        std::process::exit(1);
    }

    let path = &args[1];
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read {}: {}", path, e);
            std::process::exit(1);
        }
    };

    println!("[LOAD] {} ({} bytes)", path, data.len());

    let mut program = match GlyphProgram::from_gbin(&data) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[ERROR] {}", e);
            std::process::exit(1);
        }
    };

    println!("[PROGRAM] {} instructions, exec_flags={}",
             program.header.num_instructions, program.header.exec_flags);

    if program.header.is_gpu() {
        println!("[MODE] GPU target (cuda-oxide pipeline)");
        println!("[FALLBACK] CPU execution for now");
    } else {
        println!("[MODE] CPU");
    }

    let result = program.execute_cpu();
    println!();
    println!("[RESULT] r0 = {}", result);
    println!("[REGISTERS]");
    for (i, val) in program.registers().iter().enumerate() {
        if *val != 0.0 {
            println!("  r{} = {}", i, val);
        }
    }
    println!();
    println!("=== GENESIS OXIDE COMPLETE ===");
}
'''
    write(os.path.join(PROJECT_ROOT, 'crates', 'genesis-runtime', 'src', 'main.rs'), src)


# ════════════════════════════════════════════════════════════════
# MAIN — one pass, all crates
# ════════════════════════════════════════════════════════════════

def main():
    print('=' * 64)
    print('  GENESIS OXIDE - MANIFEST GENERATOR v2')
    print(f'  SOVEREIGN_ANCHOR = {SOVEREIGN_ANCHOR}')
    print(f'  OPCODES = {len(OPCODES)} glyph operations')
    print(f'  TARGET  = {PROJECT_ROOT}')
    print('=' * 64)
    print()

    gen_workspace()
    print()
    print('[TIER 0] genlex-types')
    gen_types()
    print()
    print('[TIER A] genlex-oxide')
    gen_oxide()
    print()
    print('[TIER B] dialect-genlex')
    gen_dialect()
    print()
    print('[TIER C] genesis-runtime')
    gen_runtime()

    # Count files
    total = sum(1 for r, _, fs in os.walk(PROJECT_ROOT)
                for f in fs if f.endswith(('.rs', '.toml')))

    print()
    print('=' * 64)
    print(f'  MANIFEST COMPLETE: {total} files, {len(OPCODES)} opcodes')
    print(f'  4 crates ready to compile')
    print('=' * 64)

if __name__ == '__main__':
    main()
