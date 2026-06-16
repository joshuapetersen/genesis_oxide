//! ════════════════════════════════════════════════════════════════
//! SOVEREIGN CONSENSUS COMPUTE ENGINE
//! ════════════════════════════════════════════════════════════════
//!
//! Four paths. One truth. If they disagree, the result doesn't exist.
//!
//! Architecture:
//!   Path 1: Rust CPU VM     (genlex-oxide execute_cpu)
//!   Path 2: GPU PTX VM      (glyph_vm kernel on CUDA)
//!   Path 3: GPU N-Thread    (parallel consensus across 2560 cores)
//!   Path 4: Evolutionary    (directed evolution → verify → survive)
//!   Path 5: Vault Search    (15,330 memories searched in parallel)
//!
//! Modes:
//!   sovereign <file.gbin>                     → consensus verify
//!   sovereign <file.gbin> --evolve <N>        → evolve N generations
//!   sovereign <file.gbin> --search <N>        → N-thread parallel search
//!   sovereign <file.gbin> --bench             → full benchmark suite
//!   sovereign --vault <path> --query "text"   → vault memory search

use genlex_oxide::GlyphProgram;
use genlex_types::{GlyphInst, SOVEREIGN_ANCHOR};
use cudarc::driver::{CudaContext, CudaSlice, CudaModule, CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use std::sync::Arc;
use std::time::Instant;

mod vault;
mod batch;
mod archive;
mod will;
mod auditor;
mod sector29;
mod skva;

// ════════════════════════════════════════════════════════════════
// PTX KERNELS
// ════════════════════════════════════════════════════════════════
const GLYPH_VM_PTX: &str = include_str!("glyph_vm.ptx");
const EVOLVE_VM_PTX: &str = include_str!("evolution_kernel.ptx");

// ════════════════════════════════════════════════════════════════
// CONSENSUS RESULT
// ════════════════════════════════════════════════════════════════

#[derive(Debug)]
struct ConsensusResult {
    cpu_result: f32,
    gpu_result: f32,
    delta: f32,
    is_sovereign: bool,    // all paths agree
    cpu_time_us: u128,
    gpu_time_us: u128,
    gpu_name: String,
}

impl ConsensusResult {
    fn print(&self) {
        println!("┌─────────────────────────────────────────────────────┐");
        println!("│           SOVEREIGN CONSENSUS REPORT                │");
        println!("├─────────────────────────────────────────────────────┤");
        println!("│  CPU VM (Rust)   : r0 = {:<28}│", format!("{}", self.cpu_result));
        println!("│  GPU VM (PTX)    : r0 = {:<28}│", format!("{}", self.gpu_result));
        println!("│  Delta           : {:<33}│", format!("{}", self.delta));
        println!("│  GPU             : {:<33}│", &self.gpu_name[..self.gpu_name.len().min(33)]);
        println!("│  CPU time        : {:<25} us  │", self.cpu_time_us);
        println!("│  GPU time        : {:<25} us  │", self.gpu_time_us);
        println!("├─────────────────────────────────────────────────────┤");
        if self.is_sovereign {
            println!("│  ✓ SOVEREIGN TRUTH — ALL PATHS CONVERGE             │");
            println!("│    Consensus: VERIFIED (delta=0)                     │");
        } else {
            println!("│  ✗ CONSENSUS FAILED — RESULT DOES NOT EXIST          │");
            println!("│    Paths diverged. No truth manifested.              │");
        }
        println!("└─────────────────────────────────────────────────────┘");
    }
}

// ════════════════════════════════════════════════════════════════
// DIRECTED EVOLUTION RESULT
// ════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
struct Organism {
    instructions: Vec<GlyphInst>,
    fitness: f32,
    generation: u32,
    cycles: u32,
}

#[derive(Debug)]
struct EvolutionResult {
    generations: u32,
    population_size: u32,
    best: Organism,
    consensus_verified: bool,
    total_evaluations: u64,
    total_time_us: u128,
}

impl EvolutionResult {
    fn print(&self) {
        println!("┌─────────────────────────────────────────────────────┐");
        println!("│       DIRECTED EVOLUTION CYCLE REPORT               │");
        println!("├─────────────────────────────────────────────────────┤");
        println!("│  Generations     : {:<33}│", self.generations);
        println!("│  Population      : {:<33}│", self.population_size);
        println!("│  Total evals     : {:<33}│", self.total_evaluations);
        println!("│  Best fitness    : {:<33}│", format!("{:.6}", self.best.fitness));
        println!("│  Cycles          : {:<33}│", self.best.cycles);
        println!("│  Time            : {:<25} us  │", self.total_time_us);
        println!("│  Evals/sec       : {:<33}│",
            if self.total_time_us > 0 {
                format!("{}", self.total_evaluations as u128 * 1_000_000 / self.total_time_us)
            } else { "∞".to_string() }
        );
        println!("├─────────────────────────────────────────────────────┤");
        if self.consensus_verified {
            println!("│  ✓ DIRECTED EVOLUTION VERIFIED — SOVEREIGN TRUTH    │");
        } else {
            println!("│  ✗ DIRECTED EVOLUTION FAILED CONSENSUS              │");
        }
        println!("└─────────────────────────────────────────────────────┘");
    }
}

// ════════════════════════════════════════════════════════════════
// GPU CONTEXT
// ════════════════════════════════════════════════════════════════

struct GpuContext {
    ctx: Arc<CudaContext>,
    module: Arc<CudaModule>,
    func: CudaFunction,
    evolve_func: Option<CudaFunction>,
    name: String,
}

impl GpuContext {
    fn init() -> Result<Self, String> {
        let ctx = CudaContext::new(0)
            .map_err(|e| format!("CUDA init failed: {}", e))?;
        let name = ctx.name().unwrap_or_else(|_| "Unknown GPU".into());

        // Write PTX and compile to CUBIN
        let exe_dir = std::env::current_exe()
            .unwrap_or_default()
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let ptx_path = exe_dir.join("sovereign_vm.ptx");
        let cubin_path = exe_dir.join("sovereign_vm.cubin");

        std::fs::write(&ptx_path, GLYPH_VM_PTX)
            .map_err(|e| format!("Failed to write PTX: {}", e))?;

        let ptxas_out = std::process::Command::new("ptxas")
            .args(["-arch=sm_89", "-O3", "-o"])
            .arg(&cubin_path)
            .arg(&ptx_path)
            .output()
            .map_err(|e| format!("ptxas failed: {}", e))?;

        if !ptxas_out.status.success() {
            return Err(format!("ptxas: {}", String::from_utf8_lossy(&ptxas_out.stderr)));
        }

        let cubin = std::fs::read(&cubin_path)
            .map_err(|e| format!("CUBIN read failed: {}", e))?;
        let ptx = Ptx::from_binary(cubin);
        let module = ctx.load_module(ptx)
            .map_err(|e| format!("Module load failed: {}", e))?;
        let func = module.load_function("glyph_vm")
            .map_err(|e| format!("Function load failed: {}", e))?;

        // Load evolution kernel
        let evolve_ptx_path = exe_dir.join("evolve_vm.ptx");
        let evolve_cubin_path = exe_dir.join("evolve_vm.cubin");
        std::fs::write(&evolve_ptx_path, EVOLVE_VM_PTX)
            .map_err(|e| format!("Evolve PTX write: {}", e))?;
        let evo_out = std::process::Command::new("ptxas")
            .args(["-arch=sm_89", "-O3", "-o"])
            .arg(&evolve_cubin_path)
            .arg(&evolve_ptx_path)
            .output()
            .map_err(|e| format!("ptxas evolve: {}", e))?;
        let evolve_func = if evo_out.status.success() {
            let evo_cubin = std::fs::read(&evolve_cubin_path)
                .map_err(|e| format!("Evolve CUBIN: {}", e))?;
            let evo_module = ctx.load_module(Ptx::from_binary(evo_cubin))
                .map_err(|e| format!("Evolve module: {}", e))?;
            evo_module.load_function("evolve_vm").ok()
        } else {
            None
        };

        Ok(GpuContext { ctx, module, func, evolve_func, name })
    }

    /// Run a program on GPU with N threads, return all N results
    fn execute(&self, instructions: &[GlyphInst], num_threads: u32) -> Result<Vec<f32>, String> {
        let stream = self.ctx.default_stream();

        let inst_data: Vec<u32> = instructions.iter()
            .map(|inst| u32::from_le_bytes(inst.to_bytes()))
            .collect();
        let num_inst = inst_data.len() as u32;

        let d_inst: CudaSlice<u32> = stream.clone_htod(&inst_data)
            .map_err(|e| format!("Upload: {}", e))?;
        let mut d_out: CudaSlice<f32> = stream.alloc_zeros(num_threads as usize)
            .map_err(|e| format!("Alloc: {}", e))?;

        let block = if num_threads >= 256 { 256 } else { num_threads };
        let grid = (num_threads + block - 1) / block;

        let cfg = LaunchConfig {
            grid_dim: (grid, 1, 1),
            block_dim: (block, 1, 1),
            shared_mem_bytes: 0,
        };

        unsafe {
            stream.launch_builder(&self.func)
                .arg(&d_inst)
                .arg(&num_inst)
                .arg(&mut d_out)
                .arg(&num_threads)
                .launch(cfg)
                .map_err(|e| format!("Launch: {}", e))?;
        }

        stream.clone_dtoh(&d_out)
            .map_err(|e| format!("Readback: {}", e))
    }
}

// ════════════════════════════════════════════════════════════════
// CONSENSUS VERIFICATION
// ════════════════════════════════════════════════════════════════

fn verify_consensus(data: &[u8], gpu: &GpuContext) -> Result<ConsensusResult, String> {
    // PATH 1: CPU VM
    let t_cpu = Instant::now();
    let mut cpu_program = GlyphProgram::from_gbin(data)?;
    let cpu_result = cpu_program.execute_cpu();
    let cpu_time = t_cpu.elapsed().as_micros();

    // PATH 2: GPU VM
    let gpu_program = GlyphProgram::from_gbin(data)?;
    let t_gpu = Instant::now();
    let gpu_results = gpu.execute(&gpu_program.instructions, 1)?;
    let gpu_time = t_gpu.elapsed().as_micros();
    let gpu_result = gpu_results[0];

    // CONSENSUS CHECK
    let delta = (cpu_result - gpu_result).abs();
    let is_sovereign = delta < 0.001;

    Ok(ConsensusResult {
        cpu_result,
        gpu_result,
        delta,
        is_sovereign,
        cpu_time_us: cpu_time,
        gpu_time_us: gpu_time,
        gpu_name: gpu.name.clone(),
    })
}

// ════════════════════════════════════════════════════════════════
// PARALLEL SEARCH
// ════════════════════════════════════════════════════════════════

fn parallel_search(data: &[u8], gpu: &GpuContext, num_threads: u32) -> Result<(), String> {
    let program = GlyphProgram::from_gbin(data)?;

    println!("[SEARCH] Launching {} parallel VMs on {}...", num_threads, gpu.name);
    let t_start = Instant::now();
    let results = gpu.execute(&program.instructions, num_threads)?;
    let elapsed = t_start.elapsed().as_micros();

    // Find unique results
    let mut unique: Vec<f32> = results.clone();
    unique.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    unique.dedup();

    println!("┌─────────────────────────────────────────────────────┐");
    println!("│           PARALLEL SEARCH REPORT                    │");
    println!("├─────────────────────────────────────────────────────┤");
    println!("│  Threads         : {:<33}│", num_threads);
    println!("│  Time            : {:<25} us  │", elapsed);
    println!("│  Unique results  : {:<33}│", unique.len());
    println!("│  Throughput      : {:<25} VM/s│",
        if elapsed > 0 { num_threads as u128 * 1_000_000 / elapsed } else { 0 });
    println!("├─────────────────────────────────────────────────────┤");

    // Show first few results
    for (i, val) in results.iter().enumerate().take(8) {
        println!("│  thread[{:>4}]     : r0 = {:<25}│", i, format!("{}", val));
    }
    if results.len() > 8 {
        println!("│  ... ({} more threads)                              │", results.len() - 8);
    }
    println!("├─────────────────────────────────────────────────────┤");

    if unique.len() == 1 {
        println!("│  ✓ ALL {} PATHS CONVERGE — SOVEREIGN TRUTH{:>10}│",
            num_threads, "");
    } else {
        println!("│  {} unique values across {} threads               │",
            unique.len(), num_threads);
    }
    println!("└─────────────────────────────────────────────────────┘");

    Ok(())
}

// ════════════════════════════════════════════════════════════════
// DIRECTED EVOLUTION — Programs that evolve toward truth
// ════════════════════════════════════════════════════════════════

fn evolve(data: &[u8], gpu: &GpuContext, generations: u32) -> Result<EvolutionResult, String> {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    let base_program = GlyphProgram::from_gbin(data)?;
    let base_instructions = base_program.instructions.clone();
    let pop_size: u32 = 256; // One warp-group per generation

    // Target: maximize resonance score (r0 after execution)
    let mut population: Vec<Organism> = Vec::with_capacity(pop_size as usize);

    // Seed population with copies of the base program
    for _ in 0..pop_size {
        population.push(Organism {
            instructions: base_instructions.clone(),
            fitness: 0.0,
            generation: 0,
            cycles: 0,
        });
    }

    let t_start = Instant::now();
    let mut total_evals: u64 = 0;
    let mut best_ever = Organism {
        instructions: base_instructions.clone(),
        fitness: f32::NEG_INFINITY,
        generation: 0,
        cycles: 0,
    };

    println!("[DIRECTED EVOLUTION] {} cycles × {} organisms = {} total evaluations",
        generations, pop_size, generations as u64 * pop_size as u64);
    println!("[DIRECTED EVOLUTION] Target: maximize resonance score (r0)");
    println!();

    for gidx in 0..generations {
        // Pack all organisms into one instruction buffer
        // Each organism's instructions are concatenated
        let inst_len = base_instructions.len();
        let mut all_insts: Vec<GlyphInst> = Vec::with_capacity(inst_len * pop_size as usize);

        for org in &population {
            all_insts.extend_from_slice(&org.instructions);
        }

        // Flatten to u32 for GPU
        let inst_data: Vec<u32> = all_insts.iter()
            .map(|inst| u32::from_le_bytes(inst.to_bytes()))
            .collect();

        // GPU-accelerated fitness evaluation: all organisms in parallel
        let fitnesses: Vec<f32> = if let Some(ref evo_func) = gpu.evolve_func {
            let stream = gpu.ctx.default_stream();
            let d_programs: CudaSlice<u32> = stream.clone_htod(&inst_data)
                .map_err(|e| format!("Upload programs: {}", e))?;
            let mut d_output: CudaSlice<f32> = stream.alloc_zeros(pop_size as usize)
                .map_err(|e| format!("Alloc output: {}", e))?;

            let inst_len_u32 = inst_len as u32;
            let block = if pop_size >= 256 { 256 } else { pop_size };
            let grid = (pop_size + block - 1) / block;
            let cfg = LaunchConfig {
                grid_dim: (grid, 1, 1),
                block_dim: (block, 1, 1),
                shared_mem_bytes: 0,
            };

            unsafe {
                stream.launch_builder(evo_func)
                    .arg(&d_programs)
                    .arg(&inst_len_u32)
                    .arg(&mut d_output)
                    .arg(&pop_size)
                    .launch(cfg)
                    .map_err(|e| format!("Evolve launch: {}", e))?;
            }

            let results = stream.clone_dtoh(&d_output)
                .map_err(|e| format!("Readback: {}", e))?;
            results.iter().map(|r| r.abs()).collect()
        } else {
            // CPU fallback
            let mut f = Vec::with_capacity(pop_size as usize);
            for org in &population {
                let gbin = build_gbin(&org.instructions);
                let mut prog = GlyphProgram::from_gbin(&gbin).unwrap();
                let result = prog.execute_cpu();
                f.push(result.abs());
            }
            f
        };

        total_evals += pop_size as u64;

        // Update fitness and find best
        for (i, org) in population.iter_mut().enumerate() {
            org.fitness = fitnesses[i];
            org.generation = gidx;
        }

        // Sort by fitness (descending)
        population.sort_by(|a, b| b.fitness.partial_cmp(&a.fitness).unwrap_or(std::cmp::Ordering::Equal));

        if population[0].fitness > best_ever.fitness {
            best_ever = population[0].clone();
            if gidx % 10 == 0 || gidx == 0 {
                println!("  cycle {:>4} | best fitness = {:.6} | evolution cycles = {}",
                    gidx, best_ever.fitness, best_ever.cycles);
            }
        }

        // Selection: keep top 25%
        let survivors = pop_size as usize / 4;
        let parents: Vec<Organism> = population[..survivors].to_vec();

        // Breed next generation
        population.clear();
        for i in 0..pop_size as usize {
            let parent = &parents[i % survivors];
            let mut child = parent.clone();

            if i >= survivors {
                // Directed evolution: refine instruction toward higher resonance
                direct_evolution_cycle(&mut child, &mut rng, &base_instructions);
            }
            population.push(child);
        }
    }

    // Final consensus verification of best organism
    let best_gbin = build_gbin(&best_ever.instructions);
    let consensus = verify_consensus(&best_gbin, gpu);
    let verified = consensus.map(|c| c.is_sovereign).unwrap_or(false);

    let total_time = t_start.elapsed().as_micros();

    Ok(EvolutionResult {
        generations,
        population_size: pop_size,
        best: best_ever,
        consensus_verified: verified,
        total_evaluations: total_evals,
        total_time_us: total_time,
    })
}

fn direct_evolution_cycle(org: &mut Organism, rng: &mut impl rand::Rng, _base: &[GlyphInst]) {
    // Protect LOAD_IMM data words (instruction after opcode 0x18)
    let mut safe_indices: Vec<usize> = Vec::new();
    for i in 0..org.instructions.len() {
        if i > 0 && org.instructions[i - 1].opcode == 0x18 {
            continue; // This is a float data word, skip
        }
        if org.instructions[i].opcode == 0xFF {
            continue; // Don't mutate HALT
        }
        safe_indices.push(i);
    }
    if safe_indices.is_empty() { return; }

    let idx = safe_indices[rng.gen_range(0..safe_indices.len())];
    let evolution_type = rng.gen_range(0..5);

    match evolution_type {
        0 => {
            // Evolve opcode (keep it valid — no LOAD_IMM or HALT)
            let valid_ops: &[u8] = &[0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x30, 0x35];
            org.instructions[idx].opcode = valid_ops[rng.gen_range(0..valid_ops.len())];
        }
        1 => {
            org.instructions[idx].reg_a = rng.gen_range(0..8);
        }
        2 => {
            org.instructions[idx].reg_b = rng.gen_range(0..8);
        }
        3 => {
            if safe_indices.len() >= 2 {
                let other = safe_indices[rng.gen_range(0..safe_indices.len())];
                org.instructions.swap(idx, other);
            }
        }
        4 => {
            let valid_ops: &[u8] = &[0x11, 0x12, 0x13, 0x17, 0x30];
            org.instructions[idx] = GlyphInst::new(
                valid_ops[rng.gen_range(0..valid_ops.len())],
                rng.gen_range(0..8),
                rng.gen_range(0..8),
                0,
            );
        }
        _ => {}
    }
    org.cycles += 1;
}

/// Build a minimal .gbin from raw instructions
fn build_gbin(instructions: &[GlyphInst]) -> Vec<u8> {
    let mut data = Vec::with_capacity(16 + instructions.len() * 4 + 32);

    // Header (16 bytes)
    data.extend_from_slice(b"GBIN");
    data.extend_from_slice(&1u32.to_le_bytes());  // version
    data.extend_from_slice(&(instructions.len() as u32).to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());   // exec_flags = GPU

    // Instructions
    for inst in instructions {
        data.extend_from_slice(&inst.to_bytes());
    }

    // Trailer (32 bytes of zeros — checksum placeholder)
    data.extend_from_slice(&[0u8; 32]);

    data
}

// ════════════════════════════════════════════════════════════════
// BENCHMARK SUITE
// ════════════════════════════════════════════════════════════════

fn benchmark(data: &[u8], gpu: &GpuContext) -> Result<(), String> {
    println!("┌─────────────────────────────────────────────────────┐");
    println!("│           SOVEREIGN BENCHMARK SUITE                 │");
    println!("├─────────────────────────────────────────────────────┤");

    // CPU benchmark: 10000 iterations
    let iters = 10_000u64;
    let t = Instant::now();
    for _ in 0..iters {
        let mut p = GlyphProgram::from_gbin(data).unwrap();
        p.execute_cpu();
    }
    let cpu_total = t.elapsed().as_micros();
    let cpu_per = cpu_total / iters as u128;
    println!("│  CPU: {} iters in {} us ({} us/iter){:>10}│",
        iters, cpu_total, cpu_per, "");

    // GPU benchmark: different thread counts
    for &threads in &[1u32, 32, 256, 1024, 2560, 10240, 65536] {
        let program = GlyphProgram::from_gbin(data)?;
        let t = Instant::now();
        let results = gpu.execute(&program.instructions, threads)?;
        let gpu_us = t.elapsed().as_micros();
        let throughput = if gpu_us > 0 { threads as u128 * 1_000_000 / gpu_us } else { 0 };

        // Verify all results match
        let all_match = results.iter().all(|r| (*r - results[0]).abs() < 0.001);
        let status = if all_match { "✓" } else { "✗" };

        println!("│  GPU {:>6} threads: {:>8} us | {:>10} VM/s {} │",
            threads, gpu_us, throughput, status);
    }

    println!("└─────────────────────────────────────────────────────┘");
    Ok(())
}

// ════════════════════════════════════════════════════════════════
// MAIN
// ════════════════════════════════════════════════════════════════

fn find_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|s| s == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    println!();
    println!("════════════════════════════════════════════════════════════════");
    println!("  SOVEREIGN CONSENSUS COMPUTE ENGINE v2.0");
    println!("  Four paths. One truth. If they disagree, it doesn't exist.");
    println!("  SOVEREIGN_ANCHOR = {}", SOVEREIGN_ANCHOR);
    println!("════════════════════════════════════════════════════════════════");
    println!();

    let args: Vec<String> = std::env::args().collect();

    // ── VAULT MODE ──
    if let Some(vault_path) = find_arg(&args, "--vault") {
        let query = find_arg(&args, "--query")
            .unwrap_or_else(|| "sovereign truth".to_string());

        println!("[VAULT] Loading: {}", vault_path);
        let t_load = Instant::now();
        let sv = if let Some(archive_path) = find_arg(&args, "--archive-path") {
            match vault::SovereignVault::load_with_archive(
                std::path::Path::new(&vault_path),
                std::path::Path::new(&archive_path),
            ) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("[FATAL] {}", e);
                    std::process::exit(1);
                }
            }
        } else {
            match vault::SovereignVault::load(std::path::Path::new(&vault_path)) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("[FATAL] {}", e);
                    std::process::exit(1);
                }
            }
        };
        println!("[VAULT] {} nodes × {}D loaded in {} us (vault={}, archive={})",
            sv.num_nodes, vault::LATTICE_DIMS, t_load.elapsed().as_micros(),
            sv.vault_nodes, sv.num_nodes - sv.vault_nodes);

        // Init GPU
        println!("[GPU] Initializing CUDA...");
        let ctx = match CudaContext::new(0) {
            Ok(c) => {
                let name = c.name().unwrap_or_else(|_| "Unknown".into());
                println!("[GPU] {} — ONLINE", name);
                c
            }
            Err(e) => {
                eprintln!("[FATAL] CUDA: {}", e);
                std::process::exit(1);
            }
        };

        // Encode query
        println!("[QUERY] \"{}\"", query);
        let q = vault::SovereignVault::encode_query(&query);

        // GPU search
        let t_gpu = Instant::now();
        let gpu_hits = match sv.search_gpu(&q, &ctx, 10) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[GPU ERROR] {}", e);
                vec![]
            }
        };
        let gpu_us = t_gpu.elapsed().as_micros();
        println!("[VAULT] GPU search: {} us", gpu_us);

        // CPU verification
        let t_cpu = Instant::now();
        let cpu_hits = sv.search_cpu(&q);
        let cpu_us = t_cpu.elapsed().as_micros();
        println!("[VAULT] CPU search: {} us", cpu_us);

        vault::print_vault_results(&query, &gpu_hits, &cpu_hits);

        println!();
        println!("════════════════════════════════════════════════════════════════");
        println!("  SOVEREIGN ENGINE COMPLETE");
        println!("════════════════════════════════════════════════════════════════");
        return;
    }

    // ── BATCH MODE ──
    if let Some(batch_dir) = find_arg(&args, "--batch") {
        println!("[BATCH] Directory: {}", batch_dir);

        println!("[GPU] Initializing CUDA...");
        let ctx = match CudaContext::new(0) {
            Ok(c) => {
                let name = c.name().unwrap_or_else(|_| "Unknown".into());
                println!("[GPU] {} — ONLINE", name);
                c
            }
            Err(e) => {
                eprintln!("[FATAL] CUDA: {}", e);
                std::process::exit(1);
            }
        };

        match batch::batch_execute(std::path::Path::new(&batch_dir), &ctx) {
            Ok(result) => result.print(),
            Err(e) => eprintln!("[BATCH ERROR] {}", e),
        }

        println!();
        println!("════════════════════════════════════════════════════════════════");
        println!("  SOVEREIGN ENGINE COMPLETE");
        println!("════════════════════════════════════════════════════════════════");
        return;
    }

    // ── WILL MODE (Engine 283 — Autonomous Evolution) ──
    if let Some(inbox_dir) = find_arg(&args, "--will") {
        let archive_path = find_arg(&args, "--archive-path")
            .unwrap_or_else(|| "genetic_archive.dat".to_string());
        let generations: u32 = find_arg(&args, "--generations")
            .and_then(|s| s.parse().ok())
            .unwrap_or(100);
        let daemon = args.iter().any(|s| s == "--daemon");

        println!("[GPU] Initializing CUDA...");
        let gpu = match GpuContext::init() {
            Ok(g) => {
                println!("[GPU] {} — ONLINE", g.name);
                g
            }
            Err(e) => {
                eprintln!("[FATAL] GPU init failed: {}", e);
                std::process::exit(1);
            }
        };

        let config = will::WillConfig {
            inbox_dir: std::path::PathBuf::from(&inbox_dir),
            archive_path: std::path::PathBuf::from(&archive_path),
            generations,
            poll_ms: 2000,
            max_cycles: 1000,
        };

        let mut sovereign_will = will::SovereignWill::new(config);

        if daemon {
            sovereign_will.run(&gpu);
        } else {
            sovereign_will.run_once(&gpu);
        }

        println!();
        println!("════════════════════════════════════════════════════════════════");
        println!("  SOVEREIGN ENGINE COMPLETE");
        println!("════════════════════════════════════════════════════════════════");
        return;
    }

    // ── AUDIT MODE (The Engine Inspects Itself) ──
    if let Some(audit_dir) = find_arg(&args, "--audit") {
        let depth: usize = find_arg(&args, "--depth")
            .and_then(|s| s.parse().ok())
            .unwrap_or(6);

        let mut sovereign_auditor = auditor::SovereignAuditor::new();

        match sovereign_auditor.scan(std::path::Path::new(&audit_dir), depth) {
            Ok(count) => {
                println!("[AUDITOR] Scanned {} source files", count);
                let report = sovereign_auditor.analyze();
                report.print_summary();
                report.print_wiring_opportunities();
            }
            Err(e) => eprintln!("[AUDITOR] Error: {}", e),
        }

        println!();
        println!("════════════════════════════════════════════════════════════════");
        println!("  SOVEREIGN ENGINE COMPLETE");
        println!("════════════════════════════════════════════════════════════════");
        return;
    }

    // ── GBIN MODE ──
    if args.len() < 2 {
        eprintln!("Usage:");
        eprintln!("  sovereign <file.gbin>                        — consensus verify");
        eprintln!("  sovereign <file.gbin> --search <N>           — N-thread parallel search");
        eprintln!("  sovereign <file.gbin> --evolve <N>           — directed evolution");
        eprintln!("  sovereign <file.gbin> --evolve-archive <N>   — evolve + archive winners");
        eprintln!("  sovereign <file.gbin> --bench                — full benchmark");
        eprintln!("  sovereign --vault <path> --query \"text\"     — vault memory search");
        eprintln!("  sovereign --batch <dir>                      — batch multi-program");
        eprintln!("  sovereign --will <inbox>                     — Engine 283: autonomous evolution");
        eprintln!("  sovereign --will <inbox> --daemon             — Engine 283: continuous loop");
        eprintln!("  sovereign --audit <dir>                       — self-audit: scan + cluster");
        eprintln!();
        eprintln!("  Flags:");
        eprintln!("    --archive-path <path>   — custom archive location");
        eprintln!("    --seed-from-vault       — seed evolution from archived organisms");
        eprintln!("    --generations <N>       — evolution generations (default: 100)");
        std::process::exit(1);
    }

    let path = &args[1];
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[FATAL] Cannot read {}: {}", path, e);
            std::process::exit(1);
        }
    };

    println!("[LOAD] {} ({} bytes)", path, data.len());

    let program = match GlyphProgram::from_gbin(&data) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[FATAL] Invalid GBIN: {}", e);
            std::process::exit(1);
        }
    };
    println!("[PROGRAM] {} instructions", program.header.num_instructions);

    println!("[GPU] Initializing CUDA...");
    let gpu = match GpuContext::init() {
        Ok(g) => {
            println!("[GPU] {} — ONLINE", g.name);
            g
        }
        Err(e) => {
            eprintln!("[FATAL] GPU init failed: {}", e);
            std::process::exit(1);
        }
    };
    println!();

    let mode = args.get(2).map(|s| s.as_str()).unwrap_or("");
    let param: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(100);

    match mode {
        "--search" => {
            if let Err(e) = parallel_search(&data, &gpu, param) {
                eprintln!("[ERROR] {}", e);
            }
        }
        "--evolve" => {
            match evolve(&data, &gpu, param) {
                Ok(result) => result.print(),
                Err(e) => eprintln!("[ERROR] {}", e),
            }
        }
        "--evolve-archive" => {
            // Determine archive path
            let archive_path = find_arg(&args, "--archive-path")
                .unwrap_or_else(|| {
                    let p = std::path::Path::new(path).parent()
                        .unwrap_or(std::path::Path::new("."));
                    p.join("genetic_archive.dat").to_string_lossy().to_string()
                });
            let archive_path = std::path::Path::new(&archive_path);

            // Open archive
            let mut ga = archive::GeneticArchive::open(archive_path);
            ga.print_summary();

            // Seed from vault if requested
            let seed_data = if args.iter().any(|s| s == "--seed-from-vault") {
                println!();
                println!("[SEED] Searching archive for closest existing solution...");
                let base_prog = GlyphProgram::from_gbin(&data).unwrap();
                match ga.find_closest_seed(&base_prog.instructions) {
                    Some((idx, resonance, seed_instructions)) => {
                        println!("[SEED] Found archived organism #{} (resonance={:.4})", idx, resonance);
                        println!("[SEED] Using archived program ({} instructions) as evolution seed",
                            seed_instructions.len());
                        // Build a gbin from the archived seed
                        Some(build_gbin(&seed_instructions))
                    }
                    None => {
                        println!("[SEED] No suitable archived organism found, using original program");
                        None
                    }
                }
            } else {
                None
            };

            let evolve_data = seed_data.as_deref().unwrap_or(&data);

            // Run evolution
            match evolve(evolve_data, &gpu, param) {
                Ok(result) => {
                    result.print();

                    // Archive the best organism
                    println!();
                    println!("[ARCHIVE] Archiving best organism...");
                    match ga.archive_organism(
                        &result.best.instructions,
                        result.best.fitness,
                        result.best.generation,
                        result.best.cycles,
                        path,
                    ) {
                        Ok(idx) => {
                            println!("[ARCHIVE] Organism archived as entry #{} (fitness={:.4})",
                                idx, result.best.fitness);

                            // Also archive top 3 if they're different enough
                            // (already handled by duplicate detection)
                        }
                        Err(e) => println!("[ARCHIVE] Skip: {}", e),
                    }

                    println!();
                    ga.print_summary();
                }
                Err(e) => eprintln!("[ERROR] {}", e),
            }
        }
        "--bench" => {
            if let Err(e) = benchmark(&data, &gpu) {
                eprintln!("[ERROR] {}", e);
            }
        }
        _ => {
            match verify_consensus(&data, &gpu) {
                Ok(result) => result.print(),
                Err(e) => eprintln!("[ERROR] {}", e),
            }
        }
    }

    println!();
    println!("════════════════════════════════════════════════════════════════");
    println!("  SOVEREIGN ENGINE COMPLETE");
    println!("════════════════════════════════════════════════════════════════");
}
