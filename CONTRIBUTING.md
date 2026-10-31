# Contributing

## Development Setup

Requirements:

- Rust stable
- .NET 10 SDK
- CMake and a C++ toolchain for `llama-cpp-2` (optional, only for the `llm` feature)

Build:

```
cd src/engine
cargo build
cargo test --workspace

cd ../app
dotnet build Veritas.Core/Veritas.Core.csproj
dotnet build Veritas.App/Veritas.App.csproj
```

## Quality Gates

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test --workspace`
- `dotnet build` with zero warnings
- `python scripts/eval.py` on the benchmark
- `python scripts/bench.py` for latency

## Commit Convention

Use Conventional Commits:

```
feat(scope): subject
fix(scope): subject
docs(scope): subject
test(scope): subject
refactor(scope): subject
chore(scope): subject
```

Subjects are imperative, max 72 characters, no trailing punctuation.

## Pull Requests

- One logical change per pull request.
- Include tests for new behavior.
- Update `CHANGELOG.md` for user-visible changes.
- Do not commit build artifacts, models, or local databases.
