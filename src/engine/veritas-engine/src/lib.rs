//! veritas-engine: native library exposing the Veritas analysis engine over a C ABI.
//!
//! Built as a `cdylib`. The C header is `include/veritas.h`.
//!
//! The engine performs three things, in order: it resolves the input (article
//! URL, pasted text, OCR poster text, reader comments), retrieves live
//! fact-check and encyclopedia evidence, and asks a local GGUF model to produce
//! a schema-constrained JSON verdict. There is no keyword-based fallback: when
//! the model is missing or fails the caller receives an error instead of a
//! fabricated verdict.

#![warn(missing_docs)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use serde_json::json;
use tracing_subscriber::EnvFilter;

mod fetch;
mod heuristics;
#[cfg(feature = "llm")]
mod llm;

use veritas_core::{
    sha256_hex, validate_request, AnalyzeRequest, AnalyzeResponse, Audit, Claim, ErrorPayload,
    Evidence, Heuristics, ProgressEvent, Verdict,
};
use veritas_models::{ModelManager, ModelStatus};

/// Progress callback type matching the C ABI.
pub type ProgressCb = Option<unsafe extern "C" fn(*const c_char, *mut std::ffi::c_void)>;

/// Prompt contract version, bumped whenever the instructions change.
const PROMPT_VERSION: &str = "veritas-id-v2";
/// Maximum characters of article text sent to the model.
const MAX_SUBJECT_CHARS: usize = 9_000;
/// Maximum characters of OCR poster text sent to the model.
const MAX_POSTER_CHARS: usize = 1_500;
/// Maximum characters of reader comments sent to the model.
const MAX_COMMENTS_CHARS: usize = 1_200;
/// Maximum characters per evidence snippet.
const MAX_EVIDENCE_CHARS: usize = 320;
/// Number of evidence items fetched per analysis.
const EVIDENCE_LIMIT: usize = 5;
/// Default output token cap.
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 1_400;

/// Opaque engine handle.
pub struct VeritasEngine {
    inner: Arc<EngineInner>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct EngineConfig {
    /// Force a specific model id instead of the best installed one.
    model_id: Option<String>,
    /// Worker threads for generation.
    threads: Option<i32>,
    /// Context window in tokens.
    context_tokens: Option<u32>,
    /// Output token cap.
    max_output_tokens: Option<u32>,
    /// Override live evidence retrieval.
    online_evidence: Option<bool>,
}

struct EngineInner {
    models: Mutex<ModelManager>,
    config: EngineConfig,
    cancel: AtomicBool,
    #[cfg(feature = "llm")]
    backend: Mutex<Option<llama_cpp_2::llama_backend::LlamaBackend>>,
    #[cfg(feature = "llm")]
    llm: Mutex<Option<(String, llm::Llm)>>,
}

/// Initialize the engine from a JSON config string. Returns null on failure.
#[no_mangle]
pub extern "C" fn veritas_init(config_json: *const c_char) -> *mut VeritasEngine {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_target(false)
            .try_init();

        let config: EngineConfig = if config_json.is_null() {
            EngineConfig::default()
        } else {
            let raw = unsafe { CStr::from_ptr(config_json) }
                .to_str()
                .unwrap_or("");
            if raw.trim().is_empty() {
                EngineConfig::default()
            } else {
                serde_json::from_str(raw).unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "invalid engine config; using defaults");
                    EngineConfig::default()
                })
            }
        };

        let cache_dir = ModelManager::default_cache_dir();
        let manager = ModelManager::new(cache_dir, veritas_models::default_specs());

        let engine = VeritasEngine {
            inner: Arc::new(EngineInner {
                models: Mutex::new(manager),
                config,
                cancel: AtomicBool::new(false),
                #[cfg(feature = "llm")]
                backend: Mutex::new(None),
                #[cfg(feature = "llm")]
                llm: Mutex::new(None),
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
        let engine = unsafe { &*engine };
        let models = model_list_json(&engine.inner);
        #[cfg(feature = "llm")]
        let loaded = engine
            .inner
            .llm
            .lock()
            .map(|slot| slot.as_ref().map(|(id, _)| id.clone()))
            .unwrap_or(None);
        #[cfg(not(feature = "llm"))]
        let loaded: Option<String> = None;
        let installed = engine
            .inner
            .models
            .lock()
            .map(|mgr| mgr.best_installed().map(|s| s.id))
            .unwrap_or(None);
        let json = json!({
            "ok": true,
            "version": env!("CARGO_PKG_VERSION"),
            "llm_feature": cfg!(feature = "llm"),
            "model_ready": installed.is_some(),
            "model_installed": installed,
            "model_loaded": loaded,
            "models": models,
        })
        .to_string();
        to_cstring(&json)
    }));
    match result {
        Ok(p) => p,
        Err(_) => to_cstring(r#"{"ok":false,"error":"panic"}"#),
    }
}

