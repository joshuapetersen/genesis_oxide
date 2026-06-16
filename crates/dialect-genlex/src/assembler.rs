//! ════════════════════════════════════════════════════════════════
//! GENLEX ASSEMBLER — .glx Source → .gbin Binary Compiler (Rust)
//! ════════════════════════════════════════════════════════════════
//!
//! Port of the Python genlex_assembler.py. Full pipeline:
//!   .glx (human-readable) → .gbin (packed binary)
//!
//! .glx syntax:
//!     OPCODE operandA operandB flags    ; comment
//!     LOAD_IMM r0 #42.0                 ; load immediate float
//!     :label                            ; jump label
//!     JUMP :label                       ; jump to label
//!     #ANCHOR                           ; built-in constant

use std::collections::HashMap;
use genlex_types::SOVEREIGN_ANCHOR;

/// Built-in constants
pub fn constants() -> HashMap<&'static str, f32> {
    let mut m = HashMap::new();
    m.insert("ANCHOR", SOVEREIGN_ANCHOR);
    m.insert("PI", std::f32::consts::PI);
    m.insert("E", std::f32::consts::E);
    m.insert("ZERO", 0.0);
    m.insert("ONE", 1.0);
    m.insert("DIMS", 57.0);
    m.insert("BILLION", 0.999999999);
    m
}

/// Master opcode table (matches Python OPCODE_TABLE)
pub fn opcode_table() -> HashMap<&'static str, u8> {
    let mut m = HashMap::new();

    // Core VM operations
    m.insert("NOP", 0x00);
    m.insert("HALT", 0xFF);
    m.insert("LOAD_CONST", 0x10);
    m.insert("LOAD_IMM", 0x18);
    m.insert("ADD", 0x11);
    m.insert("SUB", 0x13);
    m.insert("MUL", 0x12);
    m.insert("DIV", 0x14);
    m.insert("SQRT", 0x15);
    m.insert("SIN", 0x16);
    m.insert("PULSE", 0x17);
    m.insert("CMP_GT", 0x20);
    m.insert("CMP_EQ", 0x21);
    m.insert("JUMP", 0x22);
    m.insert("JUMP_IF", 0x23);
    m.insert("MOV", 0x24);
    m.insert("LOAD_MEM", 0x25);
    m.insert("STORE_MEM", 0x26);
    m.insert("RESONATE", 0x30);
    m.insert("EMBED", 0x31);
    m.insert("THREAD_ID", 0x33);
    m.insert("STORE_OUT", 0x34);
    m.insert("DENSITY", 0x35);

    // Upper Tier aliases
    m.insert("STACK_PUSH", 0x10);
    m.insert("MEMORY_ALLOC", 0x18);
    m.insert("POINTER_JUMP", 0x22);
    m.insert("CONDITIONAL_IF", 0x23);
    m.insert("EXEC_TRIGGER", 0x17);
    m.insert("DATA_DUP", 0x24);
    m.insert("REGISTER_SET", 0x18);
    m.insert("STREAM_DATA", 0x25);
    m.insert("CACHE_STORE", 0x26);
    m.insert("COMMIT_STATE", 0x36);

    // Math aliases
    m.insert("MATH_ADD", 0x11);
    m.insert("MATH_SUB", 0x13);
    m.insert("MATH_MUL", 0x12);
    m.insert("MATH_DIV", 0x14);
    m.insert("MATH_RESULT", 0x34);
    m.insert("RESONANCE_CALC", 0x30);
    m.insert("FREQ_SHIFT", 0x17);

    // Neural aliases
    m.insert("NEURAL_PULSE", 0x17);
    m.insert("PARALLEL_PULSE", 0x17);

    // System I/O
    m.insert("STRING_APPEND", 0x40);
    m.insert("BIT_SHIFT", 0x41);
    m.insert("STREAM_STOP", 0x42);
    m.insert("LOOP_START", 0x37);
    m.insert("PTR_INC", 0x43);
    m.insert("PTR_DEC", 0x44);
    m.insert("READ_INPUT", 0x45);
    m.insert("STD_OUT", 0x46);
    m.insert("FIND_PATTERN", 0x27);
    m.insert("MEM_READ", 0x25);
    m.insert("OS_SHELL", 0x50);
    m.insert("OS_APP", 0x51);
    m.insert("OS_KEY", 0x52);
    m.insert("OS_WRITE", 0x53);
    m.insert("OS_CLICK", 0x54);
    m.insert("SOVEREIGN_MIRROR", 0x58);

    m
}

/// Assembly metadata
#[derive(Debug)]
pub struct AssemblyResult {
    pub binary: Vec<u8>,
    pub instructions: usize,
    pub labels: HashMap<String, usize>,
    pub exec_flags: u32,
    pub flag_desc: String,
}

/// Parse a register operand (r0-r15), label (:name), constant (#name), or raw number
fn parse_operand(s: &str, labels: &HashMap<String, usize>) -> u8 {
    let s = s.trim();
    if s.is_empty() { return 0; }

    // Register: r0-r15
    if s.starts_with('r') {
        if let Ok(n) = s[1..].parse::<u8>() {
            return n.min(15);
        }
    }

    // Label reference
    if s.starts_with(':') {
        let label = &s[1..];
        return (*labels.get(label).unwrap_or(&0) & 0xFF) as u8;
    }

    // Built-in constant (just return 0; actual value handled by LOAD_IMM)
    if s.starts_with('#') {
        let name = &s[1..];
        if constants().contains_key(name.to_uppercase().as_str()) {
            return 0;
        }
        if let Ok(v) = name.parse::<f64>() {
            return (v as i64 & 0xFF) as u8;
        }
        return 0;
    }

    // Raw number
    if let Ok(n) = s.parse::<i64>() {
        return (n & 0xFF) as u8;
    }
    if let Ok(n) = s.parse::<f64>() {
        return (n as i64 & 0xFF) as u8;
    }

    0
}

