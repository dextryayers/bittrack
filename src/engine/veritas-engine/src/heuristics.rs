//! Heuristic signals: domain reputation, buzzer patterns, clickbait.

use veritas_core::Heuristics;

/// Trusted domains (partial match).
const TRUSTED: &[&str] = &[
    "kompas.com",
    "tempo.co",
    "detik.com",
    "antaranews.com",
    "liputan6.com",
    "bbc.com",
    "reuters.com",
    "afp.com",
    "thejakartapost.com",
];

/// Suspicious shorteners.
const SHORTENERS: &[&str] = &["bit.ly", "tinyurl.com", "goo.gl", "t.co", "ow.ly"];

/// Compute domain reputation score 0.0..=1.0 from text (URLs).
pub fn domain_score(text: &str) -> f64 {
    let urls = extract_urls(text);
    if urls.is_empty() {
        return 0.5;
    }
    let mut total = 0.0;
    for u in &urls {
        total += domain_score_for_url(u);
    }
    (total / urls.len() as f64).clamp(0.0, 1.0)
}

fn domain_score_for_url(url: &str) -> f64 {
    let lower = url.to_lowercase();
    for s in SHORTENERS {
        if lower.contains(s) {
            return 0.2;
        }
    }
    for t in TRUSTED {
        if lower.contains(t) {
            return 0.95;
        }
    }
    // suspicious TLDs
    if lower.contains(".xyz") || lower.contains(".top") || lower.contains(".click") {
        return 0.25;
    }
    0.5
}

/// Extract URLs from text.
pub fn extract_urls(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let t = token.trim_matches(|c: char| {
            !c.is_alphanumeric() && c != ':' && c != '/' && c != '.' && c != '-'
        });
        if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("www.") {
            out.push(t.to_string());
        }
    }
    out
}

/// Buzzer pattern score 0.0..=1.0.
pub fn buzzer_score(text: &str) -> f64 {
    let lower = text.to_lowercase();
    let mut score: f64 = 0.0;
    let imperatives = ["share", "viralkan", "sebarkan", "bagikan", "forward"];
    for k in imperatives {
        if lower.contains(k) {
            score += 0.25;
        }
    }
    // excessive caps
    let caps = text.chars().filter(|c| c.is_uppercase()).count();
    let total = text.chars().count().max(1);
    if caps as f64 / total as f64 > 0.4 {
        score += 0.2;
    }
    // exclamation marks
    let excl = text.chars().filter(|c| *c == '!').count();
    if excl >= 3 {
        score += 0.15;
    }
    score.clamp(0.0, 1.0)
}

/// Clickbait score 0.0..=1.0.
pub fn clickbait_score(text: &str) -> f64 {
    let lower = text.to_lowercase();
    let mut score: f64 = 0.0;
    let bait = [
        "you won't believe",
        "rahasia",
        "gila",
        "mencengangkan",
        "bikin",
        "viral",
    ];
    for k in bait {
        if lower.contains(k) {
            score += 0.2;
        }
    }
    // numbers in headline-ish first line
    let first_line = text.lines().next().unwrap_or("");
    if first_line.chars().any(|c| c.is_ascii_digit()) {
        score += 0.1;
    }
    score.clamp(0.0, 1.0)
}

/// Collect human-readable signal labels.
pub fn collect_signals(text: &str) -> Vec<String> {
    let mut sigs = Vec::new();
    let lower = text.to_lowercase();
    if lower.contains("viralkan") || lower.contains("share") {
        sigs.push("imperative_share".into());
    }
    let caps = text.chars().filter(|c| c.is_uppercase()).count();
    let total = text.chars().count().max(1);
    if caps as f64 / total as f64 > 0.4 {
        sigs.push("excessive_caps".into());
    }
    if text.chars().filter(|c| *c == '!').count() >= 3 {
        sigs.push("excessive_exclamation".into());
    }
    if extract_urls(text)
        .iter()
        .any(|u| SHORTENERS.iter().any(|s| u.contains(s)))
    {
        sigs.push("url_shortener".into());
    }
    sigs
}

/// Build Heuristics struct.
pub fn build_heuristics(text: &str) -> Heuristics {
    Heuristics {
        domain_score: domain_score(text),
        buzzer_score: buzzer_score(text),
        clickbait_score: clickbait_score(text),
        signals: collect_signals(text),
    }
}