/// Analyze text, URL, poster text, and comments. Returns owned JSON string
/// (AnalyzeResponse or ErrorPayload). Progress is reported via callback.
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
            return error_json("invalid_argument", "engine or request is null");
        }

        let engine = unsafe { &*engine };
        let req_str = unsafe { CStr::from_ptr(request_json) }
            .to_str()
            .unwrap_or("");
        let req: AnalyzeRequest = match serde_json::from_str(req_str) {
            Ok(r) => r,
            Err(e) => return error_json("bad_request", &format!("invalid json: {e}")),
        };
        if let Err(problems) = validate_request(&req) {
            return error_json("validation", &problems.join("; "));
        }

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

        match analyze(&engine.inner, &req, &report) {
            Ok(resp) => {
                report("done", 1.0, "selesai");
                match serde_json::to_string(&resp) {
                    Ok(json) => to_cstring(&json),
                    Err(e) => error_json("serialization", &e.to_string()),
                }
            }
            Err(err) => {
                if err.code == "cancelled" {
                    report("cancelled", 0.0, &err.message);
                } else {
                    report("error", 0.0, &err.message);
                }
                error_json(&err.code, &err.message)
            }
        }
    }));

    match result {
        Ok(p) => p,
        Err(_) => error_json("panic", "engine panicked"),
    }
}

