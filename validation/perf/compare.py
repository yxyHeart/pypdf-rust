#!/usr/bin/env python3
"""§10.3–§10.5: compare the reference and port performance runs.

Reads ``validation/results/perf-reference.json`` and
``validation/results/perf-rust.json`` (produced by ``run.py``), verifies that the
two runs are comparable (same workload definitions, same output sizes), and
writes ``validation/results/perf-report.md``.

Exit code:
  0  data complete and every workload produced the same output byte size
  1  missing data (a side did not run a workload successfully) or an
     output-size mismatch between reference and port
  2  usage / unreadable input

Being slower or faster never fails the comparison -- it is reported.
Failures and timeouts are listed per workload; they are never dropped.  A
recorded failure counts as incomplete data unless ``--allow-failures`` is given.
"""
from __future__ import annotations

import argparse
import json
import shlex
import sys
from pathlib import Path
from typing import Any

PERF_DIR = Path(__file__).resolve().parent
ROOT = PERF_DIR.parents[1]
RESULTS_DIR = ROOT / "validation" / "results"
DEFAULT_REF = RESULTS_DIR / "perf-reference.json"
DEFAULT_RUST = RESULTS_DIR / "perf-rust.json"
DEFAULT_WORKLOADS = PERF_DIR / "workloads.json"
DEFAULT_OUT = RESULTS_DIR / "perf-report.md"

MISSING = "\u2014"


# ---------------------------------------------------------------------------
# formatting helpers
# ---------------------------------------------------------------------------
def fmt_seconds(value: float | None) -> str:
    if value is None:
        return MISSING
    if value < 0.001:
        return f"{value * 1e6:.1f} µs"
    if value < 1.0:
        return f"{value * 1000:.1f} ms"
    return f"{value:.3f} s"


def fmt_ms(value: float | None) -> str:
    return MISSING if value is None else f"{value * 1000:.2f} ms"


def fmt_mb(value: float | None) -> str:
    return MISSING if value is None else f"{value / 1e6:.1f} MB"


def fmt_pct(value: float | None) -> str:
    if value is None:
        return MISSING
    return f"{value:+.1f}%"


def fmt_speedup(value: float | None) -> str:
    return MISSING if value is None else f"{value:.2f}×"


def fmt_loadavg(values: Any) -> str:
    if not values:
        return MISSING
    return " ".join(f"{float(value):.2f}" for value in values)


def fmt_bytes(value: int | None) -> str:
    if value is None:
        return MISSING
    if value < 1024:
        return f"{value} B"
    if value < 1024 * 1024:
        return f"{value / 1024:.1f} KiB"
    return f"{value / (1024 * 1024):.2f} MiB"


def md_escape(text: Any) -> str:
    return str(text).replace("|", "\\|").replace("\n", " ")


def table(headers: list[str], rows: list[list[str]]) -> list[str]:
    lines = ["| " + " | ".join(headers) + " |", "|" + "|".join(["---"] * len(headers)) + "|"]
    for row in rows:
        lines.append("| " + " | ".join(md_escape(cell) for cell in row) + " |")
    return lines


# ---------------------------------------------------------------------------
# loading / comparability
# ---------------------------------------------------------------------------
def load_run(path: Path, expected_backend: str, errors: list[str]) -> dict | None:
    if not path.is_file():
        errors.append(f"missing data: {path} does not exist")
        return None
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        errors.append(f"missing data: cannot read {path}: {exc}")
        return None
    if document.get("schema") != "perf-run-v1":
        errors.append(f"missing data: {path} has unexpected schema {document.get('schema')!r}")
        return None
    backend = document.get("backend")
    if backend != expected_backend:
        errors.append(
            f"missing data: {path} is labeled backend={backend!r}, expected {expected_backend!r}"
        )
    return document


def load_baselines(path: Path, warnings: list[str]) -> dict[str, int | None]:
    if not path.is_file():
        warnings.append(f"workload definitions not found at {path}; no recorded output-size baseline")
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        warnings.append(f"cannot read {path}: {exc}")
        return {}
    return {workload["id"]: workload.get("expected_output_bytes") for workload in data.get("workloads", [])}


