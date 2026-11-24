#!/usr/bin/env python3
"""Turn a pytest JUnit XML report into a stable outcome snapshot.

Gate A1 compares the outcome of every test (passed / skipped / xfailed /
failed) against the reference baseline captured from upstream pypdf.

Usage:
    python tools/baseline/snapshot.py .audit/baseline-junit.xml \
        -o .audit/baseline-outcomes.json
    python tools/baseline/snapshot.py .audit/rust-junit.xml \
        --compare .audit/baseline-outcomes.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from xml.etree import ElementTree


def load(path: Path) -> dict[str, str]:
    tree = ElementTree.parse(path)
    outcomes: dict[str, str] = {}
    for case in tree.iter("testcase"):
        classname = case.get("classname") or ""
        name = case.get("name") or ""
        test_id = f"{classname}::{name}" if classname else name
        outcome = "passed"
        for child in case:
            if child.tag == "failure":
                outcome = "failed"
            elif child.tag == "error":
                outcome = "error"
            elif child.tag == "skipped":
                outcome = "xfailed" if child.get("type") == "pytest.xfail" else "skipped"
            elif child.tag == "system-out" or child.tag == "system-err":
                continue
        outcomes[test_id] = outcome
    return outcomes


def compare(baseline: dict[str, str], current: dict[str, str]) -> int:
    problems: list[str] = []

    missing = sorted(set(baseline) - set(current))
    extra = sorted(set(current) - set(baseline))
    for test_id in missing:
        problems.append(f"MISSING {test_id} (baseline={baseline[test_id]})")
    for test_id in extra:
        problems.append(f"EXTRA {test_id} (current={current[test_id]})")
    for test_id in sorted(set(baseline) & set(current)):
        expected, actual = baseline[test_id], current[test_id]
        bad = (
            expected in ("failed", "error")
            or actual in ("failed", "error")
            or expected != actual
        )
        if bad and expected != actual:
            problems.append(f"CHANGED {test_id}: {expected} -> {actual}")
        elif actual in ("failed", "error"):
            problems.append(f"{actual.upper()} {test_id}")

    counts: dict[str, int] = {}
    for outcome in current.values():
        counts[outcome] = counts.get(outcome, 0) + 1
    print("current outcomes:", dict(sorted(counts.items())))
    if problems:
        print(f"PROBLEMS: {len(problems)}")
        for problem in problems[:80]:
            print("  -", problem)
        return 1
    print("MATCH: outcomes identical to baseline")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("xml", type=Path)
    parser.add_argument("-o", "--out", type=Path)
    parser.add_argument("--compare", type=Path)
    args = parser.parse_args()

    outcomes = load(args.xml)
    counts: dict[str, int] = {}
    for outcome in outcomes.values():
        counts[outcome] = counts.get(outcome, 0) + 1
    print(f"{len(outcomes)} tests:", dict(sorted(counts.items())))

    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(
            json.dumps(outcomes, indent=0, sort_keys=True), encoding="utf-8"
        )
        print(f"written {args.out}")

    if args.compare:
        baseline = json.loads(args.compare.read_text(encoding="utf-8"))
        return compare(baseline, outcomes)
    return 0


if __name__ == "__main__":
    sys.exit(main())