fn analyze(
    inner: &EngineInner,
    req: &AnalyzeRequest,
    report: &dyn Fn(&str, f64, &str),
) -> Result<AnalyzeResponse, ErrorPayload> {
    let opts = req.options.clone().unwrap_or_default();
    let mut warnings: Vec<String> = Vec::new();

    report("normalize", 0.05, "menyiapkan masukan");
    let url = req
        .url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_string);
    let mut title = String::new();
    let mut body = normalize_text(req.text.as_deref().unwrap_or(""));
    let mut source_url = url.clone().unwrap_or_default();
    let mut site = String::new();
    let mut published = String::new();

    if let Some(raw_url) = url.as_deref() {
        report("fetch", 0.12, "mengambil isi halaman");
        match fetch::fetch_page(raw_url) {
            Ok(page) => {
                title = page.title.trim().to_string();
                if !page.text.trim().is_empty() {
                    body = if body.is_empty() {
                        page.text
                    } else {
                        format!("{}\n\n{}", page.text, body)
                    };
                }
                source_url = page.url;
                site = page.site;
                published = page.published;
            }
            Err(e) => {
                warnings.push(format!("Gagal mengambil URL: {e}"));
                if body.trim().is_empty() {
                    return Err(ErrorPayload {
                        code: "fetch_failed".into(),
                        message: format!(
                            "Isi halaman tidak dapat diambil dan tidak ada teks alternatif: {e}"
                        ),
                    });
                }
            }
        }
    }

    let poster = normalize_text(req.poster_text.as_deref().unwrap_or(""));
    let mut comments = normalize_text(req.comments.as_deref().unwrap_or(""));
    if body.trim().is_empty() && poster.is_empty() {
        if comments.is_empty() {
            return Err(ErrorPayload {
                code: "empty_input".into(),
                message: "Tidak ada teks berita atau teks poster untuk dianalisis.".into(),
            });
        }
        // Comments-only input: the comment itself is the claim under review.
        body = comments.clone();
        comments.clear();
    }
    check_cancel(inner)?;

    let subject_hash = sha256_hex(
        format!("{}\n{}\n{}\n{}", title, body, poster, comments).as_bytes(),
    );

    // ------------------------------------------------------------------
    // Evidence retrieval
    // ------------------------------------------------------------------
    let mut evidence: Vec<Evidence> = Vec::new();
    let online = opts.online_evidence && inner.config.online_evidence.unwrap_or(true);
    if online {
        report("retrieve", 0.2, "mencari bukti dari sumber tepercaya");
        let query = evidence_query(&title, &body, &poster);
        if query.is_empty() {
            warnings.push("Tidak ada kata kunci yang cukup untuk mencari bukti.".into());
        } else {
            match fetch::search_evidence(&query, EVIDENCE_LIMIT) {
                Ok(hits) => {
                    for (index, hit) in hits.into_iter().enumerate() {
                        let text = truncate_chars(
                            &format!("{} — {}", hit.title.trim(), hit.snippet.trim()),
                            MAX_EVIDENCE_CHARS,
                        );
                        evidence.push(Evidence {
                            id: (index as u32) + 1,
                            text,
                            source: hit.source,
                            url: hit.url,
                            date: hit.date,
                            score: hit.score,
                            kind: hit.kind,
                        });
                    }
                    if evidence.is_empty() {
                        warnings.push(
                            "Tidak ada bukti relevan ditemukan; verdict mungkin tidak dapat diverifikasi."
                                .into(),
                        );
                    }
                }
                Err(e) => warnings.push(format!("Pencarian bukti gagal: {e}")),
            }
        }
    } else {
        warnings.push("Pencarian bukti daring dinonaktifkan.".into());
    }
    check_cancel(inner)?;

    // ------------------------------------------------------------------
    // Local model inference
    // ------------------------------------------------------------------
    report("prompt", 0.4, "menyusun prompt analisis");
    let system = build_system_prompt(opts.max_claims);
    let user = build_user_prompt(
        &title,
        &body,
        &poster,
        &comments,
        &source_url,
        &site,
        &published,
        &evidence,
        req.lang_hint.as_str(),
    );
    let max_tokens = inner
        .config
        .max_output_tokens
        .unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS)
        .clamp(256, 3_000);

    let first = run_inference(inner, &system, &user, max_tokens, None, report)?;
    let mut parsed = match serde_json::from_str::<ModelOutput>(&first.raw) {
        Ok(value) => value,
        Err(e) => {
            warnings.push(format!(
                "Keluaran model tidak sesuai skema ({e}); mencoba ulang."
            ));
            report("retry", 0.8, "mengulang inferensi");
            let second = run_inference(inner, &system, &user, max_tokens, Some(0x5EED), report)?;
            serde_json::from_str::<ModelOutput>(&second.raw).map_err(|e| ErrorPayload {
                code: "inference_failed".into(),
                message: format!("Model tidak menghasilkan JSON yang valid: {e}"),
            })?
        }
    };
    check_cancel(inner)?;

    // ------------------------------------------------------------------
    // Map model output into the public contract
    // ------------------------------------------------------------------
    report("map", 0.92, "menyusun hasil");
    let mut confidence = clamp_ratio(parsed.confidence);
    let truth_percentage = parsed.truth_score.clamp(0, 100) as u8;
    let mut verdict = Verdict::parse(&parsed.verdict).unwrap_or(Verdict::Unverifiable);

    if parsed.verdict.trim().is_empty() {
        warnings.push("Model tidak memilih verdict; ditandai tidak dapat diverifikasi.".into());
    }
    if parsed.truth_score < 0 || parsed.truth_score > 100 {
        warnings.push("Skor kebenaran model di luar 0..100 dan telah dipotong.".into());
    }

    if confidence < opts.threshold {
        warnings.push(format!(
            "Keyakinan model {:.0}% di bawah ambang {:.0}%; hasil ditandai tidak dapat diverifikasi.",
            confidence * 100.0,
            opts.threshold * 100.0
        ));
        verdict = Verdict::Unverifiable;
    }
    if verdict == Verdict::Hoax && truth_percentage > 50 {
        warnings.push(
            "Model memberi verdict hoax tetapi truth_score tinggi; periksa bukti secara manual."
                .into(),
        );
    }
    if verdict == Verdict::Valid && truth_percentage < 50 {
        warnings.push(
            "Model memberi verdict valid tetapi truth_score rendah; periksa bukti secara manual."
                .into(),
        );
    }
    if evidence.is_empty() && matches!(verdict, Verdict::Hoax | Verdict::Valid) {
        warnings.push(
            "Verdict tegas diberikan tanpa bukti referensi yang ditemukan; pertimbangkan verifikasi manual."
                .into(),
        );
    }
    confidence = (confidence * 100.0).round() / 100.0;

    let valid_evidence_ids: std::collections::BTreeSet<u32> =
        evidence.iter().map(|e| e.id).collect();
    let mut claims: Vec<Claim> = Vec::new();
    for (index, claim) in parsed.claims.drain(..).take(opts.max_claims as usize).enumerate() {
        let text = truncate_chars(claim.text.trim(), 600);
        if text.is_empty() {
            continue;
        }
        let importance = claim.importance.clamp(1, 5) as u8;
        let claim_type = match claim.claim_type.trim().to_ascii_lowercase().as_str() {
            "factual" | "causal" | "statistical" | "quote" | "visual" | "opinion" => {
                claim.claim_type.trim().to_ascii_lowercase()
            }
            _ => "factual".to_string(),
        };
        let claim_confidence = clamp_ratio(claim.confidence);
        let evidence_ids = claim
            .evidence_ids
            .into_iter()
            .filter(|id| valid_evidence_ids.contains(id))
            .collect();
        claims.push(Claim {
            id: (index as u32) + 1,
            text,
            claim_type,
            importance,
            verdict: Verdict::parse(&claim.verdict).unwrap_or(Verdict::Unverifiable),
            confidence: claim_confidence,
            reasoning: truncate_chars(claim.reasoning.trim(), 800),
            evidence_ids,
        });
    }
    if claims.is_empty() {
        warnings.push("Model tidak memecah klaim; hasil hanya berisi verdict utama.".into());
    }

    let heuristics = if opts.include_heuristics {
        heuristics::build_heuristics(&format!("{}\n{}\n{}\n{}", title, body, poster, comments))
    } else {
        Heuristics::default()
    };

    let mut red_flags: Vec<String> = parsed
        .red_flags
        .drain(..)
        .map(|f| truncate_chars(f.trim(), 240))
        .filter(|f| !f.is_empty())
        .take(8)
        .collect();
    red_flags.dedup();

    let summary = {
        let text = truncate_chars(parsed.summary.trim(), 1_500);
        if text.is_empty() {
            format!(
                "Model menilai konten ini sebagai {} dengan skor kebenaran {truth_percentage}% dan keyakinan {:.0}%.",
                verdict.as_str(),
                confidence * 100.0
            )
        } else {
            text
        }
    };

    Ok(AnalyzeResponse {
        verdict,
        confidence,
        truth_percentage,
        ai_percentage: parsed.ai_score.clamp(0, 100) as u8,
        summary,
        claims,
        evidence,
        heuristics,
        red_flags,
        warnings,
        audit: Audit {
            model: first.model_id,
            prompt_version: PROMPT_VERSION.into(),
            engine_version: env!("CARGO_PKG_VERSION").into(),
            timestamp_ms: now_ms(),
            input_hash: subject_hash,
        },
    })
}

