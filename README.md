# Veritas

Hoax and buzzer detector for Windows, macOS, and Linux. Local-only. No backend. No cloud.

V1.0-Alpha.

## Architecture

- UI: C# + Avalonia (MVVM), sidebar navigation, animated transitions, dark/light themes
- Engine: Rust native library (cdylib) loaded in-process via C ABI
- Retrieval: SQLite fact store with local hashing embeddings and cosine search
- Heuristics: domain reputation, buzzer patterns, clickbait, calibration, abstain
- Inference: heuristic and RAG baseline in alpha; GGUF via llama.cpp is feature-gated

See `plan.md` for the full plan.

## Build

Rust engine:

```
cd src/engine
cargo build
cargo test --workspace
```

.NET app:

```
cd src/app
dotnet build Veritas.Core/Veritas.Core.csproj
dotnet build Veritas.App/Veritas.App.csproj
```

Copy the native library next to the app binary before running:

```
copy src\engine\target\debug\veritas_engine.dll src\app\Veritas.App\bin\Debug\net10.0\
```

## Evaluate

```
python scripts/eval.py --engine src/engine/target/debug/veritas_engine.dll --dataset data/eval/benchmark.jsonl
python scripts/bench.py --engine src/engine/target/debug/veritas_engine.dll --dataset data/eval/benchmark.jsonl
```

Current benchmark: 36 samples, accuracy 1.000, macro-F1 1.000, ECE 0.104.
The benchmark is a development regression set, not a held-out test set.

Tune calibration:

```
python scripts/calibrate.py --engine src/engine/target/debug/veritas_engine.dll --dataset data/eval/benchmark.jsonl
```

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
