#!/usr/bin/env bash
set -euo pipefail

CONFIGURATION="${1:-Release}"
RUNTIME="linux-x64"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/artifacts/linux"
mkdir -p "$OUT/app" "$OUT/installer"

dotnet publish "$ROOT/src/app/Veritas.App/Veritas.App.csproj" \
  -c "$CONFIGURATION" -r "$RUNTIME" --self-contained true \
  -o "$OUT/app"

cp "$ROOT/src/engine/target/release/libveritas_engine.so" "$OUT/app/" 2>/dev/null || true

if command -v appimagetool >/dev/null 2>&1; then
  APPDIR="$OUT/Veritas.AppDir"
  mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/256x256/apps"
  cp -r "$OUT/app/"* "$APPDIR/usr/bin/"
  cat > "$APPDIR/Veritas.desktop" <<EOF
[Desktop Entry]
Name=Veritas
Exec=Veritas.App
Icon=veritas
Type=Application
Categories=Utility;
EOF
  cp "$APPDIR/Veritas.desktop" "$APPDIR/usr/share/applications/"
  appimagetool "$APPDIR" "$OUT/installer/Veritas-1.0.0-alpha.AppImage"
else
  echo "appimagetool not found; skipping AppImage creation."
fi

echo "Linux build complete: $OUT"
