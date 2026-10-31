# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 1.0.0-alpha | Yes |

## Reporting a Vulnerability

Report security issues privately to the maintainers. Do not open a public issue for
unfixed vulnerabilities. Include:

- Affected version and platform.
- Reproduction steps.
- Impact assessment.
- Suggested mitigation, if known.

## Scope

In scope:

- Native engine FFI boundary.
- Input validation and path handling.
- Model download and integrity verification.
- Local data storage.

Out of scope:

- Accuracy of verdicts (report as a regular issue).
- Third-party model licenses.
- Issues in unsupported platforms.

## Design Guarantees

- Inference and OCR run on the device; no content is sent to a backend.
- Evidence retrieval is optional. When enabled, only a short keyword query is
  sent to Bing and the Indonesian Wikipedia API; article text, poster text, and
  comments are not uploaded.
- Telemetry is off by default and never includes content.
- Model files are verified by SHA-256 before use.
- Rust panics are caught at the FFI boundary.
