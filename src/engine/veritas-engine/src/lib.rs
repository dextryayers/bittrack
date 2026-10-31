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

mod heuristics;
mod rag;

use veritas_core::{
    sha256_hex, validate_request, AnalyzeRequest, AnalyzeResponse, Audit, Claim, ErrorPayload,
    Evidence, ProgressEvent, Verdict,
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
    factdb: Mutex<rag::FactDb>,
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

        let db_path = rag::default_db_path();
        let factdb = rag::FactDb::open(&db_path, Box::new(rag::HashingEmbedder::default()))
            .unwrap_or_else(|_| {
                rag::FactDb::open_in_memory(Box::new(rag::HashingEmbedder::default())).unwrap()
            });
        if factdb.count() == 0 {
            factdb.seed(&rag::builtin_seed());
        }

        let engine = VeritasEngine {
            inner: Arc::new(EngineInner {
                models: Mutex::new(manager),
                factdb: Mutex::new(factdb),
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
///
/// # Safety
/// `engine` must be a pointer returned by `veritas_init` and must not be used
/// after this call.
#[no_mangle]
pub unsafe extern "C" fn veritas_free(engine: *mut VeritasEngine) {
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
///
/// # Safety
/// `s` must be a pointer returned by this library and must not be freed twice.
#[no_mangle]
pub unsafe extern "C" fn veritas_free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
        drop(CString::from_raw(s));
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
///
/// # Safety
/// `engine` must be a valid handle from `veritas_init`, `request_json` must be a
/// null-terminated UTF-8 string, and `user_data` must remain valid for the
/// duration of the call.
#[no_mangle]
pub unsafe extern "C" fn veritas_analyze(
    engine: *mut VeritasEngine,
    request_json: *const c_char,
    progress_cb: ProgressCb,
    user_data: *mut std::ffi::c_void,
) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() || request_json.is_null() {
            return to_cstring(
                &serde_json::to_string(&ErrorPayload {
                    code: "invalid_argument".into(),
                    message: "engine or request is null".into(),
                })
                .unwrap(),
            );
        }

        let engine = unsafe { &*engine };
        let req_str = unsafe { CStr::from_ptr(request_json) }
            .to_str()
            .unwrap_or("");
        let req: AnalyzeRequest = match serde_json::from_str(req_str) {
            Ok(r) => r,
            Err(e) => {
                return to_cstring(
                    &serde_json::to_string(&ErrorPayload {
                        code: "bad_request".into(),
                        message: format!("invalid json: {e}"),
                    })
                    .unwrap(),
                );
            }
        };

        if let Err(problems) = validate_request(&req) {
            return to_cstring(
                &serde_json::to_string(&ErrorPayload {
                    code: "validation".into(),
                    message: problems.join("; "),
                })
                .unwrap(),
            );
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
            return to_cstring(
                &serde_json::to_string(&ErrorPayload {
                    code: "cancelled".into(),
                    message: "analysis cancelled".into(),
                })
                .unwrap(),
            );
        }

        report("extract", 0.3, "extracting claims");

        let opts = req.options.clone().unwrap_or_default();
        let claims_text = split_claims(&normalized, opts.max_claims as usize);

        report("retrieve", 0.5, "retrieving evidence");

        // RAG retrieval
        let mut evidence: Vec<Evidence> = Vec::new();
        let mut evidence_counter: u32 = 0;
        let mut claim_evidence: Vec<Vec<u32>> = Vec::new();
        let mut rag_verdicts: Vec<Option<String>> = Vec::new();
        {
            let db = engine.inner.factdb.lock().unwrap();
            for ct in &claims_text {
                let hits = db.search(ct, 3);
                let mut ids = Vec::new();
                let mut best: Option<(String, f32)> = None;
                for (fc, score) in hits {
                    if score < 0.35 {
                        continue;
                    }
                    if !evidence.iter().any(|e| e.text == fc.text) {
                        evidence_counter += 1;
                        evidence.push(Evidence {
                            id: evidence_counter,
                            text: fc.text.clone(),
                            source: fc.source.clone(),
                            url: fc.url.clone(),
                            date: fc.date.clone(),
                            score: score as f64,
                        });
                    }
                    if let Some(e) = evidence.iter().find(|e| e.text == fc.text) {
                        ids.push(e.id);
                    }
                    if best.as_ref().map(|b| score > b.1).unwrap_or(true) {
                        best = Some((fc.verdict.clone(), score));
                    }
                }
                claim_evidence.push(ids);
                rag_verdicts.push(best.filter(|b| b.1 >= 0.7).map(|b| b.0));
            }
        }

        report("classify", 0.7, "classifying claims");

        let mut claims: Vec<Claim> = Vec::new();
        for (i, ct) in claims_text.iter().enumerate() {
            if engine.inner.cancel.load(Ordering::SeqCst) {
                return to_cstring(
                    &serde_json::to_string(&ErrorPayload {
                        code: "cancelled".into(),
                        message: "analysis cancelled".into(),
                    })
                    .unwrap(),
                );
            }
            let ev_ids = claim_evidence.get(i).cloned().unwrap_or_default();
            let rag_v = rag_verdicts.get(i).cloned().flatten();
            let (verdict, confidence, reasoning) =
                classify_with_rag(ct, rag_v.as_deref(), opts.threshold);
            claims.push(Claim {
                id: (i as u32) + 1,
                text: ct.clone(),
                claim_type: "factual".into(),
                importance: 3,
                verdict,
                confidence,
                reasoning,
                evidence_ids: ev_ids,
            });
        }

        report("aggregate", 0.9, "aggregating verdict");

        let heur_signals = heuristics::build_heuristics(&normalized);

        let (verdict, confidence) = heuristics::aggregate(&claims, &heur_signals, opts.threshold);

        let resp = AnalyzeResponse {
            verdict,
            confidence,
            summary: build_summary(verdict, confidence, claims.len()),
            claims,
            evidence,
            heuristics: heur_signals,
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
        Err(_) => to_cstring(
            &serde_json::to_string(&ErrorPayload {
                code: "panic".into(),
                message: "engine panicked".into(),
            })
            .unwrap(),
        ),
    }
}

/// Cancel the in-flight request.
///
/// # Safety
/// `engine` must be a valid handle from `veritas_init`.
#[no_mangle]
pub unsafe extern "C" fn veritas_cancel(engine: *mut VeritasEngine, _request_id: u64) {
    if engine.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let engine = unsafe { &*engine };
        engine.inner.cancel.store(true, Ordering::SeqCst);
    }));
}

/// List models. Returns owned JSON string.
///
/// # Safety
/// `engine` must be a valid handle from `veritas_init`.
#[no_mangle]
pub unsafe extern "C" fn veritas_list_models(engine: *const VeritasEngine) -> *mut c_char {
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
///
/// # Safety
/// `engine` must be a valid handle from `veritas_init` and `model_id` must be a
/// null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn veritas_download_model(
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
        let id = unsafe { CStr::from_ptr(model_id) }.to_str().unwrap_or("");
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
    CString::new(s)
        .unwrap_or_else(|_| CString::new("").unwrap())
        .into_raw()
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
    // Sentence split tuned for short social posts: keep imperative fragments
    // such as "SHARE SEKARANG" instead of dropping them as noise.
    let mut claims = Vec::new();
    for part in text.split(['.', '!', '?']) {
        let t = part.trim();
        if t.chars().count() >= 12 && claims.len() < max {
            claims.push(t.to_string());
        }
    }
    if claims.is_empty() && !text.trim().is_empty() {
        claims.push(text.trim().to_string());
    }
    claims
}

fn classify_with_rag(
    claim: &str,
    rag_verdict: Option<&str>,
    _threshold: f64,
) -> (Verdict, f64, String) {
    if let Some(rv) = rag_verdict {
        let v = match rv {
            "hoax" => Verdict::Hoax,
            "valid" => Verdict::Valid,
            "misleading" => Verdict::Misleading,
            "buzzer" => Verdict::Buzzer,
            "clickbait" => Verdict::Clickbait,
            _ => Verdict::Unverifiable,
        };
        return (
            v,
            0.85,
            format!("RAG match to verified fact (verdict: {rv})."),
        );
    }
    // fallback heuristic
    let lower = claim.to_lowercase();
    let clickbait_keywords = [
        "viral",
        "gila",
        "mencengangkan",
        "rahasia",
        "terkejut",
        "tercengang",
        "bikin kaget",
        "tidak akan percaya",
        "baca sampai habis",
    ];
    let clickbait_hits = clickbait_keywords
        .iter()
        .filter(|k| lower.contains(*k))
        .count();
    if clickbait_hits > 0 {
        return (
            Verdict::Clickbait,
            (0.6 + 0.05 * clickbait_hits as f64).min(0.9),
            "Heuristic: clickbait pattern detected.".into(),
        );
    }
    let hoax_keywords = ["hoax", "bohong", "palsu", "fitnah", "disinformasi"];
    let valid_keywords = ["menurut", "data", "penelitian", "resmi", "konfirmasi"];
    let buzzer_keywords = ["share", "viralkan", "sebarkan", "bagikan", "forward"];
    let hoax_hits = hoax_keywords.iter().filter(|k| lower.contains(*k)).count();
    let valid_hits = valid_keywords.iter().filter(|k| lower.contains(*k)).count();
    let buzzer_hits = buzzer_keywords
        .iter()
        .filter(|k| lower.contains(*k))
        .count();
    if buzzer_hits > hoax_hits.max(valid_hits) {
        return (
            Verdict::Buzzer,
            (0.6 + 0.05 * buzzer_hits as f64).min(0.9),
            "Heuristic: buzzer imperative pattern detected.".into(),
        );
    }
    if hoax_hits > valid_hits {
        (
            Verdict::Hoax,
            (0.55 + 0.05 * hoax_hits as f64).min(0.9),
            "Heuristic: hoax keywords detected.".into(),
        )
    } else if valid_hits > hoax_hits {
        (
            Verdict::Valid,
            (0.55 + 0.05 * valid_hits as f64).min(0.9),
            "Heuristic: valid keywords detected.".into(),
        )
    } else {
        (
            Verdict::Unverifiable,
            0.4,
            "Heuristic: insufficient signal.".into(),
        )
    }
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
