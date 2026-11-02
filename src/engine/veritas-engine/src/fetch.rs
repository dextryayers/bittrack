//! Network access for the engine: article extraction and fact-check search.
//!
//! Everything here is user-initiated, rate-limited by construction (a fixed
//! small number of requests per analysis) and bounded by hard timeouts. Only
//! public HTTP(S) hosts are accepted; loopback, private, and link-local
//! addresses are rejected to keep the engine from being used as an SSRF pivot.

use std::collections::BTreeSet;
use std::io::Read;
use std::net::{IpAddr, ToSocketAddrs};
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use reqwest::blocking::Client;
use reqwest::Url;
use scraper::{ElementRef, Html, Selector};

/// Upper bound on the amount of HTML read from a single page.
const MAX_PAGE_BYTES: u64 = 2_500_000;
/// Upper bound on extracted article text.
const MAX_ARTICLE_CHARS: usize = 24_000;
/// HTTP timeouts.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// A fetched and extracted article.
#[derive(Debug, Clone, Default)]
pub struct FetchedPage {
    /// Final URL after redirects.
    pub url: String,
    /// Page title.
    pub title: String,
    /// Extracted article body.
    pub text: String,
    /// Site name or host.
    pub site: String,
    /// Publication date as reported by the page.
    pub published: String,
}

/// A search result used as evidence.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Result title.
    pub title: String,
    /// Result URL.
    pub url: String,
    /// Result snippet.
    pub snippet: String,
    /// Site name.
    pub source: String,
    /// Publication date as reported by the source.
    pub date: String,
    /// `fact_check`, `encyclopedia`, or `web`.
    pub kind: String,
    /// Lexical relevance to the query, 0.0..=1.0.
    pub score: f64,
}

/// Errors produced by network retrieval.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// The URL is malformed or targets a non-public host.
    #[error("url not allowed: {0}")]
    UrlNotAllowed(String),
    /// The HTTP request failed.
    #[error("http error: {0}")]
    Http(String),
    /// The response was not HTML.
    #[error("unsupported content type: {0}")]
    ContentType(String),
    /// The page contained no usable article text.
    #[error("no article text extracted from {0}")]
    EmptyArticle(String),
}

fn client() -> Result<Client, FetchError> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::ACCEPT_LANGUAGE,
        reqwest::header::HeaderValue::from_static("id-ID,id;q=0.9,en;q=0.8"),
    );
    Client::builder()
        .user_agent(concat!(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ",
            "(KHTML, like Gecko) Chrome/126.0 Safari/537.36 Veritas/",
            env!("CARGO_PKG_VERSION")
        ))
        .default_headers(headers)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || validate_public_url(attempt.url().as_str()).is_err()
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| FetchError::Http(e.to_string()))
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.octets()[0] == 0)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let segments = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80)
        }
    }
}

/// Reject hosts that resolve to a non-public address. This blocks hostnames
/// that point at loopback or private ranges, not just literal IPs.
fn ensure_public_resolution(url: &Url) -> Result<(), FetchError> {
    let host = url
        .host_str()
        .ok_or_else(|| FetchError::UrlNotAllowed("missing host".into()))?;
    let port = url.port_or_known_default().unwrap_or(443);
    let mut addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| FetchError::UrlNotAllowed(format!("cannot resolve {host}: {e}")))?
        .peekable();
    if addrs.peek().is_none() {
        return Err(FetchError::UrlNotAllowed(format!(
            "cannot resolve {host}: no addresses"
        )));
    }
    for addr in addrs {
        if !is_public_ip(addr.ip()) {
            return Err(FetchError::UrlNotAllowed(format!(
                "{host} resolves to non-public address {}",
                addr.ip()
            )));
        }
    }
    Ok(())
}

