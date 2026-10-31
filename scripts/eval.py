#!/usr/bin/env python3
"""Evaluate the Veritas engine against a labeled JSONL benchmark.

Usage:
    python scripts/eval.py --engine path/to/veritas_engine.dll --dataset data/eval/benchmark.jsonl
"""

import argparse
import ctypes
import json
import sys
from collections import Counter, defaultdict


def load_engine(path: str):
    lib = ctypes.CDLL(path)
    lib.veritas_init.restype = ctypes.c_void_p
    lib.veritas_init.argtypes = [ctypes.c_char_p]
    lib.veritas_analyze.restype = ctypes.c_void_p
    lib.veritas_analyze.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_void_p, ctypes.c_void_p]
    lib.veritas_free_string.argtypes = [ctypes.c_void_p]
    lib.veritas_free.argtypes = [ctypes.c_void_p]
    return lib


def analyze(lib, handle, text: str):
    req = json.dumps({"text": text, "lang_hint": "id", "options": {"threshold": 0.5, "include_heuristics": True, "max_claims": 8}})
    ptr = lib.veritas_analyze(handle, req.encode("utf-8"), None, None)
    if not ptr:
        raise RuntimeError("engine returned null")
    try:
        raw = ctypes.string_at(ptr).decode("utf-8")
    finally:
        lib.veritas_free_string(ptr)
    return json.loads(raw)


def prf(confusion, labels):
    rows = {}
    for label in labels:
        tp = confusion[label][label]
        fp = sum(confusion[other][label] for other in labels if other != label)
        fn = sum(confusion[label][other] for other in labels if other != label)
        precision = tp / (tp + fp) if tp + fp else 0.0
        recall = tp / (tp + fn) if tp + fn else 0.0
        f1 = 2 * precision * recall / (precision + recall) if precision + recall else 0.0
        rows[label] = {"precision": precision, "recall": recall, "f1": f1, "support": tp + fn}
    return rows


def ece(pairs, bins=10):
    buckets = defaultdict(list)
    for conf, correct in pairs:
        idx = min(int(conf * bins), bins - 1)
        buckets[idx].append((conf, correct))
    total = len(pairs)
    score = 0.0
    for items in buckets.values():
        if not items:
            continue
        avg_conf = sum(c for c, _ in items) / len(items)
        acc = sum(1 for _, ok in items if ok) / len(items)
        score += (len(items) / total) * abs(acc - avg_conf)
    return score


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", required=True)
    parser.add_argument("--dataset", required=True)
    parser.add_argument("--out", default="")
    args = parser.parse_args()

    lib = load_engine(args.engine)
    handle = lib.veritas_init(None)
    if not handle:
        print("failed to init engine", file=sys.stderr)
        return 1

    samples = []
    with open(args.dataset, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                samples.append(json.loads(line))

    labels = sorted({s["label"] for s in samples} | {"unverifiable"})
    confusion = {a: Counter() for a in labels}
    pairs = []
    correct = 0
    failures = []
    predictions = []

    for s in samples:
        resp = analyze(lib, handle, s["text"])
        pred = resp.get("verdict", "unverifiable")
        conf = float(resp.get("confidence", 0.0))
        confusion[s["label"]][pred] += 1
        ok = pred == s["label"]
        correct += 1 if ok else 0
        pairs.append((conf, ok))
        predictions.append({
            "text": s["text"],
            "label": s["label"],
            "pred": pred,
            "confidence": conf,
            "correct": ok,
        })
        if not ok:
            failures.append({"text": s["text"], "label": s["label"], "pred": pred, "confidence": conf})

    metrics = prf(confusion, labels)
    macro_f1 = sum(m["f1"] for m in metrics.values()) / len(metrics)
    report = {
        "samples": len(samples),
        "accuracy": correct / len(samples) if samples else 0.0,
        "macro_f1": macro_f1,
        "ece": ece(pairs),
        "per_class": metrics,
        "confusion": {a: dict(confusion[a]) for a in labels},
        "failures": failures,
        "predictions": predictions,
    }

    text = json.dumps(report, indent=2, ensure_ascii=False)
    print(text)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as f:
            f.write(text)

    lib.veritas_free(handle)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
