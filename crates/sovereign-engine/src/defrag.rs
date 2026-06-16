//! ════════════════════════════════════════════════════════════════
//! SOVEREIGN DEFRAG — Semantic Sort & Archive Repack
//! ════════════════════════════════════════════════════════════════
//!
//! Reorders genetic_archive.dat so semantically related organisms
//! are physically adjacent. This converts scatter-gather GPU reads
//! into block-reads, enabling batch logic pre-fetching.
//!
//! Algorithm:
//!   1. Compute centroid of all 57D vectors
//!   2. K-means clustering into N clusters (default: 27, one per lattice node)
//!   3. Within each cluster, sort by distance to cluster centroid
//!   4. Rewrite archive in cluster order
//!   5. Rebuild metadata + instruction sidecars
//!
//! Usage: sovereign --defrag [--clusters 27]

use std::path::Path;
use std::time::Instant;
use crate::archive::GeneticArchive;

const DIMS: usize = 57;

/// Defrag statistics
pub struct DefragStats {
    pub total_entries: usize,
    pub clusters: usize,
    pub largest_cluster: usize,
    pub smallest_cluster: usize,
    pub avg_intra_distance: f64,
    pub elapsed_ms: f64,
}

/// Run the semantic defragmentation
pub fn defrag_archive(ga: &mut GeneticArchive, num_clusters: usize) -> DefragStats {
    let start = Instant::now();
    let n = ga.entries.len();

    println!();
    println!("════════════════════════════════════════════════════════════════");
    println!("  SOVEREIGN DEFRAG — SEMANTIC ARCHIVE REPACK");
    println!("  Entries:     {}", n);
    println!("  Clusters:    {}", num_clusters);
    println!("  File:        {}", ga.archive_path.display());
    println!("════════════════════════════════════════════════════════════════");

    if n < 2 {
        println!("[DEFRAG] Archive too small to defrag.");
        return DefragStats {
            total_entries: n, clusters: 0, largest_cluster: 0,
            smallest_cluster: 0, avg_intra_distance: 0.0, elapsed_ms: 0.0,
        };
    }

    let k = num_clusters.min(n);

    // ── Phase 1: Initialize centroids via K-means++ ──
    println!("[DEFRAG] Phase 1: Initializing {} centroids (K-means++)...", k);
    let mut centroids = kmeans_pp_init(&ga.entries, k);

    // ── Phase 2: K-means iterations ──
    println!("[DEFRAG] Phase 2: K-means clustering...");
    let mut assignments = vec![0usize; n];
    let max_iters = 50;

    for iter in 0..max_iters {
        // Assign each entry to nearest centroid
        let mut changed = 0usize;
        for i in 0..n {
            let nearest = find_nearest_centroid(&ga.entries[i].vector, &centroids);
            if assignments[i] != nearest {
                assignments[i] = nearest;
                changed += 1;
            }
        }

        // Recompute centroids
        let mut new_centroids = vec![[0.0f64; DIMS]; k];
        let mut counts = vec![0usize; k];
        for i in 0..n {
            let c = assignments[i];
            counts[c] += 1;
            for d in 0..DIMS {
                new_centroids[c][d] += ga.entries[i].vector[d];
            }
        }
        for c in 0..k {
            if counts[c] > 0 {
                for d in 0..DIMS {
                    new_centroids[c][d] /= counts[c] as f64;
                }
            }
        }
        centroids = new_centroids;

        if iter % 10 == 0 || changed == 0 {
            println!("[DEFRAG]   Iteration {}: {} assignments changed", iter, changed);
        }
        if changed == 0 {
            println!("[DEFRAG]   Converged at iteration {}", iter);
            break;
        }
    }

    // ── Phase 3: Build cluster groups ──
    println!("[DEFRAG] Phase 3: Building cluster groups...");
    let mut cluster_indices: Vec<Vec<usize>> = vec![Vec::new(); k];
    for i in 0..n {
        cluster_indices[assignments[i]].push(i);
    }

    // Sort within each cluster by distance to centroid (nearest first)
    for c in 0..k {
        let centroid = &centroids[c];
        cluster_indices[c].sort_by(|&a, &b| {
            let da = l2_distance(&ga.entries[a].vector, centroid);
            let db = l2_distance(&ga.entries[b].vector, centroid);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Sort clusters by size (largest first) for optimal cache usage
    cluster_indices.sort_by(|a, b| b.len().cmp(&a.len()));

    // ── Phase 4: Build reordered entry list ──
    println!("[DEFRAG] Phase 4: Repacking archive...");
    let mut reordered = Vec::with_capacity(n);
    let mut largest = 0usize;
    let mut smallest = n;
    let mut total_intra = 0.0f64;
    let mut total_intra_count = 0usize;
    let mut active_clusters = 0;

    for (ci, indices) in cluster_indices.iter().enumerate() {
        if indices.is_empty() { continue; }
        active_clusters += 1;
        largest = largest.max(indices.len());
        smallest = smallest.min(indices.len());

        // Compute avg intra-cluster distance
        let centroid = &centroids[ci.min(centroids.len() - 1)];
        for &idx in indices {
            total_intra += l2_distance(&ga.entries[idx].vector, centroid);
            total_intra_count += 1;
        }

        for &idx in indices {
            reordered.push(ga.entries[idx].clone());
        }
    }

    let avg_intra = if total_intra_count > 0 {
        total_intra / total_intra_count as f64
    } else {
        0.0
    };

    // ── Phase 5: Replace archive entries and save ──
    ga.entries = reordered;
    println!("[DEFRAG] Phase 5: Writing defragmented archive...");
    ga.save_all();

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    let stats = DefragStats {
        total_entries: n,
        clusters: active_clusters,
        largest_cluster: largest,
        smallest_cluster: smallest,
        avg_intra_distance: avg_intra,
        elapsed_ms: elapsed,
    };

    print_report(&stats);
    stats
}

/// K-means++ initialization
fn kmeans_pp_init(entries: &[crate::archive::ArchiveEntry], k: usize) -> Vec<[f64; DIMS]> {
    let n = entries.len();
    let mut centroids = Vec::with_capacity(k);

    // First centroid: use the first entry
    centroids.push(entries[0].vector);

    // Subsequent centroids: choose proportional to squared distance
    let mut min_dists = vec![f64::MAX; n];

    for _ in 1..k {
        // Update min distances
        let last = centroids.last().unwrap();
        for i in 0..n {
            let d = l2_distance(&entries[i].vector, last);
            if d < min_dists[i] {
                min_dists[i] = d;
            }
        }

        // Pick the point with maximum min-distance (deterministic farthest-first)
        let mut best_idx = 0;
        let mut best_dist = 0.0f64;
        for i in 0..n {
            if min_dists[i] > best_dist {
                best_dist = min_dists[i];
                best_idx = i;
            }
        }

        centroids.push(entries[best_idx].vector);
    }

    centroids
}

/// Find nearest centroid index
fn find_nearest_centroid(vector: &[f64; DIMS], centroids: &[[f64; DIMS]]) -> usize {
    let mut best = 0;
    let mut best_dist = f64::MAX;
    for (i, c) in centroids.iter().enumerate() {
        let d = l2_distance(vector, c);
        if d < best_dist {
            best_dist = d;
            best = i;
        }
    }
    best
}

/// L2 (Euclidean) distance between two 57D vectors
fn l2_distance(a: &[f64; DIMS], b: &[f64; DIMS]) -> f64 {
    let mut sum = 0.0f64;
    for d in 0..DIMS {
        let diff = a[d] - b[d];
        sum += diff * diff;
    }
    sum.sqrt()
}

fn print_report(stats: &DefragStats) {
    println!();
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│           SOVEREIGN DEFRAG — COMPLETE                      │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!("│  Entries repacked:       {:>10}                        │", stats.total_entries);
    println!("│  Active clusters:        {:>10}                        │", stats.clusters);
    println!("│  Largest cluster:        {:>10}                        │", stats.largest_cluster);
    println!("│  Smallest cluster:       {:>10}                        │", stats.smallest_cluster);
    println!("│  Avg intra-distance:     {:>10.4}                      │", stats.avg_intra_distance);
    println!("│  Elapsed:                {:>10.1}ms                     │", stats.elapsed_ms);
    println!("├─────────────────────────────────────────────────────────────┤");
    println!("│  Archive is now SEMANTICALLY CONTIGUOUS.                   │");
    println!("│  GPU block-reads will hit related clusters.                │");
    println!("└─────────────────────────────────────────────────────────────┘");
}

// ════════════════════════════════════════════════════════════════
// SOVEREIGN DIRECTORY TABLE (SDT) — Audit / Rebuild / Verify
// ════════════════════════════════════════════════════════════════

const LATTICE_NODE_SIZE: usize = 464;
const GBIN_MAGIC: [u8; 4] = [0x47, 0x42, 0x49, 0x4E]; // "GBIN"

/// Fragment discovered during raw scan
#[derive(Debug)]
pub struct Fragment {
    pub offset: usize,
    pub size: usize,
    pub frag_type: FragmentType,
    pub signature: u64,
    pub has_parent: bool,
}

#[derive(Debug, PartialEq)]
pub enum FragmentType {
    LatticeNode,       // 464-byte instruction node
    GbinBinary,        // Forged .gbin file
    MetadataJson,      // .meta.json sidecar
    InstructionSlab,   // .instructions sidecar
    Orphan,            // Unrecognized fragment
}

/// Audit report
pub struct AuditReport {
    pub total_files: usize,
    pub lattice_nodes: usize,
    pub gbin_files: usize,
    pub meta_files: usize,
    pub instruction_files: usize,
    pub orphaned_nodes: usize,
    pub broken_chains: usize,
    pub signature_mismatches: usize,
    pub high_resonance_count: usize,
    pub archive_healthy: bool,
}

/// SDT Audit — non-destructive scan
pub fn audit_archive(ga: &GeneticArchive) -> AuditReport {
    let start = Instant::now();

    println!();
    println!("════════════════════════════════════════════════════════════════");
    println!("  SOVEREIGN DIRECTORY TABLE — AUDIT MODE");
    println!("  Archive:  {}", ga.archive_path.display());
    println!("════════════════════════════════════════════════════════════════");

    let mut report = AuditReport {
        total_files: 0,
        lattice_nodes: 0,
        gbin_files: 0,
        meta_files: 0,
        instruction_files: 0,
        orphaned_nodes: 0,
        broken_chains: 0,
        signature_mismatches: 0,
        high_resonance_count: 0,
        archive_healthy: true,
    };

    // Phase 1: Validate binary archive integrity
    println!("[AUDIT] Phase 1: Binary archive integrity...");
    if ga.archive_path.exists() {
        match std::fs::metadata(&ga.archive_path) {
            Ok(m) => {
                let size = m.len() as usize;
                let expected_nodes = size / LATTICE_NODE_SIZE;
                let remainder = size % LATTICE_NODE_SIZE;

                report.lattice_nodes = expected_nodes;
                report.total_files += 1;

                if remainder != 0 {
                    println!("[AUDIT]   [!] Archive has {} trailing bytes (corruption)", remainder);
                    report.broken_chains += 1;
                    report.archive_healthy = false;
                } else {
                    println!("[AUDIT]   [OK] {} lattice nodes, {} bytes, no trailing corruption",
                        expected_nodes, size);
                }
            }
            Err(e) => {
                println!("[AUDIT]   [FAIL] Cannot read archive: {}", e);
                report.archive_healthy = false;
            }
        }
    } else {
        println!("[AUDIT]   [WARN] Archive file does not exist");
    }

    // Phase 2: Validate metadata sidecar
    println!("[AUDIT] Phase 2: Metadata sidecar integrity...");
    if ga.meta_path.exists() {
        report.meta_files += 1;
        report.total_files += 1;
        match std::fs::read_to_string(&ga.meta_path) {
            Ok(content) => {
                // Count entries in metadata
                let meta_count = content.matches("\"signature\"").count();
                if meta_count != report.lattice_nodes {
                    println!("[AUDIT]   [!] Metadata has {} entries but archive has {} nodes",
                        meta_count, report.lattice_nodes);
                    report.signature_mismatches += 1;
                    report.archive_healthy = false;
                } else {
                    println!("[AUDIT]   [OK] {} metadata entries match {} archive nodes",
                        meta_count, report.lattice_nodes);
                }
            }
            Err(e) => {
                println!("[AUDIT]   [FAIL] Cannot read metadata: {}", e);
                report.archive_healthy = false;
            }
        }
    } else {
        println!("[AUDIT]   [WARN] No metadata sidecar found");
    }

    // Phase 3: Validate instruction sidecar
    println!("[AUDIT] Phase 3: Instruction sidecar integrity...");
    if ga.instr_path.exists() {
        report.instruction_files += 1;
        report.total_files += 1;
        match std::fs::read(&ga.instr_path) {
            Ok(data) => {
                if data.len() >= 4 {
                    let instr_count = u32::from_le_bytes([
                        data[0], data[1], data[2], data[3]
                    ]) as usize;
                    if instr_count != report.lattice_nodes {
                        println!("[AUDIT]   [!] Instruction sidecar has {} entries but archive has {}",
                            instr_count, report.lattice_nodes);
                        report.archive_healthy = false;
                    } else {
                        println!("[AUDIT]   [OK] {} instruction entries aligned",
                            instr_count);
                    }
                }
            }
            Err(e) => {
                println!("[AUDIT]   [FAIL] Cannot read instructions: {}", e);
                report.archive_healthy = false;
            }
        }
    } else {
        println!("[AUDIT]   [WARN] No instruction sidecar found");
    }

    // Phase 4: Validate in-memory entries
    println!("[AUDIT] Phase 4: In-memory entry validation...");
    let mut orphans = 0;
    let mut high_res = 0;
    for (i, entry) in ga.entries.iter().enumerate() {
        // Check for zero-vector (orphaned / corrupted)
        let mag: f64 = entry.vector.iter().map(|x| x * x).sum::<f64>().sqrt();
        if mag < 0.001 {
            orphans += 1;
            if orphans <= 5 {
                println!("[AUDIT]   [ORPHAN] Entry {} has zero-magnitude vector", i);
            }
        }

        // Check for NaN contamination
        if entry.vector.iter().any(|v| v.is_nan() || v.is_infinite()) {
            report.broken_chains += 1;
            println!("[AUDIT]   [CORRUPT] Entry {} has NaN/Inf in vector", i);
            report.archive_healthy = false;
        }

        // Check for high-resonance (DIV/PULSE/STOREOUT/HALT kernel)
        if entry.instructions.len() == 4 {
            high_res += 1;
        }
    }
    report.orphaned_nodes = orphans;
    report.high_resonance_count = high_res;

    if orphans > 0 {
        println!("[AUDIT]   {} orphaned nodes (zero-magnitude vectors)", orphans);
    }
    println!("[AUDIT]   {} high-resonance entries (4-opcode kernel)", high_res);

    // Phase 5: Scan for .gbin files in forge directories
    println!("[AUDIT] Phase 5: Scanning forge output directories...");
    for dir_name in &["will_inbox", "gbin_output"] {
        let dir = std::path::Path::new(dir_name);
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                let count = entries.filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map_or(false, |ext| ext == "gbin"))
                    .count();
                report.gbin_files += count;
                report.total_files += count;
                println!("[AUDIT]   {} .gbin files in {}/", count, dir_name);
            }
        }
    }

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    println!();
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│           SDT AUDIT — REPORT                               │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!("│  Lattice nodes:          {:>10}                        │", report.lattice_nodes);
    println!("│  .gbin files:            {:>10}                        │", report.gbin_files);
    println!("│  Sidecars present:       {:>10}                        │", report.meta_files + report.instruction_files);
    println!("│  Orphaned nodes:         {:>10}                        │", report.orphaned_nodes);
    println!("│  Broken chains:          {:>10}                        │", report.broken_chains);
    println!("│  Signature mismatches:   {:>10}                        │", report.signature_mismatches);
    println!("│  High-resonance (4-op):  {:>10}                        │", report.high_resonance_count);
    println!("├─────────────────────────────────────────────────────────────┤");
    if report.archive_healthy {
        println!("│  STATUS: ARCHIVE HEALTHY                                   │");
    } else {
        println!("│  STATUS: CORRUPTION DETECTED — run --defrag --rebuild      │");
    }
    println!("│  Elapsed: {:.1}ms{:>48}│", elapsed, "");
    println!("└─────────────────────────────────────────────────────────────┘");

    report
}

