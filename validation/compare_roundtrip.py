#!/usr/bin/env python3
"""Compare two ``roundtrip-<backend>.json`` files field by field (§9.1).

The comparison walks the semantic digests produced by ``validation/roundtrip.py``
and reports every differing leaf with its case id and a precise field path such
as ``RW::resources__crazyones.pdf:variants.clone.digest.pages[0].mediabox[2]``.

Only volatile bookkeeping is skipped (wall-clock durations, workdir paths,
interpreter/module paths, per-process stderr tails, per-output file paths).
Everything semantic is compared, including warnings *category counts*, error
types and error messages (messages are already path/address-masked by the
runner).

Exit status
-----------
0 when the two files match, 1 on any difference (including a case that is
missing from one side), 2 for unreadable inputs.

Usage
-----
    .venvs/dev/bin/python validation/compare_roundtrip.py \
        validation/results/roundtrip-oracle.json validation/results/roundtrip-dev.json
    ... --json-out validation/results/compare-roundtrip.json --max-diffs 200
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import rt_common as rc  # noqa: E402

# Keys that legitimately differ between two runs (paths, timings, process info).
VOLATILE_KEYS = {
    "artifacts",
    "backend",
    "interop_targets",
    "diagnostics",
    "duration_ms",
    "duration_s",
    "started_at",
    "wall_ms",
    "workdir",
    "process",
    "pypdf",
    "python",
    "executable",
    "module",
    "rust_extension",
    "traceback",
    "outputs",
    "warning_messages",
    "log_records",
}

VALUE_WIDTH = 240


def _short(value: Any) -> str:
    if isinstance(value, str):
        if len(value) > VALUE_WIDTH:
            return (
                json.dumps(value[:VALUE_WIDTH], ensure_ascii=False)
                + f"... <{len(value)} chars>"
            )
        return json.dumps(value, ensure_ascii=False)
    rendered = json.dumps(value, ensure_ascii=False, sort_keys=True)
    if len(rendered) > VALUE_WIDTH:
        return rendered[:VALUE_WIDTH] + "..."
    return rendered


def _child(path: str, key: Any) -> str:
    if isinstance(key, int):
        return f"{path}[{key}]" if path else f"[{key}]"
    return f"{path}.{key}" if path else str(key)


def _equal(left: Any, right: Any) -> bool:
    if isinstance(left, bool) or isinstance(right, bool):
        return isinstance(left, bool) and isinstance(right, bool) and left == right
    if isinstance(left, (int, float)) and isinstance(right, (int, float)):
        return left == right
    if type(left) is not type(right):
        return False
    return left == right


def diff_values(
    left: Any,
    right: Any,
    path: str,
    diffs: list[dict[str, Any]],
    *,
    max_diffs: int,
) -> None:
    if len(diffs) >= max_diffs:
        return
    if isinstance(left, dict) and isinstance(right, dict):
        for key in sorted(set(left) | set(right), key=str):
            if key in VOLATILE_KEYS:
                continue
            child = _child(path, key)
            if key not in left:
                diffs.append({"path": child, "oracle": "<missing>", "dev": right[key]})
            elif key not in right:
                diffs.append({"path": child, "oracle": left[key], "dev": "<missing>"})
            else:
                diff_values(left[key], right[key], child, diffs, max_diffs=max_diffs)
            if len(diffs) >= max_diffs:
                return
        return
    if isinstance(left, list) and isinstance(right, list):
        if len(left) != len(right):
            diffs.append(
                {
                    "path": f"{path}.__len__",
                    "oracle": len(left),
                    "dev": len(right),
                }
            )
        for index, (left_item, right_item) in enumerate(zip(left, right)):
            diff_values(
                left_item,
                right_item,
                _child(path, index),
                diffs,
                max_diffs=max_diffs,
            )
            if len(diffs) >= max_diffs:
                return
        return
    if not _equal(left, right):
        diffs.append({"path": path, "oracle": left, "dev": right})


def case_map(payload: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {case["case_id"]: case for case in payload.get("cases", [])}


def compare_payloads(
    oracle: dict[str, Any],
    dev: dict[str, Any],
    *,
    max_diffs: int = 1000,
) -> dict[str, Any]:
    diffs: list[dict[str, Any]] = []

    # Top-level identity and selection first: a different sample set would make
    # every per-case comparison meaningless.
    top_left = {key: value for key, value in oracle.items() if key != "cases"}
    top_right = {key: value for key, value in dev.items() if key != "cases"}
    diff_values(top_left, top_right, "", diffs, max_diffs=max_diffs)

    left_cases = case_map(oracle)
    right_cases = case_map(dev)
    all_cases = sorted(set(left_cases) | set(right_cases))
    for case_id in all_cases:
        if len(diffs) >= max_diffs:
            break
        left_case = left_cases.get(case_id)
        right_case = right_cases.get(case_id)
        if left_case is None:
            diffs.append({"case": case_id, "path": "<case>", "oracle": "<missing>", "dev": "<present>"})
            continue
        if right_case is None:
            diffs.append({"case": case_id, "path": "<case>", "oracle": "<present>", "dev": "<missing>"})
            continue
        case_diffs: list[dict[str, Any]] = []
        diff_values(left_case, right_case, "", case_diffs, max_diffs=max_diffs - len(diffs))
        for diff in case_diffs:
            diffs.append({"case": case_id, **diff})
        if len(diffs) >= max_diffs:
            break

    by_workflow: dict[str, dict[str, int]] = {}
    for case_id in all_cases:
        workflow = case_id.split("::", 1)[0].split("@", 1)[0]
        bucket = by_workflow.setdefault(workflow, {"cases": 0, "with_diffs": 0, "diffs": 0})
        bucket["cases"] += 1
    for diff in diffs:
        case_id = diff.get("case")
        if not case_id:
            continue
        workflow = case_id.split("::", 1)[0].split("@", 1)[0]
        by_workflow[workflow]["diffs"] += 1
    seen = {diff["case"] for diff in diffs if diff.get("case")}
    for case_id in seen:
        workflow = case_id.split("::", 1)[0].split("@", 1)[0]
        by_workflow[workflow]["with_diffs"] += 1

    return {
        "oracle": {
            "file": None,
            "backend": oracle.get("backend"),
            "cases": len(left_cases),
            "pypdf": oracle.get("pypdf", {}).get("version"),
        },
        "dev": {
            "file": None,
            "backend": dev.get("backend"),
            "cases": len(right_cases),
            "pypdf": dev.get("pypdf", {}).get("version"),
        },
        "common_cases": len(set(left_cases) & set(right_cases)),
        "missing_in_dev": sorted(set(left_cases) - set(right_cases)),
        "missing_in_oracle": sorted(set(right_cases) - set(left_cases)),
        "diffs": diffs,
        "diff_count": len(diffs),
        "truncated": len(diffs) >= max_diffs,
        "by_workflow": by_workflow,
        "status": "MATCH" if not diffs and not (set(left_cases) ^ set(right_cases)) else "DIFF",
    }


def print_report(report: dict[str, Any], max_print: int = 200) -> None:
    print(
        f"roundtrip compare: oracle={report['oracle']['backend']} "
        f"({report['oracle']['cases']} cases, pypdf {report['oracle']['pypdf']}) "
        f"dev={report['dev']['backend']} "
        f"({report['dev']['cases']} cases, pypdf {report['dev']['pypdf']})"
    )
    print(f"common cases: {report['common_cases']}")
    if report["missing_in_dev"]:
        print(f"missing in dev ({len(report['missing_in_dev'])}): {report['missing_in_dev'][:10]}")
    if report["missing_in_oracle"]:
        print(
            f"missing in oracle ({len(report['missing_in_oracle'])}): "
            f"{report['missing_in_oracle'][:10]}"
        )
    print()
    if report["status"] == "MATCH":
        print("RESULT: MATCH - no semantic differences")
        return
    print(f"RESULT: DIFF - {report['diff_count']} field difference(s)")
    print()
    grouped: dict[str, list[dict[str, Any]]] = {}
    for diff in report["diffs"]:
        grouped.setdefault(diff.get("case") or "<top-level>", []).append(diff)
    shown = 0
    for case_id, diffs in grouped.items():
        print(f"[{case_id}]")
        for diff in diffs:
            if shown >= max_print:
                print(f"  ... output truncated at {max_print} differences")
                return
            print(f"  {diff['path']}")
            print(f"    oracle: {_short(diff['oracle'])}")
            print(f"    dev:    {_short(diff['dev'])}")
            shown += 1
        print()
    if report["truncated"]:
        print("(diff collection was truncated by --max-diffs)")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("oracle", help="roundtrip-<oracle>.json")
    parser.add_argument("dev", help="roundtrip-<dev>.json")
    parser.add_argument("--json-out", default=None)
    parser.add_argument("--max-diffs", type=int, default=1000)
    parser.add_argument("--max-print", type=int, default=200)
    args = parser.parse_args(argv)

    oracle_path = Path(args.oracle)
    dev_path = Path(args.dev)
    try:
        oracle = rc.read_json(oracle_path)
        dev = rc.read_json(dev_path)
    except (OSError, json.JSONDecodeError) as exc:
        print(f"error: cannot read result files: {exc}", file=sys.stderr)
        return 2

    if oracle.get("backend") != "oracle" or dev.get("backend") != "dev":
        print(
            "error: expected --backend oracle/dev result files, got "
            f"{oracle.get('backend')!r}/{dev.get('backend')!r}",
            file=sys.stderr,
        )
        return 2

    report = compare_payloads(oracle, dev, max_diffs=args.max_diffs)
    report["oracle"]["file"] = str(oracle_path)
    report["dev"]["file"] = str(dev_path)
    print_report(report, max_print=args.max_print)

    out_path = (
        Path(args.json_out)
        if args.json_out
        else rc.RESULTS / "compare-roundtrip.json"
    )
    rc.write_json(out_path, report)
    print(f"\nreport -> {out_path}")
    return 0 if report["status"] == "MATCH" else 1


if __name__ == "__main__":
    raise SystemExit(main())