/// Assemble a .glx source string into .gbin binary
pub fn assemble(source: &str) -> Result<AssemblyResult, String> {
    let ops = opcode_table();
    let consts = constants();
    let mut labels: HashMap<String, usize> = HashMap::new();
    let mut instructions: Vec<[u8; 4]> = Vec::new();

    let lines: Vec<&str> = source.lines().collect();

    // First pass: collect labels and count instructions
    let mut pc: usize = 0;
    for line in &lines {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.is_empty() { continue; }

        if line.starts_with(':') {
            labels.insert(line[1..].trim().to_string(), pc);
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        let token = parts[0].to_uppercase();
        if ops.get(token.as_str()) == Some(&0x18) {
            pc += 2; // LOAD_IMM uses 2 slots
        } else {
            pc += 1;
        }
    }

    // Second pass: emit instructions
    let mut has_cpu = false;
    let mut has_gpu = false;

    for (line_num, line) in lines.iter().enumerate() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with(':') { continue; }

        let parts: Vec<&str> = line.split_whitespace().collect();
        let token = parts[0].to_uppercase();

        let opcode = match ops.get(token.as_str()) {
            Some(&op) => op,
            None => {
                eprintln!("  [WARN] Line {}: unknown op '{}', emitting NOP", line_num + 1, parts[0]);
                0x00
            }
        };

        // Track execution domain
        if opcode >= 0x40 && opcode <= 0x58 {
            has_cpu = true;
        } else {
            has_gpu = true;
        }

        let a = parse_operand(parts.get(1).unwrap_or(&"0"), &labels);
        let b = parse_operand(parts.get(2).unwrap_or(&"0"), &labels);
        let flags = parse_operand(parts.get(3).unwrap_or(&"0"), &labels);

        // Special handling: LOAD_IMM with float constant
        if opcode == 0x18 && parts.len() > 2 && parts[2].starts_with('#') {
            let const_name = &parts[2][1..];
            let val = if let Some(&v) = consts.get(const_name.to_uppercase().as_str()) {
                v
            } else {
                const_name.parse::<f32>().unwrap_or(0.0)
            };
            instructions.push([opcode, a, 0, 0]);
            instructions.push(val.to_le_bytes());
            continue;
        }

        instructions.push([opcode, a, b, flags]);
    }

    // Determine exec flags
    let exec_flags: u32 = if has_gpu && has_cpu { 0 }
        else if has_gpu { 1 }
        else { 2 };

    let flag_desc = match exec_flags {
        0 => "hybrid",
        1 => "GPU-only",
        2 => "CPU-only",
        _ => "unknown",
    }.to_string();

    // Build .gbin binary
    let num_inst = instructions.len() as u32;
    let mut binary: Vec<u8> = Vec::with_capacity(16 + instructions.len() * 4 + 32);

    // Header (16 bytes)
    binary.extend_from_slice(b"GBIN");
    binary.extend_from_slice(&1u32.to_le_bytes()); // version
    binary.extend_from_slice(&num_inst.to_le_bytes());
    binary.extend_from_slice(&exec_flags.to_le_bytes());

    // Instructions
    for inst in &instructions {
        binary.extend_from_slice(inst);
    }

    // SHA-256 integrity hash
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // For now, use a simple hash (full SHA-256 would need a dependency)
    // The Python version uses hashlib.sha256 over header+payload
    let mut hasher = DefaultHasher::new();
    binary.hash(&mut hasher);
    let hash_val = hasher.finish();
    // Pad to 32 bytes
    let mut hash_bytes = [0u8; 32];
    hash_bytes[0..8].copy_from_slice(&hash_val.to_le_bytes());
    hash_bytes[8..16].copy_from_slice(&hash_val.to_be_bytes());
    hash_bytes[16..24].copy_from_slice(&(hash_val ^ 0xDEADBEEFCAFEBABE).to_le_bytes());
    hash_bytes[24..32].copy_from_slice(&(hash_val.wrapping_mul(SOVEREIGN_ANCHOR.to_bits() as u64)).to_le_bytes());
    binary.extend_from_slice(&hash_bytes);

    Ok(AssemblyResult {
        binary,
        instructions: instructions.len(),
        labels,
        exec_flags,
        flag_desc,
    })
}

/// Assemble a .glx file to .gbin
pub fn assemble_file(glx_path: &str, output_path: Option<&str>) -> Result<String, String> {
    let source = std::fs::read_to_string(glx_path)
        .map_err(|e| format!("Cannot read {}: {}", glx_path, e))?;

    let result = assemble(&source)?;

    let out_path = output_path
        .map(String::from)
        .unwrap_or_else(|| glx_path.replace(".glx", ".gbin"));

    std::fs::write(&out_path, &result.binary)
        .map_err(|e| format!("Cannot write {}: {}", out_path, e))?;

    println!("[ASSEMBLER] Compiled: {}", glx_path);
    println!("  Instructions: {}", result.instructions);
    println!("  Binary size:  {} bytes", result.binary.len());
    println!("  Exec mode:    {}", result.flag_desc);
    println!("  Labels:       {:?}", result.labels);
    println!("  Output:       {}", out_path);

    Ok(out_path)
}
