# FAQ

## Does Veritas send my data anywhere?

No. Analysis runs locally. Telemetry is off by default and never includes content.

## Why is a result marked unverifiable?

The engine abstains when confidence is below the threshold or when the evidence
is insufficient. This is intentional. A confident wrong answer is worse than an
honest abstention.

## Which model does it use?

The alpha uses a deterministic heuristic and retrieval baseline. GGUF model
support is feature-gated and will become the default in a later release.

## Can it detect hoaxes outside Indonesia?

The seed database and prompts focus on Indonesian. English support is planned.

## Does it work offline?

Yes, after installation. Model and database downloads are optional and user
initiated.

## How accurate is it?

On the bundled benchmark, the alpha reports accuracy 1.000 and macro-F1 1.000.
The benchmark has 36 samples and was used during development, so it is a
regression baseline rather than an unbiased held-out estimate.

## How do I report a false verdict?

Open an issue with the input text, expected verdict, and observed verdict. Do not
include personal or sensitive data.
