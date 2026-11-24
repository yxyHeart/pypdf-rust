#!/usr/bin/env python3
"""Compare two oracle output directories (or two canonical JSON documents).

Usage:
    python tools/oracle/compare.py .audit/a .audit/b [--max-report 40]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def load(path: Path):
    if path.is_dir():
        path = path / "audit.json"
    return json.loads(path.read_text(encoding="utf-8"))


def walk(name: str, left, right, diffs: list[str], max_report: int) -> None:
    if len(diffs) >= max_report:
        return
    if type(left) is not type(right) and not (
        isinstance(left, (int, float)) and isinstance(right, (int, float))
    ):
        diffs.append(f"{name}: type {type(left).__name__} != {type(right).__name__}")
        return
    if isinstance(left, dict):
        for key in sorted(set(left) | set(right)):
            if key not in left:
                diffs.append(f"{name}.{key}: missing on left")
            elif key not in right:
                diffs.append(f"{name}.{key}: missing on right")
            else:
                walk(f"{name}.{key}", left[key], right[key], diffs, max_report)
            if len(diffs) >= max_report:
                return
        return
    if isinstance(left, list):
        if len(left) != len(right):
            diffs.append(f"{name}: list length {len(left)} != {len(right)}")
            return
        for index, (a, b) in enumerate(zip(left, right, strict=False)):
            walk(f"{name}[{index}]", a, b, diffs, max_report)
            if len(diffs) >= max_report:
                return
        return
    if left != right:
        diffs.append(f"{name}: {left!r} != {right!r}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("left")
    parser.add_argument("right")
    parser.add_argument("--max-report", type=int, default=40)
    args = parser.parse_args()

    left = load(Path(args.left))
    right = load(Path(args.right))

    diffs: list[str] = []
    walk("", left, right, diffs, args.max_report)

    if diffs:
        print(f"DIFFERENCES: {len(diffs)} (showing at most {args.max_report})")
        for diff in diffs:
            print(f"  - {diff}")
        return 1

    print("IDENTICAL")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
