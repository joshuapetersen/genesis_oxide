//! Genesis Oxide - Runtime with GPU Execution
//! Loads .gbin programs and fires them on NVIDIA GPU silicon.
//! Supports parallel dispatch: N threads each run the full program
//! with unique thread IDs for SIMD-style resonance search.

use genlex_oxide::GlyphProgram;
use cudarc::driver::{CudaContext, CudaSlice, CudaModule, CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use std::sync::Arc;
use std::time::Instant;

/// Full-ISA PTX kernel: Genlex glyph VM on GPU
/// Supports all 23 opcodes. Each CUDA thread executes the
/// entire program independently with its own register file.
/// Thread ID is accessible via THREAD_ID opcode (0x33).
const GLYPH_VM_PTX: &str = "
.version 8.0
.target sm_89
.address_size 64

.visible .entry glyph_vm(
    .param .u64 param_instructions,
    .param .u32 param_num_inst,
    .param .u64 param_output,
    .param .u32 param_num_threads
)
{
    .reg .u32   %gtid, %nthr, %num_inst, %pc, %opcode, %ra, %rb, %tmp_u;
    .reg .u64   %inst_base, %out_base, %addr;
    .reg .u32   %inst_word;
    .reg .f32   %r0, %r1, %r2, %r3, %r4, %r5, %r6, %r7;
    .reg .f32   %anchor, %tmp, %tmp2, %mag, %acc;
    .reg .pred  %p, %ploop, %phalt, %flag;

    // Global thread ID = blockIdx.x * blockDim.x + threadIdx.x
    mov.u32     %tmp_u, %ctaid.x;
    mov.u32     %gtid, %ntid.x;
    mov.u32     %num_inst, %tid.x;
    mad.lo.u32  %gtid, %tmp_u, %gtid, %num_inst;

    // Bounds check
    ld.param.u32 %nthr, [param_num_threads];
    setp.ge.u32  %p, %gtid, %nthr;
    @%p bra      EXIT;

    ld.param.u64 %inst_base, [param_instructions];
    ld.param.u32 %num_inst, [param_num_inst];
    ld.param.u64 %out_base, [param_output];

    // Init registers to 0
    mov.f32     %r0, 0f00000000;
    mov.f32     %r1, 0f00000000;
    mov.f32     %r2, 0f00000000;
    mov.f32     %r3, 0f00000000;
    mov.f32     %r4, 0f00000000;
    mov.f32     %r5, 0f00000000;
    mov.f32     %r6, 0f00000000;
    mov.f32     %r7, 0f00000000;

    // SOVEREIGN_ANCHOR = 1.092777037037037 = 0x3F8BE01E
    mov.f32     %anchor, 0f3F8BE01E;
    mov.u32     %pc, 0;

    // Clear flag
    setp.eq.u32 %flag, %pc, %pc;

FETCH:
    setp.ge.u32 %ploop, %pc, %num_inst;
    @%ploop bra WRITE_OUT;

    // Fetch instruction word at inst_base[pc*4] (32-bit aligned)
    mul.lo.u32  %opcode, %pc, 4;
    cvt.u64.u32 %addr, %opcode;
    add.u64     %addr, %inst_base, %addr;
    ld.global.u32 %inst_word, [%addr];

    // Decode: [opcode:8][ra:4+4][rb:4+4][flags:8]
    and.b32     %opcode, %inst_word, 255;
    shr.u32     %ra, %inst_word, 8;
    and.b32     %ra, %ra, 15;
    shr.u32     %rb, %inst_word, 16;
    and.b32     %rb, %rb, 15;

    // === HALT (0xFF) ===
    setp.eq.u32 %phalt, %opcode, 255;
    @%phalt bra WRITE_OUT;

    // === NOP (0x00) ===
    setp.eq.u32 %p, %opcode, 0;
    @%p bra     NEXT;

    // === LOAD_IMM (0x18) — next word is f32 immediate ===
    setp.ne.u32 %p, %opcode, 24;
    @%p bra     SKIP_LI;
    add.u32     %pc, %pc, 1;
    mul.lo.u32  %tmp_u, %pc, 4;
    cvt.u64.u32 %addr, %tmp_u;
    add.u64     %addr, %inst_base, %addr;
    ld.global.u32 %inst_word, [%addr];
    mov.b32     %tmp, %inst_word;
    setp.eq.u32 %p, %ra, 0; @%p mov.f32 %r0, %tmp;
    setp.eq.u32 %p, %ra, 1; @%p mov.f32 %r1, %tmp;
    setp.eq.u32 %p, %ra, 2; @%p mov.f32 %r2, %tmp;
    setp.eq.u32 %p, %ra, 3; @%p mov.f32 %r3, %tmp;
    setp.eq.u32 %p, %ra, 4; @%p mov.f32 %r4, %tmp;
    setp.eq.u32 %p, %ra, 5; @%p mov.f32 %r5, %tmp;
    setp.eq.u32 %p, %ra, 6; @%p mov.f32 %r6, %tmp;
    setp.eq.u32 %p, %ra, 7; @%p mov.f32 %r7, %tmp;
    bra         NEXT;
SKIP_LI:

    // === LOAD_CONST (0x10) — r[a] = rb * 0.01 ===
    setp.ne.u32 %p, %opcode, 16;
    @%p bra     SKIP_LC;
    cvt.rn.f32.u32 %tmp, %rb;
    mul.f32     %tmp, %tmp, 0f3C23D70A;
    setp.eq.u32 %p, %ra, 0; @%p mov.f32 %r0, %tmp;
    setp.eq.u32 %p, %ra, 1; @%p mov.f32 %r1, %tmp;
    setp.eq.u32 %p, %ra, 2; @%p mov.f32 %r2, %tmp;
    setp.eq.u32 %p, %ra, 3; @%p mov.f32 %r3, %tmp;
    bra         NEXT;
SKIP_LC:

    // --- REGISTER LOAD helpers ---
    // Load r[ra] -> %tmp, r[rb] -> %tmp2
    mov.f32     %tmp, 0f00000000;
    mov.f32     %tmp2, 0f00000000;
    setp.eq.u32 %p, %ra, 0; @%p mov.f32 %tmp, %r0;
    setp.eq.u32 %p, %ra, 1; @%p mov.f32 %tmp, %r1;
    setp.eq.u32 %p, %ra, 2; @%p mov.f32 %tmp, %r2;
    setp.eq.u32 %p, %ra, 3; @%p mov.f32 %tmp, %r3;
    setp.eq.u32 %p, %ra, 4; @%p mov.f32 %tmp, %r4;
    setp.eq.u32 %p, %ra, 5; @%p mov.f32 %tmp, %r5;
    setp.eq.u32 %p, %ra, 6; @%p mov.f32 %tmp, %r6;
    setp.eq.u32 %p, %ra, 7; @%p mov.f32 %tmp, %r7;
    setp.eq.u32 %p, %rb, 0; @%p mov.f32 %tmp2, %r0;
    setp.eq.u32 %p, %rb, 1; @%p mov.f32 %tmp2, %r1;
    setp.eq.u32 %p, %rb, 2; @%p mov.f32 %tmp2, %r2;
    setp.eq.u32 %p, %rb, 3; @%p mov.f32 %tmp2, %r3;
    setp.eq.u32 %p, %rb, 4; @%p mov.f32 %tmp2, %r4;
    setp.eq.u32 %p, %rb, 5; @%p mov.f32 %tmp2, %r5;
    setp.eq.u32 %p, %rb, 6; @%p mov.f32 %tmp2, %r6;
    setp.eq.u32 %p, %rb, 7; @%p mov.f32 %tmp2, %r7;

    // === ADD (0x11) ===
    setp.ne.u32 %p, %opcode, 17;
    @%p bra     SKIP_ADD;
    add.f32     %tmp, %tmp, %tmp2;
    bra         STORE_RA;
SKIP_ADD:

    // === MUL (0x12) ===
    setp.ne.u32 %p, %opcode, 18;
    @%p bra     SKIP_MUL;
    mul.f32     %tmp, %tmp, %tmp2;
    bra         STORE_RA;
SKIP_MUL:

    // === SUB (0x13) ===
    setp.ne.u32 %p, %opcode, 19;
    @%p bra     SKIP_SUB;
    sub.f32     %tmp, %tmp, %tmp2;
    bra         STORE_RA;
SKIP_SUB:

    // === DIV (0x14) ===
    setp.ne.u32 %p, %opcode, 20;
    @%p bra     SKIP_DIV;
    div.approx.f32 %tmp, %tmp, %tmp2;
    bra         STORE_RA;
SKIP_DIV:

    // === SQRT (0x15) ===
    setp.ne.u32 %p, %opcode, 21;
    @%p bra     SKIP_SQRT;
    sqrt.approx.f32 %tmp, %tmp;
    bra         STORE_RA;
SKIP_SQRT:

    // === SIN (0x16) ===
    setp.ne.u32 %p, %opcode, 22;
    @%p bra     SKIP_SIN;
    sin.approx.f32 %tmp, %tmp;
    bra         STORE_RA;
SKIP_SIN:

    // === PULSE (0x17) — r[ra] *= ANCHOR ===
    setp.ne.u32 %p, %opcode, 23;
    @%p bra     SKIP_PULSE;
    mul.f32     %tmp, %tmp, %anchor;
    bra         STORE_RA;
SKIP_PULSE:

    // === CMP_GT (0x20) ===
    setp.ne.u32 %p, %opcode, 32;
    @%p bra     SKIP_GT;
    setp.gt.f32 %flag, %tmp, %tmp2;
    bra         NEXT;
SKIP_GT:

    // === CMP_EQ (0x21) ===
    setp.ne.u32 %p, %opcode, 33;
    @%p bra     SKIP_EQ;
    setp.eq.f32 %flag, %tmp, %tmp2;
    bra         NEXT;
SKIP_EQ:

    // === JUMP (0x22) — pc = ra ===
    setp.ne.u32 %p, %opcode, 34;
    @%p bra     SKIP_JMP;
    mov.u32     %pc, %ra;
    bra         FETCH;
SKIP_JMP:

    // === JUMP_IF (0x23) — if flag: pc = ra ===
    setp.ne.u32 %p, %opcode, 35;
    @%p bra     SKIP_JIF;
    @%flag mov.u32 %pc, %ra;
    @%flag bra     FETCH;
    bra         NEXT;
SKIP_JIF:

    // === MOV (0x24) — r[a] = r[b] ===
    setp.ne.u32 %p, %opcode, 36;
    @%p bra     SKIP_MOV;
    mov.f32     %tmp, %tmp2;
    bra         STORE_RA;
SKIP_MOV:

    // === LOAD_MEM (0x25) — same as MOV for register machine ===
    setp.ne.u32 %p, %opcode, 37;
    @%p bra     SKIP_LM;
    mov.f32     %tmp, %tmp2;
    bra         STORE_RA;
SKIP_LM:

    // === STORE_MEM (0x26) — r[b] = r[a] ===
    setp.ne.u32 %p, %opcode, 38;
    @%p bra     SKIP_SM;
    // Store tmp (r[a]) into r[rb]
    setp.eq.u32 %p, %rb, 0; @%p mov.f32 %r0, %tmp;
    setp.eq.u32 %p, %rb, 1; @%p mov.f32 %r1, %tmp;
    setp.eq.u32 %p, %rb, 2; @%p mov.f32 %r2, %tmp;
    setp.eq.u32 %p, %rb, 3; @%p mov.f32 %r3, %tmp;
    setp.eq.u32 %p, %rb, 4; @%p mov.f32 %r4, %tmp;
    setp.eq.u32 %p, %rb, 5; @%p mov.f32 %r5, %tmp;
    setp.eq.u32 %p, %rb, 6; @%p mov.f32 %r6, %tmp;
    setp.eq.u32 %p, %rb, 7; @%p mov.f32 %r7, %tmp;
    bra         NEXT;
SKIP_SM:

    // === RESONATE (0x30) — r[a] = magnitude(regs) * ANCHOR ===
    setp.ne.u32 %p, %opcode, 48;
    @%p bra     SKIP_RES;
    mul.f32     %acc, %r0, %r0;
    mul.f32     %mag, %r1, %r1;
    add.f32     %acc, %acc, %mag;
    mul.f32     %mag, %r2, %r2;
    add.f32     %acc, %acc, %mag;
    mul.f32     %mag, %r3, %r3;
    add.f32     %acc, %acc, %mag;
    mul.f32     %mag, %r4, %r4;
    add.f32     %acc, %acc, %mag;
    mul.f32     %mag, %r5, %r5;
    add.f32     %acc, %acc, %mag;
    mul.f32     %mag, %r6, %r6;
    add.f32     %acc, %acc, %mag;
    mul.f32     %mag, %r7, %r7;
    add.f32     %acc, %acc, %mag;
    sqrt.approx.f32 %acc, %acc;
    mul.f32     %tmp, %acc, %anchor;
    bra         STORE_RA;
SKIP_RES:

    // === EMBED (0x31) — fractal lattice hash ===
    setp.ne.u32 %p, %opcode, 49;
    @%p bra     SKIP_EMB;
    // Simplified 8-dim embed: r[d] = sin(val * (d+1) * ANCHOR) * 0.5 + 0.5
    mul.f32     %mag, %tmp, %anchor;
    sin.approx.f32 %mag, %mag;
    mul.f32     %mag, %mag, 0f3F000000;
    add.f32     %r0, %mag, 0f3F000000;
    mul.f32     %mag, %tmp, 0f40000000;
    mul.f32     %mag, %mag, %anchor;
    sin.approx.f32 %mag, %mag;
    mul.f32     %mag, %mag, 0f3F000000;
    add.f32     %r1, %mag, 0f3F000000;
    mul.f32     %mag, %tmp, 0f40400000;
    mul.f32     %mag, %mag, %anchor;
    sin.approx.f32 %mag, %mag;
    mul.f32     %mag, %mag, 0f3F000000;
    add.f32     %r2, %mag, 0f3F000000;
    mul.f32     %mag, %tmp, 0f40800000;
    mul.f32     %mag, %mag, %anchor;
    sin.approx.f32 %mag, %mag;
    mul.f32     %mag, %mag, 0f3F000000;
    add.f32     %r3, %mag, 0f3F000000;
    bra         NEXT;
SKIP_EMB:

    // === THREAD_ID (0x33) — r[a] = CUDA thread ID ===
    setp.ne.u32 %p, %opcode, 51;
    @%p bra     SKIP_TID;
    cvt.rn.f32.u32 %tmp, %gtid;
    bra         STORE_RA;
SKIP_TID:

    // === STORE_OUT (0x34) — store r[a] to output[tid] (same as writeback) ===
    setp.ne.u32 %p, %opcode, 52;
    @%p bra     SKIP_SO;
    cvt.u64.u32 %addr, %gtid;
    shl.b64     %addr, %addr, 2;
    add.u64     %addr, %out_base, %addr;
    st.global.f32 [%addr], %tmp;
    bra         NEXT;
SKIP_SO:

    // === DENSITY (0x35) — r[a] = avg(abs(regs)) ===
    setp.ne.u32 %p, %opcode, 53;
    @%p bra     SKIP_DEN;
    abs.f32     %acc, %r0;
    abs.f32     %mag, %r1;
    add.f32     %acc, %acc, %mag;
    abs.f32     %mag, %r2;
    add.f32     %acc, %acc, %mag;
    abs.f32     %mag, %r3;
    add.f32     %acc, %acc, %mag;
    abs.f32     %mag, %r4;
    add.f32     %acc, %acc, %mag;
    abs.f32     %mag, %r5;
    add.f32     %acc, %acc, %mag;
    abs.f32     %mag, %r6;
    add.f32     %acc, %acc, %mag;
    abs.f32     %mag, %r7;
    add.f32     %acc, %acc, %mag;
    mul.f32     %tmp, %acc, 0f3E000000;
    bra         STORE_RA;
SKIP_DEN:

    // Unknown opcode — skip
    bra         NEXT;

// === STORE r[ra] <- %tmp ===
STORE_RA:
    setp.eq.u32 %p, %ra, 0; @%p mov.f32 %r0, %tmp;
    setp.eq.u32 %p, %ra, 1; @%p mov.f32 %r1, %tmp;
    setp.eq.u32 %p, %ra, 2; @%p mov.f32 %r2, %tmp;
    setp.eq.u32 %p, %ra, 3; @%p mov.f32 %r3, %tmp;
    setp.eq.u32 %p, %ra, 4; @%p mov.f32 %r4, %tmp;
    setp.eq.u32 %p, %ra, 5; @%p mov.f32 %r5, %tmp;
    setp.eq.u32 %p, %ra, 6; @%p mov.f32 %r6, %tmp;
    setp.eq.u32 %p, %ra, 7; @%p mov.f32 %r7, %tmp;

NEXT:
    add.u32     %pc, %pc, 1;
    bra         FETCH;

WRITE_OUT:
    // Write r0 to output[tid] — coalesced 32-bit aligned store
    cvt.u64.u32 %addr, %gtid;
    shl.b64     %addr, %addr, 2;
    add.u64     %addr, %out_base, %addr;
    st.global.f32 [%addr], %r0;

EXIT:
    ret;
}
";

fn main() {
    println!("================================================================");
    println!("  GENESIS OXIDE v0.2.0 — Full ISA GPU Runtime");
    println!("  SOVEREIGN_ANCHOR = {}", genlex_types::SOVEREIGN_ANCHOR);
    println!("================================================================");
    println!();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: genesis-runtime <program.gbin> [--threads N]");
        std::process::exit(1);
    }

    let path = &args[1];

    // Parse --threads N (default 1)
    let num_threads: u32 = args.iter()
        .position(|a| a == "--threads")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

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
        println!("[MODE] GPU — {} thread(s)", num_threads);
        println!();
        match run_on_gpu(&program, num_threads) {
            Ok(results) => {
                // Show GPU results
                if num_threads == 1 {
                    println!("[GPU RESULT] r0 = {}", results[0]);
                } else {
                    println!("[GPU RESULT] {} threads completed", results.len());
                    for (i, val) in results.iter().enumerate().take(16) {
                        if *val != 0.0 {
                            println!("  thread[{}] r0 = {}", i, val);
                        }
                    }
                    if results.len() > 16 {
                        println!("  ... ({} more)", results.len() - 16);
                    }
                }
                println!();

                // CPU verification (single thread)
                let cpu_result = program.execute_cpu();
                println!("[CPU VERIFY] r0 = {}", cpu_result);
                let delta = (results[0] - cpu_result).abs();
                if delta < 0.01 {
                    println!("[MATCH] GPU and CPU agree (delta={})", delta);
                } else {
                    println!("[DRIFT] GPU={} CPU={} delta={}", results[0], cpu_result, delta);
                }
            }
            Err(e) => {
                println!("[GPU ERROR] {} — falling back to CPU", e);
                let result = program.execute_cpu();
                println!("[CPU RESULT] r0 = {}", result);
            }
        }
    } else {
        println!("[MODE] CPU");
        let result = program.execute_cpu();
        println!("[RESULT] r0 = {}", result);
    }

    println!();
    println!("[REGISTERS]");
    for (i, val) in program.registers().iter().enumerate() {
        if *val != 0.0 {
            println!("  r{} = {}", i, val);
        }
    }
    println!();
    println!("================================================================");
    println!("  GENESIS OXIDE COMPLETE");
    println!("================================================================");
}