def compare_workload_definitions(ref: dict, rust: dict, warnings: list[str], errors: list[str]) -> None:
    ref_map = {workload["id"]: workload for workload in ref.get("workloads", [])}
    rust_map = {workload["id"]: workload for workload in rust.get("workloads", [])}
    for wid in sorted(set(ref_map) | set(rust_map)):
        if wid not in ref_map:
            errors.append(f"missing data: workload {wid} exists only in the port run")
            continue
        if wid not in rust_map:
            errors.append(f"missing data: workload {wid} exists only in the reference run")
            continue
        ref_def, rust_def = ref_map[wid], rust_map[wid]
        ref_files = [item["path"] for item in ref_def.get("files", [])]
        rust_files = [item["path"] for item in rust_def.get("files", [])]
        if ref_files != rust_files:
            errors.append(f"missing data: workload {wid} uses different input files in the two runs")
        for key in ("type", "output_kind"):
            if ref_def.get(key) != rust_def.get(key):
                errors.append(
                    f"missing data: workload {wid} has different {key} "
                    f"({ref_def.get(key)!r} vs {rust_def.get(key)!r})"
                )


def compare_environments(ref: dict, rust: dict, warnings: list[str]) -> None:
    ref_env, rust_env = ref.get("environment", {}), rust.get("environment", {})
    for key, label in (
        ("machine", "machine"),
        ("platform", "platform"),
        ("python", "Python version"),
        ("pypdf_version", "pypdf version"),
    ):
        if ref_env.get(key) != rust_env.get(key):
            warnings.append(
                f"{label} differs between runs: reference={ref_env.get(key)!r} port={rust_env.get(key)!r}"
            )
    ref_config, rust_config = ref.get("config", {}), rust.get("config", {})
    for key in ("repeat", "warmup", "timeout_s", "large_file_threshold_bytes", "deterministic_secrets"):
        if ref_config.get(key) != rust_config.get(key):
            warnings.append(
                f"config {key} differs: reference={ref_config.get(key)!r} port={rust_config.get(key)!r}"
            )
    for key, label in (("hashseed", "PYTHONHASHSEED"),):
        if ref_env.get(key) != "0" or rust_env.get(key) != "0":
            warnings.append(
                f"{label} was not 0 on both sides: reference={ref_env.get(key)!r} port={rust_env.get(key)!r}"
            )
    for side, run in (("reference", ref), ("port", rust)):
        environment = run.get("environment", {})
        if environment.get("extension_changed_during_run"):
            warnings.append(
                f"{side}: the compiled extension changed on disk during the run – results are a mixed build"
            )


# ---------------------------------------------------------------------------
# per-workload metrics
# ---------------------------------------------------------------------------
def metrics(ref_workload: dict, rust_workload: dict) -> dict:
    ref_sum, rust_sum = ref_workload["summary"], rust_workload["summary"]
    ref_wall, rust_wall = ref_sum["wall"], rust_sum["wall"]
    ref_median, rust_median = ref_wall["median"], rust_wall["median"]
    speedup = None
    latency_change = None
    if ref_median and rust_median:
        speedup = ref_median / rust_median
        latency_change = (rust_median / ref_median - 1.0) * 100.0
    ref_peak = ref_sum["peak_rss_bytes"]["max"]
    rust_peak = rust_sum["peak_rss_bytes"]["max"]
    memory_change = None
    if ref_peak and rust_peak:
        memory_change = (rust_peak / ref_peak - 1.0) * 100.0
    ref_growth = (ref_sum.get("rss_growth_bytes") or {}).get("max")
    rust_growth = (rust_sum.get("rss_growth_bytes") or {}).get("max")
    growth_change = None
    if ref_growth and rust_growth:
        growth_change = (rust_growth / ref_growth - 1.0) * 100.0
    ref_outputs = ref_sum.get("output_bytes_values") or []
    rust_outputs = rust_sum.get("output_bytes_values") or []
    output_equal = bool(ref_outputs) and ref_outputs == rust_outputs
    return {
        "ref": ref_workload,
        "rust": rust_workload,
        "ref_sum": ref_sum,
        "rust_sum": rust_sum,
        "ref_median": ref_median,
        "rust_median": rust_median,
        "ref_p95": ref_wall["p95"],
        "rust_p95": rust_wall["p95"],
        "speedup": speedup,
        "latency_change_pct": latency_change,
        "ref_peak_rss": ref_peak,
        "rust_peak_rss": rust_peak,
        "memory_change_pct": memory_change,
        "ref_growth": ref_growth,
        "rust_growth": rust_growth,
        "growth_change_pct": growth_change,
        "ref_outputs": ref_outputs,
        "rust_outputs": rust_outputs,
        "output_equal": output_equal,
        "ref_ok": ref_sum["n_success"],
        "ref_total": ref_sum["n_total"],
        "rust_ok": rust_sum["n_success"],
        "rust_total": rust_sum["n_total"],
        "ref_failures": ref_sum.get("failures") or [],
        "rust_failures": rust_sum.get("failures") or [],
    }


