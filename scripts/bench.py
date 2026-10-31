#!/usr/bin/env python3
"""Latency benchmark for the Veritas engine.

Usage:
    python scripts/bench.py --engine path/to/veritas_engine.dll --dataset data/eval/benchmark.jsonl
"""

import argparse
import ctypes
import json
import statistics
import time


def load_engine(path: str):
    lib = ctypes.CDLL(path)
    lib.veritas_init.restype = ctypes.c_void_p
    lib.veritas_init.argtypes = [ctypes.c_char_p]
    lib.veritas_analyze.restype = ctypes.c_void_p
    lib.veritas_analyze.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_void_p, ctypes.c_void_p]
    lib.veritas_free_string.argtypes = [ctypes.c_void_p]
    lib.veritas_free.argtypes = [ctypes.c_void_p]
    return lib


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", required=True)
    parser.add_argument("--dataset", required=True)
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()

    lib = load_engine(args.engine)
    handle = lib.veritas_init(None)
    texts = []
    with open(args.dataset, "r", encoding="utf-8") as f:
        for line in f:
            if line.strip():
                texts.append(json.loads(line)["text"])

    latencies = []
    for _ in range(args.runs):
        for text in texts:
            req = json.dumps({"text": text, "lang_hint": "id", "options": {"threshold": 0.5, "include_heuristics": True, "max_claims": 8, "online_evidence": False}})
            start = time.perf_counter()
            ptr = lib.veritas_analyze(handle, req.encode("utf-8"), None, None)
            elapsed = (time.perf_counter() - start) * 1000.0
            if ptr:
                lib.veritas_free_string(ptr)
            latencies.append(elapsed)

    latencies.sort()
    p50 = statistics.median(latencies)
    p95 = latencies[int(len(latencies) * 0.95) - 1]
    print(json.dumps({
        "samples": len(latencies),
        "p50_ms": round(p50, 3),
        "p95_ms": round(p95, 3),
        "min_ms": round(min(latencies), 3),
        "max_ms": round(max(latencies), 3),
    }, indent=2))
    lib.veritas_free(handle)


if __name__ == "__main__":
    main()
