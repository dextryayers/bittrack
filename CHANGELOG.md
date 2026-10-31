# Changelog

All notable changes to Veritas are documented here.
Format follows Keep a Changelog. Versioning follows Semantic Versioning.

## [1.0.0-alpha] - 2026-10-31

### Added

- Rust native engine (`veritas_engine`) exposed through a documented C ABI.
- Local GGUF inference through llama.cpp (`llama-cpp-2`), constrained by a
  JSON schema. Default model: Qwen 2.5 3B Instruct Q4_K_M.
- Model catalog and resumable downloads from the Ollama registry, verified with
  SHA-256 and a sidecar marker.
- Live evidence retrieval: Bing HTML search restricted to trusted Indonesian
  fact-check, newsroom, and government hosts, plus the Indonesian Wikipedia API.
- Article extraction for news URLs.
- Poster OCR through Windows.Media.Ocr, exposed as a separate Poster tab.
- Result contract with verdict, confidence, truth percentage, summary, claims,
  numbered evidence, red flags, and warnings.
- Heuristic domain, buzzer, and clickbait signals, displayed for context only.
- History persistence (SQLite) and settings persistence (JSON).
- Evaluation harness (`scripts/eval.py`) with accuracy, macro-F1, per-class
  metrics, confusion matrix, and ECE.
- Latency benchmark (`scripts/bench.py`).
- Installer scaffolding for Windows (Inno Setup, Velopack), macOS (DMG), Linux
  (AppImage).
- Release workflow with cross-platform build matrix.

### Changed

- The local model is now the only component that produces a verdict. Keyword
  scoring, weighted aggregation, and seeded-fact retrieval were removed.
- Missing, unloadable, or corrupt models fail with `model_unavailable`; the
  engine no longer falls back to heuristics.
- A verdict below the confidence threshold is demoted to `unverifiable`, with
  the truth percentage and confidence still reported.
- Reader comments are accepted as buzzer context, and as the claim under review
  when no article or poster text is supplied.
- The UI adds URL, comment, and poster inputs, model management, truth
  percentage, red flags, and warnings.
- Benchmark scripts run with online evidence disabled for reproducibility.

### Removed

- `veritas-engine/src/rag.rs`, the SQLite fact store, and the `rusqlite`
  dependency.
- `scripts/calibrate.py` and the heuristic temperature calibration.

### Known Limitations

- No held-out evaluation of the GGUF pipeline exists. The 36-sample benchmark
  was used to develop the removed heuristic baseline and is not an accuracy
  claim for the model.
- CPU inference on a 4-core machine can take tens of seconds to a few minutes
  per article.
- Evidence retrieval depends on Bing HTML layout and may return nothing for
  niche claims.
- Poster OCR uses the installed Windows recognizer; stylized text can be
  misread, and extracted text must be checked before analysis.
- No video or audio support.
- Installer binaries are not produced in CI yet; scripts require local tooling.

[1.0.0-alpha]: https://github.com/veritas/veritas/releases/tag/v1.0.0-alpha
