# Genesis Oxide

**Sovereign Compute Engine — Rust/CUDA Native**

The clean-room rebuild of the Genesis Mission substrate in native Rust with GPU-resident execution via PTX kernels.

## Architecture

```
genesis_oxide/
├── crates/
│   ├── genlex-types/       # Core types: 23-opcode ISA, GlyphInst, GbinHeader
│   ├── genlex-oxide/       # Pliron IR integration (future)
│   ├── dialect-genlex/     # Assembler (.glx → .gbin) + Disassembler
│   ├── genesis-runtime/    # Genlex VM runtime
│   └── sovereign-engine/   # Evolution engine, vault, SKVA, batch dispatch
├── examples/               # .glx programs + compiled .gbin binaries
└── Cargo.toml              # Workspace root
```

## Crates

| Crate | Binary | Purpose |
|-------|--------|---------|
| `genlex-types` | — | Core ISA types, constants, `SOVEREIGN_ANCHOR` |
| `dialect-genlex` | `genlex-asm` | Assembler/disassembler CLI |
| `genesis-runtime` | `genesis-runtime` | Genlex VM execution |
| `sovereign-engine` | `sovereign` | Evolution engine + vault + SKVA bridge |

## Quick Start

```bash
# Build everything
cargo build --workspace

# Assemble a .glx program
cargo run -p dialect-genlex -- examples/fibonacci.glx

# Disassemble a .gbin binary
cargo run -p dialect-genlex -- --disassemble examples/fibonacci.gbin

# Run assembler self-test
cargo run -p dialect-genlex -- --self-test

# Run sovereign engine
cargo run -p sovereign-engine -- --help
```

## ISA (23 Core Opcodes)

| Opcode | Hex | Description |
|--------|-----|-------------|
| NOP | 0x00 | No operation |
| LOAD_CONST | 0x10 | Load from constant table |
| ADD | 0x11 | Float add: rA += rB |
| MUL | 0x12 | Float multiply: rA *= rB |
| SUB | 0x13 | Float subtract: rA -= rB |
| DIV | 0x14 | Float divide: rA /= rB |
| SQRT | 0x15 | Square root |
| SIN | 0x16 | Sine |
| PULSE | 0x17 | Resonance pulse: rA *= ANCHOR |
| LOAD_IMM | 0x18 | Load 32-bit float immediate |
| CMP_GT | 0x20 | Compare greater-than |
| CMP_EQ | 0x21 | Compare equal |
| JUMP | 0x22 | Unconditional jump |
| JUMP_IF | 0x23 | Conditional jump |
| MOV | 0x24 | Register move |
| LOAD_MEM | 0x25 | Load from memory |
| STORE_MEM | 0x26 | Store to memory |
| RESONATE | 0x30 | Heartbeat-modulated magnitude |
| EMBED | 0x31 | 57D lattice embedding |
| THREAD_ID | 0x33 | CUDA thread index |
| STORE_OUT | 0x34 | Store to output buffer |
| DENSITY | 0x35 | Compute density metric |
| HALT | 0xFF | Terminate |

## Constants

```
SOVEREIGN_ANCHOR = 1.092777037037037
LATTICE_DIMS     = 57
BILLION_BARRIER  = 0.999999999
```

## License

Apache-2.0 — Joshua Petersen
