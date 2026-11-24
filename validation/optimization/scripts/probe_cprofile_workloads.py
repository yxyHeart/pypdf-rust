#!/usr/bin/env python3
"""cProfile one representative case per workload family (W1, W5, W6, W7).

Writes a text dump to ``validation/optimization/baseline/raw/cprofile-<fam>-<backend>.txt``.
Read-only measurement: no library file is touched.
"""
from __future__ import annotations

import cProfile
import io
import json
import pstats
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PERF = ROOT / "validation" / "perf" / "workloads.json"
OUT = ROOT / "validation" / "optimization" / "baseline" / "raw"


def backend() -> str:
    try:
        import pypdf._pypdf  # noqa: F401

        return "rust"
    except ImportError:
        return "reference"


def files(wid: str) -> list[Path]:
    doc = json.loads(PERF.read_text())
    for workload in doc["workloads"]:
        if workload["id"] == wid:
            return [ROOT / rel for rel in workload["files"]]
    raise KeyError(wid)


def dump(name: str, profiler: cProfile.Profile, limit: int = 14) -> str:
    stream = io.StringIO()
    pstats.Stats(profiler, stream=stream).sort_stats("tottime").print_stats(limit)
    text = f"### {name}\n{stream.getvalue()}"
    path = OUT / f"cprofile-{name}-{backend()}.txt"
    path.write_text(text, encoding="utf-8")
    return str(path)


def main() -> int:
    import pypdf

    OUT.mkdir(parents=True, exist_ok=True)
    written = []

    # W1: reader init + metadata over the workload's 20 small PDFs
    prof = cProfile.Profile()
    prof.enable()
    for path in files("W1"):
        reader = pypdf.PdfReader(str(path))
        len(reader.pages)
        metadata = reader.metadata or {}
        for name in ("/Title", "/Author", "/Producer", "/Creator", "/CreationDate"):
            metadata.get(name)
    prof.disable()
    written.append(dump("w1", prof))

    for wid in ("W5", "W6", "W7"):
        prof = cProfile.Profile()
        prof.enable()
        writer = pypdf.PdfWriter()
        if wid == "W5":
            for path in files(wid):
                writer.append(str(path))
        else:
            cycle = [90, 180, 270]
            index = 0
            for path in files(wid):
                reader = pypdf.PdfReader(str(path))
                for page in reader.pages:
                    new_page = writer.add_page(page)
                    if wid == "W6":
                        new_page.rotate(int(cycle[index % 3]))
                        new_page.cropbox.lower_left = (10.0, 10.0)
                        new_page.cropbox.upper_right = (500.0, 700.0)
                    index += 1
        buffer = io.BytesIO()
        writer.write(buffer)
        prof.disable()
        written.append(dump(wid.lower(), prof))

    print("\n".join(written))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