#[derive(Debug, Deserialize)]
struct ModelOutput {
    verdict: String,
    truth_score: i32,
    confidence: i32,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    claims: Vec<ModelClaim>,
    #[serde(default)]
    red_flags: Vec<String>,
    #[serde(default)]
    ai_score: i32,
}

#[derive(Debug, Deserialize)]
struct ModelClaim {
    text: String,
    #[serde(rename = "type", default)]
    claim_type: String,
    #[serde(default)]
    importance: i32,
    #[serde(default)]
    verdict: String,
    #[serde(default)]
    confidence: i32,
    #[serde(default)]
    reasoning: String,
    #[serde(default)]
    evidence_ids: Vec<u32>,
}

struct Inference {
    model_id: String,
    raw: String,
}

#[cfg(feature = "llm")]
fn run_inference(
    inner: &EngineInner,
    system: &str,
    user: &str,
    max_tokens: u32,
    seed: Option<u32>,
    report: &dyn Fn(&str, f64, &str),
) -> Result<Inference, ErrorPayload> {
    use llama_cpp_2::llama_backend::LlamaBackend;

    let spec = {
        let manager = inner.models.lock().map_err(|_| ErrorPayload {
            code: "internal".into(),
            message: "model manager lock poisoned".into(),
        })?;
        let spec = match inner.config.model_id.as_ref() {
            Some(id) => manager.spec(id).ok_or_else(|| ErrorPayload {
                code: "model_unavailable".into(),
                message: format!("Model '{id}' tidak ada dalam katalog."),
            })?,
            None => manager.best_installed().ok_or_else(|| ErrorPayload {
                code: "model_unavailable".into(),
                message: format!(
                    "Tidak ada model GGUF terpasang. Upload model di Pengaturan. Folder: {}",
                    manager.cache_dir().display()
                ),
            })?,
        };
        let status = manager.status(&spec.id);
        if status != ModelStatus::Ready {
            return Err(ErrorPayload {
                code: "model_unavailable".into(),
                message: format!(
                    "Model '{}' belum siap (status: {}). Unduh ulang di Pengaturan.",
                    spec.id,
                    status.as_str()
                ),
            });
        }
        spec
    };

    let mut backend_slot = inner.backend.lock().map_err(|_| ErrorPayload {
        code: "internal".into(),
        message: "backend lock poisoned".into(),
    })?;
    if backend_slot.is_none() {
        report("load_model", 0.45, "menginisialisasi llama.cpp");
        let backend = LlamaBackend::init().map_err(|e| ErrorPayload {
            code: "model_load_failed".into(),
            message: format!("Gagal menginisialisasi llama.cpp: {e}"),
        })?;
        *backend_slot = Some(backend);
    }
    let backend = backend_slot.as_ref().expect("backend initialized above");

    let mut slot = inner.llm.lock().map_err(|_| ErrorPayload {
        code: "internal".into(),
        message: "model lock poisoned".into(),
    })?;
    let needs_load = slot
        .as_ref()
        .map(|(id, _)| id != &spec.id)
        .unwrap_or(true);
    if needs_load {
        report(
            "load_model",
            0.5,
            &format!("memuat {} ke RAM", spec.id),
        );
        let path = inner
            .models
            .lock()
            .map_err(|_| ErrorPayload {
                code: "internal".into(),
                message: "model manager lock poisoned".into(),
            })?
            .model_path(&spec.id);
        let threads = inner
            .config
            .threads
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|n| n.get() as i32)
                    .unwrap_or(4)
                    .clamp(1, 8)
            })
            .clamp(1, 16);
        let n_ctx = inner.config.context_tokens.unwrap_or(8_192).clamp(2_048, 32_768);
        let model = llm::Llm::load(backend, &path, threads, n_ctx).map_err(|e| ErrorPayload {
            code: "model_load_failed".into(),
            message: format!("Gagal memuat model '{}': {e}", spec.id),
        })?;
        *slot = Some((spec.id.clone(), model));
    }
    let (model_id, model) = slot.as_ref().expect("model loaded above");

    report("generate", 0.55, "model sedang menganalisis");
    let mut on_token = |produced: u32, total: u32| {
        let ratio = if total == 0 {
            0.0
        } else {
            f64::from(produced) / f64::from(total)
        };
        report(
            "generate",
            0.55 + 0.3 * ratio.clamp(0.0, 1.0),
            &format!("menghasilkan {produced}/{total} token"),
        );
    };
    let raw = model
        .generate_json(
            backend,
            system,
            user,
            max_tokens,
            seed,
            &inner.cancel,
            &mut on_token,
        )
        .map_err(|e| {
            let code = match e {
                llm::LlmError::Cancelled => "cancelled",
                llm::LlmError::Timeout => "inference_timeout",
                llm::LlmError::ModelMissing(_) => "model_unavailable",
                llm::LlmError::PromptTooLong { .. } => "prompt_too_long",
                _ => "inference_failed",
            };
            ErrorPayload {
                code: code.into(),
                message: e.to_string(),
            }
        })?;
    Ok(Inference {
        model_id: format!("{model_id} (llama.cpp GGUF)"),
        raw,
    })
}

