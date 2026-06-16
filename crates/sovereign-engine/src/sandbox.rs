//! ════════════════════════════════════════════════════════════════
//! SOVEREIGN SANDBOX — Real-time Telemetry for the N-LP Dashboard
//! ════════════════════════════════════════════════════════════════
//!
//! Writes structured JSON telemetry after each N-LP query.
//! The HTML dashboard polls this file for real-time visualization.

use std::fs;
use std::path::Path;

use crate::nlp::NlpResult;
use crate::ace::{AceSmartToken, TriangulationResult, AceValidation};

/// Full telemetry snapshot for a single query
pub struct SandboxTelemetry {
    pub query: String,
    pub intent_primary: String,
    pub intent_secondary: String,
    pub intent_action: String,
    pub intent_confidence: f32,
    pub query_vector: Vec<f32>,
    pub nlp_result: NlpResult,
    pub ace_tokens: Vec<AceSmartToken>,
    pub triangulation: TriangulationResult,
    pub validation: AceValidation,
    pub ace_receipt: String,
    pub query_ms: f64,
    pub ace_ms: f64,
    pub archive_size: usize,
}

impl SandboxTelemetry {
    /// Write telemetry as JSON to a file
    pub fn write_json(&self, path: &Path) {
        let mut json = String::with_capacity(8192);
        json.push_str("{\n");

        // Query metadata
        json.push_str(&format!("  \"query\": {:?},\n", self.query));
        json.push_str(&format!("  \"intent_primary\": {:?},\n", self.intent_primary));
        json.push_str(&format!("  \"intent_secondary\": {:?},\n", self.intent_secondary));
        json.push_str(&format!("  \"intent_action\": {:?},\n", self.intent_action));
        json.push_str(&format!("  \"intent_confidence\": {},\n", self.intent_confidence));
        json.push_str(&format!("  \"verified\": {},\n", self.nlp_result.verified));
        json.push_str(&format!("  \"drift\": {},\n", self.nlp_result.drift));
        json.push_str(&format!("  \"query_ms\": {:.3},\n", self.query_ms));
        json.push_str(&format!("  \"ace_ms\": {:.3},\n", self.ace_ms));
        json.push_str(&format!("  \"total_ms\": {:.3},\n", self.query_ms + self.ace_ms));
        json.push_str(&format!("  \"archive_size\": {},\n", self.archive_size));
        json.push_str(&format!("  \"timestamp\": {},\n",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()));

        // Query vector (57D)
        json.push_str("  \"query_vector\": [");
        for (i, v) in self.query_vector.iter().enumerate() {
            if i > 0 { json.push(','); }
            json.push_str(&format!("{:.6}", v));
        }
        json.push_str("],\n");

        // Top-K matches
        json.push_str("  \"matches\": [\n");
        for (i, m) in self.nlp_result.matches.iter().enumerate() {
            if i > 0 { json.push_str(",\n"); }
            json.push_str(&format!(
                "    {{\"rank\":{},\"archive_index\":{},\"resonance\":{:.6},\"r0_output\":{:.12},\"anchor_delta\":{:.12},\"instructions\":{},\"generation\":{},\"fitness\":{:.6}}}",
                i + 1,
                m.archive_index,
                m.resonance,
                m.r0_output,
                m.anchor_delta,
                m.instruction_count,
                m.generation,
                m.evolved_fitness
            ));
        }
        json.push_str("\n  ],\n");

        // ACE Tokens
        json.push_str("  \"ace_tokens\": [\n");
        for (i, t) in self.ace_tokens.iter().enumerate() {
            if i > 0 { json.push_str(",\n"); }
            json.push_str(&format!(
                "    {{\"term\":{:?},\"fingerprint\":\"{:016x}\",\"lattice_node\":{},\"context\":{:?}}}",
                t.term, t.fingerprint, t.lattice_node, t.context
            ));
        }
        json.push_str("\n  ],\n");

        // Lattice distribution (27 nodes)
        json.push_str("  \"lattice_distribution\": [");
        let mut dist = [0u32; 28];
        for t in &self.ace_tokens {
            dist[t.lattice_node as usize] += 1;
        }
        for i in 1..28 {
            if i > 1 { json.push(','); }
            json.push_str(&format!("{}", dist[i]));
        }
        json.push_str("],\n");

        // Bank triangulation
        let banks: Vec<String> = self.triangulation.active_banks.iter()
            .map(|b| format!("{:?}", b)).collect();
        json.push_str(&format!("  \"banks_active\": {:?},\n", banks));
        json.push_str(&format!("  \"bank_status\": {:?},\n", self.triangulation.status));
        json.push_str(&format!("  \"gamma_active\": {},\n", self.triangulation.gamma_active));
        json.push_str(&format!("  \"beta_ready\": {},\n", self.triangulation.beta_ready));

        // ACE Hypervisor
        json.push_str(&format!("  \"ace_valid\": {},\n", self.validation.valid));
        json.push_str(&format!("  \"ace_scope\": {:?},\n", self.validation.scope));
        json.push_str(&format!("  \"ace_reason\": {:?},\n", self.validation.reason));
        json.push_str(&format!("  \"ace_receipt\": {:?}\n", self.ace_receipt));

        json.push_str("}\n");

        if let Err(e) = fs::write(path, &json) {
            eprintln!("[SANDBOX] Failed to write telemetry: {}", e);
        }
    }
}
