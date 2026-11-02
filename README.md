# Veritas

Hoax and buzzer analysis for Windows, macOS, and Linux. Inference is local; no
backend and no cloud model.

V1.0-Alpha.

## Architecture

- UI: C# + Avalonia (MVVM), sidebar navigation, poster tab, dark/light themes
- Engine: Rust native library (cdylib) loaded in-process via C ABI
- Inference: local GGUF model through llama.cpp (`llama-cpp-2`). The model is
  uploaded and validated in Pengaturan; the engine reads GGUF metadata, rejects
  incompatible or embedding-only files, then loads the weights to prove the file
  works. JSON output is requested in the prompt and parsed strictly; GBNF
  grammar constraints are disabled because they crashed llama.cpp on small
  models.
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
copy src\engine\target\debug\veritas_engine.dll src\app\Veritas.App\bin\Debug\net10.0\
```

## Models

There is no in-app downloader. Upload any llama.cpp-compatible `.gguf` in
Pengaturan (drag and drop works), or place the file directly in
`%LOCALAPPDATA%\Veritas\models\`. The app shows the architecture, quantization,
context length, and chat-template status it read from the file header, copies the
file atomically, and then loads it once to confirm it is usable. A model that
fails inspection or loading is rejected; it is never selected silently.

Verified on the development machine: Qwen 2.5 3B Instruct Q4_K_M,
1,929,903,008 bytes, 32K context, chat template present.

## Honest limits

- `confidence` and `truth_percentage` are uncalibrated model outputs. They are
  not probabilities validated against a labelled Indonesian benchmark.
- No held-out accuracy result exists for the GGUF pipeline. The old 36-sample
  development score belonged to the removed heuristic baseline.
- Evidence retrieval depends on public search and Wikipedia; thin evidence caps
  confidence and can demote a decisive verdict to `unverifiable`.

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

Scripts exist under `installer/` but no packaged installer is checked in.

- Windows: `installer/windows/build.ps1` needs Inno Setup 6 (`iscc`) and writes
  `artifacts/windows/installer/Veritas-Setup-1.0.0-alpha.exe`.
- macOS: `installer/macos/build.sh`
- Linux: `installer/linux/build.sh`

Until those artifacts are produced, run the app from source with `dotnet run`.

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
