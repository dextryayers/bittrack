# Veritas User Guide

## Install

Download the package for your platform from the Releases page.

- Windows: run `Veritas-Setup-1.0.0-alpha.exe`.
- macOS: open the DMG and drag Veritas to Applications.
- Linux: run the AppImage or install the `.deb`/`.rpm`.

## First Run

Veritas works offline. On first launch it creates a local fact database in your
user data directory and seeds it with verified Indonesian claims. No account is
required.

## Analyze Text

1. Open the Beranda view.
2. Paste an article or message.
3. Select Analisis sekarang.
4. Review the verdict, confidence, claims, evidence, and heuristic signals.

## Understand the Result

- Hoax: fabricated or demonstrably false.
- Misleading: partly true but framed deceptively.
- Clickbait: headline exaggerates or misrepresents the body.
- Buzzer: coordinated amplification pattern.
- Valid: consistent with verified sources.
- Unverifiable: insufficient evidence to decide.

Confidence is calibrated. A low confidence result is an invitation to verify
manually, not a final judgment.

## History

Every analysis is stored locally. Open Riwayat to review or clear it. History is
never uploaded.

## Settings

- Model: choose the GGUF model tier.
- Threshold: higher values make verdicts more conservative.
- Language: Indonesian or English.
- Theme: system, light, or dark.
- Telemetry: off by default.

## Privacy

Veritas performs inference locally. The analysis path makes no network calls.
Network access is used only for user-initiated model or database updates.
