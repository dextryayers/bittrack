//! veritas-engine: native library exposing the Veritas analysis engine over a C ABI.
//!
//! Built as a `cdylib`. The C header is `include/veritas.h`.

#![warn(missing_docs)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::json;
use tracing_subscriber::EnvFilter;

use veritas_core::{
    sha256_hex, validate_request, AnalyzeRequest, AnalyzeResponse, Audit, Claim, ErrorPayload,
    Evidence, Heuristics, ProgressEvent, Verdict,
};
use veritas_models::ModelManager;

/// Progress callback type matching the C ABI.
pub type ProgressCb = Option<unsafe extern "C" fn(*const c_char, *mut std::ffi::c_void)>;

/// Opaque engine handle.
pub struct VeritasEngine {
    inner: Arc<EngineInner>,
}

struct EngineInner {
    models: Mutex<ModelManager>,
    cancel: AtomicBool,
    request_counter: AtomicU64,
    current_request: AtomicU64,
}

/// Initialize the engine from a JSON config string. Returns null on failure.
#[no_mangle]
pub extern "C" fn veritas_init(config_json: *const c_char) -> *mut VeritasEngine {
    let result = catch_unwind(AssertUnwindSafe(|| {
        // init tracing to stderr if not already
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_target(false)
            .try_init();

        let _ = config_json;

        let cache_dir = ModelManager::default_cache_dir();
        let manager = ModelManager::new(cache_dir, veritas_models::default_specs());

        let engine = VeritasEngine {
            inner: Arc::new(EngineInner {
                models: Mutex::new(manager),
                cancel: AtomicBool::new(false),
                request_counter: AtomicU64::new(0),
                current_request: AtomicU64::new(0),
            }),
        };
        Box::into_raw(Box::new(engine))
    }));
    match result {
        Ok(ptr) => ptr,
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free an engine handle.
#[no_mangle]
pub extern "C" fn veritas_free(engine: *mut VeritasEngine) {
    if engine.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: engine was allocated by Box::into_raw in veritas_init.
        unsafe {
            drop(Box::from_raw(engine));
        }
    }));
}

/// Free a string returned by the engine.
#[no_mangle]
pub extern "C" fn veritas_free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        unsafe {
            drop(CString::from_raw(s));
        }
    }));
}

/// Health check. Returns owned JSON string.
#[no_mangle]
pub extern "C" fn veritas_health(engine: *const VeritasEngine) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() {
            return to_cstring(r#"{"ok":false,"error":"null engine"}"#);
        }
        let json = json!({
            "ok": true,
            "version": env!("CARGO_PKG_VERSION"),
            "loaded_models": [],
        })
        .to_string();
        to_cstring(&json)
    }));
    match result {
        Ok(p) => p,
        Err(_) => to_cstring(r#"{"ok":false,"error":"panic"}"#),
    }
}