def command_text(run: dict) -> str:
    invocation = run.get("invocation") or {}
    argv = invocation.get("argv") or []
    cwd = invocation.get("cwd") or ""
    if not argv:
        return MISSING
    return f"cd {shlex.quote(cwd)} && PYTHONHASHSEED=0 " + " ".join(shlex.quote(str(part)) for part in argv)


def failure_text(failures: list[dict]) -> str:
    if not failures:
        return MISSING
    parts = []
    for failure in failures:
        error = failure.get("error") or {}
        kind = "timeout" if failure.get("timeout") else error.get("type", "error")
        parts.append(f"rep {failure.get('index')}: {kind}")
    return "; ".join(parts)


# ---------------------------------------------------------------------------
# report
# ---------------------------------------------------------------------------
def build_report(
    ref: dict,
    rust: dict,
    pairs: list[tuple[dict, dict, dict]],
    baselines: dict[str, int | None],
    warnings: list[str],
    errors: list[str],
    failures_present: bool,
    allow_failures: bool,
) -> tuple[str, list[str]]:
    ref_config, rust_config = ref.get("config", {}), rust.get("config", {})
    ref_env, rust_env = ref.get("environment", {}), rust.get("environment", {})
    threshold = ref_config.get("large_file_threshold_bytes") or 250000

    mismatch_notes = [f"{pair['ref']['id']} output-size mismatch" for pair in pairs if not pair["output_equal"]]
    verdict_fail = bool(errors) or bool(mismatch_notes) or (failures_present and not allow_failures)
    verdict = "FAIL" if verdict_fail else "PASS"

    lines: list[str] = []
    lines.append("# Performance comparison: reference pypdf 7.1.1 vs pypdf-rust port")
    lines.append("")
    lines.append(f"- Generated: {time_now()} (local)")
    lines.append(f"- Reference run: `{ref.get('backend')}` – {ref.get('generated_at_local')} – `{ref_env.get('module')}`")
    lines.append(f"- Port run: `{rust.get('backend')}` – {rust.get('generated_at_local')} – `{rust_env.get('module')}`")
    lines.append(f"- Repetitions per workload: reference {ref_config.get('repeat')} / port {rust_config.get('repeat')} "
                 f"(warm-up runs excluded: {ref_config.get('warmup')} / {rust_config.get('warmup')})")
    lines.append(f"- Per-repetition timeout: {ref_config.get('timeout_s')} s")
    lines.append(f"- PYTHONHASHSEED: reference {ref_env.get('hashseed')!r} / port {rust_env.get('hashseed')!r}")
    lines.append(f"- Machine: {ref_env.get('machine')} / {ref_env.get('platform')}")
    lines.append(f"- Data verdict: **{verdict}** (data integrity only: missing data / output-size equality; "
                 f"not a performance acceptance decision)")
    lines.append("")
    if errors:
        lines.append("## Blocking data problems")
        lines.append("")
        for note in errors:
            lines.append(f"- {note}")
        lines.append("")
    if warnings:
        lines.append("## Warnings")
        lines.append("")
        for note in warnings:
            lines.append(f"- {note}")
        lines.append("")

    lines.append("## Environment")
    lines.append("")
    lines.extend(table(
        ["item", "reference", "port"],
        [
            ["backend label", str(ref.get("backend")), str(rust.get("backend"))],
            ["Python", str(ref_env.get("python")), str(rust_env.get("python"))],
            ["executable", str(ref_env.get("executable")), str(rust_env.get("executable"))],
            ["pypdf version", str(ref_env.get("pypdf_version")), str(rust_env.get("pypdf_version"))],
            ["module", str(ref_env.get("module")), str(rust_env.get("module"))],
            ["rust extension", str(ref_env.get("rust_extension")), str(rust_env.get("rust_extension"))],
            ["platform", str(ref_env.get("platform")), str(rust_env.get("platform"))],
            ["cpu count", str(ref_env.get("cpu_count")), str(rust_env.get("cpu_count"))],
            ["loadavg (1/5/15 min) at start", fmt_loadavg(ref_env.get("loadavg_start")),
             fmt_loadavg(rust_env.get("loadavg_start"))],
            ["loadavg (1/5/15 min) at end", fmt_loadavg(ref_env.get("loadavg_end")),
             fmt_loadavg(rust_env.get("loadavg_end"))],
            ["shell uptime before", str((ref.get("machine_state_shell") or {}).get("uptime_before")),
             str((rust.get("machine_state_shell") or {}).get("uptime_before"))],
            ["shell uptime after", str((ref.get("machine_state_shell") or {}).get("uptime_after")),
             str((rust.get("machine_state_shell") or {}).get("uptime_after"))],
            ["extension sha256[:16]", str(((ref_env.get("rust_extension_identity") or {}).get("sha256") or "")[:16] or MISSING),
             str(((rust_env.get("rust_extension_identity") or {}).get("sha256") or "")[:16] or MISSING)],
            ["started (local)", str(ref.get("generated_at_local")), str(rust.get("generated_at_local"))],
        ],
    ))
    lines.append("")

    lines.append("## Commands")
    lines.append("")
    lines.append(f"- Reference: `{command_text(ref)}`")
    lines.append(f"- Port: `{command_text(rust)}`")
    lines.append(f"- Comparison: described by `validation/perf/run_perf.sh`; the baseline sizes in "
                 f"`validation/perf/workloads.json` were measured with the reference run.")
    lines.append("")

    lines.append("## Method")
    lines.append("")
    lines.append("- Reference and port always run in two separate, sequential processes; each process measures "
                 "exactly one backend, so imports, caches and allocator state cannot leak between sides.")
    lines.append("- Every workload performs one warm-up repetition (discarded) followed by the counted "
                 "repetitions.  The raw per-repetition wall time, CPU time, sampled peak RSS and output byte "
                 "count are stored in the two result JSON files.")
    lines.append("- `PYTHONHASHSEED=0` on both sides and `tools/oracle/prng.py::install_deterministic_secrets()` "
                 "is called before every repetition, so written PDFs are reproducible; output sizes and "
                 "sha256 digests are compared between sides and against the recorded reference baselines.")
    lines.append("- Timed region: the workload operation only.  Summary serialization and hashing happen after "
                 "the clock is stopped, so they do not inflate the measured latency.")
    lines.append("- Failures and timeouts are recorded per repetition, counted, and listed; `compare.py` fails "
                 "the run instead of silently dropping them.  A timeout is delivered with SIGALRM, so a "
                 "repetition blocked inside native code that never returns to the interpreter can overshoot "
                 "the wall-clock timeout (inherent limitation of in-process measurement).")
    lines.append("")

    lines.append("## Summary")
    lines.append("")
    lines.append(f"- Workloads compared: {len(pairs)}")
    faster = sum(1 for pair in pairs if pair["speedup"] is not None and pair["speedup"] > 1.0)
    slower = sum(1 for pair in pairs if pair["speedup"] is not None and pair["speedup"] < 1.0)
    lines.append(f"- Port faster: {faster}; port slower: {slower}; "
                 f"no timing data: {len(pairs) - faster - slower}")
    lines.append(f"- Output-size mismatches: {len(mismatch_notes)}")
    total_failures = sum(len(pair["ref_failures"]) + len(pair["rust_failures"]) for pair in pairs)
    lines.append(f"- Recorded failures/timeouts: {total_failures}")
    classes = ", ".join(
        f"{klass}: {', '.join(pair['ref']['id'] for pair in pairs if pair['ref'].get('size_class') == klass)}"
        for klass in ("small", "large", "mixed")
        if any(pair["ref"].get("size_class") == klass for pair in pairs)
    )
    lines.append(f"- File-size separation: {classes} (workloads with per-file data are additionally "
                 "split into large/small files below)")
    ref_load = fmt_loadavg(ref_env.get("loadavg_start"))
    rust_load = fmt_loadavg(rust_env.get("loadavg_start"))
    cpus = ref_env.get("cpu_count")
    lines.append(f"- Machine load at start (1-min, 5-min, 15-min): reference {ref_load}, port {rust_load} "
                 f"on {cpus} CPUs; a 1-min load above the CPU count means the machine was oversubscribed "
                 "and the two runs did not see identical conditions.")
    lines.append("- Measurement conditions: this is a shared macOS workstation with persistent system "
                 "background load that cannot be quiesced (WindowServer, the DeepSeek Harness renderer, "
                 "StorageManagement and similar system processes keep the 1-minute load average around 20+ "
                 "on a 10-CPU machine).  Reference and port ran sequentially in separate processes under "
                 "the same conditions; no repository builds or test suites were started by the validation "
                 "team while the final measurement was running.")
    lines.append("")
    lines.append("Metric definitions: `speedup = ref_median / port_median` (higher is better), "
                 "`latency Δ = (port_median / ref_median - 1) × 100`, "
                 "`RSS Δ = (port_peak_RSS / ref_peak_RSS - 1) × 100`. "
                 "Peak RSS is the maximum sampled resident-set size of a counted repetition "
                 "(sampled every ~1.5 ms while the repetition runs).")
    lines.append("")

    lines.append("## Per-workload results")
    lines.append("")
    rows = []
    for pair in pairs:
        wid = pair["ref"]["id"]
        status = "ok"
        if pair["ref_ok"] == 0 or pair["rust_ok"] == 0:
            status = "NO DATA"
        elif pair["ref_failures"] or pair["rust_failures"]:
            status = "FAILURES RECORDED"
        if not pair["output_equal"]:
            status = "SIZE MISMATCH"
        rows.append([
            wid,
            pair["ref"].get("name", ""),
            pair["ref"].get("size_class", ""),
            str(len(pair["ref"].get("files", []))),
            fmt_seconds(pair["ref_median"]),
            fmt_seconds(pair["rust_median"]),
            fmt_speedup(pair["speedup"]),
            fmt_pct(pair["latency_change_pct"]),
            fmt_seconds(pair["ref_p95"]),
            fmt_seconds(pair["rust_p95"]),
            fmt_mb(pair["ref_peak_rss"]),
            fmt_mb(pair["rust_peak_rss"]),
            fmt_pct(pair["memory_change_pct"]),
            (" / ".join(fmt_bytes(value) for value in pair["ref_outputs"]) + " / "
             + " / ".join(fmt_bytes(value) for value in pair["rust_outputs"]))
            if pair["ref_outputs"] and pair["rust_outputs"] else MISSING,
            status,
        ])
    lines.extend(table(
        ["workload", "name", "size class", "files", "ref median", "port median", "speedup",
         "latency Δ", "ref p95", "port p95", "ref peak RSS", "port peak RSS", "RSS Δ",
         "output bytes ref/port", "status"],
        rows,
    ))
    lines.append("")

    lines.append("## Latency distribution (counted repetitions)")
    lines.append("")
    rows = []
    for pair in pairs:
        ref_wall, rust_wall = pair["ref_sum"]["wall"], pair["rust_sum"]["wall"]
        rows.append([
            pair["ref"]["id"],
            fmt_seconds(ref_wall["min"]), fmt_seconds(ref_wall["median"]),
            fmt_seconds(ref_wall["p95"]), fmt_seconds(ref_wall["max"]),
            fmt_seconds(rust_wall["min"]), fmt_seconds(rust_wall["median"]),
            fmt_seconds(rust_wall["p95"]), fmt_seconds(rust_wall["max"]),
        ])
    lines.extend(table(
        ["workload", "ref min", "ref median", "ref p95", "ref max",
         "port min", "port median", "port p95", "port max"],
        rows,
    ))
    lines.append("")

    lines.append("## CPU time and peak memory")
    lines.append("")
    rows = []
    for pair in pairs:
        ref_cpu, rust_cpu = pair["ref_sum"]["cpu"], pair["rust_sum"]["cpu"]
        ref_peak = pair["ref_sum"]["peak_rss_bytes"]
        rust_peak = pair["rust_sum"]["peak_rss_bytes"]
        ref_maxrss = max(
            (rep.get("proc_maxrss_bytes") or 0) for rep in pair["ref"].get("repetitions", []) if rep.get("success")
        ) or None
        rust_maxrss = max(
            (rep.get("proc_maxrss_bytes") or 0) for rep in pair["rust"].get("repetitions", []) if rep.get("success")
        ) or None
        rows.append([
            pair["ref"]["id"],
            fmt_seconds(ref_cpu["median"]), fmt_seconds(rust_cpu["median"]),
            fmt_mb(ref_peak["max"]), fmt_mb(rust_peak["max"]),
            fmt_pct(pair["memory_change_pct"]),
            fmt_mb(pair["ref_growth"]), fmt_mb(pair["rust_growth"]),
            fmt_pct(pair["growth_change_pct"]),
            fmt_mb(ref_maxrss), fmt_mb(rust_maxrss),
        ])
    lines.extend(table(
        ["workload", "ref CPU median", "port CPU median", "ref peak RSS (max rep)",
         "port peak RSS (max rep)", "RSS Δ", "ref working-set growth", "port working-set growth",
         "growth Δ", "ref ru_maxrss", "port ru_maxrss"],
        rows,
    ))
    lines.append("")
    lines.append("Peak RSS is sampled current RSS, so it is *not* a process-lifetime high-water mark. "
                 "It can still include pages that the allocator retained from earlier workloads; the "
                 "working-set growth column (peak RSS minus RSS at repetition start) removes that bias. "
                 "`ru_maxrss` is the process high-water mark and is shown for reference only.")
    lines.append("")

    lines.append("## Output-size check")
    lines.append("")
    rows = []
    for pair in pairs:
        wid = pair["ref"]["id"]
        expected = baselines.get(wid)
        ref_sha = (pair["ref_sum"].get("output_sha256_values") or [None])[0]
        rust_sha = (pair["rust_sum"].get("output_sha256_values") or [None])[0]
        ref_sha_text = ref_sha[:16] if ref_sha else MISSING
        rust_sha_text = rust_sha[:16] if rust_sha else MISSING
        rows.append([
            wid,
            pair["ref"].get("output_kind", ""),
            f"{expected:,} B" if expected is not None else "not recorded",
            f"{pair['ref_outputs'][0]:,} B" if len(pair["ref_outputs"]) == 1 else str(pair["ref_outputs"]),
            f"{pair['rust_outputs'][0]:,} B" if len(pair["rust_outputs"]) == 1 else str(pair["rust_outputs"]),
            ref_sha_text,
            rust_sha_text,
            "yes" if pair["output_equal"] else "**NO**",
        ])
    lines.extend(table(
        ["workload", "output kind", "recorded baseline", "ref bytes", "port bytes",
         "ref sha256[:16]", "port sha256[:16]", "equal"],
        rows,
    ))
    lines.append("")
    lines.append("Recorded baselines come from `validation/perf/workloads.json`; they were captured from the "
                 "reference implementation on the same corpus. Equality of the `ref`/`port` columns is the "
                 "pass criterion; a drift of the reference against the recorded baseline is reported as a warning.")
    lines.append("")

    lines.append("## Success and failure counts")
    lines.append("")
    rows = []
    for pair in pairs:
        rows.append([
            pair["ref"]["id"],
            f"{pair['ref_ok']}/{pair['ref_total']}",
            f"{pair['rust_ok']}/{pair['rust_total']}",
            str(pair["ref_sum"]["n_timeout"]),
            str(pair["rust_sum"]["n_timeout"]),
            failure_text(pair["ref_failures"]),
            failure_text(pair["rust_failures"]),
        ])
    lines.extend(table(
        ["workload", "ref ok/total", "port ok/total", "ref timeouts", "port timeouts",
         "ref failures", "port failures"],
        rows,
    ))
    lines.append("")
    failure_details = []
    for pair in pairs:
        for side, failures in (("reference", pair["ref_failures"]), ("port", pair["rust_failures"])):
            for failure in failures:
                error = failure.get("error") or {}
                failure_details.append(
                    f"- {pair['ref']['id']} ({side}) rep {failure.get('index')}: "
                    f"{'timeout' if failure.get('timeout') else error.get('type')} – "
                    f"{md_escape(error.get('message', ''))[:300]}"
                )
    if failure_details:
        lines.append("Failure details:")
        lines.append("")
        lines.extend(failure_details)
        lines.append("")
    else:
        lines.append("No failures or timeouts were recorded.")
        lines.append("")

    lines.append("## Per-file latency")
    lines.append("")
    lines.append("The reader/text/image workloads time each input file separately inside every repetition "
                 "(the timed region is identical for both backends). Files at or above "
                 f"{fmt_bytes(threshold)} are reported as *large*, the rest as *small*. For W8 the per-file "
                 "time covers open + text extraction + append for that file; the shared writer serialization "
                 "is only included in the workload total.")
    lines.append("")
    for pair in pairs:
        ref_files = {item["path"]: item["bytes"] for item in pair["ref"].get("files", [])}
        ref_med = pair["ref_sum"].get("per_file_medians_s") or {}
        rust_med = pair["rust_sum"].get("per_file_medians_s") or {}
        if not ref_med and not rust_med:
            continue
        paths = [path for path in ref_files if path in ref_med or path in rust_med]
        if not paths:
            continue
        lines.append(f"### {pair['ref']['id']} – {pair['ref'].get('name', '')}")
        lines.append("")
        rows = []
        class_totals: dict[str, dict[str, float]] = {"large": {"ref": 0.0, "port": 0.0},
                                                     "small": {"ref": 0.0, "port": 0.0}}
        for path in sorted(paths, key=lambda item: (-ref_files.get(item, 0), item)):
            size = ref_files.get(path)
            klass = "large" if size is not None and size >= threshold else "small"
            ref_value = ref_med.get(path)
            rust_value = rust_med.get(path)
            speedup = (ref_value / rust_value) if (ref_value and rust_value) else None
            rows.append([
                path,
                fmt_bytes(size),
                klass,
                fmt_ms(ref_value),
                fmt_ms(rust_value),
                fmt_speedup(speedup),
            ])
            if ref_value:
                class_totals[klass]["ref"] += ref_value
            if rust_value:
                class_totals[klass]["port"] += rust_value
        lines.extend(table(["file", "bytes", "class", "ref median", "port median", "speedup"], rows))
        lines.append("")
        rows = []
        for klass in ("large", "small"):
            ref_total = class_totals[klass]["ref"]
            port_total = class_totals[klass]["port"]
            if not ref_total and not port_total:
                continue
            speedup = (ref_total / port_total) if port_total else None
            rows.append([
                klass,
                str(sum(1 for path in paths if (ref_files.get(path) or 0) >= threshold) if klass == "large"
                    else sum(1 for path in paths if (ref_files.get(path) or 0) < threshold)),
                fmt_ms(ref_total or None),
                fmt_ms(port_total or None),
                fmt_speedup(speedup),
            ])
        lines.extend(table(["class", "files", "ref total (sum of medians)", "port total", "speedup"], rows))
        lines.append("")

    lines.append("## Regressions and anomalies")
    lines.append("")
    anomalies = []
    for pair in pairs:
        wid = pair["ref"]["id"]
        if pair["speedup"] is not None and pair["speedup"] < 1.0:
            anomalies.append(
                f"- **{wid}** port is slower: speedup {pair['speedup']:.2f}×, "
                f"latency change {fmt_pct(pair['latency_change_pct'])} "
                f"({fmt_seconds(pair['ref_median'])} → {fmt_seconds(pair['rust_median'])})."
            )
        peak_delta = (pair["rust_peak_rss"] or 0) - (pair["ref_peak_rss"] or 0)
        growth_delta = (pair["rust_growth"] or 0) - (pair["ref_growth"] or 0)
        # Only flag memory when the *working set* grew: absolute peak RSS carries
        # allocator retention from earlier workloads and is noisy on tiny deltas.
        if (pair["growth_change_pct"] is not None and pair["growth_change_pct"] > 25.0
                and growth_delta > 2_000_000):
            anomalies.append(
                f"- **{wid}** port working-set growth is larger: "
                f"{fmt_mb(pair['ref_growth'])} → {fmt_mb(pair['rust_growth'])} "
                f"({fmt_pct(pair['growth_change_pct'])}); absolute peak RSS "
                f"{fmt_mb(pair['ref_peak_rss'])} → {fmt_mb(pair['rust_peak_rss'])} "
                f"({fmt_pct(pair['memory_change_pct'])})."
            )
        elif (pair["memory_change_pct"] is not None and pair["memory_change_pct"] > 10.0
              and peak_delta > 5_000_000 and growth_delta > 1_000_000):
            anomalies.append(
                f"- **{wid}** port uses more peak RSS: "
                f"{fmt_mb(pair['ref_peak_rss'])} → {fmt_mb(pair['rust_peak_rss'])} "
                f"({fmt_pct(pair['memory_change_pct'])}), working-set growth "
                f"{fmt_mb(pair['ref_growth'])} → {fmt_mb(pair['rust_growth'])} "
                f"({fmt_pct(pair['growth_change_pct'])})."
            )
        if not pair["output_equal"]:
            anomalies.append(
                f"- **{wid}** output-size mismatch: reference {pair['ref_outputs']} vs port "
                f"{pair['rust_outputs']} bytes."
            )
        if len(pair["ref_outputs"]) > 1:
            anomalies.append(f"- **{wid}** reference output size is not deterministic across repetitions: "
                             f"{pair['ref_outputs']} bytes.")
        if len(pair["rust_outputs"]) > 1:
            anomalies.append(f"- **{wid}** port output size is not deterministic across repetitions: "
                             f"{pair['rust_outputs']} bytes.")
        ref_sha = (pair["ref_sum"].get("output_sha256_values") or [])
        rust_sha = (pair["rust_sum"].get("output_sha256_values") or [])
        if pair["output_equal"] and ref_sha and rust_sha and ref_sha != rust_sha:
            anomalies.append(
                f"- **{wid}** output size matches but the content digest differs "
                f"(ref {ref_sha[0][:16]}… vs port {rust_sha[0][:16]}…); inspect semantics separately."
            )
        expected = baselines.get(wid)
        if expected is not None and pair["ref_outputs"] and pair["ref_outputs"] != [expected]:
            anomalies.append(
                f"- **{wid}** reference output size drifted from the recorded baseline: "
                f"expected {expected} B, measured {pair['ref_outputs']} B."
            )
        if pair["ref_failures"] or pair["rust_failures"]:
            anomalies.append(
                f"- **{wid}** incomplete repetitions: reference {failure_text(pair['ref_failures'])}, "
                f"port {failure_text(pair['rust_failures'])}."
            )
    if anomalies:
        lines.extend(anomalies)
    else:
        lines.append("No anomalies: the port is neither slower nor larger than the reference on these workloads.")
    lines.append("")

    lines.append("## Workload definitions")
    lines.append("")
    rows = []
    for pair in pairs:
        workload = pair["ref"]
        rows.append([
            workload["id"],
            workload.get("name", ""),
            workload["type"],
            workload.get("size_class", ""),
            workload.get("output_kind", ""),
            ", ".join(sorted({str(key) for key in (workload.get("params") or {})})) or MISSING,
            str(len(workload.get("files", []))),
        ])
    lines.extend(table(["id", "name", "operation", "size class", "output kind", "parameters", "files"], rows))
    lines.append("")
    for pair in pairs:
        workload = pair["ref"]
        lines.append(f"- **{workload['id']}** {workload.get('description', '')}")
        lines.append(f"  - files: {', '.join(item['path'] for item in workload.get('files', []))}")
    lines.append("")
    lines.append("---")
    lines.append("")
    lines.append("Raw per-repetition data: `validation/results/perf-reference.json`, "
                 "`validation/results/perf-rust.json`. Logs: `validation/results/logs/`.")
    lines.append("")
    return "\n".join(lines), mismatch_notes


