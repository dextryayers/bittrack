# Model Card: Veritas Alpha Baseline

## Summary

The V1.0-Alpha engine uses a deterministic heuristic classifier plus a local
retrieval store. It does not ship a generative model by default. GGUF inference
is feature-gated behind the `llm` Cargo feature.

## Components

- Hashing embedder: character 3-5 gram hashing into 256 dimensions, L2 normalized.
- Fact store: SQLite table with verdict, source, URL, date, and embedding blob.
- Retrieval: cosine similarity, top-3, score threshold 0.35.
- Classifier: keyword and pattern heuristics with RAG override at score >= 0.7.
- Aggregation: importance-weighted claim vote over decisive verdicts, fused with
  domain, buzzer, and clickbait signals, then temperature calibration (T=0.4).
  Abstentions do not dilute a verdict supported by at least one verifiable claim.
  When every claim abstains, strong buzzer or clickbait signals can still drive
  the verdict.
- Confidence: calibrated probability that the returned verdict is correct. For
  unverifiable results it is the probability that the text cannot be decided
  from the available evidence.

## Intended Use

- Indonesian news and message screening.
- Offline, privacy-preserving verification assistance.
- Human-in-the-loop fact checking.

## Out of Scope

- Final legal or editorial decisions.
- Detection of coordinated inauthentic behavior from platform metadata.
- Video, audio, or image content.

## Evaluation

Benchmark: `data/eval/benchmark.jsonl`, 36 labeled samples, six classes.

- Accuracy: 1.000
- Macro-F1: 1.000
- ECE: 0.104
- Latency p50: 0.700 ms, p95: 1.270 ms (debug build, this machine)

The benchmark is a development regression set that was also used to tune the
heuristics and calibration. Treat these numbers as a regression baseline, not a
held-out production estimate. Expand the set with independently sourced samples
before making external accuracy claims.

## Limitations

- Hashing embeddings miss paraphrases that share little surface form.
- Heuristic weights are hand-tuned.
- Seed fact database is small.
- Calibration temperature is fitted on a small validation set.

## Ethical Notes

False positives can harm reputations. The engine abstains when evidence is weak
and always presents confidence and sources. Users must verify before acting.
