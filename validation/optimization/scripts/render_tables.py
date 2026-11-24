#!/usr/bin/env python3
"""Render the markdown tables used in ``diag_reader.md`` / ``diag_writer.md``
from the raw JSON produced by ``rw_diag.py`` and ``rw_microbench.py``.

    .venvs/dev/bin/python validation/optimization/scripts/render_tables.py \
        --rust   validation/optimization/raw/rw_diag_rust.json \
        --reference validation/optimization/raw/rw_diag_reference.json \
        --microbench-rust validation/optimization/raw/rw_micro_rust.json \
        --microbench-reference validation/optimization/raw/rw_micro_reference.json \
        --section reader      # or writer / micro

Tables are printed to stdout; the reports quote them verbatim.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def ms(value, digits=2):
    if value is None:
        return "—"
    return f"{value:.{digits}f}"


def ratio(rust, ref, digits=2):
    if not rust or not ref:
        return "—"
    return f"{rust / ref:.{digits}f}×"


def get(entry, *path, default=None):
    node = entry
    for key in path:
        if not isinstance(node, dict) or key not in node:
            return default
        node = node[key]
    return node


# ---------------------------------------------------------------------------
# shared tables
# ---------------------------------------------------------------------------


def table_layers(wid, rust, ref):
    layers = get(rust, "layers") or {}
    layers_ref = get(ref, "layers") or {}
    if not layers:
        return f"_no layered data for {wid}_"
    lines = [
        f"#### {wid} — layered wall time (median of the counted passes; min–max in brackets)",
        "",
        "| layer (cumulative) | port median | port min–max | reference median | reference min–max | port/reference |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for name, data in layers.items():
        describe = data["describe"]
        other = (layers_ref.get(name) or {}).get("describe")
        lines.append(
            "| {name} | {rp} ms | {rmin}–{rmax} | {op} ms | {omin}–{omax} | {ratio} |".format(
                name=name,
                rp=ms(describe["median"]),
                rmin=ms(describe["min"]),
                rmax=ms(describe["max"]),
                op=ms(other["median"]) if other else "—",
                omin=ms(other["min"]) if other else "—",
                omax=ms(other["max"]) if other else "—",
                ratio=ratio(describe["median"], other["median"]) if other else "—",
            )
        )
    # derived deltas
    names = list(layers)
    lines.append("")
    lines.append("| derived stage (= layer difference) | port | reference | port/reference |")
    lines.append("|---|---:|---:|---:|")
    for previous, current in zip(names, names[1:]):
        r_delta = layers[current]["describe"]["median"] - layers[previous]["describe"]["median"]
        ref_delta = None
        if layers_ref.get(current) and layers_ref.get(previous):
            ref_delta = (
                layers_ref[current]["describe"]["median"] - layers_ref[previous]["describe"]["median"]
            )
        lines.append(
            f"| {current} − {previous} | {ms(r_delta)} ms | "
            f"{ms(ref_delta)} ms | {ratio(r_delta, ref_delta) if ref_delta else '—'} |"
        )
    return "\n".join(lines)


def table_stages(wid, rust, ref, top=22):
    stages = get(rust, "stages", "stages") or {}
    stages_ref = get(ref, "stages", "stages") or {}
    if not stages:
        return f"_no stage data for {wid}_"
    lines = [
        f"#### {wid} — stage timers (inclusive wall time, median of repeats)",
        "",
        "| stage | calls (port / ref) | port ms | reference ms | port/reference | stage % of workload (port) |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    wall = get(rust, "stages", "wall_ms", "describe", "median") or 0.0
    for name, data in list(stages.items())[:top]:
        other = stages_ref.get(name) or {}
        lines.append(
            "| `{name}` | {calls:.0f} / {calls_ref:.0f} | {port} | {refm} | {ratio} | {pct:.0f}% |".format(
                name=name,
                calls=data["calls_median"],
                calls_ref=other.get("calls_median", 0),
                port=ms(data["total_ms_median"]),
                refm=ms(other.get("total_ms_median")),
                ratio=ratio(data["total_ms_median"], other.get("total_ms_median")),
                pct=(100.0 * data["total_ms_median"] / wall) if wall else 0.0,
            )
        )
    lines.append(
        f"| **workload wall** | | **{ms(wall)}** | **{ms(get(ref, 'stages', 'wall_ms', 'describe', 'median'))}** | "
        f"**{ratio(wall, get(ref, 'stages', 'wall_ms', 'describe', 'median'))}** | 100%% |"
    )
    lines.append("")
    lines.append(
        "_Stage rows are inclusive: a stage that is called from another stage (e.g. `get_object` from "
        "`_flatten`) is contained in its caller, so rows must not be summed._"
    )
    return "\n".join(lines)


def table_per_file(wid, rust, ref, keys, header):
    files = get(rust, "per_file") or {}
    files_ref = get(ref, "per_file") or {}
    if not files:
        return f"_no per-file data for {wid}_"
    lines = [
        f"#### {wid} — per input file",
        "",
        "| file | bytes | port total | ref total | port/ref | " + " | ".join(header) + " |",
        "|---|---:|---:|---:|---:|" + "---:|" * len(header),
    ]
    rows = []
    for name, data in files.items():
        wall = get(data, "stages", "wall_ms", "describe", "median") or 0.0
        wall_ref = get(files_ref.get(name) or {}, "stages", "wall_ms", "describe", "median") or 0.0
        values = []
        for key in keys:
            stage = get(data, "stages", "stages", key) or {}
            values.append(ms(stage.get("total_ms_median"), 2) if stage else "—")
        rows.append((wall, name, data, wall_ref, values))
    rows.sort(key=lambda row: -row[0])
    for wall, name, data, wall_ref, values in rows:
        lines.append(
            f"| `{name}` | {data.get('file_bytes', '—')} | {ms(wall)} | {ms(wall_ref)} | "
            f"{ratio(wall, wall_ref)} | " + " | ".join(values) + " |"
        )
    return "\n".join(lines)


def table_boundary(wid, rust, ref, top=25):
    counts = get(rust, "counts") or {}
    counts_ref = get(ref, "counts") or {}
    if not counts:
        return f"_no call-count data for {wid}_"
    lines = [
        f"#### {wid} — call counts (port; Python→Rust boundary and object materialisation)",
        "",
        f"Total Python→Rust boundary calls: **{counts['total_boundary_calls']:,}** "
        f"(reference: {counts_ref.get('total_boundary_calls', 'n/a') if counts_ref else 'n/a'} — the reference has no "
        f"extension boundary). Materialised PDF objects: **{counts['total_materialized']:,}**.",
        "",
        "| # | intercepted callable | port calls | share of boundary calls |",
        "|---|---|---:|---:|",
    ]
    boundary = [(k, v) for k, v in counts["counts"].items() if not k.startswith("new:")]
    total = counts["total_boundary_calls"] or 1
    for index, (name, value) in enumerate(boundary[:top], start=1):
        lines.append(f"| {index} | `{name}` | {value:,} | {100.0 * value / total:.1f}% |")
    lines.append(f"| | **all boundary calls** | **{total:,}** | 100.0% |")
    return "\n".join(lines)


def table_materialization(wid, rust):
    counts = get(rust, "counts") or {}
    if not counts:
        return f"_no materialisation data for {wid}_"
    created = [(k[4:], v) for k, v in counts["counts"].items() if k.startswith("new:")]
    created.sort(key=lambda kv: -kv[1])
    if not created:
        return f"_no materialisation data for {wid}_"
    lines = [
        f"#### {wid} — Python PDF objects materialised (port, counted at `__new__`)",
        "",
        "| class | instances |",
        "|---|---:|",
    ]
    for name, value in created:
        lines.append(f"| `{name}` | {value:,} |")
    lines.append(f"| **total** | **{sum(v for _, v in created):,}** |")
    return "\n".join(lines)


def table_phases(wid, rust, ref):
    phases = get(rust, "counts", "by_phase") or {}
    phases_ref = get(ref, "counts", "by_phase") or {}
    if not phases:
        return f"_no phase data for {wid}_"
    lines = [
        f"#### {wid} — calls per workload phase (port)",
        "",
        "| phase | Python→Rust boundary calls | materialised objects | ref boundary calls |",
        "|---|---:|---:|---:|",
    ]
    for name, slot in phases.items():
        boundary = sum(v for k, v in slot.items() if not k.startswith("new:"))
        created = sum(v for k, v in slot.items() if k.startswith("new:"))
        other = phases_ref.get(name) or {}
        other_boundary = sum(v for k, v in other.items() if not k.startswith("new:"))
        lines.append(
            f"| `{name}` | {boundary:,} | {created:,} | {other_boundary:,} |"
            if other
            else f"| `{name}` | {boundary:,} | {created:,} | — |"
        )
    return "\n".join(lines)


def table_profile(wid, rust, ref, top=15):
    blocks = []
    for label, entry in (("port", rust), ("reference", ref)):
        profile = get(entry, "profile") or {}
        if not profile:
            blocks.append(f"_no profile data for {wid} ({label})_")
            continue
        total = get(profile, "summary", "total_tt_s") or 0.0
        rows = profile.get("rows") or []
        lines = [
            f"#### {wid} — cProfile top sites ({label}, 3 reps merged)",
            "",
            f"Total profiled self time {total * 1e3:.1f} ms; "
            + ", ".join(
                f"{kind} {value * 1e3:.1f} ms"
                for kind, value in sorted(profile.get("classification", {}).items(), key=lambda kv: -kv[1])
            ),
            "",
            "| # | site | kind | calls | self ms | self % | cum ms |",
            "|---|---|---|---:|---:|---:|---:|",
        ]
        for index, row in enumerate(rows[:top], start=1):
            pct = (100.0 * row["tottime_s"] / total) if total else 0.0
            lines.append(
                f"| {index} | `{row['label']}` | {row['kind']} | {row['ncalls']:,} | "
                f"{row['tottime_s'] * 1e3:.2f} | {pct:.1f}% | {row['cumtime_s'] * 1e3:.2f} |"
            )
        blocks.append("\n".join(lines))
    return "\n\n".join(blocks)


def table_facts(wid, rust, ref):
    facts = get(rust, "counts", "facts") or []
    facts_ref = get(ref, "counts", "facts") or []
    if not facts:
        return f"_no facts for {wid}_"
    keys = sorted({key for fact in facts for key in fact})
    lines = [
        f"#### {wid} — output / object-graph facts",
        "",
        "| fact | port | reference |",
        "|---|---:|---:|",
    ]
    latest = facts[-1]
    latest_ref = facts_ref[-1] if facts_ref else {}
    for key in keys:
        if key in {"workload", "out_sha256"}:
            continue
        lines.append(f"| `{key}` | {latest.get(key)} | {latest_ref.get(key, '—')} |")
    lines.append(f"| `out_sha256` | `{str(latest.get('out_sha256'))[:16]}…` | `{str(latest_ref.get('out_sha256'))[:16]}…` |")
    return "\n".join(lines)


def table_counts_by_workload(wids, rust, ref):
    lines = [
        "#### Writer workloads — key call counts (all numbers are per workload repetition)",
        "",
        "| workload | boundary calls (port) | materialised objects (port) | clone calls (port) | `_add_object` | `write_to_stream` | objects in writer | out bytes |",
        "|---|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for wid in wids:
        entry = get(rust, "workloads", wid) or {}
        counts = get(entry, "counts") or {}
        if not counts:
            continue
        table = counts["counts"]
        clone_keys = [k for k in table if k.endswith("_clone") or k == "py:IndirectObject.clone"]
        clones = sum(table[k] for k in clone_keys)
        writes = table.get("datastructures_dictionaryobject_write_to_stream", 0) + table.get(
            "datastructures_arrayobject_write_to_stream", 0
        ) + table.get("datastructures_streamobject_write_to_stream", 0)
        facts = (counts.get("facts") or [{}])[-1]
        lines.append(
            f"| {wid} | {counts['total_boundary_calls']:,} | {counts['total_materialized']:,} | {clones:,} | "
            f"{table.get('py:PdfWriter._add_object', 0):,} | {writes:,} | {facts.get('objects_in_writer', '—')} | "
            f"{facts.get('out_bytes', '—'):,} |"
        )
    return "\n".join(lines)


def table_microbench(rust, ref):
    rows = {}
    for label, data in (("rust", rust), ("reference", ref)):
        if not data:
            continue
        for row in data["results"]:
            rows.setdefault(row["label"], {})[label] = row["median_ns_per_op"]
    lines = [
        "#### Microbenchmarks (median ns per operation, 5 samples)",
        "",
        "| benchmark | port ns/op | reference ns/op | port/reference | note |",
        "|---|---:|---:|---:|---|",
    ]
    notes = {}
    for data in (rust, ref):
        if not data:
            continue
        for row in data["results"]:
            notes.setdefault(row["label"], row.get("note", ""))
    for label, values in rows.items():
        port = values.get("rust")
        refv = values.get("reference")
        lines.append(
            f"| `{label}` | {port:,.0f} | {refv:,.0f} | "
            f"{ratio(port, refv) if port and refv else '—'} | {notes.get(label, '')} |"
            if port and refv
            else f"| `{label}` | {port:,.0f} | {refv or 0:,.0f} | — | {notes.get(label, '')} |"
        )
    return "\n".join(lines)


# ---------------------------------------------------------------------------


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--rust", required=True)
    parser.add_argument("--reference", required=True)
    parser.add_argument("--microbench-rust")
    parser.add_argument("--microbench-reference")
    parser.add_argument("--section", default="all", choices=["all", "reader", "writer", "micro"])
    args = parser.parse_args(argv)

    rust = load(args.rust)
    ref = load(args.reference)
    micro_rust = load(args.microbench_rust) if args.microbench_rust else None
    micro_ref = load(args.microbench_reference) if args.microbench_reference else None

    out = []
    if args.section in {"all", "reader"}:
        for wid in ("W1", "W2"):
            out.append(f"<!-- {wid} -->")
            out.append(table_layers(wid, get(rust, "workloads", wid), get(ref, "workloads", wid)))
            out.append("")
            if wid == "W1":
                out.append(
                    table_per_file(
                        wid,
                        get(rust, "workloads", wid),
                        get(ref, "workloads", wid),
                        keys=["open:__init__", "tree:_flatten", "open:get_object", "open:_get_object_from_stream"],
                        header=["port open", "port flatten", "port get_object", "port objStm"],
                    )
                )
            else:
                out.append(
                    table_per_file(
                        wid,
                        get(rust, "workloads", wid),
                        get(ref, "workloads", wid),
                        keys=["open:__init__", "tree:_flatten", "traverse:_get_rectangle", "traverse:rotation"],
                        header=["port open", "port flatten", "port box reads", "port rotation"],
                    )
                )
            out.append("")
            out.append(table_stages(wid, get(rust, "workloads", wid), get(ref, "workloads", wid)))
            out.append("")
            out.append(table_phases(wid, get(rust, "workloads", wid), get(ref, "workloads", wid)))
            out.append("")
            out.append(table_boundary(wid, get(rust, "workloads", wid), get(ref, "workloads", wid)))
            out.append("")
            out.append(table_materialization(wid, get(rust, "workloads", wid)))
            out.append("")
            out.append(table_profile(wid, get(rust, "workloads", wid), get(ref, "workloads", wid)))
            out.append("")

    if args.section in {"all", "writer"}:
        for wid in ("W5", "W6", "W7"):
            out.append(f"<!-- {wid} -->")
            out.append(table_layers(wid, get(rust, "workloads", wid), get(ref, "workloads", wid)))
            out.append("")
            out.append(table_stages(wid, get(rust, "workloads", wid), get(ref, "workloads", wid)))
            out.append("")
            out.append(table_phases(wid, get(rust, "workloads", wid), get(ref, "workloads", wid)))
            out.append("")
            out.append(table_profile(wid, get(rust, "workloads", wid), get(ref, "workloads", wid), top=12))
            out.append("")
            out.append(table_facts(wid, get(rust, "workloads", wid), get(ref, "workloads", wid)))
            out.append("")
        out.append(table_counts_by_workload(("W5", "W6", "W7"), rust, ref))
        out.append("")

    if args.section in {"all", "micro"} and micro_rust:
        out.append(table_microbench(micro_rust, micro_ref))

    print("\n".join(out))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
