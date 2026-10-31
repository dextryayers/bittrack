# Model Card: Veritas

## Version

V1.0-Alpha

## Components

- Inference: [llama.cpp](https://github.com/ggml-org/llama.cpp) via the
  `llama-cpp-2` Rust bindings. CPU only, no GPU requirement.
- Default model: Qwen 2.5 3B Instruct, Q4_K_M quantization, 1,929,903,008 bytes.
  Source blob: `registry.ollama.ai/v2/library/qwen2.5`, digest
  `sha256:5ee4f07cdb9beadbbb293e85803c569b01bd37ed059d2715faa7bb405f31caa6`.
- Optional models: Qwen 2.5 7B Instruct Q4_K_M (4.68 GB), Qwen 2.5 1.5B
  Instruct Q4_K_M (986 MB), Qwen 2.5 0.5B Instruct Q4_K_M (398 MB).
- Poster OCR: Windows.Media.Ocr, running on device.
- Evidence retrieval: Bing HTML search restricted to trusted hosts, plus the
  Indonesian Wikipedia API.
- Decoding: model chat template, JSON-schema grammar, greedy first attempt,
  one fixed-seed retry, 300-second ceiling.

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

1. The engine resolves the input: URL fetch and article extraction, pasted text,
   OCR poster text, and comments.
2. It retrieves up to five evidence items from trusted hosts. Evidence is shown
   to the model with numbered ids and is also returned to the user.
3. The local model receives the text and evidence. Its output is constrained to
   a JSON schema containing verdict, truth score, confidence, summary, claims,
   and red flags.
4. The engine clamps ranges, removes fabricated evidence ids, applies the
   confidence threshold, and records warnings.
5. Heuristic signals are computed only for display. They cannot change the
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

Known measured facts on the development machine (Intel i5-4570, 4 cores, 16 GB
RAM, CPU-only, Q4_K_M 3B):

- Model file size: 1,929,903,008 bytes, SHA-256 verified.
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

## Ethical Notes

False positives can harm reputations. The engine abstains when confidence is
low, exposes evidence and warnings, and should be used to support human
verification, not replace it. Do not present a Veritas result as proof that a
person or organization lied.

## Provenance

- Engine prompt version: `veritas-id-v2`.
- Model id and prompt version are recorded in the audit payload of every result.
- Input SHA-256 is recorded for reproducibility.
