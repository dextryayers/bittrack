//! veritas-core: domain types, schemas, and pure logic shared by the engine.
//!
//! This crate has no I/O and no unsafe code. It is the single source of truth
//! for the JSON contracts exchanged across the FFI boundary.

#![deny(unsafe_code)]
#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Verdict categories. A result may carry multiple tags but has one primary verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Fabricated or demonstrably false.
    Hoax,
    /// Contains truth but framed deceptively.
    Misleading,
    /// Headline exaggerates or misrepresents body.
    Clickbait,
    /// Coordinated amplification pattern.
    Buzzer,
    /// Consistent with verified sources.
    Valid,
    /// Insufficient evidence to decide.
    Unverifiable,
}

impl Verdict {
    /// Parse a verdict from its stable string form.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value.trim().to_ascii_lowercase().as_str() {
            "hoax" => Verdict::Hoax,
            "misleading" => Verdict::Misleading,
            "clickbait" => Verdict::Clickbait,
            "buzzer" => Verdict::Buzzer,
            "valid" => Verdict::Valid,
            "unverifiable" => Verdict::Unverifiable,
            _ => return None,
        })
    }

    /// Stable string form used in JSON and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Hoax => "hoax",
            Verdict::Misleading => "misleading",
            Verdict::Clickbait => "clickbait",
            Verdict::Buzzer => "buzzer",
            Verdict::Valid => "valid",
            Verdict::Unverifiable => "unverifiable",
        }
    }
}

/// Request payload from the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeRequest {
    /// Raw article text. Required unless `url`, `poster_text`, or `comments` is
    /// present.
    pub text: Option<String>,
    /// Article URL. Required unless `text`, `poster_text`, or `comments` is
    /// present.
    pub url: Option<String>,
    /// Reader comments to include as additional context. When no other input is
    /// present, the comments become the claim under review.
    #[serde(default)]
    pub comments: Option<String>,
    /// Text extracted from an uploaded poster image.
    #[serde(default)]
    pub poster_text: Option<String>,
    /// Language hint.
    pub lang_hint: String,
    /// Options.
    #[serde(default)]
    pub options: Option<AnalyzeOptions>,
}

/// Options for analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalyzeOptions {
    /// Minimum model confidence 0.0..=1.0 required before a decisive verdict is
    /// accepted. Lower confidence results are reported as `unverifiable`.
    pub threshold: f64,
    /// Include heuristic signals in output. Signals are never used as a verdict.
    pub include_heuristics: bool,
    /// Maximum claims to accept from the model.
    pub max_claims: u32,
    /// Fetch live fact-check and encyclopedia evidence before inference.
    pub online_evidence: bool,
    /// Run a second inference with a different seed and demote the verdict when
    /// the two passes disagree. This doubles latency and is opt-in.
    pub strict_verification: bool,
}

impl Default for AnalyzeOptions {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            include_heuristics: true,
            max_claims: 8,
            online_evidence: true,
            strict_verification: false,
        }
    }
}

/// A single atomic claim extracted from the text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    /// Stable id within this analysis.
    pub id: u32,
    /// Claim text.
    pub text: String,
    /// Claim type. Serialized as `type` to match the UI contract.
    #[serde(rename = "type")]
    pub claim_type: String,
    /// Importance 1..=5.
    pub importance: u8,
    /// Per-claim verdict.
    pub verdict: Verdict,
    /// Per-claim confidence 0.0..=1.0.
    pub confidence: f64,
    /// Short reasoning.
    pub reasoning: String,
    /// Evidence ids supporting this claim.
    pub evidence_ids: Vec<u32>,
}

/// Retrieved evidence snippet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// Stable id within this analysis.
    pub id: u32,
    /// Snippet text.
    pub text: String,
    /// Source name.
    pub source: String,
    /// Source URL.
    pub url: String,
    /// Publication date (ISO 8601).
    pub date: String,
    /// Retrieval score 0.0..=1.0.
    pub score: f64,
    /// Evidence kind: `fact_check`, `encyclopedia`, or `web`.
    pub kind: String,
}

/// Heuristic signals.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Heuristics {
    /// Domain reputation 0.0..=1.0.
    pub domain_score: f64,
    /// Buzzer pattern score 0.0..=1.0.
    pub buzzer_score: f64,
    /// Clickbait score 0.0..=1.0.
    pub clickbait_score: f64,
    /// Human-readable signal labels.
    pub signals: Vec<String>,
}

/// Audit metadata for reproducibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Audit {
    /// Model id used.
    pub model: String,
    /// Prompt version.
    pub prompt_version: String,
    /// Engine version.
    pub engine_version: String,
    /// Unix timestamp ms.
    pub timestamp_ms: i64,
    /// SHA-256 of normalized input.
    pub input_hash: String,
}

