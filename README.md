# Veritas

Hoax and buzzer detector for Windows, macOS, and Linux. Local-only. No backend. No cloud.

V1.0-Alpha.

## Architecture

- UI: C# + Avalonia (MVVM)
- Engine: Rust native library (cdylib) loaded in-process via C ABI
- Inference: GGUF via llama.cpp (planned in later phases; heuristic baseline in alpha)
- Storage: SQLite + sqlite-vec (RAG, planned)

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

## Run smoke test

```
cd src/app/Veritas.Smoke
dotnet run
```

## License

MIT OR Apache-2.0