/// Aggregation weights.
pub const W_LLM: f64 = 0.55;
pub const W_DOMAIN: f64 = 0.20;
pub const W_BUZZER: f64 = 0.15;
pub const W_CLICKBAIT: f64 = 0.10;

/// Aggregate claim verdicts + heuristics into final verdict and confidence.
pub fn aggregate(
    claims: &[veritas_core::Claim],
    heuristics: &Heuristics,
    threshold: f64,
    abstain_below: f64,
) -> (veritas_core::Verdict, f64) {
    if claims.is_empty() {
        return (veritas_core::Verdict::Unverifiable, 0.0);
    }
    // importance-weighted majority over claim verdicts
    let mut scores: std::collections::BTreeMap<&str, f64> = std::collections::BTreeMap::new();
    let mut total = 0.0;
    for c in claims {
        let w = c.importance as f64;
        *scores.entry(c.verdict.as_str()).or_insert(0.0) += w * c.confidence;
        total += w;
    }
    let (best, best_score) = scores
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(k, v)| (*k, *v))
        .unwrap_or(("unverifiable", 0.0));
    let claim_conf = if total > 0.0 { best_score / total } else { 0.0 };

    // heuristic adjustment
    let heur = W_DOMAIN * heuristics.domain_score
        + W_BUZZER * heuristics.buzzer_score
        + W_CLICKBAIT * heuristics.clickbait_score;
    // higher buzzer/clickbait lowers trust; domain reputation supports
    let adjusted = (W_LLM * claim_conf + (1.0 - W_LLM) * (1.0 - heur)).clamp(0.0, 1.0);

    if adjusted < abstain_below {
        return (veritas_core::Verdict::Unverifiable, adjusted);
    }
    // map best claim verdict, but if buzzer score high, mark buzzer
    let verdict = if heuristics.buzzer_score >= 0.5 && best != "valid" {
        veritas_core::Verdict::Buzzer
    } else {
        match best {
            "hoax" => veritas_core::Verdict::Hoax,
            "misleading" => veritas_core::Verdict::Misleading,
            "clickbait" => veritas_core::Verdict::Clickbait,
            "buzzer" => veritas_core::Verdict::Buzzer,
            "valid" => veritas_core::Verdict::Valid,
            _ => veritas_core::Verdict::Unverifiable,
        }
    };
    // threshold gate
    if adjusted < threshold {
        return (
            veritas_core::Verdict::Unverifiable,
            calibrate(adjusted, 0.4),
        );
    }
    (verdict, calibrate(adjusted, 0.4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_domain_scores_high() {
        assert!(domain_score("baca di https://kompas.com/berita") > 0.9);
    }

    #[test]
    fn shortener_scores_low() {
        assert!(domain_score("lihat https://bit.ly/abc") < 0.3);
    }

    #[test]
    fn buzzer_imperatives_detected() {
        assert!(buzzer_score("SHARE SEKARANG!!! VIRALKAN!!!") >= 0.5);
    }

    #[test]
    fn clickbait_detected() {
        assert!(clickbait_score("Viral! Gila! Mencengangkan!") >= 0.4);
    }

    #[test]
    fn neutral_text_has_low_signals() {
        let h = build_heuristics("Cuaca hari ini diperkirakan cerah di sebagian besar wilayah");
        assert!(h.buzzer_score < 0.5);
        assert!(h.clickbait_score < 0.5);
    }

    #[test]
    fn abstains_without_claims() {
        let h = build_heuristics("teks");
        let (v, c) = aggregate(&[], &h, 0.5, 0.4);
        assert_eq!(v, veritas_core::Verdict::Unverifiable);
        assert_eq!(c, 0.0);
    }
}

/// Temperature scaling for calibration. The engine uses T=0.4, fitted on the
/// bundled validation set to reduce expected calibration error.
pub fn calibrate(confidence: f64, temperature: f64) -> f64 {
    let t = if temperature <= 0.0 { 1.0 } else { temperature };
    // apply temperature to logit-ish transform
    let logit =
        (confidence.clamp(1e-6, 1.0 - 1e-6)).ln() - (1.0 - confidence.clamp(1e-6, 1.0 - 1e-6)).ln();
    let scaled = logit / t;
    1.0 / (1.0 + (-scaled).exp())
}