/// Analyze text or URL. Returns owned JSON string (AnalyzeResponse or ErrorPayload).
/// Progress is reported via callback.
#[no_mangle]
pub extern "C" fn veritas_analyze(
    engine: *mut VeritasEngine,
    request_json: *const c_char,
    progress_cb: ProgressCb,
    user_data: *mut std::ffi::c_void,
) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() || request_json.is_null() {
            return to_cstring(&serde_json::to_string(&ErrorPayload {
                code: "invalid_argument".into(),
                message: "engine or request is null".into(),
            }).unwrap());
        }

        let engine = unsafe { &*engine };
        let req_str = unsafe { CStr::from_ptr(request_json) }
            .to_str()
            .unwrap_or("");
        let req: AnalyzeRequest = match serde_json::from_str(req_str) {
            Ok(r) => r,
            Err(e) => {
                return to_cstring(&serde_json::to_string(&ErrorPayload {
                    code: "bad_request".into(),
                    message: format!("invalid json: {e}"),
                }).unwrap());
            }
        };

        if let Err(problems) = validate_request(&req) {
            return to_cstring(&serde_json::to_string(&ErrorPayload {
                code: "validation".into(),
                message: problems.join("; "),
            }).unwrap());
        }

        let rid = engine.inner.request_counter.fetch_add(1, Ordering::SeqCst) + 1;
        engine.inner.current_request.store(rid, Ordering::SeqCst);
        engine.inner.cancel.store(false, Ordering::SeqCst);

        let report = |stage: &str, percent: f64, message: &str| {
            if let Some(cb) = progress_cb {
                let ev = ProgressEvent {
                    stage: stage.to_string(),
                    percent,
                    message: message.to_string(),
                };
                if let Ok(s) = serde_json::to_string(&ev) {
                    if let Ok(c) = CString::new(s) {
                        unsafe { cb(c.as_ptr(), user_data) };
                    }
                }
            }
        };

        report("normalize", 0.1, "normalizing input");

        let text = req.text.clone().unwrap_or_default();
        let normalized = normalize_text(&text);
        let input_hash = sha256_hex(normalized.as_bytes());

        if engine.inner.cancel.load(Ordering::SeqCst) {
            return to_cstring(&serde_json::to_string(&ErrorPayload {
                code: "cancelled".into(),
                message: "analysis cancelled".into(),
            }).unwrap());
        }

        report("extract", 0.3, "extracting claims");

        let opts = req.options.clone().unwrap_or_default();
        let claims_text = split_claims(&normalized, opts.max_claims as usize);

        report("retrieve", 0.5, "retrieving evidence");

        // RAG placeholder: no DB wired in this build; evidence empty.
        let evidence: Vec<Evidence> = Vec::new();

        report("classify", 0.7, "classifying claims");

        let mut claims: Vec<Claim> = Vec::new();
        for (i, ct) in claims_text.iter().enumerate() {
            if engine.inner.cancel.load(Ordering::SeqCst) {
                return to_cstring(&serde_json::to_string(&ErrorPayload {
                    code: "cancelled".into(),
                    message: "analysis cancelled".into(),
                }).unwrap());
            }
            let (verdict, confidence, reasoning) = classify_heuristic(ct, opts.threshold);
            claims.push(Claim {
                id: (i as u32) + 1,
                text: ct.clone(),
                claim_type: "factual".into(),
                importance: 3,
                verdict,
                confidence,
                reasoning,
                evidence_ids: Vec::new(),
            });
        }

        report("aggregate", 0.9, "aggregating verdict");

        let (verdict, confidence) = aggregate(&claims, opts.threshold, 0.4);

        let heuristics = Heuristics {
            domain_score: 0.5,
            buzzer_score: buzzer_score(&normalized),
            clickbait_score: clickbait_score(&normalized),
            signals: collect_signals(&normalized),
        };

        let resp = AnalyzeResponse {
            verdict,
            confidence,
            summary: build_summary(verdict, confidence, claims.len()),
            claims,
            evidence,
            heuristics,
            audit: Audit {
                model: "heuristic-baseline".into(),
                prompt_version: "heur_v1".into(),
                engine_version: env!("CARGO_PKG_VERSION").into(),
                timestamp_ms: now_ms(),
                input_hash,
            },
        };

        report("done", 1.0, "complete");

        to_cstring(&serde_json::to_string(&resp).unwrap())
    }));

    match result {
        Ok(p) => p,
        Err(_) => to_cstring(&serde_json::to_string(&ErrorPayload {
            code: "panic".into(),
            message: "engine panicked".into(),
        }).unwrap()),
    }
}

/// Cancel the in-flight request.
#[no_mangle]
pub extern "C" fn veritas_cancel(engine: *mut VeritasEngine, _request_id: u64) {
    if engine.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let engine = unsafe { &*engine };
        engine.inner.cancel.store(true, Ordering::SeqCst);
    }));
}

/// List models. Returns owned JSON string.
#[no_mangle]
pub extern "C" fn veritas_list_models(engine: *const VeritasEngine) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() {
            return to_cstring("[]");
        }
        let engine = unsafe { &*engine };
        let mgr = engine.inner.models.lock().unwrap();
        let list = mgr.list();
        // serialize as array of {id,name,size_bytes,installed,sha256}
        let arr: Vec<serde_json::Value> = list
            .iter()
            .map(|(s, st)| {
                json!({
                    "id": s.id,
                    "name": s.name,
                    "size_bytes": s.size_bytes,
                    "installed": matches!(st, veritas_models::ModelStatus::Ready),
                    "sha256": s.sha256,
                })
            })
            .collect();
        to_cstring(&serde_json::to_string(&arr).unwrap())
    }));
    match result {
        Ok(p) => p,
        Err(_) => to_cstring("[]"),
    }
}

