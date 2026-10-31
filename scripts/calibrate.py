#!/usr/bin/env python3
"""Sweep temperature scaling for confidence calibration.

The engine applies temperature T=0.4 after aggregation. This script recovers
the pre-calibration logit from each response and reports the expected
calibration error (ECE) for a range of alternative temperatures, so the
constant in `heuristics::aggregate` can be tuned against the benchmark.

Usage:
    python scripts/calibrate.py --engine path/to/veritas_engine.dll \
        --dataset data/eval/benchmark.jsonl
"""

import argparse
import json
import math

from eval import analyze, ece, load_engine


def logit(probability: float) -> float:
    p = min(max(probability, 1e-6), 1.0 - 1e-6)
    return math.log(p / (1.0 - p))


def sigmoid(value: float) -> float:
    return 1.0 / (1.0 + math.exp(-value))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", required=True)
    parser.add_argument("--dataset", required=True)
    parser.add_argument("--current-temperature", type=float, default=0.4)
    parser.add_argument("--min-temperature", type=float, default=0.2)
    parser.add_argument("--max-temperature", type=float, default=1.0)
    parser.add_argument("--step", type=float, default=0.05)
    args = parser.parse_args()

    lib = load_engine(args.engine)
    handle = lib.veritas_init(None)
    if not handle:
        print("failed to init engine")
        return 1

    samples = []
    with open(args.dataset, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                samples.append(json.loads(line))

    raw_pairs = []
    for sample in samples:
        response = analyze(lib, handle, sample["text"])
        confidence = float(response.get("confidence", 0.0))
        raw_logit = logit(confidence) * args.current_temperature
        raw_pairs.append((raw_logit, response.get("verdict") == sample["label"]))

    sweep = []
    steps = int(round((args.max_temperature - args.min_temperature) / args.step))
    for index in range(steps + 1):
        temperature = round(args.min_temperature + index * args.step, 2)
        calibrated = [
            (sigmoid(raw_logit / temperature), correct)
            for raw_logit, correct in raw_pairs
        ]
        sweep.append({"temperature": temperature, "ece": round(ece(calibrated), 4)})

    best = min(sweep, key=lambda row: row["ece"])
    print(json.dumps({"samples": len(samples), "best": best, "sweep": sweep}, indent=2))

    lib.veritas_free(handle)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
