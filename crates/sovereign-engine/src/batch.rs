//! ════════════════════════════════════════════════════════════════
//! BATCH EXECUTOR — N Programs, One Launch, "2000 Brains"
//! ════════════════════════════════════════════════════════════════
//!
//! Loads N different .gbin programs from a directory,
//! pads them to equal instruction length, packs into one GPU buffer,
//! and launches N threads where each thread runs a different program.

use std::path::{Path, PathBuf};
use cudarc::driver::{CudaContext, CudaSlice, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use std::sync::Arc;
use std::time::Instant;

use genlex_oxide::GlyphProgram;
use genlex_types::{GlyphInst, SOVEREIGN_ANCHOR};

const BATCH_VM_PTX: &str = include_str!("batch_vm.ptx");

/// A loaded program ready for batch execution
struct BatchProgram {
    path: PathBuf,
    instructions: Vec<GlyphInst>,
    cpu_result: f32,
}

/// Batch execution results
pub struct BatchResult {
    pub results: Vec<(String, f32, f32)>, // (filename, gpu_result, cpu_result)
    pub consensus_count: usize,
    pub total_programs: usize,
    pub kernel_us: u128,
}

impl BatchResult {
    pub fn print(&self) {
        println!("┌─────────────────────────────────────────────────────┐");
        println!("│         SOVEREIGN BATCH EXECUTION REPORT            │");
        println!("├─────────────────────────────────────────────────────┤");
        println!("│  Programs: {:<8}  Kernel: {} us             │",
            self.total_programs,
            format!("{}", self.kernel_us));
        println!("├─────────────────────────────────────────────────────┤");

        for (name, gpu, cpu) in &self.results {
            let delta = (gpu - cpu).abs();
            let status = if delta < 0.01 { "OK" } else { "!!" };
            let short_name = if name.len() > 20 { &name[..20] } else { name };
            println!("│  {:<22} GPU={:>12.4}  CPU={:>12.4} {} │",
                short_name, gpu, cpu, status);
        }

        println!("├─────────────────────────────────────────────────────┤");
        let pct = if self.total_programs > 0 {
            self.consensus_count as f64 / self.total_programs as f64 * 100.0
        } else { 0.0 };

        if pct >= 100.0 {
            println!("│  ✓ UNANIMOUS CONSENSUS — {}/{} programs agree    │",
                self.consensus_count, self.total_programs);
        } else if pct >= 80.0 {
            println!("│  ~ STRONG CONSENSUS — {}/{} ({:.0}%) agree           │",
                self.consensus_count, self.total_programs, pct);
        } else {
            println!("│  ✗ NO CONSENSUS — {}/{} ({:.0}%) agree               │",
                self.consensus_count, self.total_programs, pct);
        }
        println!("└─────────────────────────────────────────────────────┘");
    }
}

/// Execute a batch of .gbin programs on GPU
pub fn batch_execute(
    dir: &Path,
    ctx: &Arc<CudaContext>,
) -> Result<BatchResult, String> {
    // Scan directory for .gbin files
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("Cannot read directory: {}", e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "gbin").unwrap_or(false))
        .collect();

    if entries.is_empty() {
        return Err("No .gbin files found in directory".into());
    }

    println!("[BATCH] Found {} .gbin programs", entries.len());

    // Load all programs
    let mut programs: Vec<BatchProgram> = Vec::new();
    for path in &entries {
        let data = std::fs::read(path)
            .map_err(|e| format!("Read {}: {}", path.display(), e))?;
        match GlyphProgram::from_gbin(&data) {
            Ok(mut prog) => {
                let cpu_result = prog.execute_cpu();
                let instructions = prog.instructions.clone();
                programs.push(BatchProgram {
                    path: path.clone(),
                    instructions,
                    cpu_result,
                });
            }
            Err(e) => {
                eprintln!("[BATCH] Skip {}: {}", path.display(), e);
            }
        }
    }

    if programs.is_empty() {
        return Err("No valid .gbin programs loaded".into());
    }

    let num_progs = programs.len();
    println!("[BATCH] Loaded {} valid programs", num_progs);

    // Find max instruction length and pad all programs
    let max_len = programs.iter().map(|p| p.instructions.len()).max().unwrap_or(0);
    println!("[BATCH] Max instruction length: {} (padded with HALT)", max_len);

    // Pack all programs contiguously: program[0] || program[1] || ... || program[N-1]
    let mut packed: Vec<u32> = Vec::with_capacity(num_progs * max_len);
    for prog in &programs {
        for inst in &prog.instructions {
            packed.push(u32::from_le_bytes(inst.to_bytes()));
        }
        // Pad with HALT (0xFF000000)
        for _ in prog.instructions.len()..max_len {
            packed.push(0x000000FF); // HALT in LE format
        }
    }

    // Compile batch PTX
    let stream = ctx.default_stream();
    let exe_dir = std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let ptx_path = exe_dir.join("batch_vm.ptx");
    let cubin_path = exe_dir.join("batch_vm.cubin");

    std::fs::write(&ptx_path, BATCH_VM_PTX)
        .map_err(|e| format!("PTX write: {}", e))?;

    let ptxas = std::process::Command::new("ptxas")
        .args(["-arch=sm_89", "-O3", "-o"])
        .arg(&cubin_path)
        .arg(&ptx_path)
        .output()
        .map_err(|e| format!("ptxas: {}", e))?;

    if !ptxas.status.success() {
        return Err(format!("ptxas: {}", String::from_utf8_lossy(&ptxas.stderr)));
    }

    let cubin = std::fs::read(&cubin_path)
        .map_err(|e| format!("CUBIN read: {}", e))?;
    let module = ctx.load_module(Ptx::from_binary(cubin))
        .map_err(|e| format!("Module: {}", e))?;
    let func = module.load_function("evolve_vm")
        .map_err(|e| format!("Function: {}", e))?;

    // Upload and execute
    let d_programs: CudaSlice<u32> = stream.clone_htod(&packed)
        .map_err(|e| format!("Upload: {}", e))?;
    let mut d_output: CudaSlice<f32> = stream.alloc_zeros(num_progs)
        .map_err(|e| format!("Alloc: {}", e))?;

    let inst_len = max_len as u32;
    let num_progs_u32 = num_progs as u32;
    let block = if num_progs_u32 >= 256 { 256 } else { num_progs_u32 };
    let grid = (num_progs_u32 + block - 1) / block;

    let cfg = LaunchConfig {
        grid_dim: (grid, 1, 1),
        block_dim: (block, 1, 1),
        shared_mem_bytes: 0,
    };

    let t_kernel = Instant::now();
    unsafe {
        stream.launch_builder(&func)
            .arg(&d_programs)
            .arg(&inst_len)
            .arg(&mut d_output)
            .arg(&num_progs_u32)
            .launch(cfg)
            .map_err(|e| format!("Launch: {}", e))?;
    }

    let gpu_results = stream.clone_dtoh(&d_output)
        .map_err(|e| format!("Readback: {}", e))?;
    let kernel_us = t_kernel.elapsed().as_micros();

    // Build results
    let mut results = Vec::new();
    let mut consensus_count = 0;

    for (i, prog) in programs.iter().enumerate() {
        let gpu_val = gpu_results[i];
        let cpu_val = prog.cpu_result;
        let filename = prog.path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("program_{}", i));

        if (gpu_val - cpu_val).abs() < 0.01 || 
           (gpu_val.abs() > 0.001 && cpu_val.abs() > 0.001 &&
            ((gpu_val - cpu_val).abs() / gpu_val.abs().max(cpu_val.abs())) < 0.05) {
            consensus_count += 1;
        }

        results.push((filename, gpu_val, cpu_val));
    }

    Ok(BatchResult {
        results,
        consensus_count,
        total_programs: num_progs,
        kernel_us,
    })
}