fn run_on_gpu(program: &GlyphProgram, num_threads: u32) -> Result<Vec<f32>, String> {
    let t_start = Instant::now();

    // Initialize CUDA context on GPU 0
    let ctx: Arc<CudaContext> = CudaContext::new(0)
        .map_err(|e| format!("CUDA init failed: {}", e))?;

    let name = ctx.name().unwrap_or_else(|_| "Unknown GPU".into());
    println!("[GPU] Device: {}", name);

    // Load PTX module — compile to CUBIN first for reliable loading
    let exe_dir = std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let ptx_path = exe_dir.join("glyph_vm.ptx");
    let cubin_path = exe_dir.join("glyph_vm.cubin");

    std::fs::write(&ptx_path, GLYPH_VM_PTX)
        .map_err(|e| format!("Failed to write PTX: {}", e))?;

    // Compile PTX -> CUBIN
    let ptxas_out = std::process::Command::new("ptxas")
        .args(["-arch=sm_89", "-o"])
        .arg(&cubin_path)
        .arg(&ptx_path)
        .output()
        .map_err(|e| format!("ptxas failed to run: {}", e))?;

    if !ptxas_out.status.success() {
        let err = String::from_utf8_lossy(&ptxas_out.stderr);
        return Err(format!("ptxas error: {}", err));
    }
    println!("[GPU] PTX compiled to CUBIN ({} bytes)", 
             std::fs::metadata(&cubin_path).map(|m| m.len()).unwrap_or(0));

    // Load CUBIN binary
    let cubin_data = std::fs::read(&cubin_path)
        .map_err(|e| format!("Failed to read CUBIN: {}", e))?;
    let ptx = Ptx::from_binary(cubin_data);
    let module: Arc<CudaModule> = ctx.load_module(ptx)
        .map_err(|e| format!("Module load failed: {}", e))?;
    let func: CudaFunction = module.load_function("glyph_vm")
        .map_err(|e| format!("Function load failed: {}", e))?;
    println!("[GPU] Kernel loaded: glyph_vm (23 opcodes, full ISA)");

    let stream = ctx.default_stream();

    // Pack instructions as u32 (little-endian, 32-bit aligned for coalesced reads)
    let inst_data: Vec<u32> = program.instructions.iter().map(|inst| {
        u32::from_le_bytes(inst.to_bytes())
    }).collect();
    let num_inst = inst_data.len() as u32;

    // Upload instructions to GPU
    let d_instructions: CudaSlice<u32> = stream.clone_htod(&inst_data)
        .map_err(|e| format!("Upload failed: {}", e))?;

    // Allocate output: one f32 per thread (coalesced, N-scaled)
    let mut d_output: CudaSlice<f32> = stream.alloc_zeros(num_threads as usize)
        .map_err(|e| format!("Alloc failed: {}", e))?;

    println!("[GPU] {} instructions uploaded ({} bytes, 32-bit aligned)",
             num_inst, num_inst * 4);
    println!("[GPU] Output buffer: {} x f32 ({} bytes)",
             num_threads, num_threads * 4);

    // Launch config: scale threads across warps
    let block_size = if num_threads >= 256 { 256 } else { num_threads };
    let grid_size = (num_threads + block_size - 1) / block_size;

    let cfg = LaunchConfig {
        grid_dim: (grid_size, 1, 1),
        block_dim: (block_size, 1, 1),
        shared_mem_bytes: 0,
    };

    println!("[GPU] Launch: grid=({},1,1) block=({},1,1) threads={}",
             grid_size, block_size, num_threads);

    let t_launch = Instant::now();

    unsafe {
        stream.launch_builder(&func)
            .arg(&d_instructions)
            .arg(&num_inst)
            .arg(&mut d_output)
            .arg(&num_threads)
            .launch(cfg)
            .map_err(|e| format!("Launch failed: {}", e))?;
    }

    // Synchronize and read back
    let results: Vec<f32> = stream.clone_dtoh(&d_output)
        .map_err(|e| format!("Readback failed: {}", e))?;

    let t_done = Instant::now();
    let kernel_us = t_done.duration_since(t_launch).as_micros();
    let total_us = t_done.duration_since(t_start).as_micros();

    println!("[GPU] Kernel: {}us | Total: {}us", kernel_us, total_us);

    Ok(results)
}