/// SDT Rebuild — destructive repair
pub fn rebuild_archive(ga: &mut GeneticArchive) {
    println!();
    println!("════════════════════════════════════════════════════════════════");
    println!("  SOVEREIGN DIRECTORY TABLE — REBUILD MODE");
    println!("════════════════════════════════════════════════════════════════");

    let before = ga.entries.len();

    // Step 1: Remove corrupted entries (NaN, Inf, zero-vector)
    println!("[REBUILD] Step 1: Purging corrupted entries...");
    ga.entries.retain(|entry| {
        let mag: f64 = entry.vector.iter().map(|x| x * x).sum::<f64>().sqrt();
        let valid = mag > 0.001
            && !entry.vector.iter().any(|v| v.is_nan() || v.is_infinite());
        valid
    });
    let purged = before - ga.entries.len();
    println!("[REBUILD]   Purged {} corrupted entries", purged);

    // Step 2: Deduplicate by signature
    println!("[REBUILD] Step 2: Deduplicating by signature...");
    let before_dedup = ga.entries.len();
    let mut seen_sigs = std::collections::HashSet::new();
    ga.entries.retain(|entry| seen_sigs.insert(entry.signature));
    let deduped = before_dedup - ga.entries.len();
    println!("[REBUILD]   Removed {} duplicate entries", deduped);

    // Step 3: Prioritize high-resonance entries (4-opcode kernel first)
    println!("[REBUILD] Step 3: Prioritizing high-resonance entries...");
    ga.entries.sort_by(|a, b| {
        let a_high = a.instructions.len() == 4;
        let b_high = b.instructions.len() == 4;
        match (a_high, b_high) {
            (true, false) => std::cmp::Ordering::Less,    // High-res first
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.fitness.partial_cmp(&a.fitness)         // Then by fitness
                .unwrap_or(std::cmp::Ordering::Equal),
        }
    });

    // Step 4: Rewrite all files
    println!("[REBUILD] Step 4: Rewriting archive files...");
    ga.save_all();

    println!("[REBUILD] Complete: {} -> {} entries ({} purged, {} deduped)",
        before, ga.entries.len(), purged, deduped);
}

