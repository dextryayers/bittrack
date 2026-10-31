#!/usr/bin/env bash
set -euo pipefail

CONFIGURATION="${1:-Release}"
RUNTIME="osx-x64"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/artifacts/macos"
mkdir -p "$OUT/app" "$OUT/installer"

dotnet publish "$ROOT/src/app/Veritas.App/Veritas.App.csproj" \
  -c "$CONFIGURATION" -r "$RUNTIME" --self-contained true \
  -o "$OUT/app"

cp "$ROOT/src/engine/target/release/libveritas_engine.dylib" "$OUT/app/" 2>/dev/null || true

if command -v create-dmg >/dev/null 2>&1; then
  create-dmg \
    --volname "Veritas" \
    --window-pos 200 120 \
    --window-size 600 400 \
    --icon-size 100 \
    --app-drop-link 450 185 \
    "$OUT/installer/Veritas-1.0.0-alpha.dmg" \
    "$OUT/app"
else
  echo "create-dmg not found; skipping DMG creation."
fi

echo "macOS build complete: $OUT"
