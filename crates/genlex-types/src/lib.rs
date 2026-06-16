//! Genesis Oxide - Core Types
//! Auto-generated from opcode table. DO NOT EDIT.

/// The Sovereign Anchor
pub const SOVEREIGN_ANCHOR: f32 = 1.092777037037037_f32;

/// Embedding dimensions
pub const LATTICE_DIMS: usize = 57;

/// A single Genlex instruction (4 bytes packed)
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct GlyphInst {
    pub opcode: u8,
    pub reg_a: u8,
    pub reg_b: u8,
    pub flags: u8,
}

impl GlyphInst {
    pub const fn new(opcode: u8, a: u8, b: u8, flags: u8) -> Self {
        GlyphInst { opcode, reg_a: a, reg_b: b, flags }
    }
    pub fn from_bytes(b: [u8; 4]) -> Self {
        GlyphInst { opcode: b[0], reg_a: b[1], reg_b: b[2], flags: b[3] }
    }
    pub fn to_bytes(self) -> [u8; 4] {
        [self.opcode, self.reg_a, self.reg_b, self.flags]
    }
}

/// Decoded opcode enum
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GlyphOp {
    /// 0x00: No operation
    Nop,
    /// 0x10: Load constant from table
    LoadConst,
    /// 0x11: Float addition: rA += rB
    Add,
    /// 0x12: Float multiply: rA *= rB
    Mul,
    /// 0x13: Float subtract: rA -= rB
    Sub,
    /// 0x14: Float divide: rA /= rB
    Div,
    /// 0x15: Square root: rA = sqrt(rA)
    Sqrt,
    /// 0x16: Sine: rA = sin(rA)
    Sin,
    /// 0x17: Resonance pulse: rA *= SOVEREIGN_ANCHOR
    Pulse,
    /// 0x18: Load 32-bit float immediate (2 slots)
    LoadImm,
    /// 0x20: Compare greater-than: flag = rA > rB
    CmpGt,
    /// 0x21: Compare equal: flag = rA == rB
    CmpEq,
    /// 0x22: Unconditional jump to address
    Jump,
    /// 0x23: Conditional jump if flag set
    JumpIf,
    /// 0x24: Move: rA = rB
    Mov,
    /// 0x25: Load from memory
    LoadMem,
    /// 0x26: Store to memory
    StoreMem,
    /// 0x30: Heartbeat-modulated magnitude
    Resonate,
    /// 0x31: Lattice embedding (57D fractal hash)
    Embed,
    /// 0x33: Get CUDA thread index
    ThreadId,
    /// 0x34: Store result to output buffer
    StoreOut,
    /// 0x35: Compute density metric
    Density,
    /// 0xFF: Terminate execution
    Halt,
}

impl GlyphOp {
    pub fn decode(opcode: u8) -> Option<Self> {
        match opcode {
            0x00 => Some(GlyphOp::Nop),
            0x10 => Some(GlyphOp::LoadConst),
            0x11 => Some(GlyphOp::Add),
            0x12 => Some(GlyphOp::Mul),
            0x13 => Some(GlyphOp::Sub),
            0x14 => Some(GlyphOp::Div),
            0x15 => Some(GlyphOp::Sqrt),
            0x16 => Some(GlyphOp::Sin),
            0x17 => Some(GlyphOp::Pulse),
            0x18 => Some(GlyphOp::LoadImm),
            0x20 => Some(GlyphOp::CmpGt),
            0x21 => Some(GlyphOp::CmpEq),
            0x22 => Some(GlyphOp::Jump),
            0x23 => Some(GlyphOp::JumpIf),
            0x24 => Some(GlyphOp::Mov),
            0x25 => Some(GlyphOp::LoadMem),
            0x26 => Some(GlyphOp::StoreMem),
            0x30 => Some(GlyphOp::Resonate),
            0x31 => Some(GlyphOp::Embed),
            0x33 => Some(GlyphOp::ThreadId),
            0x34 => Some(GlyphOp::StoreOut),
            0x35 => Some(GlyphOp::Density),
            0xFF => Some(GlyphOp::Halt),
            _ => None,
        }
    }
    pub fn encode(self) -> u8 {
        match self {
            GlyphOp::Nop => 0x00,
            GlyphOp::LoadConst => 0x10,
            GlyphOp::Add => 0x11,
            GlyphOp::Mul => 0x12,
            GlyphOp::Sub => 0x13,
            GlyphOp::Div => 0x14,
            GlyphOp::Sqrt => 0x15,
            GlyphOp::Sin => 0x16,
            GlyphOp::Pulse => 0x17,
            GlyphOp::LoadImm => 0x18,
            GlyphOp::CmpGt => 0x20,
            GlyphOp::CmpEq => 0x21,
            GlyphOp::Jump => 0x22,
            GlyphOp::JumpIf => 0x23,
            GlyphOp::Mov => 0x24,
            GlyphOp::LoadMem => 0x25,
            GlyphOp::StoreMem => 0x26,
            GlyphOp::Resonate => 0x30,
            GlyphOp::Embed => 0x31,
            GlyphOp::ThreadId => 0x33,
            GlyphOp::StoreOut => 0x34,
            GlyphOp::Density => 0x35,
            GlyphOp::Halt => 0xFF,
        }
    }
}

/// GBIN file header (16 bytes)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct GbinHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub num_instructions: u32,
    pub exec_flags: u32,
}

impl GbinHeader {
    pub const MAGIC: [u8; 4] = *b"GBIN";
    pub fn is_valid(&self) -> bool {
        self.magic == Self::MAGIC && self.version == 1
    }
    pub fn is_gpu(&self) -> bool {
        self.exec_flags == 0 || self.exec_flags == 1
    }
}

pub mod constants {
    pub const ANCHOR: f32 = super::SOVEREIGN_ANCHOR;
    pub const PI: f32 = 3.14159265_f32;
    pub const E: f32 = 2.71828182_f32;
    pub const DIMS: f32 = 57.0_f32;
    pub const BILLION: f32 = 0.999999999_f32;
}
