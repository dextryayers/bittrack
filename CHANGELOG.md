# Changelog

All notable changes to Veritas are documented here.
Format follows Keep a Changelog. Versioning follows Semantic Versioning.

## [1.0.0-alpha] - 2026-10-31

### Added

- Rust native engine (`veritas_engine`) exposed through a documented C ABI.
- Local RAG fact store backed by SQLite with hashing embeddings and cosine retrieval.
- Built-in Indonesian fact seed dataset.
- Heuristics: domain reputation, URL analysis, buzzer patterns, clickbait, weighted aggregation, abstain, calibration.
- C# Avalonia UI with sidebar navigation, five views, animations, dark/light themes.
- History persistence (SQLite) and settings persistence (JSON).
- Evaluation harness (`scripts/eval.py`) with accuracy, macro-F1, per-class metrics, confusion matrix, ECE.
- Latency benchmark (`scripts/bench.py`).
- Installer scaffolding for Windows (Inno Setup, Velopack), macOS (DMG), Linux (AppImage).
- Release workflow with cross-platform build matrix.

### Known Limitations

- LLM inference (GGUF) is feature-gated and not enabled by default; heuristic baseline is active.
- Semantic embeddings (bge-m3) are planned; hashing embedder is used in alpha.
- No OCR, video, or audio support.
- Installer binaries are not produced in CI yet; scripts require local tooling.

[1.0.0-alpha]: https://github.com/veritas/veritas/releases/tag/v1.0.0-alpha
