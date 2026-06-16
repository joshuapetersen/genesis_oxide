//! ════════════════════════════════════════════════════════════════
//! ACE — Adaptive Context Engine (Full Token Stack)
//! ════════════════════════════════════════════════════════════════
//!
//! Ported from:
//!   - ACE_Token_Nexus.py    → 64-bit fingerprint + 27-node lattice
//!   - ACE_Token_Engine.py   → Smart Token generation
//!   - Ace_Token.py          → HMAC-SHA256 signed auth tokens
//!   - Token_Bank_System.py  → ALPHA/BETA/GAMMA bank routing
//!   - Ace.py                → Sovereign hypervisor validation
//!
//! Architecture:
//!   Text → SHA256 → 64-bit fingerprint → 27-node lattice → Smart Token
//!   Smart Token → Bank Router (ALPHA/BETA/GAMMA) → Triangulated Output
//!   All outputs → ACE Hypervisor validation → Accept or Reject

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// ACE constants (from Sovereign_Constants.py)
pub const ACE_64_BIT_MASK: u64 = 0xFFFF_FFFF_FFFF_FFFF; // u64::MAX
pub const ACE_32_BIT_MASK: u32 = 0xFFFF_FFFF;
pub const ACE_16_BIT_MASK: u16 = 0xFFFF;
pub const ACE_THRESHOLD: f32 = 0.0001;
pub const LATTICE_NODES: u64 = 27;

// ════════════════════════════════════════════════════════════════
// ACE TOKEN NEXUS — The Identity Primitive
// ════════════════════════════════════════════════════════════════

/// The unified identity primitive. Converts any text into a
/// 64-bit addressable fingerprint mapped to a 27-node lattice.
pub struct AceTokenNexus;

impl AceTokenNexus {
    /// Generate a 64-bit fingerprint from text (SHA256 → u64 mask)
    pub fn fingerprint(text: &str) -> u64 {
        // SHA256 hash
        let hash = Self::sha256_bytes(text.as_bytes());
        // Take first 8 bytes as u64
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&hash[..8]);
        u64::from_be_bytes(bytes) & ACE_64_BIT_MASK
    }

    /// Map a fingerprint to a 27-node lattice coordinate (1-27)
    pub fn lattice_coordinate(fingerprint: u64) -> u8 {
        ((fingerprint % LATTICE_NODES) + 1) as u8
    }

    /// SHA256 hash of raw bytes
    fn sha256_bytes(data: &[u8]) -> [u8; 32] {
        // Minimal SHA256 implementation (no external deps)
        // Using the k constants and the standard algorithm
        sha256_compute(data)
    }

    /// Generate a bearer token: scope.timestamp.nonce.signature
    pub fn bearer_token(scope: &str, secret: &[u8; 32]) -> String {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let nonce = ts ^ 0xDEAD_BEEF_CAFE_1337; // deterministic nonce from time
        let payload = format!("{}.{}.{:016x}", scope, ts, nonce);

        // HMAC-SHA256
        let sig = hmac_sha256(secret, payload.as_bytes());
        let sig_hex: String = sig.iter().map(|b| format!("{:02x}", b)).collect();

        format!("{}.{}", payload, sig_hex)
    }

    /// Validate a bearer token
    pub fn validate_bearer(token: &str, secret: &[u8; 32]) -> AceValidation {
        let parts: Vec<&str> = token.splitn(4, '.').collect();
        if parts.len() != 4 {
            return AceValidation::rejected("MALFORMED_TOKEN");
        }

        let scope = parts[0];
        let ts_str = parts[1];
        let _nonce = parts[2];
        let signature = parts[3];

        // Reconstruct payload
        let payload = format!("{}.{}.{}", scope, ts_str, _nonce);
        let expected_sig = hmac_sha256(secret, payload.as_bytes());
        let expected_hex: String = expected_sig.iter().map(|b| format!("{:02x}", b)).collect();

        // Timing-safe comparison
        if signature.len() != expected_hex.len() {
            return AceValidation::rejected("INVALID_SIGNATURE");
        }
        let mut diff = 0u8;
        for (a, b) in signature.bytes().zip(expected_hex.bytes()) {
            diff |= a ^ b;
        }
        if diff != 0 {
            return AceValidation::rejected("INVALID_SIGNATURE");
        }

        // Check expiration (default TTL: 86400 seconds)
        if let Ok(ts) = ts_str.parse::<u64>() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if now > ts + 86400 {
                return AceValidation::rejected("TOKEN_EXPIRED");
            }
        }

        AceValidation {
            valid: true,
            scope: scope.to_string(),
            reason: "VERIFIED".to_string(),
        }
    }
}

