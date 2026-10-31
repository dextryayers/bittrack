# Veritas

Hoax and buzzer analysis for Windows, macOS, and Linux. Inference is local; no
backend and no cloud model.

V1.0-Alpha.

## Architecture

- UI: C# + Avalonia (MVVM), sidebar navigation, poster tab, dark/light themes
- Engine: Rust native library (cdylib) loaded in-process via C ABI
- Inference: local GGUF model through llama.cpp (`llama-cpp-2`), JSON-schema
  constrained output. Default model: Qwen 2.5 3B Instruct Q4_K_M.
- Evidence: live Bing search restricted to trusted Indonesian fact-check,
  newsroom, and government hosts, plus the Indonesian Wikipedia API.
- Poster OCR: Windows.Media.Ocr, on-device, no third-party runtime.
- Heuristics: domain, buzzer, and clickbait signals are displayed for context
  and never generate a verdict.
- No keyword or seeded-fact fallback: if the model is missing, analysis fails
  with `model_unavailable`.

See `plan.md` for the original plan and `docs/model-card.md` for current limits.

## Build

Rust engine (requires libclang for bindgen; set `LIBCLANG_PATH`):

```
cd src/engine
cargo build
cargo test --workspace
```

.NET app (multi-targets `net10.0` and `net10.0-windows10.0.19041.0`):

```
cd src/app
dotnet build Veritas.Core/Veritas.Core.csproj
dotnet build Veritas.App/Veritas.App.csproj
```

Copy the native library next to the app binary before running:

```
copy src\engine\target\debug\veritas_engine.dll src\app\Veritas.App\bin\Debug\net10.0-windows10.0.19041.0\
```

Download the default model from the app's Pengaturan tab (1.93 GB, resumed and
SHA-256 verified), or place a GGUF file in
`%LOCALAPPDATA%\Veritas\models\qwen2.5-3b-instruct-q4_k_m.gguf`.

## Real-model test

```
cd src/engine
cargo test -p veritas-engine real_model -- --ignored --nocapture
```

## Evaluate

```
python scripts/eval.py --engine src/engine/target/debug/veritas_engine.dll --dataset data/eval/benchmark.jsonl
python scripts/bench.py --engine src/engine/target/debug/veritas_engine.dll --dataset data/eval/benchmark.jsonl
```

The bundled 36-sample set was used to develop the removed heuristic baseline.
It is not a held-out estimate for the GGUF pipeline, and no external accuracy
claim should be made until a fresh labeled set is evaluated.

## Installers

- Windows: `installer/windows/build.ps1`, Inno Setup script `installer/windows/veritas.iss`
- macOS: `installer/macos/build.sh`
- Linux: `installer/linux/build.sh`

## Docs

- `docs/user-guide.md`
- `docs/faq.md`
- `docs/model-card.md`
- `docs/prompt-card.md`
- `SECURITY.md`
- `CONTRIBUTING.md`
- `CHANGELOG.md`

## License

MIT OR Apache-2.0