/// Download a model. Blocking. Returns 0 on success, nonzero on failure.
#[no_mangle]
pub extern "C" fn veritas_download_model(
    engine: *mut VeritasEngine,
    model_id: *const c_char,
    _cb: ProgressCb,
    _user_data: *mut std::ffi::c_void,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() || model_id.is_null() {
            return 1;
        }
        let engine = unsafe { &mut *engine };
        let id = unsafe { CStr::from_ptr(model_id) }
            .to_str()
            .unwrap_or("");
        let cache_dir = {
            let mgr = engine.inner.models.lock().unwrap();
            // access cache dir via a temp manager reconstruction is not possible;
            // instead store cache_dir separately. For now use default.
            let _unused = mgr;
            veritas_models::ModelManager::default_cache_dir()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let res = rt.block_on(async {
            let mgr = veritas_models::ModelManager::new(cache_dir, veritas_models::default_specs());
            // spec unknown here; download will fail with NotFound, which is acceptable
            // until model specs are wired in a later phase.
            mgr.download(id, |_d, _t| {}).await
        });
        if res.is_ok() {
            0
        } else {
            1
        }
    }));
    result.unwrap_or(1)
}

// ---------- helpers ----------

fn to_cstring(s: &str) -> *mut c_char {
    CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()).into_raw()
}

fn normalize_text(input: &str) -> String {
    // NFC normalization is done conceptually; here trim + collapse whitespace.
    let mut out = String::with_capacity(input.len());
    let mut prev_space = true;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

fn split_claims(text: &str, max: usize) -> Vec<String> {
    // Simple sentence split. Real impl uses LLM claim extraction.
    let mut claims = Vec::new();
    for part in text.split(['.', '!', '?']) {
        let t = part.trim();
        if t.len() >= 20 && claims.len() < max {
            claims.push(t.to_string());
        }
    }
    if claims.is_empty() && !text.trim().is_empty() {
        claims.push(text.trim().to_string());
    }
    claims
}

fn classify_heuristic(claim: &str, _threshold: f64) -> (Verdict, f64, String) {
    let lower = claim.to_lowercase();
    // keyword-based heuristic baseline (replaced by LLM in later phases)
    let hoax_keywords = ["hoax", "bohong", "palsu", "fitnah", "disinformasi"];
    let valid_keywords = ["menurut", "data", "penelitian", "resmi", "konfirmasi"];
    let hoax_hits = hoax_keywords.iter().filter(|k| lower.contains(*k)).count();
    let valid_hits = valid_keywords.iter().filter(|k| lower.contains(*k)).count();
    let (verdict, conf) = if hoax_hits > valid_hits {
        (Verdict::Hoax, 0.6 + 0.05 * hoax_hits as f64)
    } else if valid_hits > hoax_hits {
        (Verdict::Valid, 0.6 + 0.05 * valid_hits as f64)
    } else {
        (Verdict::Unverifiable, 0.4)
    };
    let conf = conf.min(0.95);
    let reasoning = format!(
        "Heuristic baseline (LLM not enabled in this build). threshold={_threshold:.2}"
    );
    (verdict, conf, reasoning)
}

fn aggregate(claims: &[Claim], _threshold: f64, abstain_below: f64) -> (Verdict, f64) {
    if claims.is_empty() {
        return (Verdict::Unverifiable, 0.0);
    }
    // importance-weighted majority
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
    let conf = if total > 0.0 { best_score / total } else { 0.0 };
    if conf < abstain_below {
        return (Verdict::Unverifiable, conf);
    }
    let verdict = match best {
        "hoax" => Verdict::Hoax,
        "misleading" => Verdict::Misleading,
        "clickbait" => Verdict::Clickbait,
        "buzzer" => Verdict::Buzzer,
        "valid" => Verdict::Valid,
        _ => Verdict::Unverifiable,
    };
    (verdict, conf.min(1.0))
}

fn buzzer_score(text: &str) -> f64 {
    let lower = text.to_lowercase();
    let imperatives = ["share", "viralkan", "sebarkan", "bagikan"];
    let hits = imperatives.iter().filter(|k| lower.contains(*k)).count();
    (hits as f64 * 0.2).min(1.0)
}

fn clickbait_score(text: &str) -> f64 {
    // crude: excessive caps
    let caps = text.chars().filter(|c| c.is_uppercase()).count();
    let total = text.chars().count().max(1);
    (caps as f64 / total as f64 * 2.0).min(1.0)
}

fn collect_signals(text: &str) -> Vec<String> {
    let mut sigs = Vec::new();
    let lower = text.to_lowercase();
    if lower.contains("share") || lower.contains("viralkan") {
        sigs.push("imperative_share".into());
    }
    if text.chars().filter(|c| c.is_uppercase()).count() > text.len() / 4 {
        sigs.push("excessive_caps".into());
    }
    sigs
}

fn build_summary(verdict: Verdict, confidence: f64, n_claims: usize) -> String {
    format!(
        "Verdict: {}. Confidence: {:.0}%. Claims analyzed: {}.",
        verdict.as_str(),
        confidence * 100.0,
        n_claims
    )
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