#[cfg(not(feature = "llm"))]
fn run_inference(
    _inner: &EngineInner,
    _system: &str,
    _user: &str,
    _max_tokens: u32,
    _seed: Option<u32>,
    _report: &dyn Fn(&str, f64, &str),
) -> Result<Inference, ErrorPayload> {
    Err(ErrorPayload {
        code: "model_unavailable".into(),
        message: "Engine dibangun tanpa dukungan inferensi GGUF (fitur 'llm' mati).".into(),
    })
}

fn check_cancel(inner: &EngineInner) -> Result<(), ErrorPayload> {
    if inner.cancel.load(Ordering::SeqCst) {
        Err(ErrorPayload {
            code: "cancelled".into(),
            message: "Analisis dibatalkan.".into(),
        })
    } else {
        Ok(())
    }
}

fn clamp_ratio(value: i32) -> f64 {
    (f64::from(value) / 100.0).clamp(0.0, 1.0)
}

fn evidence_query(title: &str, body: &str, poster: &str) -> String {
    let base = if title.trim().is_empty() {
        if poster.trim().is_empty() {
            body
        } else {
            poster
        }
    } else {
        title
    };
    let words: Vec<&str> = base.split_whitespace().take(40).collect();
    let query = words.join(" ");
    if query.chars().count() >= 12 {
        query
    } else {
        body.split_whitespace().take(40).collect::<Vec<_>>().join(" ")
    }
}

