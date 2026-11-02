# Model Card: Veritas

## Version

V1.0-Alpha

## Components

- Inference: [llama.cpp](https://github.com/ggml-org/llama.cpp) via the
  `llama-cpp-2` Rust bindings. CPU only, no GPU requirement.
- Model: no model is bundled or downloaded by the app. The user supplies any
  llama.cpp-compatible GGUF file through Pengaturan. The engine reads its
  metadata first and only then loads the weights, so a malformed or
  architecture-mismatched file is rejected before it can produce a verdict.
- Verified development model: Qwen 2.5 3B Instruct, Q4_K_M quantization,
  1,929,903,008 bytes, 32K trained context, chat template present. Model
  weights are not distributed with Veritas; their licence applies separately.
- Poster OCR: Windows.Media.Ocr, running on device.
- Evidence retrieval: Bing HTML search restricted to trusted hosts, plus the
  Indonesian Wikipedia API.
- Decoding: the model's own chat template, greedy first attempt, one fixed-seed
  retry, 300-second ceiling. JSON is requested in the prompt and parsed
  strictly; grammar-constrained decoding is disabled because it crashed
  llama.cpp on small models.

## Intended Use

- Indonesian news, message, and poster screening.
- Offline, privacy-preserving verification assistance.
- Human-in-the-loop fact checking.

## Out of Scope

- Final legal or editorial decisions.
- Detection of coordinated inauthentic behavior from platform metadata.
- Video or audio analysis.
- Claims that require evidence the retriever cannot reach (paywalls, private
  groups, deleted posts).

## How the Verdict Is Produced

1. The engine loads and validates the selected GGUF model before any network
   work, so a missing or broken model fails fast instead of producing a
   plausible-looking answer.
2. It resolves the input: URL fetch and article extraction, pasted text, OCR
   poster text, and comments.
3. It retrieves up to five evidence items from trusted hosts. Evidence is shown
   to the model with numbered ids and is also returned to the user.
4. The local model receives the text and evidence and is asked for a JSON object
   containing verdict, truth score, confidence, summary, claims, and red flags.
5. The engine parses that object, clamps ranges, drops fabricated evidence ids,
   and applies deterministic grounding: no evidence caps confidence and demotes
   decisive verdicts, a hoax verdict without a fact-check result is capped, and
   claims that cite no evidence reduce confidence.
6. The confidence threshold is applied, and every cap or contradiction is
   recorded as a warning. Optional strict verification runs a second inference
   with a fixed seed and demotes the result if the two verdicts disagree.
7. Heuristic signals are computed only for display. They cannot change the
   verdict.

If the model is missing, fails to load, times out, or returns unparsable output
after one retry, the engine returns an error. It never falls back to keywords or
a seeded fact database.

## Output

- `verdict`: hoax, misleading, clickbait, buzzer, valid, unverifiable.
- `truth_percentage`: model estimate (0-100) that the core claim is true.
- `confidence`: model estimate (0-1) that the verdict is correct.
- `summary`: 2-4 Indonesian sentences.
- `claims`: up to the configured limit, with type, importance, confidence,
  reasoning, and evidence ids.
- `evidence`: source, URL, date, retrieval score, and kind (`fact_check`,
  `encyclopedia`, `web`).
- `warnings` and `red_flags`: non-fatal problems and manipulation patterns.

## Evaluation

No held-out evaluation of the GGUF pipeline exists yet. The 36-sample
`data/eval/benchmark.jsonl` set was used to develop the removed heuristic
baseline; its previous accuracy and macro-F1 values do not describe this system
and must not be quoted as model accuracy.

Known facts on the development machine (Intel i5-4570, 4 cores, 16 GB RAM,
CPU-only, Q4_K_M 3B):

- Model file size: 1,929,903,008 bytes. The GGUF header reports 32,768-token
  context, chat template present, quantization Q4_K_M.
- End-to-end latency and verdict quality must be measured with
  `cargo test -p veritas-engine real_model -- --ignored --nocapture`; record the
  output before publishing performance claims.

## Limitations

- A 3B model has limited factual knowledge and can still be wrong when evidence
  is thin. The schema and prompt reduce, but do not eliminate, hallucination.
- Evidence retrieval depends on Bing HTML layout and may return nothing for
  niche claims. Missing evidence lowers confidence and often yields
  unverifiable.
- Trusted-host filtering excludes useful sources outside the allowlist.
- Windows OCR reads Latin script with the installed `en-US` recognizer; it may
  misread stylized Indonesian text. OCR errors propagate into the analysis.
- CPU inference on 4 cores is slow: prompt processing and generation can take
  tens of seconds to a few minutes per article.
- Comments are context for buzzer patterns, not a separate claim set.
- No calibration of the model's confidence has been performed on a labeled
  Indonesian set. The threshold is a user-set guardrail, not a calibrated
  probability.
- Upload validation checks GGUF metadata and a real load, not output quality.
  A file can load and still be a poor fact-checking model.
- Context length is honoured per model: long inputs are truncated to a budget
  derived from the loaded model's `n_ctx_train`, and the truncation is not a
  substitute for reading the omitted text.

## Ethical Notes

False positives can harm reputations. The engine abstains when confidence is
low, exposes evidence and warnings, and should be used to support human
verification, not replace it. Do not present a Veritas result as proof that a
person or organization lied.

## Provenance

- Engine prompt version: `veritas-id-v3`.
- Model id and prompt version are recorded in the audit payload of every result.
- Input SHA-256 is recorded for reproducibility.
