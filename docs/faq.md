# FAQ

## Does Veritas send my data anywhere?

The model runs locally, and full article text, poster text, and comments are
never uploaded. To find evidence, Veritas sends a short keyword query derived
from the article title to Bing and the Indonesian Wikipedia API; results are
filtered to trusted fact-check, newsroom, government, and encyclopedia hosts.
You can turn this off in Pengaturan. Telemetry is off by default.

## Why is a result marked unverifiable?

The engine abstains when the model's confidence is below the threshold, when it
cannot produce a schema-valid answer, or when the retrieved evidence does not
support a decision. A confident wrong answer is worse than an honest abstention.

## Which model does it use?

Qwen 2.5 3B Instruct Q4_K_M (1.93 GB) by default, executed with llama.cpp on the
CPU. Larger 7B and smaller 1.5B/0.5B variants are listed in Pengaturan. The app
refuses to produce a verdict if no model is installed; there is no keyword
fallback.

## Can it detect hoaxes outside Indonesia?

The prompt, evidence hosts, and language defaults target Indonesian. English
input is accepted, but evidence coverage and accuracy are not evaluated.

## Does it work offline?

Inference works offline once the model is downloaded. Evidence retrieval needs
network access; disable it to run fully offline, at the cost of fewer citations.

## How accurate is it?

There is no held-out accuracy measurement for the GGUF pipeline yet. The old
36-sample development benchmark belonged to the removed heuristic baseline and
must not be quoted as an accuracy claim for the model. Treat every verdict as
decision support and verify high-impact claims manually.

## How do I report a false verdict?

Open an issue with the input text, expected verdict, observed verdict, and the
model id from the result panel. Do not include personal or sensitive data.

## Why is image analysis a separate tab?

OCR is a distinct step with its own failure modes. The Poster tab lets you
inspect and correct the extracted text before it is analyzed, instead of hiding
OCR errors inside a single paste box.
