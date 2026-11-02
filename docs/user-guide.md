# Veritas User Guide

## Install

No packaged installer is checked in yet. Run from source:

```
dotnet run --project src/app/Veritas.App -f net10.0
```

For Windows packaging, `installer/windows/build.ps1` requires Inno Setup 6 and
writes `artifacts/windows/installer/Veritas-Setup-1.0.0-alpha.exe`.

## First Run

Veritas needs a local GGUF model. Open Pengaturan and choose Unggah .gguf (or
drag the file onto the model panel). The app reads the GGUF header, shows the
architecture, quantization, context length, and chat-template status, copies the
file atomically, then loads it once to confirm it works. A file that fails
inspection or loading is rejected. There is no in-app downloader and no account.

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
2. Select Pilih gambar poster, or drag an image onto the panel.
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
different questions and can disagree. Neither is calibrated against a labelled
Indonesian benchmark; treat both as decision support.

Warnings report problems such as a failed URL fetch, missing evidence, low
confidence, or inconsistent model output. A warning is not a verdict.

## History

Every analysis stores the full result locally in
`%LOCALAPPDATA%\Veritas\history.sqlite`. Open Riwayat to search, reopen the
complete verdict with its claims and evidence, or delete individual entries.
History is never uploaded.

## Settings

- Model: upload, inspect, preload, or delete GGUF files. Drag and drop works.
- Online evidence: fetch fact-check and encyclopedia citations.
- Strict verification: run a second inference and demote the result if the two
  verdicts disagree. This doubles latency and tests consistency, not truth.
- Threshold: higher values make verdicts more conservative.
- Language: Indonesian or English.
- Theme: system, light, or dark.
- No telemetry is collected or sent.

Model and settings changes take effect immediately.

## Privacy

Inference and OCR run on your device. Full content is never uploaded. Evidence
search sends only a short keyword query to public search APIs; disable it for
fully offline operation.
