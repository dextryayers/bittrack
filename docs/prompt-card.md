# Prompt Card: veritas-id-v3

## Version

`veritas-id-v3`

## Purpose

Ask a local Qwen 2.5 Instruct model to judge the core claim of an Indonesian
news article, poster, or comment thread, using retrieved fact-check and
encyclopedia evidence. The model is the only component that produces a verdict.

## Input

- Optional article title, body, site, and publication date resolved from a URL.
- Optional pasted article text.
- Optional OCR text from an uploaded poster.
- Optional reader comments, used only as buzzer/amplification context.
- Up to five numbered evidence items from trusted hosts (fact-check sites,
  Indonesian Wikipedia, established newsrooms, government agencies).
- A language hint, the user's maximum claim count, and a subject-text budget
  derived from the loaded model's real context window.

## Rules

1. Use only the supplied text and numbered evidence. Never invent facts,
   numbers, quotes, dates, or sources.
2. `verdict` is one of `hoax`, `misleading`, `clickbait`, `buzzer`, `valid`,
   `unverifiable`.
3. `truth_score` is the probability (0-100) that the core claim is true.
   `confidence` is the probability (0-100) that the chosen verdict is correct.
   They are independent.
4. Irrelevant evidence must be ignored. Missing evidence lowers confidence; it
   is not proof that a claim is false.
5. A claim is only `hoax` when there is an indication it is fabricated or
   demonstrably false, not merely because supporting evidence is absent.
6. The summary is 2-4 Indonesian sentences: core claim, what the evidence
   shows, then the conclusion.
7. Claims are capped by the user's `max_claims` setting (default 8). Each claim
   cites evidence ids that exist in the supplied list.
8. `red_flags` lists only manipulation patterns actually present in the input.

## Decoding

- The model's own chat template is applied through llama.cpp. A model without a
  template gets a ChatML fallback and a warning.
- JSON is requested in the prompt and parsed strictly. Grammar-constrained
  decoding is intentionally disabled: GBNF sampling crashed llama.cpp with small
  models, and a crash eliminates any accuracy a grammar might add.
- The first attempt uses greedy decoding for reproducibility. If the object
  cannot be parsed, one retry uses temperature 0.6 with a fixed seed.
- Optional strict verification runs a second greedy inference with a different
  fixed seed and demotes the verdict if the two runs disagree. It tests
  consistency, not correctness, and doubles latency.
- Generation is cooperative-cancellable and has a 600-second wall-clock cap.

## Output

```json
{
  "verdict": "hoax | misleading | clickbait | buzzer | valid | unverifiable",
  "truth_score": 0,
  "confidence": 0,
  "summary": "2-4 sentences",
  "claims": [
    {
      "text": "claim",
      "type": "factual | causal | statistical | quote | visual | opinion",
      "importance": 1,
      "verdict": "hoax | misleading | clickbait | buzzer | valid | unverifiable",
      "confidence": 0,
      "reasoning": "short explanation",
      "evidence_ids": [1]
    }
  ],
  "red_flags": ["pattern"]
}
```

The engine maps this into `AnalyzeResponse`, clamps all ranges, drops evidence
ids that do not exist, and applies deterministic grounding before the configured
confidence threshold. Missing evidence caps confidence and demotes decisive
verdicts; a hoax verdict without a fact-check result is capped; claims with no
evidence references reduce confidence. Every cap is surfaced as a warning.

## Evaluation

No held-out accuracy estimate exists yet for this prompt. The previous
heuristic baseline's 36-sample development score does not apply to a generative
model. Treat verdicts as decision support and verify high-impact claims.

## Changelog

- `veritas-id-v3`: adaptive context budget, model inspection/preload, stricter
  evidence grounding, optional self-consistency check, removal of the false
  grammar-constraint claim.
- `veritas-id-v2`: GGUF inference, evidence grounding, truth score, red flags.
- `heur_v1`: removed keyword/RAG baseline.