def time_now() -> str:
    import time

    return time.strftime("%Y-%m-%d %H:%M:%S")


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------
def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--ref", type=Path, default=DEFAULT_REF)
    parser.add_argument("--rust", type=Path, default=DEFAULT_RUST)
    parser.add_argument("--workloads", type=Path, default=DEFAULT_WORKLOADS)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--allow-failures", action="store_true",
                        help="do not fail the comparison on recorded failures/timeouts (still reported)")
    args = parser.parse_args(argv)

    errors: list[str] = []
    warnings: list[str] = []
    ref = load_run(args.ref, "reference", errors)
    rust = load_run(args.rust, "rust", errors)
    if ref is None or rust is None:
        print("compare: FAILED – missing data:", file=sys.stderr)
        for note in errors:
            print(f"  - {note}", file=sys.stderr)
        return 2

    compare_workload_definitions(ref, rust, warnings, errors)
    compare_environments(ref, rust, warnings)
    baselines = load_baselines(args.workloads, warnings)

    ref_map = {workload["id"]: workload for workload in ref.get("workloads", [])}
    rust_map = {workload["id"]: workload for workload in rust.get("workloads", [])}
    order = [workload["id"] for workload in ref.get("workloads", [])]
    pairs: list[dict] = []
    for wid in sorted(set(ref_map) & set(rust_map), key=lambda item: order.index(item)):
        ref_workload, rust_workload = ref_map[wid], rust_map[wid]
        pair = metrics(ref_workload, rust_workload)
        if pair["ref_ok"] == 0:
            errors.append(f"missing data: workload {wid} has no successful reference repetition")
        if pair["rust_ok"] == 0:
            errors.append(f"missing data: workload {wid} has no successful port repetition")
        pairs.append(pair)

    failures_present = any(pair["ref_failures"] or pair["rust_failures"] for pair in pairs)
    report, mismatch_notes = build_report(
        ref, rust, pairs, baselines, warnings, errors, failures_present, args.allow_failures
    )
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(report + "\n", encoding="utf-8")

    print(f"compare: report written to {args.out}")
    print("")
    print(f"{'workload':8s} {'ref median':>12s} {'port median':>12s} {'speedup':>8s} "
          f"{'latency Δ':>10s} {'RSS Δ':>9s} {'output':>10s} status")
    for pair in pairs:
        status = "ok"
        if pair["ref_ok"] == 0 or pair["rust_ok"] == 0:
            status = "NO DATA"
        elif pair["ref_failures"] or pair["rust_failures"]:
            status = "FAILURES"
        if not pair["output_equal"]:
            status = "SIZE MISMATCH"
        print(
            f"{pair['ref']['id']:8s} {fmt_seconds(pair['ref_median']):>12s} "
            f"{fmt_seconds(pair['rust_median']):>12s} {fmt_speedup(pair['speedup']):>8s} "
            f"{fmt_pct(pair['latency_change_pct']):>10s} {fmt_pct(pair['memory_change_pct']):>9s} "
            f"{('equal' if pair['output_equal'] else 'DIFF'):>10s} {status}"
        )
    if errors:
        print("", file=sys.stderr)
        print("compare: blocking data problems:", file=sys.stderr)
        for note in errors:
            print(f"  - {note}", file=sys.stderr)
    if mismatch_notes:
        print("", file=sys.stderr)
        print("compare: output-size mismatches: " + ", ".join(mismatch_notes), file=sys.stderr)
    if failures_present:
        print("", file=sys.stderr)
        print("compare: failures/timeouts were recorded – see the report", file=sys.stderr)

    failed = bool(errors) or bool(mismatch_notes) or (failures_present and not args.allow_failures)
    print("")
    print(f"compare: {'FAIL' if failed else 'PASS'}")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
