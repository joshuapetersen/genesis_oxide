//! ════════════════════════════════════════════════════════════════
//! GENLEX-ASM — Genlex Assembler/Disassembler CLI
//! ════════════════════════════════════════════════════════════════
//!
//! Usage:
//!   genlex-asm input.glx              → compile to input.gbin
//!   genlex-asm input.glx -o out.gbin  → explicit output path
//!   genlex-asm --disassemble input.gbin → disassemble to text
//!   genlex-asm --self-test            → verify assembler

use dialect_genlex::{assembler, disassembler};
use genlex_types::SOVEREIGN_ANCHOR;

fn self_test() {
    println!("════════════════════════════════════════════════════════════════");
    println!("  GENLEX ASSEMBLER SELF-TEST (Rust Native)");
    println!("  SOVEREIGN_ANCHOR = {}", SOVEREIGN_ANCHOR);
    println!("════════════════════════════════════════════════════════════════");

    // Test 1: Arithmetic program
    println!("\n[TEST 1] Arithmetic program");
    let source1 = "; Hello Resonance — first Genlex program
LOAD_IMM r0 #42.0         ; load 42.0 into r0
LOAD_IMM r1 #ANCHOR       ; load sovereign anchor into r1
MUL r0 r1                 ; r0 = 42.0 * 1.092777...
HALT                      ; stop
";
    let result1 = assembler::assemble(source1).unwrap();
    println!("  Instructions: {}", result1.instructions);
    println!("  Binary size:  {} bytes", result1.binary.len());
    println!("  Exec mode:    {}", result1.flag_desc);
    assert_eq!(result1.instructions, 6, "Expected 6 instructions (2 LOAD_IMM×2 + MUL + HALT)");

    // Disassemble it
    println!("  Disassembly:");
    let dis = disassembler::disassemble(&result1.binary).unwrap();
    println!("{}", dis);

    // Test 2: Resonance computation
    println!("\n[TEST 2] Resonance computation");
    let source2 = "; 57D resonance score
LOAD_IMM r0 #1.0
LOAD_IMM r1 #2.0
LOAD_IMM r2 #3.0
LOAD_IMM r3 #4.0
RESONATE r0 0 0            ; heartbeat-modulated magnitude
STORE_OUT r0 0 0
HALT
";
    let result2 = assembler::assemble(source2).unwrap();
    println!("  Instructions: {} ({} LOAD_IMM + RESONATE + STORE + HALT)", result2.instructions,
        result2.instructions - 3);
    println!("  Binary size:  {} bytes, {}", result2.binary.len(), result2.flag_desc);

    // Test 3: Labels and jumps
    println!("\n[TEST 3] Loop with labels");
    let source3 = "; Loop: multiply r0 by ANCHOR N times
LOAD_IMM r0 #1.0
LOAD_IMM r1 #5.0
:loop
PULSE r0 0 0               ; r0 *= ANCHOR
LOAD_CONST r2 1 0
SUB r1 r2
CMP_GT r1 r2 0
JUMP_IF :loop 0 0
HALT
";
    let result3 = assembler::assemble(source3).unwrap();
    println!("  Labels: {:?}", result3.labels);
    println!("  Binary size: {} bytes", result3.binary.len());
    println!("  Disassembly:");
    let dis3 = disassembler::disassemble(&result3.binary).unwrap();
    println!("{}", dis3);

    // Verify the assembled binary is valid
    let gbin = &result1.binary;
    assert_eq!(&gbin[0..4], b"GBIN");
    let version = u32::from_le_bytes([gbin[4], gbin[5], gbin[6], gbin[7]]);
    assert_eq!(version, 1);

    println!("\n════════════════════════════════════════════════════════════════");
    println!("  ALL TESTS PASSED — Rust-native assembler verified");
    println!("════════════════════════════════════════════════════════════════");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage:");
        eprintln!("  genlex-asm input.glx              — compile to .gbin");
        eprintln!("  genlex-asm input.glx -o out.gbin  — explicit output");
        eprintln!("  genlex-asm --disassemble in.gbin  — disassemble");
        eprintln!("  genlex-asm --self-test             — verify assembler");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "--self-test" => self_test(),
        "--disassemble" | "-d" => {
            if args.len() < 3 {
                eprintln!("[ERROR] Usage: genlex-asm --disassemble <file.gbin>");
                std::process::exit(1);
            }
            match disassembler::disassemble_file(&args[2]) {
                Ok(text) => println!("{}", text),
                Err(e) => {
                    eprintln!("[ERROR] {}", e);
                    std::process::exit(1);
                }
            }
        }
        _ => {
            let glx_path = &args[1];
            let out_path = if args.len() > 3 && args[2] == "-o" {
                Some(args[3].as_str())
            } else {
                None
            };
            match assembler::assemble_file(glx_path, out_path) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("[ERROR] {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