// ════════════════════════════════════════════════════════════════
// ACE SMART TOKEN — The Coordinate-Based Token
// ════════════════════════════════════════════════════════════════

/// An ACE Smart Token — replaces probabilistic tokens with
/// coordinate-based deterministic tokens.
#[derive(Debug, Clone)]
pub struct AceSmartToken {
    /// Original text
    pub term: String,
    /// 64-bit hash fingerprint (the "address")
    pub fingerprint: u64,
    /// Lattice coordinate 1-27 (the "semantic home")
    pub lattice_node: u8,
    /// Context/intent label
    pub context: String,
    /// Creation timestamp
    pub timestamp: u64,
}

/// ACE Token Engine — generates Smart Tokens from text
pub struct AceTokenEngine {
    /// In-memory token map (the "hippocampus")
    token_map: HashMap<u64, AceSmartToken>,
    /// Maximum tokens before LRU eviction
    max_tokens: usize,
}

impl AceTokenEngine {
    pub fn new() -> Self {
        AceTokenEngine {
            token_map: HashMap::new(),
            max_tokens: 10_000,
        }
    }

    /// Tokenize a phrase into an ACE Smart Token
    pub fn tokenize(&mut self, phrase: &str, context: &str) -> AceSmartToken {
        let fingerprint = AceTokenNexus::fingerprint(phrase);
        let lattice_node = AceTokenNexus::lattice_coordinate(fingerprint);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let token = AceSmartToken {
            term: phrase.to_string(),
            fingerprint,
            lattice_node,
            context: context.to_string(),
            timestamp,
        };

        // LRU eviction if at capacity
        if self.token_map.len() >= self.max_tokens {
            if let Some(&oldest_key) = self.token_map.keys().next() {
                self.token_map.remove(&oldest_key);
            }
        }

        self.token_map.insert(fingerprint, token.clone());
        token
    }

    /// Tokenize a full sentence — each word becomes a Smart Token
    pub fn tokenize_sentence(&mut self, text: &str, context: &str) -> Vec<AceSmartToken> {
        text.split_whitespace()
            .map(|word| self.tokenize(word, context))
            .collect()
    }

    /// Look up a token by fingerprint
    pub fn lookup(&self, fingerprint: u64) -> Option<&AceSmartToken> {
        self.token_map.get(&fingerprint)
    }

    /// Get all tokens in a specific lattice node
    pub fn tokens_at_node(&self, node: u8) -> Vec<&AceSmartToken> {
        self.token_map.values()
            .filter(|t| t.lattice_node == node)
            .collect()
    }

    /// Number of tokens in the map
    pub fn count(&self) -> usize {
        self.token_map.len()
    }
}

// ════════════════════════════════════════════════════════════════
// TOKEN BANK SYSTEM — ALPHA / BETA / GAMMA Routing
// ════════════════════════════════════════════════════════════════

/// Bank type for token routing
#[derive(Debug, Clone, PartialEq)]
pub enum TokenBank {
    /// Information store (non-executable data)
    Alpha,
    /// Tool registry (executable logic)
    Beta,
    /// Metadata/State (inhibitory control)
    Gamma,
}

/// Result of bank triangulation
#[derive(Debug, Clone)]
pub struct TriangulationResult {
    /// Which banks were activated
    pub active_banks: Vec<TokenBank>,
    /// The triangulated status
    pub status: String,
    /// Gamma inhibition state
    pub gamma_active: bool,
    /// Beta execution ready
    pub beta_ready: bool,
}

/// Token Bank System — routes tokens into three banks
pub struct TokenBankSystem {
    alpha: Vec<AceSmartToken>,
    beta: Vec<AceSmartToken>,
    gamma: Vec<AceSmartToken>,
}

