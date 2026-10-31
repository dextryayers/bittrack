# Windows build script (PowerShell)
param(
    [string]$Configuration = "Release",
    [string]$Runtime = "win-x64"
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path "$PSScriptRoot/../.."
$out = Join-Path $root "artifacts/windows"
New-Item -ItemType Directory -Force -Path $out | Out-Null

# Publish app + native engine
dotnet publish "$root/src/app/Veritas.App/Veritas.App.csproj" `
    -c $Configuration -r $Runtime --self-contained true `
    -p:PublishSingleFile=false -o "$out/app"

# Copy native engine (built separately)
Copy-Item "$root/src/engine/target/release/veritas_engine.dll" "$out/app/" -Force

# Velopack (optional): dotnet tool install -g vpk
if (Get-Command vpk -ErrorAction SilentlyContinue) {
    vpk pack -u Veritas -v 1.0.0-alpha -p "$out/app" -e "Veritas.App.exe" -o "$out/installer"
} else {
    Write-Host "vpk not found; skipping Velopack packaging. Install with: dotnet tool install -g vpk"
}

# Inno Setup (optional)
if (Get-Command iscc -ErrorAction SilentlyContinue) {
    iscc "$root/installer/windows/veritas.iss" /O"$out/installer"
} else {
    Write-Host "Inno Setup not found; skipping .iss compile."
}

Write-Host "Windows build complete: $out"
