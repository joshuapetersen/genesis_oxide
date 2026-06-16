//! ════════════════════════════════════════════════════════════════
//! SOVEREIGN VAULT — GPU Memory Search
//! ════════════════════════════════════════════════════════════════
//!
//! Loads the BrainScar vault (15,330 nodes × 57D × 464 bytes)
//! into GPU VRAM and searches the entire vault in one kernel launch.
//!
//! Data path:
//!   brain_scar_vault.dat → f64[15330×57] → f32[15330×57] → VRAM
//!   query string → GeometricTokenizer → f32[57] → VRAM
//!   GPU kernel: 15,330 threads compute euclidean distance in parallel
//!   Result: top-K memories ranked by resonance score

use std::path::Path;
use cudarc::driver::{CudaContext, CudaSlice, CudaModule, CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use std::sync::Arc;
use std::time::Instant;

use genlex_types::SOVEREIGN_ANCHOR;

/// BrainScar vault constants (must match Python BrainScar_Bridge.py)
pub const LATTICE_POINTS: usize = 15_330;
pub const LATTICE_DIMS: usize = 57;
pub const LATTICE_NODE_SIZE: usize = 464; // 57 f64 + 1 u64 = 58 * 8
pub const PHI: f32 = 1.618033988;
pub const HEARTBEAT: f32 = SOVEREIGN_ANCHOR;
pub const SUPER_SYMMETRY: f32 = 1.1;

/// PTX kernel source for vault search
const VAULT_SEARCH_PTX: &str = include_str!("vault_search.ptx");

/// A loaded vault in host memory (f32 format)
pub struct SovereignVault {
    /// Flattened memory vectors: [node0_dim0, node0_dim1, ..., node0_dim56, node1_dim0, ...]
    pub memories: Vec<f32>,
    /// Number of valid nodes loaded (vault + archive)
    pub num_nodes: usize,
    /// Number of nodes from the original vault (before archive)
    pub vault_nodes: usize,
}

/// A search result
#[derive(Debug, Clone)]
pub struct VaultHit {
    pub index: usize,
    pub resonance: f32,
}

impl SovereignVault {
    /// Load from brain_scar_vault.dat
    pub fn load(path: &Path) -> Result<Self, String> {
        let data = std::fs::read(path)
            .map_err(|e| format!("Cannot read vault: {}", e))?;

        let expected = LATTICE_POINTS * LATTICE_NODE_SIZE;
        if data.len() < expected {
            return Err(format!("Vault too small: {} bytes (expected {})", data.len(), expected));
        }

        let num_nodes = data.len() / LATTICE_NODE_SIZE;
        let actual_nodes = num_nodes.min(LATTICE_POINTS);

        // Extract 57 f64 values per node, convert to f32
        let mut memories = Vec::with_capacity(actual_nodes * LATTICE_DIMS);

        for i in 0..actual_nodes {
            let base = i * LATTICE_NODE_SIZE;
            for d in 0..LATTICE_DIMS {
                let offset = base + d * 8;
                if offset + 8 <= data.len() {
                    let val = f64::from_le_bytes([
                        data[offset], data[offset+1], data[offset+2], data[offset+3],
                        data[offset+4], data[offset+5], data[offset+6], data[offset+7],
                    ]);
                    memories.push(val as f32);
                } else {
                    memories.push(0.0);
                }
            }
        }

        Ok(SovereignVault {
            memories,
            num_nodes: actual_nodes,
            vault_nodes: actual_nodes,
        })
    }

    /// Load vault + archive union
    /// Archive nodes appear after vault nodes in the search results
    pub fn load_with_archive(vault_path: &Path, archive_path: &Path) -> Result<Self, String> {
        let mut vault = Self::load(vault_path)?;
        let vault_count = vault.num_nodes;

        if archive_path.exists() {
            let archive_data = std::fs::read(archive_path)
                .map_err(|e| format!("Cannot read archive: {}", e))?;

            let archive_nodes = archive_data.len() / LATTICE_NODE_SIZE;
            println!("[VAULT] Loading {} archive nodes from {}",
                archive_nodes, archive_path.display());

            for i in 0..archive_nodes {
                let base = i * LATTICE_NODE_SIZE;
                for d in 0..LATTICE_DIMS {
                    let offset = base + d * 8;
                    if offset + 8 <= archive_data.len() {
                        let val = f64::from_le_bytes([
                            archive_data[offset], archive_data[offset+1],
                            archive_data[offset+2], archive_data[offset+3],
                            archive_data[offset+4], archive_data[offset+5],
                            archive_data[offset+6], archive_data[offset+7],
                        ]);
                        vault.memories.push(val as f32);
                    } else {
                        vault.memories.push(0.0);
                    }
                }
            }

            vault.num_nodes += archive_nodes;
            println!("[VAULT] Total: {} nodes ({} vault + {} archive)",
                vault.num_nodes, vault_count, archive_nodes);
        }

        Ok(vault)
    }

    /// Encode a text query into a 57D vector using GeometricTokenizer
    /// (matches Python BrainScar_Bridge.py bundle_sentence)
    pub fn encode_query(text: &str) -> [f32; LATTICE_DIMS] {
        let mut bundle = [0.0f32; LATTICE_DIMS];

        for (pos, ch) in text.to_lowercase().chars().enumerate() {
            let seed = ch as u32 as f32 * HEARTBEAT;

            // xyz[0..27]: sin(seed + i * PHI + pos)
            for i in 0..27 {
                bundle[i] += (seed + i as f32 * PHI + pos as f32).sin();
            }
            // einstein[27..39]: cos(seed + i * PHI)
            for i in 0..12 {
                bundle[27 + i] += (seed + i as f32 * PHI).cos();
            }
            // polarity[39..51]: sin(seed * PHI + i)
            for i in 0..12 {
                bundle[39 + i] += (seed * PHI + i as f32).sin();
            }
            // phi[51..56]: PHI^(-i)
            for i in 0..5 {
                bundle[51 + i] += PHI.powi(-(i as i32));
            }
            // architect_anchor[56]
            bundle[56] += SUPER_SYMMETRY;
        }

        // Normalize onto 57D hypersphere
        let mag: f32 = bundle.iter().map(|x| x * x).sum::<f32>().sqrt();
        if mag > 0.0 {
            for v in bundle.iter_mut() {
                *v /= mag;
            }
        }

        bundle
    }

    /// Search the vault on GPU
    pub fn search_gpu(
        &self,
        query: &[f32; LATTICE_DIMS],
        ctx: &Arc<CudaContext>,
        top_k: usize,
    ) -> Result<Vec<VaultHit>, String> {
        let stream = ctx.default_stream();

        // Compile PTX to CUBIN
        let exe_dir = std::env::current_exe()
            .unwrap_or_default()
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let ptx_path = exe_dir.join("vault_search.ptx");
        let cubin_path = exe_dir.join("vault_search.cubin");

        std::fs::write(&ptx_path, VAULT_SEARCH_PTX)
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
        let func = module.load_function("vault_search")
            .map_err(|e| format!("Function: {}", e))?;

        // Upload data
        let d_memories: CudaSlice<f32> = stream.clone_htod(&self.memories)
            .map_err(|e| format!("Upload memories: {}", e))?;
        let d_query: CudaSlice<f32> = stream.clone_htod(query.as_slice())
            .map_err(|e| format!("Upload query: {}", e))?;
        let mut d_output: CudaSlice<f32> = stream.alloc_zeros(self.num_nodes)
            .map_err(|e| format!("Alloc output: {}", e))?;

        let num_nodes = self.num_nodes as u32;
        let dims = LATTICE_DIMS as u32;

        let block = 256u32;
        let grid = (num_nodes + block - 1) / block;

        let cfg = LaunchConfig {
            grid_dim: (grid, 1, 1),
            block_dim: (block, 1, 1),
            shared_mem_bytes: 0,
        };

        let t_launch = Instant::now();
        unsafe {
            stream.launch_builder(&func)
                .arg(&d_memories)
                .arg(&d_query)
                .arg(&mut d_output)
                .arg(&num_nodes)
                .arg(&dims)
                .launch(cfg)
                .map_err(|e| format!("Launch: {}", e))?;
        }

        let scores = stream.clone_dtoh(&d_output)
            .map_err(|e| format!("Readback: {}", e))?;
        let kernel_us = t_launch.elapsed().as_micros();

        println!("[VAULT] Kernel: {} us ({} nodes × {}D)", kernel_us, self.num_nodes, LATTICE_DIMS);

        // Find top-K
        let mut hits: Vec<VaultHit> = scores.iter().enumerate()
            .map(|(i, &s)| VaultHit { index: i, resonance: s })
            .collect();
        hits.sort_by(|a, b| b.resonance.partial_cmp(&a.resonance).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(top_k);

        Ok(hits)
    }

    /// CPU search for verification
    pub fn search_cpu(&self, query: &[f32; LATTICE_DIMS]) -> Vec<VaultHit> {
        let mut hits: Vec<VaultHit> = (0..self.num_nodes)
            .map(|i| {
                let base = i * LATTICE_DIMS;
                let mut dist_sq = 0.0f32;
                for d in 0..LATTICE_DIMS {
                    let diff = self.memories[base + d] - query[d];
                    dist_sq += diff * diff;
                }
                VaultHit {
                    index: i,
                    resonance: 1.0 / (1.0 + dist_sq.sqrt()),
                }
            })
            .collect();
        hits.sort_by(|a, b| b.resonance.partial_cmp(&a.resonance).unwrap_or(std::cmp::Ordering::Equal));
        hits
    }
}

/// Print vault search results
pub fn print_vault_results(query: &str, hits: &[VaultHit], cpu_hits: &[VaultHit]) {
    // Intent map (matches Python BrainScar_Bridge.py)
    let intent_map: &[(usize, &str, &str)] = &[
        (0, "--strike", "Repository Strike"),
        (1, "--mmlu", "Truth Calibration"),
        (2, "--saa", "Agentic Audit"),
        (3, "--predict", "Chain-of-Thought"),
        (4, "--titan", "Benchmark"),
        (5, "--swarm", "Agent Swarm"),
        (6, "--synthesize", "Synthesize"),
        (7, "--cybernetic", "Cybernetic"),
        (8, "--dream", "Dream State"),
        (9, "--mesh", "Sovereign Mesh"),
        (10, "--ouroboros", "Ouroboros"),
    ];

    println!("┌─────────────────────────────────────────────────────┐");
    println!("│           SOVEREIGN VAULT SEARCH REPORT             │");
    println!("├─────────────────────────────────────────────────────┤");
    println!("│  Query: {:<44}│", &query[..query.len().min(44)]);
    println!("├─────────────────────────────────────────────────────┤");

    // Show top hits
    for (rank, hit) in hits.iter().enumerate().take(10) {
        let label = intent_map.iter()
            .find(|(idx, _, _)| *idx == hit.index)
            .map(|(_, cmd, desc)| format!("{} ({})", cmd, desc))
            .unwrap_or_else(|| format!("memory[{:>5}]", hit.index));
        let trunc = if label.len() > 34 { &label[..34] } else { &label };
        println!("│  {:>2}. {:<34} {:.6} │", rank + 1, trunc, hit.resonance);
    }

    // Consensus check between GPU and CPU top result
    println!("├─────────────────────────────────────────────────────┤");
    if !hits.is_empty() && !cpu_hits.is_empty() {
        let gpu_top = hits[0].index;
        let cpu_top = cpu_hits[0].index;
        let delta = (hits[0].resonance - cpu_hits[0].resonance).abs();

        if gpu_top == cpu_top && delta < 0.01 {
            println!("│  ✓ GPU/CPU CONSENSUS — SOVEREIGN TRUTH              │");
            println!("│    GPU top: node[{}] = {:.6}                     │", gpu_top,
                format!("{:.6}", hits[0].resonance));
            println!("│    CPU top: node[{}] = {:.6}                     │", cpu_top,
                format!("{:.6}", cpu_hits[0].resonance));
        } else {
            println!("│  ✗ GPU/CPU DIVERGED                                 │");
            println!("│    GPU: node[{}] = {:.6}                         │", gpu_top, hits[0].resonance);
            println!("│    CPU: node[{}] = {:.6}                         │", cpu_top, cpu_hits[0].resonance);
        }
    }
    println!("└─────────────────────────────────────────────────────┘");
}