fn build_system_prompt(max_claims: u32) -> String {
    format!(
        "Anda adalah Veritas, analis fakta independen berbahasa Indonesia.\n\
         Tugas: menilai kebenaran klaim utama dari berita, poster, atau komentar yang diberikan.\n\n\
         Aturan wajib:\n\
         1. Gunakan HANYA teks masukan dan bukti referensi bernomor. Jangan mengarang fakta, angka, kutipan, tanggal, atau sumber.\n\
         2. verdict harus salah satu: hoax (salah atau difabrikasi), misleading (benar sebagian tetapi menyesatkan), clickbait (judul melebih-lebihkan isi), buzzer (pola amplifikasi terkoordinasi), valid (didukung bukti tepercaya), unverifiable (bukti tidak cukup).\n\
         3. truth_score = probabilitas 0-100 bahwa inti klaim BENAR. confidence = keyakinan 0-100 pada verdict yang Anda pilih. Keduanya berbeda dan tidak harus sama.\n\
         4. Jika bukti referensi tidak relevan atau tidak ada, JANGAN memaksa memakainya. Turunkan confidence dan gunakan unverifiable bila ragu.\n\
         5. Tidak adanya bukti pendukung BUKAN bukti bahwa klaim salah. Hoax hanya jika ada indikasi klaim itu palsu atau difabrikasi.\n\
         6. Ringkasan 2-4 kalimat bahasa Indonesia: klaim inti, apa yang ditunjukkan bukti, lalu kesimpulan.\n\
         7. Pecah maksimal {max_claims} klaim terpenting. Isi evidence_ids dengan nomor bukti referensi yang relevan; array kosong bila tidak ada.\n\
         8. red_flags: pola manipulasi yang benar-benar terlihat, misalnya ajakan menyebarkan sebelum dihapus, sumber anonim, angka tanpa dasar, atau judul sensasional. Array kosong bila tidak ada.\n\
         9. ai_score = probabilitas 0-100 bahwa teks masukan ditulis oleh AI (bukan manusia). Perhatikan pola repetitif, kosakata generik, struktur terlalu rapi, dan kurangnya detail personal/spesifik. Skor tinggi = kemungkinan besar ditulis AI.\n\
         10. Balas HANYA JSON sesuai skema, tanpa teks lain."
    )
}

