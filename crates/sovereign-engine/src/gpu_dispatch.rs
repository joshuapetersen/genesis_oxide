//! ════════════════════════════════════════════════════════════════
//! GPU DISPATCH — Parallel Evolution Evaluation on CUDA
//! ════════════════════════════════════════════════════════════════
//!
//! Launches the evolve_vm PTX kernel to evaluate N glyph programs
//! simultaneously on GPU. Each CUDA thread runs one complete program
//! and writes its r0 result to the output buffer.
//!
//! Performance: 
//!   CPU: 32 programs × 10K cycles = ~320K ops serial
//!   GPU: 32 programs × 10K cycles = ~320K ops parallel = ~100x speedup

use cudarc::driver::{CudaContext, CudaSlice, CudaModule, CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use genlex_types::GlyphInst;
use std::sync::Arc;

/// GPU evaluation context for batch program execution
pub struct GpuEvolver {
    pub ctx: Arc<CudaContext>,
    pub evolve_func: CudaFunction,
    pub name: String,
}

impl GpuEvolver {
    /// Initialize GPU and load the evolution kernel
    pub fn init(evolve_ptx: &str) -> Result<Self, String> {
        let ctx = CudaContext::new(0)
            .map_err(|e| format!("CUDA init: {}", e))?;
        let name = ctx.name().unwrap_or_else(|_| "Unknown GPU".into());

        // Compile PTX to CUBIN via ptxas
        let exe_dir = std::env::current_exe()
            .unwrap_or_default()
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let ptx_path = exe_dir.join("evolve_vm_ignite.ptx");
        let cubin_path = exe_dir.join("evolve_vm_ignite.cubin");

        std::fs::write(&ptx_path, evolve_ptx)
            .map_err(|e| format!("PTX write: {}", e))?;

        let ptxas_out = std::process::Command::new("ptxas")
            .args(["-arch=sm_89", "-O3", "-o"])
            .arg(&cubin_path)
            .arg(&ptx_path)
            .output()
            .map_err(|e| format!("ptxas: {}", e))?;

        if !ptxas_out.status.success() {
            return Err(format!("ptxas error: {}",
                String::from_utf8_lossy(&ptxas_out.stderr)));
        }

        let cubin = std::fs::read(&cubin_path)
            .map_err(|e| format!("CUBIN read: {}", e))?;
        let module = ctx.load_module(Ptx::from_binary(cubin))
            .map_err(|e| format!("Module load: {}", e))?;
        let evolve_func = module.load_function("evolve_vm")
            .map_err(|e| format!("Function load: {}", e))?;

        println!("[GPU] {} — Evolution kernel loaded", name);
        Ok(GpuEvolver { ctx, evolve_func, name })
    }

    /// Evaluate N programs in parallel on GPU
    /// 
    /// Each program must have exactly `inst_len` instructions (padded with NOPs).
    /// Returns a Vec<f32> with one result per program (r0 after execution).
    pub fn evaluate_batch(
        &self,
        programs: &[Vec<GlyphInst>],
        inst_len: usize,
    ) -> Result<Vec<f32>, String> {
        let n = programs.len();
        if n == 0 {
            return Ok(vec![]);
        }

        let stream = self.ctx.default_stream();

        // Flatten all programs into a single u32 buffer
        // Each instruction is 4 bytes = 1 u32
        let mut flat_programs: Vec<u32> = Vec::with_capacity(n * inst_len);
        for prog in programs {
            for i in 0..inst_len {
                if i < prog.len() {
                    let bytes = prog[i].to_bytes();
                    flat_programs.push(u32::from_le_bytes(bytes));
                } else {
                    // Pad with NOP (0x00000000)
                    flat_programs.push(0);
                }
            }
        }

        // Upload to GPU
        let d_programs: CudaSlice<u32> = stream.clone_htod(&flat_programs)
            .map_err(|e| format!("GPU upload programs: {}", e))?;
        let mut d_output: CudaSlice<f32> = stream.alloc_zeros(n)
            .map_err(|e| format!("GPU alloc output: {}", e))?;

        // Launch kernel: one thread per program
        let block_size = 256u32;
        let grid_size = ((n as u32) + block_size - 1) / block_size;
        let cfg = LaunchConfig {
            grid_dim: (grid_size, 1, 1),
            block_dim: (block_size, 1, 1),
            shared_mem_bytes: 0,
        };

        let inst_len_u32 = inst_len as u32;
        let num_progs_u32 = n as u32;

        unsafe {
            stream.launch_builder(&self.evolve_func)
                .arg(&d_programs)
                .arg(&inst_len_u32)
                .arg(&mut d_output)
                .arg(&num_progs_u32)
                .launch(cfg)
                .map_err(|e| format!("Kernel launch: {}", e))?;
        }

        // Download results
        let results = stream.clone_dtoh(&d_output)
            .map_err(|e| format!("GPU download: {}", e))?;

        Ok(results)
    }
}