/// Reject anything that is not a public HTTP(S) endpoint.
fn validate_public_url(raw: &str) -> Result<Url, FetchError> {
    let url = Url::parse(raw).map_err(|e| FetchError::UrlNotAllowed(e.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(FetchError::UrlNotAllowed(format!(
            "scheme {} is not allowed",
            url.scheme()
        )));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(FetchError::UrlNotAllowed(
            "credentials in URL are not allowed".into(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| FetchError::UrlNotAllowed("missing host".into()))?
        .to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".local") || host.ends_with(".internal") {
        return Err(FetchError::UrlNotAllowed(host));
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_public_ip(ip) {
            return Err(FetchError::UrlNotAllowed(host));
        }
    }
    Ok(url)
}

fn select(selector: &str) -> Selector {
    Selector::parse(selector).expect("static selector is valid")
}

fn text_of(el: ElementRef<'_>) -> String {
    el.text().collect::<Vec<_>>().join(" ").trim().to_string()
}

fn meta_content(doc: &Html, selector: &str) -> Option<String> {
    let sel = select(selector);
    doc.select(&sel)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Fetch a URL and extract the main article text.
pub fn fetch_page(raw_url: &str) -> Result<FetchedPage, FetchError> {
    let url = validate_public_url(raw_url)?;
    ensure_public_resolution(&url)?;
    let resp = client()?
        .get(url.clone())
        .send()
        .map_err(|e| FetchError::Http(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(FetchError::Http(format!("status {status}")));
    }
    let final_url = resp.url().clone();
    ensure_public_resolution(&final_url)?;
    let final_url = final_url.to_string();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !content_type.is_empty()
        && !content_type.contains("text/html")
        && !content_type.contains("application/xhtml")
        && !content_type.contains("text/plain")
    {
        return Err(FetchError::ContentType(content_type));
    }
    let mut bytes = Vec::new();
    resp.take(MAX_PAGE_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|e| FetchError::Http(e.to_string()))?;
    let html = String::from_utf8_lossy(&bytes).to_string();

    let page = if content_type.contains("text/plain") {
        FetchedPage {
            url: final_url.clone(),
            title: url.path().rsplit('/').next().unwrap_or("").to_string(),
            text: truncate_chars(html.trim(), MAX_ARTICLE_CHARS),
            site: url.host_str().unwrap_or("").to_string(),
            published: String::new(),
        }
    } else {
        extract_article(&html, &final_url)
    };
    if page.text.trim().chars().count() < 80 {
        return Err(FetchError::EmptyArticle(final_url));
    }
    Ok(page)
}

/// Extract title, date, site, and paragraph text from an HTML document.
#[must_use]
pub fn extract_article(html: &str, page_url: &str) -> FetchedPage {
    let doc = Html::parse_document(html);
    let title = meta_content(&doc, "meta[property='og:title']")
        .or_else(|| meta_content(&doc, "meta[name='twitter:title']"))
        .or_else(|| {
            doc.select(&select("h1"))
                .next()
                .map(text_of)
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            doc.select(&select("title"))
                .next()
                .map(text_of)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_default();
    let site = meta_content(&doc, "meta[property='og:site_name']")
        .or_else(|| Url::parse(page_url).ok().and_then(|u| u.host_str().map(str::to_string)))
        .unwrap_or_default();
    let published = meta_content(&doc, "meta[property='article:published_time']")
        .or_else(|| meta_content(&doc, "meta[name='date']"))
        .or_else(|| meta_content(&doc, "meta[name='pubdate']"))
        .or_else(|| {
            doc.select(&select("time[datetime]"))
                .next()
                .and_then(|el| el.value().attr("datetime"))
                .map(str::to_string)
        })
        .unwrap_or_default();

    let paragraph_sel = select("p");
    let roots: Vec<ElementRef<'_>> = ["article", "main", "[role='main']", "body"]
        .iter()
        .filter_map(|s| doc.select(&select(s)).next())
        .collect();
    let mut chunks: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut total_chars = 0usize;
    for root in roots {
        for p in root.select(&paragraph_sel) {
            let text = text_of(p);
            let len = text.chars().count();
            if len < 40 || len > 4000 {
                continue;
            }
            let key = text.to_lowercase();
            if seen.insert(key) {
                chunks.push(text);
                total_chars += len + 1;
            }
            if total_chars >= MAX_ARTICLE_CHARS {
                break;
            }
        }
        if total_chars >= 400 {
            break;
        }
    }
    if chunks.is_empty() {
        // Last resort: body text without scripts/styles, which scraper already
        // excludes from `text()`.
        if let Some(body) = doc.select(&select("body")).next() {
            let text = text_of(body);
            if text.chars().count() >= 80 {
                chunks.push(text);
            }
        }
    }
    FetchedPage {
        url: page_url.to_string(),
        title,
        text: truncate_chars(&chunks.join("\n\n"), MAX_ARTICLE_CHARS),
        site,
        published,
    }
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

pub(crate) const STOPWORDS: &[&str] = &[
    "yang", "dan", "untuk", "dengan", "dari", "ini", "itu", "akan", "pada", "juga", "tidak",
    "dalam", "adalah", "sebagai", "telah", "sudah", "bisa", "dapat", "oleh", "atau", "karena",
    "para", "kata", "lebih", "sangat", "agar", "saat", "setelah", "sebelum", "kepada", "tentang",
    "the", "and", "for", "with", "from", "this", "that", "have", "has", "are", "was", "were",
    "will", "would", "can", "could", "not", "mengandung", "menyembunyikan", "pemerintah",
    "faktanya", "sebelum", "dihapus", "asli", "bukti", "sebarkan", "tolong", "penting",
    "wajib", "mohon", "virall", "viral",
];

fn tokens(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 4 && !STOPWORDS.contains(t))
        .map(str::to_string)
        .collect()
}

fn containment(query: &BTreeSet<String>, text: &BTreeSet<String>) -> f64 {
    if query.is_empty() {
        return 0.0;
    }
    let hits = query.intersection(text).count();
    hits as f64 / query.len() as f64
}

fn relevance(query: &BTreeSet<String>, title: &str, snippet: &str) -> f64 {
    let title_score = containment(query, &tokens(title));
    let snippet_score = containment(query, &tokens(snippet));
    (0.65 * title_score + 0.35 * snippet_score).clamp(0.0, 1.0)
}

fn compact_query(text: &str, max_words: usize) -> String {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-'))
        .filter(|w| w.chars().count() >= 3)
        .take(max_words)
        .collect::<Vec<_>>()
        .join(" ")
}

const TRUSTED_HOSTS: &[&str] = &[
    "turnbackhoax.id",
    "mafindo.or.id",
    "cekfakta.tempo.co",
    "tempo.co",
    "kompas.com",
    "kompas.id",
    "antaranews.com",
    "liputan6.com",
    "detik.com",
    "cnnindonesia.com",
    "cnbcindonesia.com",
    "tirto.id",
    "katadata.co.id",
    "bbc.com",
    "reuters.com",
    "apnews.com",
    "kemkes.go.id",
    "kemenkes.go.id",
    "who.int",
    "unicef.org",
    "id.wikipedia.org",
    "kominfo.go.id",
    "komdigi.go.id",
    "bi.go.id",
    "setkab.go.id",
    "presidenri.go.id",
    "jakartapost.com",
];

fn host_allowed(host: &str) -> bool {
    TRUSTED_HOSTS
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
}

fn classify_source(url: &Url) -> &'static str {
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    let path = url.path().to_ascii_lowercase();
    if host.contains("turnbackhoax")
        || host.contains("mafindo")
        || host.contains("cekfakta")
        || path.contains("cekfakta")
        || path.contains("cek-fakta")
    {
        "fact_check"
    } else if host.ends_with("wikipedia.org") {
        "encyclopedia"
    } else {
        "web"
    }
}

fn source_name(url: &Url) -> String {
    url.host_str()
        .unwrap_or("")
        .trim_start_matches("www.")
        .to_string()
}

/// Decode Bing's `ck/a?...&u=a1<base64url>` redirect into the target URL.
fn decode_bing_target(href: &str) -> Option<String> {
    let url = Url::parse(href).ok()?;
    let host = url.host_str()?;
    if host != "bing.com" && !host.ends_with(".bing.com") {
        return None;
    }
    let encoded = url
        .query_pairs()
        .find(|(k, _)| k == "u")
        .map(|(_, v)| v.into_owned())?;
    let payload = encoded.strip_prefix("a1")?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.as_bytes()).ok()?;
    let target = String::from_utf8(bytes).ok()?;
    Url::parse(&target).ok()?;
    Some(target)
}

/// Search Bing for fact-check coverage. Returns only trusted-host results.
pub fn bing_search(query: &str, limit: usize) -> Result<Vec<SearchHit>, FetchError> {
    let encoded = urlencoding::encode(query);
    let url = format!(
        "https://www.bing.com/search?q={encoded}&count={}&mkt=id-ID&setlang=id&cc=ID",
        limit.clamp(1, 20)
    );
    let html = client()?
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| FetchError::Http(e.to_string()))?
        .text()
        .map_err(|e| FetchError::Http(e.to_string()))?;
    let doc = Html::parse_document(&html);
    let item_sel = select("li.b_algo");
    let link_sel = select("h2 a[href]");
    let p_sel = select("p");
    let date_sel = select("span.news_dt");
    let query_tokens = tokens(query);
    let mut hits = Vec::new();
    for item in doc.select(&item_sel) {
        let Some(link) = item.select(&link_sel).next() else {
            continue;
        };
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        let Some(target) = decode_bing_target(href).or_else(|| {
            href.starts_with("http")
                .then(|| href.to_string())
                .filter(|h| Url::parse(h).is_ok())
        }) else {
            continue;
        };
        let Ok(url) = Url::parse(&target) else {
            continue;
        };
        let host = url.host_str().unwrap_or("").to_ascii_lowercase();
        if !host_allowed(&host) {
            continue;
        }
        let title = text_of(link);
        let snippet = item
            .select(&p_sel)
            .next()
            .map(text_of)
            .unwrap_or_default();
        if title.is_empty() || snippet.is_empty() {
            continue;
        }
        let date = item
            .select(&date_sel)
            .next()
            .map(text_of)
            .unwrap_or_default();
        let score = relevance(&query_tokens, &title, &snippet);
        if score < 0.15 {
            continue;
        }
        hits.push(SearchHit {
            title,
            source: source_name(&url),
            url: url.to_string(),
            snippet,
            date,
            kind: classify_source(&url).to_string(),
            score,
        });
    }
    Ok(hits)
}

/// Query the Indonesian Wikipedia API for background context.
pub fn wikipedia_search(query: &str, limit: usize) -> Result<Vec<SearchHit>, FetchError> {
    let encoded = urlencoding::encode(query);
    let url = format!(
        "https://id.wikipedia.org/w/api.php?action=query&list=search&srsearch={encoded}&format=json&srlimit={}&utf8=1",
        limit.clamp(1, 10)
    );
    let body = client()?
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| FetchError::Http(e.to_string()))?
        .text()
        .map_err(|e| FetchError::Http(e.to_string()))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| FetchError::Http(e.to_string()))?;
    let query_tokens = tokens(query);
    let mut hits = Vec::new();
    if let Some(items) = parsed["query"]["search"].as_array() {
        for item in items {
            let title = item["title"].as_str().unwrap_or("").to_string();
            let snippet_html = item["snippet"].as_str().unwrap_or("");
            let snippet = strip_tags(snippet_html);
            if title.is_empty() || snippet.is_empty() {
                continue;
            }
            let normalized_title = title.replace(' ', "_");
            let encoded_title = urlencoding::encode(&normalized_title);
            let page_url = format!("https://id.wikipedia.org/wiki/{encoded_title}");
            let score = relevance(&query_tokens, &title, &snippet);
            if score < 0.15 {
                continue;
            }
            hits.push(SearchHit {
                title,
                url: page_url,
                snippet,
                source: "id.wikipedia.org".into(),
                date: String::new(),
                kind: "encyclopedia".into(),
                score,
            });
        }
    }
    Ok(hits)
}

fn strip_tags(html: &str) -> String {
    let fragment = Html::parse_fragment(html);
    fragment.root_element().text().collect::<Vec<_>>().join(" ")
}

/// Gather evidence for a claim from fact-check search plus Wikipedia.
///
/// Returns `Ok(empty)` when the network works but nothing relevant was found,
/// and `Err` only when every source failed.
pub fn search_evidence(query: &str, max: usize) -> Result<Vec<SearchHit>, FetchError> {
    let compact = compact_query(query, 14);
    if compact.is_empty() {
        return Ok(Vec::new());
    }
    let mut errors: Vec<String> = Vec::new();
    let mut hits: Vec<SearchHit> = Vec::new();
    match bing_search(&format!("{compact} cek fakta"), 10) {
        Ok(mut h) => hits.append(&mut h),
        Err(e) => errors.push(format!("bing: {e}")),
    }
    match bing_search(&format!("site:turnbackhoax.id {compact}"), 5) {
        Ok(mut h) => hits.append(&mut h),
        Err(e) => errors.push(format!("bing-site: {e}")),
    }
    match wikipedia_search(&compact, 3) {
        Ok(mut h) => hits.append(&mut h),
        Err(e) => errors.push(format!("wikipedia: {e}")),
    }

    let mut seen = BTreeSet::new();
    hits.retain(|h| seen.insert(h.url.trim_end_matches('/').to_string()));
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(max);

    // Enrich the most relevant fact-check and encyclopedia hits with their full
    // article text. Search snippets are often too short to ground a verdict,
    // and a truncated snippet can make the model guess. Fetch failures are
    // non-fatal: the snippet stays as the evidence.
    let mut enriched = 0;
    for hit in hits.iter_mut() {
        if enriched >= 2 {
            break;
        }
        if hit.kind != "fact_check" && hit.kind != "encyclopedia" {
            continue;
        }
        if let Ok(page) = fetch_page(&hit.url) {
            let text = page.text.trim();
            if !text.is_empty() {
                hit.snippet = truncate_chars(text, super::MAX_EVIDENCE_CHARS);
                enriched += 1;
            }
        }
    }

    if hits.is_empty() && !errors.is_empty() {
        return Err(FetchError::Http(errors.join("; ")));
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn rejects_private_urls() {
        assert!(validate_public_url("http://127.0.0.1:8080/x").is_err());
        assert!(validate_public_url("http://localhost/x").is_err());
        assert!(validate_public_url("file:///etc/passwd").is_err());
        assert!(validate_public_url("https://example.com/x").is_ok());
    }

    #[test]
    fn classifies_public_and_private_addresses() {
        assert!(is_public_ip("8.8.8.8".parse().unwrap()));
        assert!(!is_public_ip("127.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("10.1.2.3".parse().unwrap()));
        assert!(!is_public_ip("192.168.1.1".parse().unwrap()));
        assert!(!is_public_ip("169.254.169.254".parse().unwrap()));
        assert!(!is_public_ip("::1".parse().unwrap()));
        assert!(!is_public_ip("fd00::1".parse().unwrap()));
        assert!(!is_public_ip("::ffff:127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn decodes_only_bing_redirects() {
        let payload = URL_SAFE_NO_PAD.encode("https://turnbackhoax.id/artikel");
        let href = format!("https://www.bing.com/ck/a?u=a1{payload}");
        assert_eq!(
            decode_bing_target(&href).as_deref(),
            Some("https://turnbackhoax.id/artikel")
        );
        assert!(decode_bing_target("https://evilbing.com/ck/a?u=a1abc").is_none());
        assert!(decode_bing_target("https://example.com/x").is_none());
    }

    #[test]
    fn extracts_paragraphs_and_metadata() {
        let html = r#"
            <html><head>
              <meta property="og:title" content="Contoh Judul">
              <meta property="og:site_name" content="Contoh Situs">
              <meta property="article:published_time" content="2026-01-02T03:04:05Z">
            </head><body>
              <nav>menu menu menu</nav>
              <article>
                <h1>Contoh Judul</h1>
                <p>Teks paragraf pertama yang cukup panjang untuk dianggap sebagai isi artikel berita.</p>
                <p>Teks paragraf kedua yang juga cukup panjang dan berbeda dari paragraf sebelumnya.</p>
                <script>alert('noise')</script>
              </article>
            </body></html>"#;
        let page = extract_article(html, "https://example.com/berita");
        assert_eq!(page.title, "Contoh Judul");
        assert_eq!(page.site, "Contoh Situs");
        assert_eq!(page.published, "2026-01-02T03:04:05Z");
        assert!(page.text.contains("paragraf pertama"));
        assert!(page.text.contains("paragraf kedua"));
        assert!(!page.text.contains("alert"));
    }

    #[test]
    fn relevance_prefers_matching_titles() {
        let q = tokens("vaksin covid microchip pelacakan");
        let relevant = relevance(&q, "Hoax vaksin COVID-19 microchip", "Klaim pelacakan microchip");
        let other = relevance(&q, "Cuaca hari ini", "Prakiraan cerah");
        assert!(relevant > other);
    }
}
