#!/usr/bin/env python3
"""Cross-check Rust float formatting against CPython on many random doubles.

Usage:
    .venvs/oracle/bin/python tools/probe/pyfmt_crosscheck.py --count 200000
"""

from __future__ import annotations

import argparse
import random
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "target" / "release" / "examples" / "pyfmt_dump"
FLOAT_WRITE_PRECISION = 8


def myrepr(value: float) -> str:
    from math import log10

    if value == 0:
        return "0.0"
    try:
        nb = FLOAT_WRITE_PRECISION - int(log10(abs(value)))
        return f"{value:.{max(1, nb)}f}".rstrip("0").rstrip(".")
    except (OverflowError, ValueError) as exc:
        return "!" + type(exc).__name__


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--count", type=int, default=200_000)
    parser.add_argument("--seed", type=int, default=7)
    args = parser.parse_args()

    if not BINARY.exists():
        print(f"missing {BINARY}; run: cargo build --release -p pypdf-rs-core --examples")
        return 2

    rng = random.Random(args.seed)
    values: list[float] = [0.0, -0.0, 1.0, 1e16, 1e-5, float("inf"), float("-inf"), float("nan")]
    for _ in range(args.count):
        values.append(struct.unpack(">d", rng.getrandbits(64).to_bytes(8, "big"))[0])

    payload = "\n".join(struct.pack(">d", v).hex() for v in values) + "\n"
    result = subprocess.run(
        [str(BINARY)], input=payload, capture_output=True, text=True, check=True
    )

    bad = 0
    for value, line in zip(values, result.stdout.splitlines(), strict=False):
        bits, rust_repr, rust_f8, rust_f2, rust_myrepr = line.split("\t")
        expected = [
            repr(value),
            f"{value:.8f}",
            f"{value:.2f}",
            myrepr(value),
        ]
        actual = [rust_repr, rust_f8, rust_f2, rust_myrepr]
        if expected != actual:
            bad += 1
            if bad <= 10:
                print(f"MISMATCH {bits}: python={expected} rust={actual}")
    print(f"checked {len(values)} values, mismatches: {bad}")
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
