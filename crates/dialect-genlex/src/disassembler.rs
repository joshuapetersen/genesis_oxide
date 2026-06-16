//! ════════════════════════════════════════════════════════════════
//! GENLEX DISASSEMBLER — .gbin Binary → Readable Text
//! ════════════════════════════════════════════════════════════════

use std::collections::HashMap;

/// Reverse opcode lookup (opcode → name)
fn opcode_names() -> HashMap<u8, &'static str> {
    let mut m = HashMap::new();
    m.insert(0x00, "NOP");
    m.insert(0xFF, "HALT");
    m.insert(0x10, "LOAD_CONST");
    m.insert(0x11, "ADD");
    m.insert(0x12, "MUL");
    m.insert(0x13, "SUB");
    m.insert(0x14, "DIV");
    m.insert(0x15, "SQRT");
    m.insert(0x16, "SIN");
    m.insert(0x17, "PULSE");
    m.insert(0x18, "LOAD_IMM");
    m.insert(0x20, "CMP_GT");
    m.insert(0x21, "CMP_EQ");
    m.insert(0x22, "JUMP");
    m.insert(0x23, "JUMP_IF");
    m.insert(0x24, "MOV");
    m.insert(0x25, "LOAD_MEM");
    m.insert(0x26, "STORE_MEM");
    m.insert(0x30, "RESONATE");
    m.insert(0x31, "EMBED");
    m.insert(0x33, "THREAD_ID");
    m.insert(0x34, "STORE_OUT");
    m.insert(0x35, "DENSITY");
    m.insert(0x36, "COMMIT");
    m.insert(0x37, "LOOP_START");
    m.insert(0x40, "STRING_APPEND");
    m.insert(0x41, "BIT_SHIFT");
    m.insert(0x42, "STREAM_STOP");
    m.insert(0x43, "PTR_INC");
    m.insert(0x44, "PTR_DEC");
    m.insert(0x45, "READ_INPUT");
    m.insert(0x46, "STD_OUT");
    m.insert(0x50, "OS_SHELL");
    m.insert(0x58, "SOVEREIGN_MIRROR");
    m
}

/// Disassemble a .gbin binary to human-readable text
pub fn disassemble(binary: &[u8]) -> Result<String, String> {
    if binary.len() < 16 {
        return Err("Binary too small".into());
    }
    if &binary[0..4] != b"GBIN" {
        return Err("Not a valid .gbin file".into());
    }

    let version = u32::from_le_bytes([binary[4], binary[5], binary[6], binary[7]]);
    let num_inst = u32::from_le_bytes([binary[8], binary[9], binary[10], binary[11]]);
    let flags = u32::from_le_bytes([binary[12], binary[13], binary[14], binary[15]]);

    let flag_str = match flags {
        0 => "hybrid",
        1 => "GPU",
        2 => "CPU",
        _ => "unknown",
    };

    let payload = &binary[16..binary.len().saturating_sub(32)];
    let names = opcode_names();

    let mut lines = Vec::new();
    lines.push(format!("; GBIN v{} | {} instructions | {}", version, num_inst, flag_str));
    lines.push(String::new());

    let mut pc: usize = 0;
    while pc * 4 + 3 < payload.len() {
        let base = pc * 4;
        let opcode = payload[base];
        let a = payload[base + 1];
        let b = payload[base + 2];
        let f = payload[base + 3];

        let name = names.get(&opcode)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("0x{:02X}", opcode));

        if opcode == 0x18 && (pc + 1) * 4 + 3 < payload.len() {
            // LOAD_IMM — next 4 bytes are float
            let imm_base = (pc + 1) * 4;
            let imm_val = f32::from_le_bytes([
                payload[imm_base], payload[imm_base + 1],
                payload[imm_base + 2], payload[imm_base + 3],
            ]);
            lines.push(format!("  {:4}: {:<16} r{} #{}", pc, name, a, imm_val));
            pc += 2;
        } else if opcode == 0xFF {
            lines.push(format!("  {:4}: HALT", pc));
            pc += 1;
        } else if opcode == 0x00 {
            lines.push(format!("  {:4}: NOP", pc));
            pc += 1;
        } else {
            lines.push(format!("  {:4}: {:<16} r{} r{} {}", pc, name, a, b, f));
            pc += 1;
        }
    }

    Ok(lines.join("\n"))
}

/// Disassemble a .gbin file
pub fn disassemble_file(path: &str) -> Result<String, String> {
    let data = std::fs::read(path)
        .map_err(|e| format!("Cannot read {}: {}", path, e))?;
    disassemble(&data)
}
