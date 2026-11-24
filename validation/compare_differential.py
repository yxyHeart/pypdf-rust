#!/usr/bin/env python3
"""Compare two validation/differential.py outputs field by field."""
from __future__ import annotations

import json
import sys
from pathlib import Path


def walk(a, b, path, diffs):
    if type(a) is not type(b):
        diffs.append((path, a, b)); return
    if isinstance(a, dict):
        for key in sorted(set(a) | set(b)):
            if key not in a: diffs.append((f"{path}.{key}", "<missing>", b[key]))
            elif key not in b: diffs.append((f"{path}.{key}", a[key], "<missing>"))
            else: walk(a[key], b[key], f"{path}.{key}", diffs)
    elif isinstance(a, list):
        if len(a) != len(b):
            diffs.append((f"{path}.length", len(a), len(b)))
        for index in range(min(len(a), len(b))):
            walk(a[index], b[index], f"{path}[{index}]", diffs)
    elif a != b:
        diffs.append((path, a, b))


def main() -> int:
    ref = json.loads(Path(sys.argv[1]).read_text())
    rust = json.loads(Path(sys.argv[2]).read_text())
    diffs: list[tuple[str, object, object]] = []
    walk(ref, rust, "", diffs)
    print(f"documents: {len(ref)} vs {len(rust)} | differences: {len(diffs)}")
    for path, a, b in diffs[:80]:
        print(f"  DIFF {path}: {str(a)[:110]!r} != {str(b)[:110]!r}")
    print(f"RESULT: {'IDENTICAL' if not diffs else 'DIFFERENCES'}")
    return 0 if not diffs else 1


if __name__ == "__main__":
    raise SystemExit(main())
