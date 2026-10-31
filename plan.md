# Plan: Veritas - Hoax/Buzzer Detector (Cross-Platform, Local-Only, No Backend)

> Codename: Veritas (Latin: truth)
> Document version: 2.0 (deep revision)
> Software version target: V1.0-Alpha
> Date: 2026-10-31
> Status: Draft for review
> Principle: No backend. No cloud. No microservices. One local app, one native engine library, fully offline-capable.

---

## 0. Executive Summary

Veritas is a desktop application for Windows, macOS, and Linux that detects hoax news, buzzer activity, clickbait, and misleading content with high accuracy and precision. All inference runs locally on the user's machine using GGUF models. There is no backend server, no cloud dependency, and no telemetry by default.

Key decisions in this revision:

- No backend. The Rust engine is compiled as a native library (cdylib) and loaded in-process by the C# UI via a documented C ABI. There is no separate process, no gRPC, no localhost port, no daemon.
- Anti AI slop. Every decision is auditable. The LLM is used only for structured reasoning on top of deterministic preprocessing. Outputs follow strict JSON schemas. Confidence is calibrated. The system abstains when evidence is insufficient.
- Single deployable unit per OS. Installer bundles the app and the native engine. Models and the fact database are downloaded on first run with checksum verification.
- Offline-first. After initial setup, the app works with no network.

UI/UX: C# + Avalonia UI, modern and elegant, Fluent theme, dark/light mode.

Engine: Rust, lightweight, memory-safe, high performance, GGUF inference, RAG, heuristics.

---

## 1. Project Charter

### 1.1 Mission
Empower Indonesian (and English) readers to verify news quickly, privately, and accurately, without relying on cloud services or sending their data anywhere.

### 1.2 Vision
Become the reference open-source desktop fact-checking tool for the Indonesian ecosystem, with a calibrated, explainable, and honest AI core.

### 1.3 Success Definition
- Macro-F1 >= 0.85 on the Indonesian hoax benchmark.
- Median latency <= 5 seconds for a 500-word article on a mid-range CPU.
- Crash rate < 0.5% of sessions.
- User satisfaction (CSAT) >= 4.0/5 in closed beta.
- Model download success rate >= 98%.

### 1.4 Non-Goals (v1)
- No video or audio detection (planned for v2).
- No real-time internet fact-checking (offline RAG only; online mode is v2).
- No social media crawler.
- No public API or backend service.
- No user accounts or cloud sync.

### 1.5 Constraints
- Must run on machines with 4 GB RAM minimum (with lightweight model).
- Must work fully offline after first-run setup.
- Must not collect content or personal data by default.
- Must be installable without administrator rights (per-user install default).

---

## 2. Target Users and Use Cases

### 2.1 Personas

| Persona | Need | Key features |
|---------|------|--------------|
| Journalist / fact-checker | Fast verification with sources | Batch input, source list, export report |
| General reader | Check a WhatsApp message or article | Paste text, simple verdict, share |
| Researcher | Analyze buzzer patterns | Metadata, confidence, audit log |
| NGO / institution | Mass screening | Batch mode, JSON/CSV export, CLI |

### 2.2 Use Cases

- UC1. Paste article text, get verdict + reasoning + sources.
- UC2. Input article URL, app scrapes and analyzes.
- UC3. Paste screenshot (OCR) then analyze (v1.1).
- UC4. Batch file (CSV/TXT) to aggregate report.
- UC5. Browse history and bookmarked results.
- UC6. Export a verification report (JSON, and optionally PDF).

---

## 3. Architecture (No Backend)

### 3.1 High-Level Design

```
+-------------------------------------------------------------+
|                  C# Avalonia UI (Veritas.App)               |
|  Views (XAML) | ViewModels (MVVM) | Services (DI)          |
+-----------------------------+-------------------------------+
                              |  P/Invoke (C ABI)
                              |  in-process, same executable tree
+-----------------------------v-------------------------------+
|              Rust Engine (veritas_engine.dll / .so / .dylib) |
|  Parser | Classifier (GGUF) | RAG (SQLite+vec) | Heuristics |
|  Model Manager (download, verify, cache, hot-swap)           |
+-------------------------------------------------------------+
        |                         |
  +-----v-----+            +------v------+
  | GGUF      |            | Fact DB     |
  | models/   |            | SQLite+vec  |
  +-----------+            +-------------+
```

There is no network service. The Rust library is loaded into the UI process. Calls are direct function calls across the FFI boundary. No serialization over sockets, no port, no daemon, no backend.

### 3.2 Why In-Process FFI (No Backend)

