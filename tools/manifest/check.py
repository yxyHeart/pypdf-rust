#!/usr/bin/env python3
"""Verify that every upstream pypdf symbol is mapped to a Rust/Python landing spot.

Gate A6 ("nothing left unported") uses two annotation forms inside the Rust
sources:

    // @pypdf: pypdf.generic._base.PdfObject.write_to_stream
    // @pypdf-module: pypdf.constants

Modules declared as ``python`` in ``tools/codegen/port_status.json`` are covered
by a verbatim Python file (type-only modules such as exceptions/protocols).

Usage:
    python tools/manifest/check.py                 # report
    python tools/manifest/check.py --require 100   # gate
    python tools/manifest/check.py --write-report docs/SYMBOL_COVERAGE.md
"""

from __future__ import annotations

import argparse
import json
import re
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SYMBOLS = ROOT / "docs" / "symbols.json"
STATUS = ROOT / "tools" / "codegen" / "port_status.json"
RUST_GLOB = "crates/**/*.rs"

ITEM_RE = re.compile(r"@pypdf:\s*(\S+)")
MODULE_RE = re.compile(r"@pypdf-module:\s*(\S+)")


def collect_annotations() -> tuple[set[str], set[str]]:
    items: set[str] = set()
    modules: set[str] = set()
    for path in ROOT.glob(RUST_GLOB):
        text = path.read_text(encoding="utf-8")
        items.update(ITEM_RE.findall(text))
        modules.update(MODULE_RE.findall(text))
    return items, modules


def symbol_paths(module: dict) -> list[str]:
    """Every class, method, class attribute, module function and constant."""
    base = module["module"]
    paths: list[str] = []
    for member in module["members"]:
        paths.append(f"{base}.{member['name']}")
        if member["kind"] == "class":
            for child in member.get("members") or []:
                paths.append(f"{base}.{member['name']}.{child['name']}")
    return paths


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--require", type=float, default=None, help="required coverage %%")
    parser.add_argument("--write-report", type=Path)
    parser.add_argument("--show-missing", type=int, default=0)
    args = parser.parse_args()

    symbols = json.loads(SYMBOLS.read_text(encoding="utf-8"))
    status = json.loads(STATUS.read_text(encoding="utf-8")) if STATUS.exists() else {}
    annotated, annotated_modules = collect_annotations()

    rows: list[tuple[str, int, int, str]] = []
    missing: list[str] = []
    total = 0
    covered = 0
    for module in symbols["modules"]:
        name = module["module"]
        entry = status.get(name, {})
        mode = entry.get("mode", "stub")
        paths = symbol_paths(module)
        module_total = len(paths)
        if mode == "python":
            module_covered = module_total
        elif name in annotated_modules:
            module_covered = module_total
        else:
            module_covered = sum(1 for path in paths if path in annotated)
        if mode != "python":
            for path in paths:
                if path not in annotated and name not in annotated_modules:
                    missing.append(path)
        total += module_total
        covered += module_covered
        rows.append((name, module_covered, module_total, mode))

    percent = (covered / total * 100.0) if total else 100.0
    stalled = [row for row in rows if row[1] == 0]
    print(f"modules: {len(rows)}  symbols: {total}  covered: {covered}  ({percent:.2f}%)")
    print(f"unported modules: {len(stalled)}")
    if args.show_missing:
        for path in missing[: args.show_missing]:
            print("  missing:", path)

    if args.write_report:
        lines = [
            "# 符号覆盖报告（Symbol Coverage）",
            "",
            "> 由 `tools/manifest/check.py` 从 `docs/symbols.json` 与 Rust 源码中的",
            "> `// @pypdf:` / `// @pypdf-module:` 注解自动生成。门禁 A6 要求 100%。",
            "",
            f"总计：**{covered}/{total}**（{percent:.2f}%）",
            "",
            "| 模块 | 已覆盖 | 总数 | 覆盖 | 实现方式 |",
            "|---|---:|---:|---:|---|",
        ]
        for name, done, total_symbols, mode in sorted(rows):
            ratio = (done / total_symbols * 100) if total_symbols else 100.0
            lines.append(
                f"| `{name}` | {done} | {total_symbols} | {ratio:.0f}% | {mode} |"
            )
        lines.append("")
        if missing:
            lines.append("## 未覆盖符号（前 200 个）")
            lines.append("")
            for path in missing[:200]:
                lines.append(f"- `{path}`")
            lines.append("")
        args.write_report.parent.mkdir(parents=True, exist_ok=True)
        args.write_report.write_text("\n".join(lines), encoding="utf-8")
        print(f"report written to {args.write_report}")

    if args.require is not None and percent + 1e-9 < args.require:
        print(f"FAIL: coverage {percent:.2f}% < required {args.require}%")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
