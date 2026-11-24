#!/usr/bin/env python3
"""Generate golden vectors for CPython's float formatting.

The Rust port must reproduce Python's float text exactly, because writer output
bytes depend on it (`FloatObject.myrepr`) and text extraction embeds floats in
content streams.

Writes, into ``crates/pypdf-rs-core/tests/data/``:

* ``pyfmt_golden.tsv`` -- bits(hex) TAB repr TAB str TAB myrepr TAB f1 TAB f2 TAB f4 TAB f6 TAB f8
* ``prng_golden.tsv``  -- seed TAB first 4 u64 outputs TAB token_bytes(16) hex

Usage:
    .venvs/oracle/bin/python tools/probe/gen_pyfmt_golden.py
"""

from __future__ import annotations

import random
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
OUT = ROOT / "crates" / "pypdf-rs-core" / "tests" / "data"

sys.path.insert(0, str(ROOT / "tools" / "oracle"))
from prng import SplitMix64  # noqa: E402

FLOAT_WRITE_PRECISION = 8  # pypdf.generic._base.FLOAT_WRITE_PRECISION


def myrepr(value: float) -> str:
    """Verbatim copy of pypdf.generic._base.FloatObject.myrepr."""
    from math import log10

    if value == 0:
        return "0.0"
    nb = FLOAT_WRITE_PRECISION - int(log10(abs(value)))
    return f"{value:.{max(1, nb)}f}".rstrip("0").rstrip(".")


def bits(value: float) -> str:
    return struct.pack(">d", value).hex()


def curated() -> list[float]:
    values = [
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        1.25,
        1.5,
        2.0,
        3.0,
        10.0,
        100.0,
        1e-3,
        1e-4,
        1e-5,
        1e-6,
        1e15,
        1e16,
        1e17,
        1.2345678901234567,
        1234567890123456.0,
        1.7976931348623157e308,
        5e-324,
        2.2250738585072014e-308,
        0.1,
        0.2,
        0.3,
        1 / 3,
        2 / 3,
        -1 / 3,
        1e-7,
        123.456,
        612.0,
        792.0,
        595.2755905511812,
        841.8897637795277,
        42.195,
        1e21,
        1e22,
        9.999999999999999e15,
    ]
    values.extend([float("inf"), float("-inf"), float("nan")])
    return values


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    rng = random.Random(20261124)

    samples: list[float] = curated()
    for _ in range(6000):
        raw = rng.getrandbits(64)
        value = struct.unpack(">d", raw.to_bytes(8, "big"))[0]
        if value != value:  # skip NaN, covered separately
            continue
        samples.append(value)
    for exponent in range(-320, 309, 7):
        samples.append(float(f"1e{exponent}"))
        samples.append(float(f"-3.7e{exponent}"))

    def myrepr_or_marker(value: float) -> str:
        try:
            return myrepr(value)
        except (OverflowError, ValueError) as exc:
            return "!" + type(exc).__name__

    lines = ["bits\trepr\tstr\tmyrepr\tf1\tf2\tf4\tf6\tf8"]
    for value in samples:
        lines.append(
            "\t".join(
                [
                    bits(value),
                    repr(value),
                    str(value),
                    myrepr_or_marker(value),
                    f"{value:.1f}",
                    f"{value:.2f}",
                    f"{value:.4f}",
                    f"{value:.6f}",
                    f"{value:.8f}",
                ]
            )
        )
    (OUT / "pyfmt_golden.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8")

    prng_lines = ["seed\tu64_0\tu64_1\tu64_2\tu64_3\ttoken16\tuuid4"]
    for seed in [0, 1, 42, 0xC0FFEE5EED1234, 2**64 - 1, 123456789]:
        stream = SplitMix64(seed)
        words = [stream.next_u64() for _ in range(4)]
        token_stream = SplitMix64(seed)
        token = token_stream.token_bytes(16)
        uuid_stream = SplitMix64(seed)
        import uuid as uuid_module

        uuid_value = uuid_module.UUID(bytes=uuid_stream.token_bytes(16), version=4)
        prng_lines.append(
            "\t".join([str(seed), *[str(word) for word in words], token.hex(), str(uuid_value)])
        )
    (OUT / "prng_golden.tsv").write_text("\n".join(prng_lines) + "\n", encoding="utf-8")

    print(f"wrote {len(samples)} float vectors and {len(prng_lines) - 1} prng vectors to {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