#[allow(clippy::too_many_arguments)]
fn build_user_prompt(
    title: &str,
    body: &str,
    poster: &str,
    comments: &str,
    source_url: &str,
    site: &str,
    published: &str,
    evidence: &[Evidence],
    lang_hint: &str,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("BAHASA: {lang_hint}\n"));
    out.push_str(&format!(
        "SUMBER: {}\n",
        if source_url.is_empty() {
            "(teks diunggah pengguna)"
        } else {
            source_url
        }
    ));
    if !site.is_empty() {
        out.push_str(&format!("SITUS: {site}\n"));
    }
    if !published.is_empty() {
        out.push_str(&format!("TANGGAL TERBIT: {published}\n"));
    }
    out.push_str(&format!(
        "JUDUL: {}\n\n",
        if title.is_empty() { "(tidak ada)" } else { title }
    ));
    out.push_str("TEKS UTAMA:\n");
    let body = truncate_chars(body.trim(), MAX_SUBJECT_CHARS);
    out.push_str(if body.is_empty() { "(tidak ada)" } else { &body });
    out.push_str("\n\n");
    if !poster.is_empty() {
        out.push_str("TEKS POSTER (hasil OCR, mungkin ada salah baca):\n");
        out.push_str(&truncate_chars(poster, MAX_POSTER_CHARS));
        out.push_str("\n\n");
    }
    if !comments.is_empty() {
        out.push_str("KOMENTAR PEMBACA (konteks pola buzzer, bukan klaim utama):\n");
        out.push_str(&truncate_chars(comments, MAX_COMMENTS_CHARS));
        out.push_str("\n\n");
    }
    out.push_str("BUKTI REFERENSI:\n");
    if evidence.is_empty() {
        out.push_str("(tidak ada bukti referensi yang ditemukan)\n");
    } else {
        for item in evidence {
            out.push_str(&format!(
                "[{}] {} | {} | {} | {}\n    {}\n",
                item.id,
                item.kind,
                item.source,
                if item.date.is_empty() { "-" } else { &item.date },
                item.url,
                item.text
            ));
        }
    }
    out.push_str(
        "\nNilai sekarang. Ingat: bukti yang tidak relevan harus diabaikan, dan ketidakpastian harus dinyatakan dengan jujur.",
    );
    out
}

/// Cancel the in-flight request or download.
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

fn model_list_json(inner: &EngineInner) -> Vec<serde_json::Value> {
    let manager = match inner.models.lock() {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    manager
        .list()
        .into_iter()
        .map(|(spec, status)| {
            let path = manager.model_path(&spec.id);
            let downloaded = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            json!({
                "id": spec.id,
                "name": spec.name,
                "tier": spec.tier,
                "size_bytes": spec.size_bytes,
                "downloaded_bytes": downloaded,
                "sha256": spec.sha256,
                "min_ram_bytes": spec.min_ram_bytes,
                "status": status.as_str(),
                "installed": status == ModelStatus::Ready,
                "path": path.display().to_string(),
                "url": spec.url,
            })
        })
        .collect()
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
        let list = model_list_json(&engine.inner);
        to_cstring(&serde_json::to_string(&list).unwrap_or_else(|_| "[]".into()))
    }));
    match result {
        Ok(p) => p,
        Err(_) => to_cstring("[]"),
    }
}

// ---------- helpers ----------

fn error_json(code: &str, message: &str) -> *mut c_char {
    let payload = ErrorPayload {
        code: code.to_string(),
        message: message.to_string(),
    };
    to_cstring(&serde_json::to_string(&payload).unwrap_or_else(|_| {
        r#"{"code":"internal","message":"failed to serialize error"}"#.into()
    }))
}

fn to_cstring(s: &str) -> *mut c_char {
    CString::new(s.replace('\0', " "))
        .unwrap_or_else(|_| CString::new("").unwrap())
        .into_raw()
}