/// Keywords that trigger Beta bank (tool/execution)
const BETA_KEYWORDS: &[&str] = &[
    "solve", "calculate", "math", "compute", "execute", "run",
    "image", "video", "research", "canvas", "build", "forge",
    "evolve", "search", "query", "activate", "dispatch",
];

/// Keywords that trigger Gamma bank (identity/state)
const GAMMA_KEYWORDS: &[&str] = &[
    "sovereign", "sarah", "genesis", "anchor", "truth",
    "verify", "audit", "identity", "lock", "seal",
];

impl TokenBankSystem {
    pub fn new() -> Self {
        TokenBankSystem {
            alpha: Vec::new(),
            beta: Vec::new(),
            gamma: Vec::new(),
        }
    }

    /// Ingest a set of ACE tokens and route them to banks
    pub fn ingest(&mut self, tokens: &[AceSmartToken], raw_input: &str) -> TriangulationResult {
        let input_lower = raw_input.to_lowercase();
        let mut active = Vec::new();

        // Route to Gamma (identity/state)
        if GAMMA_KEYWORDS.iter().any(|kw| input_lower.contains(kw)) {
            for t in tokens {
                self.gamma.push(t.clone());
            }
            active.push(TokenBank::Gamma);
        }

        // Route to Beta (tools/execution)
        if BETA_KEYWORDS.iter().any(|kw| input_lower.contains(kw)) {
            for t in tokens {
                self.beta.push(t.clone());
            }
            active.push(TokenBank::Beta);
        }

        // Always route to Alpha (information store)
        for t in tokens {
            self.alpha.push(t.clone());
        }
        active.push(TokenBank::Alpha);

        self.triangulate(&active)
    }

    /// Triangulate: Gamma inhibits Alpha, Beta executes on Alpha's data
    fn triangulate(&self, active: &[TokenBank]) -> TriangulationResult {
        let gamma_active = active.contains(&TokenBank::Gamma);
        let beta_ready = active.contains(&TokenBank::Beta);

        let status = if beta_ready && !self.alpha.is_empty() {
            "LOGIC_DENSITY_STABLE".to_string()
        } else if beta_ready {
            "ERROR_BETA_WITHOUT_ALPHA".to_string()
        } else if gamma_active {
            "GAMMA_INHIBITION_ACTIVE".to_string()
        } else {
            "IDLE_STATE".to_string()
        };

        TriangulationResult {
            active_banks: active.to_vec(),
            status,
            gamma_active,
            beta_ready,
        }
    }

    /// Clear all banks
    pub fn clear(&mut self) {
        self.alpha.clear();
        self.beta.clear();
        self.gamma.clear();
    }

    /// Bank sizes
    pub fn stats(&self) -> (usize, usize, usize) {
        (self.alpha.len(), self.beta.len(), self.gamma.len())
    }
}

// ════════════════════════════════════════════════════════════════
// ACE HYPERVISOR — Validation Enforcement
// ════════════════════════════════════════════════════════════════

/// Validation result from the ACE hypervisor
#[derive(Debug, Clone)]
pub struct AceValidation {
    pub valid: bool,
    pub scope: String,
    pub reason: String,
}

impl AceValidation {
    fn rejected(reason: &str) -> Self {
        AceValidation {
            valid: false,
            scope: String::new(),
            reason: reason.to_string(),
        }
    }
}

/// ACE Hypervisor — wraps N-LP responses with sovereign validation
pub struct AceHypervisor {
    secret: [u8; 32],
}

impl AceHypervisor {
    /// Create with a deterministic secret derived from SOVEREIGN_ANCHOR
    pub fn new() -> Self {
        use genlex_types::SOVEREIGN_ANCHOR;
        let anchor_bytes = SOVEREIGN_ANCHOR.to_le_bytes();
        let mut seed = [0u8; 32];
        // Derive secret from anchor (deterministic)
        for i in 0..32 {
            seed[i] = anchor_bytes[i % 4]
                .wrapping_add(i as u8)
                .wrapping_mul(0x9E)
                .wrapping_add(0x37);
        }
        let secret = sha256_compute(&seed);
        AceHypervisor { secret }
    }