- Single deployable unit, simpler installer.
- Lowest possible latency (no IPC, no serialization).
- No port conflicts, no firewall prompts, no service management.
- Works in restricted environments (corporate, offline).
- Crash isolation is sacrificed, mitigated by safe Rust, panic boundaries, and a recovery mode (see Section 13.5).

### 3.3 Component Responsibilities

| Component | Language | Responsibility |
|-----------|----------|----------------|
| Veritas.App | C# | UI, MVVM, theming, settings, history |
| Veritas.Core | C# | DTOs, FFI wrapper, config, logging, storage |
| veritas_engine | Rust (cdylib) | Inference, RAG, heuristics, model management |
| veritas-core (Rust lib) | Rust | Shared logic, pure functions, testable |
| veritas-models (Rust lib) | Rust | Download and cache models |
| veritas-proto (docs) | Markdown/JSON | FFI contract documentation (not protobuf) |

### 3.4 FFI Contract (C ABI)

The contract is a C header (`veritas.h`) generated by `cbindgen` from Rust. C# calls via `DllImport`. Memory ownership rules are explicit.

Core functions:

```
// Lifecycle
VeritasEngine* veritas_init(const char* config_json);
void           veritas_free(VeritasEngine* engine);

// Analysis (synchronous; progress via callback; result returned as owned string)
char* veritas_analyze(
    VeritasEngine* engine,
    const char* request_json,
    VeritasProgressCb progress_cb,
    void* user_data
);

// Cancellation
void veritas_cancel(VeritasEngine* engine, uint64_t request_id);

// Memory
void veritas_free_string(char* s);

// Health
char* veritas_health(VeritasEngine* engine);

// Models
char* veritas_list_models(VeritasEngine* engine);
int   veritas_download_model(VeritasEngine* engine, const char* model_id,
                             VeritasProgressCb cb, void* user_data);
```

Callback type:

```
typedef void (*VeritasProgressCb)(const char* json_event, void* user_data);
```

Rules:
- Rust owns all returned `char*`. C# must call `veritas_free_string`.
- Request and response are UTF-8 JSON. No binary marshaling.
- Progress events are JSON objects with `stage`, `percent`, `message`.
- The engine handle is thread-safe (internal mutex + async runtime).
- C# wraps calls in `Task.Run` to keep the UI thread free.
- Panic in Rust is caught at the FFI boundary and converted to an error JSON. No unwind across FFI.

### 3.5 Data Flow (UC1: Paste text)

1. UI collects text, builds `AnalyzeRequest` JSON.
2. C# calls `veritas_analyze` on a background task.
3. Rust normalizes text, detects language, segments sentences.
4. Rust extracts atomic claims via LLM (JSON schema output).
5. For each claim, Rust embeds and retrieves top-k evidence from the local fact DB (RAG).
6. Rust classifies each claim (verdict, confidence, reasoning) using the LLM.
7. Rust applies heuristics (domain reputation, buzzer signals, clickbait).
8. Rust aggregates into a document-level verdict with calibrated confidence.
9. Rust invokes progress callbacks during steps 3-8.
10. Rust returns the final `AnalyzeResponse` JSON.
11. C# frees the string, deserializes, renders.

No network is used in this flow.

---

## 4. Technology Stack

### 4.1 C# / .NET
- .NET 8 LTS (or 9), AOT-friendly, single-file publish.
- Avalonia 11.x for UI.
- Avalonia Fluent Theme plus custom design tokens.
- CommunityToolkit.Mvvm for source-generated MVVM.
- Microsoft.Extensions.DependencyInjection and Logging.
- Serilog for structured logging.
- Velopack for installer and auto-update.
- Microsoft.Data.Sqlite for local history.
- System.Text.Json for serialization.
- P/Invoke via `LibraryImport` source generators (.NET 7+).

