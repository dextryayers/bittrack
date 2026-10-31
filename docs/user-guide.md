# Veritas User Guide

## Install

Download the package for your platform from the Releases page.

- Windows: run `Veritas-Setup-1.0.0-alpha.exe`.
- macOS: open the DMG and drag Veritas to Applications.
- Linux: run the AppImage or install the `.deb`/`.rpm`.

## First Run

Veritas needs a local GGUF model. Open Pengaturan, select a model, and choose
Unduh / lanjutkan model. The default Qwen 2.5 3B model is 1.93 GB; partial
downloads resume and the finished file is verified with SHA-256. No account is
required.

## Analyze Text or a Link

1. Open Beranda.
2. Paste article text, a news URL, or reader comments. At least one is required.
3. Select Analisis sekarang.
4. Wait while the engine fetches the article, searches trusted evidence, and
   runs the local model. Progress is shown under the buttons.
5. Review the result in Analisis: verdict, model confidence, truth percentage,
   summary, claims, evidence, red flags, and warnings.

## Analyze a Poster

1. Open Poster.
2. Select Pilih gambar poster and choose a PNG, JPEG, BMP, WebP, or TIFF image.
3. Windows OCR extracts the text locally. Inspect and correct it in the text box.
4. Select Analisis poster. The result appears in the Analisis tab.

OCR quality depends on image resolution and typography. Low-quality posters can
produce incorrect text; always check it before analysis.

## Understand the Result

- Hoax: fabricated or demonstrably false.
- Misleading: partly true but framed deceptively.
- Clickbait: headline exaggerates or misrepresents the body.
- Buzzer: coordinated amplification pattern.
- Valid: consistent with trusted sources.
- Unverifiable: insufficient evidence or confidence.

**Keyakinan verdict** is the model's confidence that its verdict is correct.
**Kebenaran klaim** is its estimate that the core claim is true. They answer
different questions and can disagree.

Warnings report problems such as a failed URL fetch, missing evidence, low
confidence, or inconsistent model output. A warning is not a verdict.

## History

Every analysis is stored locally in `%LOCALAPPDATA%\Veritas\history.sqlite`.
Open Riwayat to review or clear it. History is never uploaded.

## Settings

- Model: choose, download, or resume a GGUF model.
- Online evidence: fetch fact-check and encyclopedia citations.
- Threshold: higher values make verdicts more conservative.
- Language: Indonesian or English.
- Theme: system, light, or dark.
- Telemetry: off by default.

Model and evidence changes take effect after restarting the app.

## Privacy

Inference and OCR run on your device. Full content is never uploaded. Evidence
search sends only a short keyword query to public search APIs; disable it for
fully offline operation.