fn normalize_text(input: &str) -> String {
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

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Free the string on the Rust side; used by tests to avoid leaking.
#[cfg(test)]
fn take_string(ptr: *mut c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let value = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .to_string();
    unsafe { veritas_free_string(ptr) };
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(text: &str) -> AnalyzeRequest {
        AnalyzeRequest {
            text: Some(text.into()),
            url: None,
            comments: None,
            poster_text: None,
            lang_hint: "id".into(),
            options: Some(veritas_core::AnalyzeOptions {
                online_evidence: false,
                ..Default::default()
            }),
        }
    }

    #[test]
    fn init_and_free_roundtrip() {
        let engine = veritas_init(std::ptr::null());
        assert!(!engine.is_null());
        let health = take_string(veritas_health(engine));
        let value: serde_json::Value = serde_json::from_str(&health).unwrap();
        assert_eq!(value["ok"], true);
        unsafe { veritas_free(engine) };
    }

    #[test]
    fn invalid_request_returns_validation_error() {
        let engine = veritas_init(std::ptr::null());
        let json = CString::new(r#"{"text":"","lang_hint":"id"}"#).unwrap();
        let out = unsafe { veritas_analyze(engine, json.as_ptr(), None, std::ptr::null_mut()) };
        let value: serde_json::Value = serde_json::from_str(&take_string(out)).unwrap();
        assert_eq!(value["code"], "validation");
        unsafe { veritas_free(engine) };
    }

    #[test]
    fn missing_model_is_reported_not_faked() {
        // A config that points at a spec which is not installed must fail loudly
        // instead of falling back to keyword heuristics.
        let config = CString::new(r#"{"model_id":"qwen2.5-0.5b-instruct-q4_k_m","online_evidence":false}"#)
            .unwrap();
        let engine = veritas_init(config.as_ptr());
        let req = request("Vaksin COVID-19 mengandung microchip 5G. Sebarkan sebelum dihapus!");
        let json = CString::new(serde_json::to_string(&req).unwrap()).unwrap();
        let out = unsafe { veritas_analyze(engine, json.as_ptr(), None, std::ptr::null_mut()) };
        let value: serde_json::Value = serde_json::from_str(&take_string(out)).unwrap();
        assert_eq!(value["code"], "model_unavailable");
        unsafe { veritas_free(engine) };
    }

    #[test]
    fn prompt_marks_missing_evidence() {
        let prompt = build_user_prompt(
            "Judul",
            "Isi berita yang cukup panjang.",
            "",
            "",
            "https://example.com/a",
            "example.com",
            "2026-01-01",
            &[],
            "id",
        );
        assert!(prompt.contains("tidak ada bukti referensi"));
        assert!(prompt.contains("TEKS UTAMA"));
    }

    #[test]
    fn prompt_includes_evidence_ids() {
        let evidence = vec![Evidence {
            id: 1,
            text: "Klaim microchip telah dibantah".into(),
            source: "turnbackhoax.id".into(),
            url: "https://turnbackhoax.id/x".into(),
            date: "2024-01-01".into(),
            score: 0.8,
            kind: "fact_check".into(),
        }];
        let prompt = build_user_prompt(
            "Vaksin microchip",
            "Isi",
            "",
            "",
            "",
            "",
            "",
            &evidence,
            "id",
        );
        assert!(prompt.contains("[1] fact_check | turnbackhoax.id"));
    }

    #[test]
    fn query_prefers_title() {
        assert_eq!(
            evidence_query("Vaksin COVID-19 microchip 5G", "isi lain", ""),
            "Vaksin COVID-19 microchip 5G"
        );
    }

    /// End-to-end check against the real 1.9 GB GGUF model. Run with:
    /// `cargo test -p veritas-engine real_model -- --ignored --nocapture`
    #[test]
    #[ignore = "requires the downloaded GGUF model"]
    fn real_model_produces_schema_valid_verdict() {
        let engine = veritas_init(std::ptr::null());
        assert!(!engine.is_null());
        let req = request(
            "VIRAL! Vaksin COVID-19 mengandung microchip 5G dan pemerintah menyembunyikan \
             faktanya. Sebarkan sebelum dihapus! Bukti asli 100%.",
        );
        let json = CString::new(serde_json::to_string(&req).unwrap()).unwrap();
        let out = unsafe { veritas_analyze(engine, json.as_ptr(), None, std::ptr::null_mut()) };
        let raw = take_string(out);
        println!("{raw}");
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(
            value.get("code").is_none(),
            "engine returned error: {raw}"
        );
        assert!(value["verdict"].is_string());
        assert!(value["truth_percentage"].as_u64().unwrap() <= 100);
        assert!(!value["summary"].as_str().unwrap().is_empty());
        unsafe { veritas_free(engine) };
    }
}