### 4.2 Rust
- Rust stable (latest).
- tokio for async runtime inside the library.
- llama-cpp-2 for GGUF inference.
- rusqlite + sqlite-vec for vector store.
- reqwest for model and DB downloads (first-run only, user-initiated).
- sha2 for checksums.
- tracing for logging (forwarded to C# via callback or file).
- cbindgen for header generation.
- thiserror / anyhow for errors.
- serde + serde_json for JSON.

### 4.3 Build and CI
- GitHub Actions matrix: windows-latest, macos-latest, ubuntu-latest.
- cargo + dotnet CLI.
- cbindgen in build pipeline.
- Inno Setup (Windows alt), create-dmg (mac), appimagetool (Linux).

---

## 5. Repository Structure

```
veritas/
  plan.md
  README.md
  .github/workflows/
    ci.yml
    release.yml
  include/
    veritas.h                  (generated)
  src/
    app/
      Veritas.App/             (Avalonia)
      Veritas.Core/            (DTOs, FFI, config)
      Veritas.Tests/           (xUnit)
    engine/
      Cargo.toml               (workspace)
      veritas-engine/          (cdylib crate)
      veritas-core/            (lib: logic)
      veritas-models/          (lib: download/cache)
  data/
    factdb/                  (seed snapshot)
    prompts/                 (versioned prompts)
    taxonomies/              (hoax taxonomy ID)
  models/                    (gitignored)
  scripts/
    build.ps1
    build.sh
    download-models.sh
    eval.py
  installer/
    windows/
    macos/
    linux/
  docs/
    architecture.md
    ffi.md
    prompts.md
    eval.md
    threat-model.md
```

---

## 6. Phases and Sub-Phases

### Phase 0 - Foundation and Decisions (Week 1)
- 0.1 Finalize tech stack and model choice.
- 0.2 Set up repository, branching strategy (trunk-based with tags), commit conventions.
- 0.3 CI skeleton: build matrix, lint, test, format checks.
- 0.4 Draft FFI contract (`veritas.h`) and JSON schemas.
- 0.5 Logging and observability baseline.
- 0.6 Legal review: model licenses, data source terms.
- 0.7 Threat model (STRIDE) draft.

Deliverable: repository skeleton, green CI, contract draft, threat model draft.

### Phase 1 - Rust Engine Core (Weeks 2-4)
- 1.1 Scaffold Rust workspace, cdylib crate, cbindgen.
- 1.2 FFI boundary: init, free, analyze, cancel, free_string, health.
- 1.3 Panic boundary: catch_unwind at every FFI entry.
- 1.4 Model manager: download with resume, SHA-256 verify, cache dir, hot-swap.
- 1.5 Integrate llama-cpp-2: load GGUF, basic completion, streaming tokens.
- 1.6 Prompt templates (versioned, JSON schema output).
- 1.7 Claim extraction module.
- 1.8 Classification module (verdict, confidence, reasoning).
- 1.9 Progress callbacks during analysis.
- 1.10 Error handling: structured error JSON, no panics across FFI.
- 1.11 Unit tests and golden tests (prompt to expected JSON).

Deliverable: engine library that can `Analyze(text)` and return verdict JSON.

### Phase 2 - RAG and Fact Database (Weeks 4-5)
- 2.1 SQLite schema + sqlite-vec.
- 2.2 Embedding pipeline (bge-m3 GGUF or candle).
- 2.3 Ingestion scripts: TurnBackHoax, Mafindo, Kompas, AFP, Liputan6, Tempo.
- 2.4 Chunking and dedup (cosine > 0.95).
- 2.5 Retrieval: top-k cosine + MMR.
- 2.6 Seed snapshot builder (<= 50 MB compressed).
- 2.7 Update mechanism (online sync, delta, user-initiated).
- 2.8 Evaluation: retrieval recall@k.

Data sources (content):
- TurnBackHoax (turnbackhoax.id)
- Mafindo (mafindo.or.id)
- Kompas Cek Fakta (kompas.com/cekfakta)
- AFP Fact Check Indonesia (factcheck.afp.com/afp-indonesia)
- Liputan6 Cek Fakta
- Tempo Cek Fakta
- (English) Snopes, PolitiFact, Reuters Fact Check, Full Fact

### Phase 3 - Heuristics and Anti-Buzzer (Weeks 5-6)
- 3.1 Domain reputation list (trusted, neutral, suspicious).
- 3.2 URL analysis (shorteners, suspicious TLD, redirects, IP hosts).
- 3.3 Buzzer pattern detection:
  - Repeated imperative phrases ("share!", "viralkan!").
  - Extreme sentiment (anger, hatred).
  - Claims without sources.
  - Copy-paste identical text detection.
  - (v2) Timing burst, account age (requires metadata).
- 3.4 Clickbait headline detector (regex + LLM judge).
- 3.5 Aggregation scorer (weighted fusion of LLM + heuristics).
- 3.6 Calibration (temperature scaling) on validation set.
- 3.7 Abstain logic (confidence below threshold -> "insufficient evidence").

### Phase 4 - C# Avalonia UI (Weeks 6-9)
- 4.1 Scaffold Avalonia project, DI, theming.
- 4.2 Design system: colors, typography, spacing, components, tokens.
- 4.3 Screens: Home, Analyze, History, Settings, About.
- 4.4 FFI wrapper in C# with `LibraryImport`, async Task wrappers.
- 4.5 Engine lifecycle: load on startup, unload on exit, safe mode on failure.
- 4.6 First-run wizard: model download, DB download, consent.
- 4.7 Offline/online status indicator.
- 4.8 Localization (ID/EN) via .resx.
- 4.9 Accessibility: keyboard nav, screen reader, contrast (WCAG 2.1 AA).
- 4.10 Animations and micro-interactions (restrained, elegant).

### Phase 5 - Installer and Distribution (Weeks 9-10)
- 5.1 Velopack setup (Windows Setup.exe).
- 5.2 macOS DMG + code signing + notarization.
- 5.3 Linux AppImage + .deb + .rpm.
- 5.4 Auto-update channel (stable/beta) with delta.
- 5.5 Uninstaller + data cleanup policy.
- 5.6 Size optimization (trim, AOT, compression).

### Phase 6 - Evaluation and Hardening (Weeks 10-12)
- 6.1 Build Indonesian benchmark dataset (>= 2000 labeled samples).
- 6.2 Metrics: accuracy, precision, recall, F1 per class, ECE.
- 6.3 Red-team: adversarial prompts, prompt injection, jailbreak.
- 6.4 Performance profiling: CPU, RAM, latency p50/p95.
- 6.5 Security audit: input sanitization, path traversal, model integrity.
- 6.6 Privacy review: no telemetry default, opt-in only.
- 6.7 Closed beta (20 users), feedback collection.

### Phase 7 - Release and Post-Launch (Week 12+)
- 7.1 V1.0-Alpha release on GitHub Releases (tag `v1.0.0-alpha`).
- 7.2 Documentation: user guide, FAQ, model card, prompt cards.
- 7.3 Feedback loop: in-app feedback to GitHub issues.
- 7.4 Roadmap v1.1 (OCR, GPU, Flatpak).

---

## 7. Detection Pipeline (Detail)

### 7.1 Input Normalization
- Strip HTML, normalize whitespace, unicode NFC.
- Detect language (fast heuristic + optional fasttext).
- Split sentences, extract metadata (URLs, dates, named entities).

### 7.2 Claim Extraction (LLM, JSON Schema)

Prompt v1:

```
You are a fact-checking assistant. Extract atomic, verifiable claims from the text.
Return JSON only: { "claims": [ { "id": int, "text": str, "type": "factual|opinion|prediction", "importance": 1-5 } ] }
Only factual claims. Max 8 claims. No commentary outside JSON.
```

- Temperature 0.1, top_p 0.9.
- Validate against JSON schema. On failure, retry with repair prompt (max 2 retries).
- On repeated failure, return partial result with error flag.

### 7.3 Classification (Per Claim)

```
Given claim C and evidence E (retrieved), classify:
- verdict: "hoax" | "valid" | "misleading" | "unverifiable"
- confidence: 0.0-1.0
- reasoning: short explanation (<= 60 words)
- sources: list of evidence ids used
Output JSON only.
```

- Few-shot examples (3-5) from the labeled dataset.
- Chain-of-thought is internal; output is only JSON.
- Abstain when evidence is empty or confidence below threshold.

### 7.4 Aggregation

- Per-claim verdict becomes document-level verdict by importance-weighted majority.
- Heuristic adjustments: domain reputation score, buzzer signal score, clickbait score.
- Final confidence: calibrated fusion (temperature scaling).

### 7.5 Explainability Payload

```json
{
  "verdict": "hoax",
  "confidence": 0.92,
  "summary": "...",
  "claims": [],
  "evidence": [],
  "heuristics": { "domain_score": 0.3, "buzzer_score": 0.7, "clickbait_score": 0.1 },
  "audit": {
    "model": "qwen2.5-7b-q4",
    "prompt_version": "cls_v3",
    "engine_version": "1.0.0",
    "timestamp_ms": 0,
    "input_hash": "sha256:..."
  }
}
```

Every verdict ships with this payload. No black-box output.

---

## 8. Model Strategy

### 8.1 Model Matrix

| Tier | Model | Size (Q4) | RAM min | Use case |
|------|-------|-----------|---------|----------|
| Standard (default) | Qwen 2.5 7B Instruct | ~4.7 GB | 8 GB | Primary accuracy |
| Lightweight | Qwen 2.5 3B Instruct | ~2.0 GB | 4 GB | Low RAM / fast |
| Embedding | bge-m3 (GGUF) | ~1.1 GB | 4 GB | RAG retrieval |
| Tiny (fallback) | Gemma 2 2B | ~1.6 GB | 4 GB | Very constrained |

### 8.2 Selection Logic
- On first run, detect available RAM.
- If RAM >= 8 GB, default to Standard; else Lightweight.
- User can override in Settings.
- If selected model fails to load, fall back to next smaller tier with a warning.

### 8.3 Download and Cache
- Cache dir: `%LOCALAPPDATA%/Veritas/models` (Windows), `~/Library/Application Support/Veritas/models` (macOS), `~/.local/share/veritas/models` (Linux).
- Metadata file `models.json` with version, sha256, size.
- Download over HTTPS with resume (Range requests) and progress events to UI.
- Verify SHA-256 before use. On mismatch, re-download.
- Hot-swap model without restarting the app.

### 8.4 Prompt Versioning
- Prompts live in `data/prompts/*.md` with frontmatter (version, target model, date).
- Prompt hash is recorded in the audit payload.
- Prompts are immutable per version; new version = new file.

---

## 9. Fact Database (RAG)

### 9.1 Schema

```sql
CREATE TABLE claims (
  id INTEGER PRIMARY KEY,
  text TEXT NOT NULL,
  verdict TEXT NOT NULL,
  source TEXT NOT NULL,
  url TEXT,
  date TEXT,
  embedding BLOB,
  lang TEXT DEFAULT 'id'
);
CREATE VIRTUAL TABLE claims_vec USING vec0(embedding float[1024]);
```

### 9.2 Ingestion Pipeline
- Scraper (Python, dev-only) produces JSONL.
- Rust importer chunks, embeds, dedups (cosine > 0.95).
- Chunk size 512 tokens, overlap 64.
- Rebuild snapshot weekly; delta sync on user request.

### 9.3 Retrieval
- Query embedding, top-20 cosine, MMR (lambda 0.7), final top-5.
- Filter by language and prefer recent.
- Return snippet, source, URL, date, score.

### 9.4 Seed Snapshot
- Compressed SQLite, <= 50 MB, bundled via download on first run.
- Contains ~50k verified claims.

---

## 10. Heuristics and Anti-Buzzer Detail

### 10.1 Domain Reputation

| Tier | Examples | Score |
|------|----------|-------|
| Trusted | kompas, tempo, detik, antaranews, liputan6, bbc, reuters, afp | 0.9-1.0 |
| Neutral | blogs, forums, personal sites | 0.4-0.6 |
| Suspicious | new domains (<6 months), odd TLDs, IP hosts, URL shorteners | 0.0-0.3 |

### 10.2 Buzzer Signal Table

| Signal | Weight | Detection |
|--------|--------|-----------|
| Repeated imperative ("share!", "viralkan!") | 0.25 | regex |
| Extreme sentiment (anger, hatred) | 0.20 | lexicon + LLM |
| Claim without any source | 0.15 | NER + check |
| Copy-paste identical text across posts | 0.20 | hash + similarity (v1: text-level) |
| Anonymous or no-profile author | 0.20 | heuristic on text only (v2: metadata) |

### 10.3 Clickbait
- Headline with exaggerated numbers, ALL CAPS, excessive emoji.
- Mismatch between headline and body (LLM judge).

### 10.4 Aggregation Formula

```
final_score = 0.55 * llm_score + 0.20 * domain_score + 0.15 * buzzer_score + 0.10 * clickbait_score
verdict = threshold(final_score, user_threshold)
if confidence < abstain_threshold: verdict = "unverifiable"
```

Weights are tunable and documented. Calibration via temperature scaling on validation set.

---

## 11. Indonesian Hoax Taxonomy

Veritas classifies into five categories:

1. Hoax: fabricated or demonstrably false.
2. Misleading: contains some truth but framed deceptively.
3. Clickbait: headline exaggerates or misrepresents body.
4. Buzzer: coordinated amplification pattern (content may be true or false).
5. Valid: consistent with verified sources.

A single item can carry multiple tags (e.g., "misleading + buzzer").

---

## 12. UI/UX Design

### 12.1 Design Principles
- Clean, minimal, trust-inspiring. Blue/teal palette with restrained accents.
- Typography: Inter or system font, clear hierarchy.
- Dark and light mode, follow OS.
- Responsive: minimum width 900px, scalable.
- Micro-interactions: progress streaming, confidence gauge animation. Restrained, not flashy.

### 12.2 Screens

Home:
- Hero text: "Cek kebenaran berita dalam hitungan detik".
- Large input box with tabs (Text / URL).
- Quick example chips.
- Three most recent checks.

Analyze:
- Progress stepper: Normalize, Extract claims, Retrieve, Classify, Aggregate.
- Result card: verdict badge (color-coded), confidence gauge, summary.
- Claims list (expandable) with evidence and sources.
- Heuristics panel (collapsible).
- Actions: copy report, export JSON, share.

History:
- Search and filter (verdict, date range).
- Card list with snippet.
- Bulk export.

Settings:
- Model selector and download manager.
- Threshold slider (strict / balanced / lenient).
- Language, theme, update channel.
- Privacy: telemetry toggle (default off).

About:
- Version, licenses, credits, links.

### 12.3 Accessibility (WCAG 2.1 AA)
- Keyboard navigation for all actions.
- Shortcuts: Ctrl+Enter analyze, Ctrl+L clear, Esc cancel.
- Screen reader labels on all interactive elements.
- Contrast ratio >= 4.5:1 for text.
- High-contrast mode support.
- No color-only signaling (icons + text).

### 12.4 Copy Guidelines
- Verdicts use plain language ("Terindikasi hoax", not "False").
- Confidence shown as percentage with verbal band (Tinggi/Sedang/Rendah).
- Abstain message: "Bukti tidak cukup untuk menyimpulkan. Verifikasi manual disarankan."
- Always include disclaimer: "Hasil ini adalah bantuan, bukan keputusan final."

---

## 13. Reliability and Recovery

### 13.1 Safe Mode
If the engine fails to load or crashes during init, the app enters Safe Mode:
- Shows diagnostic info.
- Offers "Reset model" and "Re-download" actions.
- Disables analysis until resolved.
- Logs bundle available for support.

### 13.2 Crash Handling
- Rust panics caught at FFI boundary, converted to error JSON.
- C# wraps FFI calls in try/catch; on access violation, app shows recovery dialog and restarts engine load.
- Auto-save of in-progress input.

### 13.3 Data Integrity
- Atomic writes for history and config (write temp + rename).
- SQLite WAL mode for crash safety.
- Model files verified by hash before load.

---

## 14. Installer and Distribution

### 14.1 Windows
- Velopack Setup.exe, per-user install default, optional elevation.
- Bundles: app + native engine DLL + .NET runtime (framework-dependent to reduce size).
- Shortcuts: Start Menu, Desktop (opt-in).
- File association for `.veritas` report files (optional).

### 14.2 macOS
- DMG with drag-to-Applications.
- Code signing (Developer ID) + notarization + staple.
- Universal binary (x86_64 + arm64) or separate per arch.

### 14.3 Linux
- AppImage (portable, no root).
- .deb (Debian/Ubuntu) and .rpm (Fedora).
- Flatpak (v1.1).

### 14.4 Auto-Update
- Velopack delta updates (only changed bytes).
- Channels: stable (default), beta (opt-in).
- Silent check on startup; prompt user before applying.
- Rollback: keep previous version for one update cycle.

### 14.5 Size Budget
- Installer (without model): <= 80 MB.
- Model download: first-run wizard with time estimate.
- Total disk (standard setup): ~7 GB (model + DB + app).

---

## 15. Testing Strategy

### 15.1 Unit Tests
- Rust: cargo test (core logic, parsing, scoring, FFI safety).
- C#: xUnit (ViewModels, services, FFI wrapper).

### 15.2 Integration Tests
- FFI round-trip tests (JSON in/out).
- End-to-end analysis with a tiny model (Gemma 2B) in CI.

### 15.3 Evaluation (ML)
- Dataset: >= 2000 labeled Indonesian samples.
- Split: 70/15/15 (train/val/test). Models are not fine-tuned; prompt tuning on val.
- Metrics: accuracy, macro-F1, per-class precision/recall, confusion matrix, ECE.
- Release gate: macro-F1 >= 0.85, ECE <= 0.1.
- Ablation: with/without RAG, with/without heuristics.

### 15.4 Performance
- Latency p50 <= 5s, p95 <= 12s (7B Q4, 8-core CPU).
- RAM <= 6 GB peak.
- Startup <= 3s.
- Model load <= 10s.

### 15.5 Security
- Fuzz input (malformed UTF-8, huge text, null bytes).
- Path traversal in file open.
- Model file tampering (hash check).
- Prompt injection red-team suite.
- FFI boundary fuzzing.

---

## 16. Dataset Construction

### 16.1 Sources
- TurnBackHoax, Mafindo, Kompas, AFP, Liputan6, Tempo (hoax + valid).
- Balanced across categories and time (2019-2026).

### 16.2 Labeling Guidelines
- Two independent annotators per sample.
- Disagreement resolved by a third reviewer.
- Inter-annotator agreement target: Cohen's kappa >= 0.75.
- Labels: hoax, misleading, clickbait, buzzer, valid (multi-label).

### 16.3 Quality Gates
- No duplicate or near-duplicate samples (cosine > 0.9).
- Minimum 200 samples per category.
- Stratified by source and time.

---

## 17. Security, Privacy, and Compliance

- Telemetry default OFF. Opt-in anonymous counts (analyze count, model used). Never content, never PII.
- History stored locally; optional encryption (DPAPI/Keychain/libsecret).
- No cloud: inference is local; network used only for user-initiated updates/downloads.
- Model licenses documented: Qwen (Apache 2.0), Gemma (Gemma ToS), bge (MIT).
- Data source terms respected: store snippets + URLs, not full articles.
- GDPR-style: one-click delete history and cache.
- Input sanitization at FFI boundary (length limits, UTF-8 validation).

---

## 18. Observability

- Structured JSON logs, daily rotation, max 50 MB total.
- Crash reports: opt-in, anonymized, uploaded as GitHub issues.
- In-app Diagnostics bundle (logs + config + model info) for support.
- Local metrics: latency, confidence distribution (for calibration monitoring).
- No remote telemetry endpoint.

---

## 19. Threat Model (STRIDE Summary)

| Threat | Category | Mitigation |
|--------|----------|------------|
| Malicious model file | Tampering | SHA-256 verify, trusted source list |
| Prompt injection via input | Tampering | Strict JSON schema, input length limits, no tool use |
| Path traversal in file open | Elevation | Canonicalize paths, sandbox to allowed dirs |
| Memory corruption in FFI | Denial of service | Safe Rust, panic boundaries, fuzzing |
| Data exfiltration | Info disclosure | No network in analysis path; telemetry opt-in |
| Model license violation | Legal | Document licenses; bundle only allowed models |
| False accusation harm | Ethics | Abstain + disclaimer + human review guidance |

Full threat model: docs/threat-model.md.

---

## 20. Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| 7B model too heavy for old laptops | High | Tiered models, auto-select by RAM |
| Low accuracy on Indonesian (limited data) | High | RAG + heuristics; collect dataset; feedback loop |
| Slow CPU inference | Medium | Q4_K_M quant, thread tuning, optional GPU in v1.1 |
| False positives harming reputation | High | Abstain + threshold; disclaimer; human review |
| Prompt injection | Medium | Strict schema; sanitize; no tools |
| Installer bloat | Low | Framework-dependent publish + compression |
| UI inconsistency across OS | Medium | Design tokens + testing matrix |

---

## 21. Roadmap

- v0.1 (M3): Engine + CLI, no UI.
- v0.5 (M6): Avalonia UI alpha, text only.
- v0.9 (M9): Beta with URL + history + installer.
- V1.0-Alpha (M12): First public alpha. Feature-complete core, known limitations documented. Not production-stable.
- V1.0-Beta (M14): Hardening, performance, accessibility, installer polish.
- v1.0.0 (M16): Stable release.
- v1.1: OCR (screenshot), GPU acceleration (CUDA/Metal), Flatpak.
- v1.2: Online fact-check (opt-in), browser extension.
- v2.0: Video/audio, multi-modal, fine-tuned Indonesian model.

---

## 22. Code Quality and Governance

### 22.1 Style
- Rust: rustfmt + clippy (deny warnings in CI).
- C#: dotnet format, StyleCop, nullable reference types enabled.
- Markdown: markdownlint.

### 22.2 Quality Gates (CI)
- Format check.
- Lint (clippy, StyleCop).
- Unit tests.
- Integration tests with tiny model.
- FFI safety tests.
- Size budget check.

### 22.3 Branching
- trunk-based: `main` is always releasable.
- Feature branches: `feat/<short>`, `fix/<short>`.
- Tags: `v1.0.0`.

### 22.4 Commit Convention
- Conventional Commits: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`.
- No merge commits on main (rebase).

### 22.5 Dependency Policy
- Pin versions in lockfiles.
- Review new dependencies (license + maintenance).
- Prefer well-maintained crates/packages.
- No telemetry or analytics dependencies.

---

## 23. Documentation Plan

- README.md: quick start, build, install.
- docs/architecture.md: deep dive.
- docs/ffi.md: C ABI contract and memory rules.
- docs/prompts.md: prompt versions and examples.
- docs/eval.md: dataset and metrics.
- docs/threat-model.md: STRIDE.
- In-app help and FAQ.
- Model card (per model).
- Prompt cards (per prompt version).

---

## 24. Open Questions

1. Model default final: Qwen 2.5 7B vs Llama 3.1 8B? (Recommendation: Qwen for Indonesian.)
2. GPU support in v1? (Recommendation: defer to v1.1 to keep build simple.)
3. Export report format: PDF (QuestPDF) vs HTML? (Recommendation: JSON + HTML in v1, PDF in v1.1.)
4. User accounts? (Recommendation: none, fully local and anonymous.)
5. Data sourcing: scraping vs public HuggingFace datasets? (Recommendation: public datasets + licensed scraping.)

---

## 25. References and Links

### Frameworks
- Avalonia UI: https://avaloniaui.net
- .NET 8: https://dotnet.microsoft.com/download/dotnet/8.0
- Rust: https://www.rust-lang.org
- llama.cpp: https://github.com/ggerganov/llama.cpp
- llama-cpp-rs: https://github.com/utilityai/llama-cpp-rs
- candle: https://github.com/huggingface/candle
- sqlite-vec: https://github.com/asg017/sqlite-vec
- Velopack: https://velopack.io
- cbindgen: https://github.com/mozilla/cbindgen

### Models
- Qwen 2.5: https://huggingface.co/Qwen/Qwen2.5-7B-Instruct-GGUF
- bge-m3: https://huggingface.co/BAAI/bge-m3
- Gemma 2: https://huggingface.co/google/gemma-2-2b-it-GGUF

### Data
- TurnBackHoax: https://turnbackhoax.id
- Mafindo: https://mafindo.or.id
- Kompas Cek Fakta: https://www.kompas.com/cekfakta
- AFP Fact Check ID: https://factcheck.afp.com/afp-indonesia
- HuggingFace datasets (ID hoax): https://huggingface.co/datasets?search=hoax+indonesian

### Tools
- Inno Setup: https://jrsoftware.org/isinfo.php
- create-dmg: https://github.com/create-dmg/create-dmg
- AppImage: https://appimage.org
- QuestPDF: https://www.questpdf.com

### Standards
- WCAG 2.1: https://www.w3.org/WAI/WCAG21/quickref/
- OWASP Top 10: https://owasp.org/Top10/
- Conventional Commits: https://www.conventionalcommits.org

---

## Appendix A - FFI Contract (Draft C Header)

```c
#ifndef VERITAS_H
#define VERITAS_H

#include <stdint.h>

typedef struct VeritasEngine VeritasEngine;

typedef void (*VeritasProgressCb)(const char* json_event, void* user_data);

VeritasEngine* veritas_init(const char* config_json);
void           veritas_free(VeritasEngine* engine);

char* veritas_analyze(
    VeritasEngine* engine,
    const char* request_json,
    VeritasProgressCb progress_cb,
    void* user_data
);

void veritas_cancel(VeritasEngine* engine, uint64_t request_id);
void veritas_free_string(char* s);

char* veritas_health(VeritasEngine* engine);
char* veritas_list_models(VeritasEngine* engine);
int   veritas_download_model(VeritasEngine* engine, const char* model_id,
                             VeritasProgressCb cb, void* user_data);

#endif
```

---

## Appendix B - JSON Schemas

### AnalyzeRequest

```json
{
  "text": "string (required if url empty)",
  "url": "string (required if text empty)",
  "lang_hint": "id | en | auto",
  "options": {
    "threshold": 0.5,
    "include_heuristics": true,
    "max_claims": 8
  }
}
```

### AnalyzeResponse

```json
{
  "verdict": "hoax | valid | misleading | unverifiable",
  "confidence": 0.0,
  "summary": "string",
  "claims": [
    {
      "id": 1,
      "text": "string",
      "type": "factual | opinion | prediction",
      "importance": 1,
      "verdict": "hoax | valid | misleading | unverifiable",
      "confidence": 0.0,
      "reasoning": "string",
      "evidence_ids": [1]
    }
  ],
  "evidence": [
    {
      "id": 1,
      "text": "string",
      "source": "string",
      "url": "string",
      "date": "string",
      "score": 0.0
    }
  ],
  "heuristics": {
    "domain_score": 0.0,
    "buzzer_score": 0.0,
    "clickbait_score": 0.0,
    "signals": ["string"]
  },
  "audit": {
    "model": "string",
    "prompt_version": "string",
    "engine_version": "string",
    "timestamp_ms": 0,
    "input_hash": "string"
  }
}
```

### Progress Event

```json
{
  "stage": "normalize | extract | retrieve | classify | aggregate",
  "percent": 0.0,
  "message": "string"
}
```

---

## Appendix C - Config Schema

Engine (`config.toml`):

```toml
[model]
default = "qwen2.5-7b-q4"
cache_dir = ""

[inference]
threads = 0
ctx_size = 4096
gpu_layers = 0

[detection]
threshold = 0.5
max_claims = 8
abstain_below = 0.4

[rag]
top_k = 5
mmr_lambda = 0.7
```

App (`settings.json`): managed by C#, mirrors relevant subset.

---

## Appendix D - Model Card Template

For each model shipped or recommended:

- Name and version.
- Source URL and license.
- Size (Q4) and RAM requirement.
- Languages supported.
- Intended use and limitations.
- Known failure modes.
- Evaluation results (macro-F1, ECE) on the benchmark.
- Date added.

---

## Appendix E - Prompt Card Template

For each prompt version:

- Version ID (e.g., `cls_v3`).
- Target model.
- Purpose.
- Full prompt text.
- JSON schema.
- Few-shot examples.
- Evaluation score on val set.
- Date and author.
- Changelog vs previous.

---

End of document. Feedback and revisions welcome before Phase 0 begins.