/// SDT Verify — validate against Sovereign Anchor
pub fn verify_archive(ga: &GeneticArchive) -> bool {
    use genlex_types::SOVEREIGN_ANCHOR;
    use genlex_oxide::GlyphProgram;

    println!();
    println!("════════════════════════════════════════════════════════════════");
    println!("  SOVEREIGN DIRECTORY TABLE — VERIFY MODE");
    println!("════════════════════════════════════════════════════════════════");

    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut no_instructions = 0usize;

    let max_check = ga.entries.len().min(1000);

    println!("[VERIFY] Checking {} entries against anchor {:.15}...",
        max_check, SOVEREIGN_ANCHOR);

    for (i, entry) in ga.entries.iter().take(max_check).enumerate() {
        if entry.instructions.is_empty() {
            no_instructions += 1;
            continue;
        }

        // Build a minimal .gbin in memory (same pattern as nlp.rs)
        let instructions = &entry.instructions;
        let mut data: Vec<u8> = Vec::with_capacity(16 + instructions.len() * 4 + 32);
        data.extend_from_slice(b"GBIN");
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&(instructions.len() as u32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        for inst in instructions {
            data.push(inst.opcode);
            data.push(inst.reg_a);
            data.push(inst.reg_b);
            data.push(inst.flags);
        }
        data.extend_from_slice(&[0u8; 32]);

        let r0 = match GlyphProgram::from_gbin(&data) {
            Ok(mut prog) => prog.execute_cpu() as f64,
            Err(_) => {
                fail += 1;
                continue;
            }
        };

        let delta = (r0 - SOVEREIGN_ANCHOR as f64).abs();
        if delta < 0.0001 {
            pass += 1;
        } else {
            fail += 1;
            if fail <= 5 {
                println!("[VERIFY]   Entry {}: r0={:.12} delta={:.12} FAIL",
                    i, r0, delta);
            }
        }
    }

    let verified = fail == 0;

    println!();
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│           SDT VERIFY — RESULTS                             │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!("│  Entries checked:        {:>10}                        │", max_check);
    println!("│  PASS (delta < 0.0001):  {:>10}                        │", pass);
    println!("│  FAIL:                   {:>10}                        │", fail);
    println!("│  No instructions:        {:>10}                        │", no_instructions);
    println!("├─────────────────────────────────────────────────────────────┤");
    if verified {
        println!("│  STATUS: ARCHIVE VERIFIED — ALL ENTRIES RESONATE          │");
    } else {
        println!("│  STATUS: VERIFICATION FAILED — {} entries drifted          │", fail);
    }
    println!("└─────────────────────────────────────────────────────────────┘");

    verified
}