    /// Validate an N-LP result through the ACE hypervisor
    pub fn validate_nlp(&self, result: &super::nlp::NlpResult) -> AceValidation {
        // Rule 1: Must have matches
        if result.matches.is_empty() {
            return AceValidation::rejected("NO_MATCHES");
        }

        // Rule 2: Top result must be verified (anchor delta < threshold)
        if result.drift > ACE_THRESHOLD {
            return AceValidation::rejected(&format!(
                "DRIFT_EXCEEDS_ACE_THRESHOLD: {:.12} > {}",
                result.drift, ACE_THRESHOLD
            ));
        }

        // Rule 3: Must have intent classification
        let intent = super::nlp::SovereignNlp::classify_intent(&result.query);
        if intent.primary == "UNKNOWN" {
            return AceValidation::rejected("NO_INTENT_CLASSIFICATION");
        }

        AceValidation {
            valid: true,
            scope: format!("ACE_VERIFIED:{}:{}", intent.primary, result.matches.len()),
            reason: "SOVEREIGN_TRUTH".to_string(),
        }
    }

    /// Generate a signed receipt for a verified result
    pub fn sign_result(&self, result: &super::nlp::NlpResult) -> String {
        let scope = format!("NLP_RESULT:{}", result.query.len());
        AceTokenNexus::bearer_token(&scope, &self.secret)
    }
}

// ════════════════════════════════════════════════════════════════
// SHA256 — Minimal Implementation (No External Dependencies)
// ════════════════════════════════════════════════════════════════

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256_compute(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    // Pad the message
    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit (64-byte) block
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4], chunk[i * 4 + 1],
                chunk[i * 4 + 2], chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g; g = f; f = e;
            e = d.wrapping_add(temp1);
            d = c; c = b; b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut result = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        result[i*4..i*4+4].copy_from_slice(&val.to_be_bytes());
    }
    result
}

fn hmac_sha256(key: &[u8; 32], message: &[u8]) -> [u8; 32] {
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..32 {
        ipad[i] ^= key[i];
        opad[i] ^= key[i];
    }

    // inner = SHA256(ipad || message)
    let mut inner_input = ipad.to_vec();
    inner_input.extend_from_slice(message);
    let inner = sha256_compute(&inner_input);

    // outer = SHA256(opad || inner)
    let mut outer_input = opad.to_vec();
    outer_input.extend_from_slice(&inner);
    sha256_compute(&outer_input)
}

/// Print ACE token diagnostics
pub fn print_ace_report(
    tokens: &[AceSmartToken],
    triangulation: &TriangulationResult,
    validation: &AceValidation,
) {
    println!();
    println!("┌─ ACE TOKEN REPORT ──────────────────────────────────────────┐");
    println!("│  Tokens:       {:4}                                        │", tokens.len());

    // Lattice distribution
    let mut lattice_dist = [0u32; 28]; // 1-27
    for t in tokens {
        lattice_dist[t.lattice_node as usize] += 1;
    }
    let max_node = (1..28).max_by_key(|&i| lattice_dist[i]).unwrap_or(1);
    println!("│  Lattice peak: node {:2} ({} tokens)                         │",
        max_node, lattice_dist[max_node]);

    // Bank status
    println!("├─ BANK TRIANGULATION ────────────────────────────────────────┤");
    let banks: Vec<String> = triangulation.active_banks.iter()
        .map(|b| format!("{:?}", b)).collect();
    println!("│  Active:       {:46}│", banks.join(", "));
    println!("│  Status:       {:46}│", triangulation.status);
    println!("│  Gamma:        {:46}│",
        if triangulation.gamma_active { "INHIBITION_ACTIVE" } else { "DORMANT" });
    println!("│  Beta:         {:46}│",
        if triangulation.beta_ready { "EXECUTION_READY" } else { "IDLE" });

    // Validation
    println!("├─ ACE HYPERVISOR ────────────────────────────────────────────┤");
    if validation.valid {
        println!("│  ✓ ACE VALIDATED — {}  │", validation.scope);
    } else {
        println!("│  ✗ ACE REJECTED — {}  │", validation.reason);
    }
    println!("│  Reason:       {:46}│", validation.reason);
    println!("└─────────────────────────────────────────────────────────────┘");
}
