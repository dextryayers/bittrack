# Prompt Card: heuristic-baseline

## Version

`heur_v1`

## Purpose

Classify Indonesian claims into hoax, misleading, clickbait, buzzer, valid, or
unverifiable without a generative model.

## Input

- Normalized claim text.
- Optional RAG match with a verified fact verdict.

## Rules

1. If RAG match score >= 0.7, use the matched fact verdict with confidence 0.85.
2. If clickbait keywords are present, return clickbait.
3. If buzzer imperatives dominate, return buzzer.
4. If hoax keywords dominate, return hoax.
5. If valid keywords dominate, return valid.
6. Otherwise return unverifiable.

## Aggregation

1. Claims with a decisive verdict vote with weight `importance * confidence`.
2. Unverifiable claims abstain and do not dilute a decisive verdict.
3. If every claim abstains, strong buzzer or clickbait signals can still drive
   the verdict.
4. Confidence is calibrated with temperature T=0.4. For abstentions it is the
   probability that the text cannot be decided from the available evidence.

## Output

```
{
  "verdict": "hoax | misleading | clickbait | buzzer | valid | unverifiable",
  "confidence": 0.0,
  "reasoning": "short explanation"
}
```

## Evaluation

See `docs/model-card.md`. Prompt version is recorded in the audit payload of
every result.

## Changelog

- `heur_v1`: initial keyword and pattern baseline with RAG override.
