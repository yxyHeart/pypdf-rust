#!/usr/bin/env python3
"""Generate a deterministic mutated PDF corpus for robustness (gate A7).

Mutations are structure-aware-ish: byte flips, truncation, xref/startxref
corruption, stream length tampering, object renumbering and comment injection.
The corpus is generated once and then fed identically to the oracle and to the
Rust port; both sides must agree on success/failure and on the canonical result.

Usage:
    python tools/fuzz/gen_corpus.py --out .audit/fuzz/corpus --count 200 --seed 1
"""

from __future__ import annotations

import argparse
import random
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RESOURCES = ROOT / "resources"


def pdf_files() -> list[Path]:
    return sorted(RESOURCES.rglob("*.pdf"))


def mutate(data: bytes, rng: random.Random) -> bytes:
    strategy = rng.randrange(8)
    if not data:
        return b"%PDF-1.4\n%%EOF\n"
    if strategy == 0:  # flip random bytes
        out = bytearray(data)
        for _ in range(rng.randrange(1, 8)):
            index = rng.randrange(len(out))
            out[index] ^= 1 << rng.randrange(8)
        return bytes(out)
    if strategy == 1:  # truncate
        cut = rng.randrange(1, max(2, len(data)))
        return data[:cut]
    if strategy == 2:  # corrupt startxref offset
        marker = data.rfind(b"startxref")
        if marker < 0:
            return data
        out = bytearray(data)
        digits_start = marker + len(b"startxref")
        for index in range(digits_start, min(digits_start + 20, len(out))):
            if out[index : index + 1].isdigit():
                out[index : index + 1] = str(rng.randrange(10)).encode()
        return bytes(out)
    if strategy == 3:  # tamper with a stream /Length value
        out = bytearray(data)
        marker = out.find(b"/Length")
        if marker >= 0:
            end = marker + 7
            while end < len(out) and out[end : end + 1].isdigit():
                end += 1
            out[marker + 7 : end] = str(rng.randrange(0, 10**6)).encode()
        return bytes(out)
    if strategy == 4:  # delete a chunk
        start = rng.randrange(len(data))
        length = rng.randrange(1, 64)
        return data[:start] + data[start + length :]
    if strategy == 5:  # insert junk
        start = rng.randrange(len(data))
        junk = bytes(rng.randrange(256) for _ in range(rng.randrange(1, 64)))
        return data[:start] + junk + data[start:]
    if strategy == 6:  # duplicate a chunk
        start = rng.randrange(len(data))
        length = rng.randrange(1, 256)
        chunk = data[start : start + length]
        return data[:start] + chunk + data[start:]
    # strategy 7: renumber an indirect object header
    out = bytearray(data)
    marker = out.find(b" 0 obj")
    if marker >= 0:
        index = marker
        start = index
        while start > 0 and out[start - 1 : start].isdigit():
            start -= 1
        out[start:index] = str(rng.randrange(1, 9999)).encode()
    return bytes(out)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--count", type=int, default=200)
    parser.add_argument("--seed", type=int, default=20261124)
    args = parser.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    sources = pdf_files()
    if not sources:
        print("no PDFs under resources/")
        return 1

    rng = random.Random(args.seed)
    manifest = []
    for index in range(args.count):
        source = sources[rng.randrange(len(sources))]
        data = source.read_bytes()
        mutated = mutate(data, rng)
        target = args.out / f"{index:06d}.pdf"
        target.write_bytes(mutated)
        manifest.append({"file": target.name, "source": source.name, "size": len(mutated)})

    (args.out / "manifest.json").write_text(__import__("json").dumps(manifest, indent=0))
    print(f"generated {args.count} mutated PDFs in {args.out} from {len(sources)} sources")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
