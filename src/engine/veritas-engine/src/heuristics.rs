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
    let Some(host) = host_of(url) else {
        return 0.5;
    };
    if SHORTENERS
        .iter()
        .any(|s| host == *s || host.ends_with(&format!(".{s}")))
    {
        return 0.2;
    }
    if TRUSTED
        .iter()
        .any(|t| host == *t || host.ends_with(&format!(".{t}")))
    {
        return 0.95;
    }
    if [".xyz", ".top", ".click"].iter().any(|tld| host.ends_with(tld)) {
        return 0.25;
    }
    0.5
}

fn host_of(raw: &str) -> Option<String> {
    let normalized = if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_string()
    } else {
        format!("https://{}", raw.trim_start_matches("www."))
    };
    reqwest::Url::parse(&normalized)
        .ok()?
        .host_str()
        .map(|h| h.to_ascii_lowercase())
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
        .filter_map(|u| host_of(u))
        .any(|host| SHORTENERS.iter().any(|s| host == *s || host.ends_with(&format!(".{s}"))))
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
    fn lookalike_domain_is_not_trusted() {
        assert!(domain_score("baca di https://kompas.com.evil.example/berita") < 0.6);
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
}