/// Final analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeResponse {
    /// Primary verdict.
    pub verdict: Verdict,
    /// Model confidence 0.0..=1.0 that the verdict is correct.
    pub confidence: f64,
    /// Model-estimated probability 0..=100 that the core claim is true.
    pub truth_percentage: u8,
    /// Model-estimated probability 0..=100 that the text was generated by AI.
    pub ai_percentage: u8,
    /// One-paragraph summary.
    pub summary: String,
    /// Extracted claims.
    pub claims: Vec<Claim>,
    /// Retrieved evidence.
    pub evidence: Vec<Evidence>,
    /// Heuristic signals.
    pub heuristics: Heuristics,
    /// Manipulation patterns identified by the model.
    pub red_flags: Vec<String>,
    /// Non-fatal problems encountered while producing this result.
    pub warnings: Vec<String>,
    /// Audit metadata.
    pub audit: Audit,
}

/// Progress event streamed during analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// Stage name.
    pub stage: String,
    /// 0.0..=1.0.
    pub percent: f64,
    /// Human-readable message.
    pub message: String,
}

/// Error payload returned across FFI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    /// Machine-readable code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

/// Compute SHA-256 hex of bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode(hasher.finalize().as_slice())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Validate an AnalyzeRequest. Returns Ok or a list of problems.
pub fn validate_request(req: &AnalyzeRequest) -> Result<(), Vec<String>> {
    let mut problems = Vec::new();
    let len = |value: &Option<String>| -> usize {
        value.as_ref().map(|v| v.trim().chars().count()).unwrap_or(0)
    };
    let has_text = len(&req.text) > 0;
    let has_url = len(&req.url) > 0;
    let has_poster = len(&req.poster_text) > 0;
    let has_comments = len(&req.comments) > 0;
    if !has_text && !has_url && !has_poster && !has_comments {
        problems.push("text, url, poster_text, or comments must be provided".to_string());
    }
    if len(&req.text) > 200_000 {
        problems.push("text exceeds 200000 characters".to_string());
    }
    if len(&req.comments) > 20_000 {
        problems.push("comments exceed 20000 characters".to_string());
    }
    if len(&req.poster_text) > 20_000 {
        problems.push("poster_text exceeds 20000 characters".to_string());
    }
    if let Some(url) = req.url.as_ref().map(|u| u.trim()).filter(|u| !u.is_empty()) {
        if url.len() > 2048 {
            problems.push("url exceeds 2048 characters".to_string());
        }
        let lower = url.to_ascii_lowercase();
        if !(lower.starts_with("https://") || lower.starts_with("http://")) {
            problems.push("url must start with http:// or https://".to_string());
        }
    }
    let opts = req.options.clone().unwrap_or_default();
    if !(0.0..=1.0).contains(&opts.threshold) {
        problems.push("threshold must be in [0,1]".to_string());
    }
    if opts.max_claims == 0 || opts.max_claims > 32 {
        problems.push("max_claims must be in 1..=32".to_string());
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_roundtrip() {
        for v in [
            Verdict::Hoax,
            Verdict::Misleading,
            Verdict::Clickbait,
            Verdict::Buzzer,
            Verdict::Valid,
            Verdict::Unverifiable,
        ] {
            let s = serde_json::to_string(&v).unwrap();
            let back: Verdict = serde_json::from_str(&s).unwrap();
            assert_eq!(v, back);
        }
    }

    #[test]
    fn validate_rejects_empty() {
        let req = AnalyzeRequest {
            text: None,
            url: None,
            comments: None,
            poster_text: None,
            lang_hint: "auto".into(),
            options: Some(AnalyzeOptions::default()),
        };
        assert!(validate_request(&req).is_err());
    }

    #[test]
    fn validate_accepts_text() {
        let req = AnalyzeRequest {
            text: Some("hello".into()),
            url: None,
            comments: None,
            poster_text: None,
            lang_hint: "auto".into(),
            options: Some(AnalyzeOptions::default()),
        };
        assert!(validate_request(&req).is_ok());
    }

    #[test]
    fn validate_accepts_poster_only() {
        let req = AnalyzeRequest {
            text: None,
            url: None,
            comments: None,
            poster_text: Some("VIRAL vaksin microchip".into()),
            lang_hint: "id".into(),
            options: Some(AnalyzeOptions::default()),
        };
        assert!(validate_request(&req).is_ok());
    }

    #[test]
    fn validate_accepts_comments_only() {
        let req = AnalyzeRequest {
            text: None,
            url: None,
            comments: Some("Kata teman saya vaksin ini berbahaya.".into()),
            poster_text: None,
            lang_hint: "id".into(),
            options: Some(AnalyzeOptions::default()),
        };
        assert!(validate_request(&req).is_ok());
    }

    #[test]
    fn validate_rejects_non_http_url() {
        let req = AnalyzeRequest {
            text: None,
            url: Some("file:///etc/passwd".into()),
            comments: None,
            poster_text: None,
            lang_hint: "id".into(),
            options: Some(AnalyzeOptions::default()),
        };
        assert!(validate_request(&req).is_err());
    }

    #[test]
    fn sha256_known() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
