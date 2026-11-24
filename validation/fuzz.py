#!/usr/bin/env python3
"""Mutation-fuzzing differential wrapper for the pypdf Rust port (plan §8.4).

Generates ``N`` deterministic mutated PDFs with the project's existing
``tools/fuzz/gen_corpus.py``, executes the existing ``tools/fuzz/run.py``
scenario in both virtual environments, and compares the two JSON result files.

The scenario itself (open with ``strict=False``, page count, metadata, labels,
outline, trailer keys, first-page text) is unchanged: this wrapper is only an
orchestrator + comparator.  The comparison must be *identical*:

* same success/failure decision per file,
* same exception class and message per file,
* same canonical object graph / text for the files that parse.

Hard crashes (segfault inside the PyO3 extension), Rust panics and timeouts are
counted separately for each side and are never reported as a pass.

Outputs (under ``validation/results/`` by default):
  * fuzz-comparison.json   summary + per-file differences
  * fuzz-raw-oracle.json   raw result file of the reference implementation
  * fuzz-raw-port.json     raw result file of the port
  * fuzz-report.md         human readable report

Usage:
    python validation/fuzz.py
    python validation/fuzz.py --count 500 --seed 7 --timeout 10
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

VALIDATION_DIR = Path(__file__).resolve().parent
ROOT = VALIDATION_DIR.parent
RESULTS_DIR = VALIDATION_DIR / "results"
TOOLS_FUZZ = ROOT / "tools" / "fuzz"
RESOURCES = ROOT / "resources"

PYTHON_EXE: dict[str, Path] = {
    "oracle": ROOT / ".venvs" / "oracle" / "bin" / "python",
    "port": ROOT / ".venvs" / "dev" / "bin" / "python",
}

PANIC_PATTERNS = ("panicked at", "PanicException", "RUST_BACKTRACE", "stack overflow")


def git_commit() -> str:
    try:
        completed = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=str(ROOT),
            capture_output=True,
            text=True,
            timeout=10,
        )
        if completed.returncode == 0:
            return completed.stdout.strip()
    except Exception:  # noqa: BLE001
        pass
    return "unknown"


def package_fingerprint(backend: str) -> dict[str, Any] | None:
    """Hash the pypdf package used by a backend (detects concurrent rebuilds)."""
    if backend == "port":
        package = ROOT / "python" / "pypdf"
    else:
        matches = sorted(PYTHON_EXE[backend].parents[1].glob("lib/python*/site-packages/pypdf"))
        package = matches[0] if matches else None
    if package is None or not package.exists():
        return None
    digest = hashlib.sha256()
    files = 0
    for path in sorted(package.rglob("*")):
        if not path.is_file() or path.suffix not in {".py", ".so", ".pyd", ".pyi"}:
            continue
        digest.update(path.relative_to(package).as_posix().encode())
        digest.update(b"\0")
        digest.update(hashlib.sha256(path.read_bytes()).digest())
        files += 1
    return {"package": str(package), "files": files, "sha256": digest.hexdigest()}


def probe_env() -> dict[str, str]:
    import os

    env = dict(os.environ)
    env.pop("PYTHONPATH", None)
    env["PYTHONHASHSEED"] = "0"
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    env["PYTHONUNBUFFERED"] = "1"
    return env


def count_sources() -> int:
    return len(sorted(RESOURCES.rglob("*.pdf"))) if RESOURCES.exists() else 0


def run_command(
    command: list[str], *, timeout: float, label: str
) -> dict[str, Any]:
    started = time.monotonic()
    try:
        completed = subprocess.run(
            command,
            capture_output=True,
            text=True,
            timeout=timeout,
            cwd=str(ROOT),
            env=probe_env(),
        )
        return {
            "label": label,
            "command": command,
            "returncode": completed.returncode,
            "timed_out": False,
            "wall_s": round(time.monotonic() - started, 3),
            "stdout_tail": (completed.stdout or "")[-4000:],
            "stderr_tail": (completed.stderr or "")[-4000:],
        }
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout or ""
        stderr = exc.stderr or ""
        if isinstance(stdout, bytes):
            stdout = stdout.decode("utf-8", "replace")
        if isinstance(stderr, bytes):
            stderr = stderr.decode("utf-8", "replace")
        return {
            "label": label,
            "command": command,
            "returncode": None,
            "timed_out": True,
            "wall_s": round(time.monotonic() - started, 3),
            "stdout_tail": stdout[-4000:],
            "stderr_tail": stderr[-4000:],
        }


def looks_like_panic(text: str) -> bool:
    return any(pattern.lower() in (text or "").lower() for pattern in PANIC_PATTERNS)


def load_json(path: Path) -> dict[str, Any] | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception:  # noqa: BLE001
        return None


def deep_diff(
    left: Any, right: Any, path: str, out: list[dict[str, Any]], limit: int
) -> None:
    if len(out) >= limit:
        return
    if isinstance(left, dict) and isinstance(right, dict):
        for key in sorted(set(left) | set(right)):
            child = f"{path}/{key}"
            if key not in left:
                out.append({"path": child, "oracle": "<missing>", "port": right[key]})
            elif key not in right:
                out.append({"path": child, "oracle": left[key], "port": "<missing>"})
            else:
                deep_diff(left[key], right[key], child, out, limit)
            if len(out) >= limit:
                return
        return
    if isinstance(left, list) and isinstance(right, list):
        if len(left) != len(right):
            out.append(
                {
                    "path": f"{path}/length",
                    "oracle": len(left),
                    "port": len(right),
                }
            )
        for index, (left_item, right_item) in enumerate(zip(left, right)):
            deep_diff(left_item, right_item, f"{path}[{index}]", out, limit)
            if len(out) >= limit:
                return
        return
    if type(left) is not type(right) and not (
        isinstance(left, (int, float)) and isinstance(right, (int, float))
    ):
        out.append({"path": path, "oracle": left, "port": right, "reason": "type"})
        return
    if left != right:
        out.append({"path": path, "oracle": left, "port": right})


def classify_file_error(entry: dict[str, Any] | None) -> dict[str, Any] | None:
    if not entry:
        return None
    return entry.get("error")


def side_stats(
    *,
    backend: str,
    process: dict[str, Any],
    payload: dict[str, Any] | None,
    raw_path: Path,
) -> dict[str, Any]:
    files = (payload or {}).get("files") or {}
    stats = {
        "backend": backend,
        "python_exe": str(PYTHON_EXE[backend]),
        "raw_json": str(raw_path),
        "files": len(files),
        "ok": 0,
        "errors": 0,
        "timeouts": 0,
        "panics": 0,
        "crash": False,
        "wall_s": process.get("wall_s"),
        "returncode": process.get("returncode"),
        "timed_out": process.get("timed_out"),
        "stderr_tail": process.get("stderr_tail"),
        "stdout_tail": process.get("stdout_tail"),
        "panic_suspected": looks_like_panic(process.get("stderr_tail") or ""),
        "missing_files": [],
        "timeout_files": [],
        "panic_files": [],
        "error_files": [],
    }
    for name, entry in files.items():
        error = classify_file_error(entry)
        if error is None:
            stats["ok"] += 1
            continue
        error_type = str(error.get("type") or "")
        if "Timeout" in error_type:
            stats["timeouts"] += 1
            if len(stats["timeout_files"]) < 50:
                stats["timeout_files"].append(name)
        elif "Panic" in error_type or looks_like_panic(str(error.get("msg") or "")):
            stats["panics"] += 1
            if len(stats["panic_files"]) < 50:
                stats["panic_files"].append(name)
        else:
            stats["errors"] += 1
            if len(stats["error_files"]) < 50:
                stats["error_files"].append(name)
    if process.get("timed_out") or process.get("returncode") not in (0, None):
        stats["crash"] = True
    if payload is None:
        stats["crash"] = True
        stats["parse_error"] = "run.py did not produce a readable JSON result file"
    return stats


def compare_files(
    oracle_payload: dict[str, Any] | None,
    port_payload: dict[str, Any] | None,
    *,
    max_diffs: int,
) -> dict[str, Any]:
    oracle_files = (oracle_payload or {}).get("files") or {}
    port_files = (port_payload or {}).get("files") or {}
    missing = sorted(set(oracle_files) ^ set(port_files))
    decision_diffs: list[dict[str, Any]] = []
    message_diffs: list[dict[str, Any]] = []
    result_diffs: list[dict[str, Any]] = []

    for name in sorted(set(oracle_files) & set(port_files)):
        oracle_entry = oracle_files[name]
        port_entry = port_files[name]
        oracle_error = oracle_entry.get("error")
        port_error = port_entry.get("error")
        if (oracle_error is None) != (port_error is None):
            decision_diffs.append(
                {
                    "file": name,
                    "oracle": {"error": oracle_error},
                    "port": {"error": port_error},
                    "reason": "success/failure decision differs",
                }
            )
            continue
        if oracle_error is not None:
            if oracle_error.get("type") != port_error.get("type"):
                decision_diffs.append(
                    {
                        "file": name,
                        "oracle": oracle_error,
                        "port": port_error,
                        "reason": "exception class differs",
                    }
                )
            elif oracle_error.get("msg") != port_error.get("msg"):
                message_diffs.append(
                    {
                        "file": name,
                        "oracle": oracle_error.get("msg"),
                        "port": port_error.get("msg"),
                        "type": oracle_error.get("type"),
                    }
                )
            continue
        diffs: list[dict[str, Any]] = []
        deep_diff(oracle_entry.get("result"), port_entry.get("result"), "", diffs, max_diffs)
        if diffs:
            result_diffs.append({"file": name, "diffs": diffs[:max_diffs]})

    decision_diffs = decision_diffs[:max_diffs]
    message_diffs = message_diffs[:max_diffs]
    result_diffs = result_diffs[:max_diffs]
    identical = not (missing or decision_diffs or message_diffs or result_diffs)
    return {
        "identical": identical,
        "timeouts": {
            "oracle": [],
            "port": [],
            "same_files": [],
            "only_oracle": [],
            "only_port": [],
        },
        "missing_files": missing[:max_diffs],
        "decision_diffs": decision_diffs,
        "message_diffs": message_diffs,
        "result_diffs": result_diffs,
        "truncated": {
            "decision_diffs": False,
            "message_diffs": False,
            "result_diffs": False,
        },
    }


def render_report(payload: dict[str, Any]) -> str:
    lines: list[str] = []
    lines.append("# Mutation fuzzing report (§8.4)")
    lines.append("")
    lines.append(f"Generated: `{payload['generated_at']}` (UTC)  ")
    lines.append(f"Repository commit: `{payload['git_commit']}`")
    lines.append("")
    corpus = payload["corpus"]
    lines.append("## Corpus")
    lines.append("")
    lines.append(
        f"* {corpus['count']} mutated PDF(s), seed `{corpus['seed']}`, "
        f"source PDFs under `resources/`: {corpus['sources']}"
    )
    lines.append(f"* per-file timeout: {corpus['per_file_timeout_s']} s")
    lines.append(f"* corpus directory: `{corpus['dir']}`")
    lines.append("")
    lines.append("## Per-side execution")
    lines.append("")
    lines.append("| Side | Files | Handled | Raised | Timeouts | Panics | Hard crash | Wall |")
    lines.append("|---|---|---|---|---|---|---|---|")
    for side in ("oracle", "port"):
        stats = payload["backends"][side]
        lines.append(
            "| {} | {} | {} | {} | {} | {} | {} | {} s |".format(
                side,
                stats["files"],
                stats["ok"],
                stats["errors"],
                stats["timeouts"],
                stats["panics"],
                stats["crash"],
                stats["wall_s"],
            )
        )
    lines.append("")
    for side in ("oracle", "port"):
        stats = payload["backends"][side]
        if stats.get("panic_suspected") or stats.get("crash"):
            lines.append(f"### {side}: crash/panic evidence")
            lines.append("")
            lines.append(f"* returncode: `{stats.get('returncode')}`, timed_out: `{stats.get('timed_out')}`")
            lines.append("")
            if stats.get("stderr_tail"):
                lines.append("```")
                lines.append(stats["stderr_tail"][-3000:])
                lines.append("```")
            lines.append("")
    comparison = payload["comparison"]
    timeouts = comparison.get("timeouts") or {}
    if timeouts.get("oracle") or timeouts.get("port"):
        lines.append("### Timeouts")
        lines.append("")
        lines.append(
            "* oracle: {} file(s) timed out; port: {} file(s) timed out; "
            "both sides on the same file: {}.".format(
                len(timeouts.get("oracle") or []),
                len(timeouts.get("port") or []),
                len(timeouts.get("same_files") or []),
            )
        )
        lines.append(
            "* A timeout is reported explicitly as TIMEOUT and is never treated as PASS."
        )
        lines.append("")
    lines.append("## Comparison (must be identical)")
    lines.append("")
    lines.append(
        "Result: **{}** — {} missing file(s), {} decision difference(s), "
        "{} message difference(s), {} canonical-result difference(s).".format(
            "IDENTICAL" if comparison["identical"] else "DIFFERENT",
            len(comparison["missing_files"]),
            len(comparison["decision_diffs"]),
            len(comparison["message_diffs"]),
            len(comparison["result_diffs"]),
        )
    )
    lines.append("")
    if comparison.get("environment_stable") is False:
        lines.append(
            "> **WARNING — torn run**: the pypdf package fingerprint changed during the "
            "fuzz run (concurrent rebuild). The comparison below is not authoritative; "
            "re-run on a quiet tree."
        )
        lines.append("")
    if comparison["decision_diffs"]:
        lines.append("### Decision / exception-class differences")
        lines.append("")
        for diff in comparison["decision_diffs"]:
            lines.append(
                "* `{}`: {}".format(diff["file"], json.dumps(diff, default=str)[:500])
            )
        lines.append("")
    if comparison["missing_files"]:
        lines.append("### Files present on one side only")
        lines.append("")
        for name in comparison["missing_files"]:
            lines.append(f"* `{name}`")
        lines.append("")
    if comparison["message_diffs"]:
        lines.append("### Message differences (same exception class)")
        lines.append("")
        for diff in comparison["message_diffs"]:
            lines.append(
                "* `{}` ({}): oracle `{}` vs port `{}`".format(
                    diff["file"], diff.get("type"), diff.get("oracle"), diff.get("port")
                )
            )
        lines.append("")
    if comparison["result_diffs"]:
        lines.append("### Canonical result differences")
        lines.append("")
        for diff in comparison["result_diffs"]:
            lines.append(f"* `{diff['file']}`:")
            for item in diff["diffs"][:10]:
                lines.append(
                    "  * `{}`: oracle={} port={}".format(
                        item.get("path"),
                        json.dumps(item.get("oracle"), default=str)[:160],
                        json.dumps(item.get("port"), default=str)[:160],
                    )
                )
        lines.append("")
    lines.append("## Reproduction")
    lines.append("")
    lines.append("```bash")
    lines.append(
        "python validation/fuzz.py --count {} --seed {} --timeout {}".format(
            corpus["count"], corpus["seed"], corpus["per_file_timeout_s"]
        )
    )
    lines.append("```")
    lines.append("")
    lines.append(
        "Raw evidence: `validation/results/fuzz-raw-oracle.json`, "
        "`validation/results/fuzz-raw-port.json`, `validation/results/fuzz-comparison.json`."
    )
    lines.append("")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--count", type=int, default=200, help="number of mutated PDFs")
    parser.add_argument("--seed", type=int, default=20261124)
    parser.add_argument("--timeout", type=int, default=10, help="per-file alarm timeout inside run.py")
    parser.add_argument(
        "--global-timeout",
        type=float,
        default=900.0,
        help="hard wall-clock timeout for each run.py process",
    )
    parser.add_argument("--corpus-dir", type=Path, default=None)
    parser.add_argument("--keep-corpus", action="store_true")
    parser.add_argument("--max-diffs", type=int, default=50)
    parser.add_argument("--skip-generate", action="store_true", help="reuse --corpus-dir as-is")
    args = parser.parse_args(argv)

    for required in (TOOLS_FUZZ / "gen_corpus.py", TOOLS_FUZZ / "run.py"):
        if not required.exists():
            print(f"[fuzz] missing helper: {required}", file=sys.stderr)
            return 2

    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    work_dir = args.corpus_dir
    temporary = work_dir is None
    if temporary:
        work_dir = Path(tempfile.mkdtemp(prefix="pypdf-fuzz-corpus-"))
    work_dir.mkdir(parents=True, exist_ok=True)
    corpus_dir = work_dir / "corpus"

    if not args.skip_generate:
        print(f"[fuzz] generating {args.count} mutated PDFs (seed {args.seed}) ...")
        generated = run_command(
            [
                sys.executable,
                str(TOOLS_FUZZ / "gen_corpus.py"),
                "--out",
                str(corpus_dir),
                "--count",
                str(args.count),
                "--seed",
                str(args.seed),
            ],
            timeout=180.0,
            label="gen_corpus",
        )
        print((generated.get("stdout_tail") or "").strip())
        if generated["returncode"] != 0 or not corpus_dir.exists():
            print(
                f"[fuzz] corpus generation failed (returncode={generated['returncode']})",
                file=sys.stderr,
            )
            print(generated.get("stderr_tail") or "", file=sys.stderr)
            return 2

    fingerprints_before = {side: package_fingerprint(side) for side in PYTHON_EXE}
    backends: dict[str, dict[str, Any]] = {}
    raw_paths: dict[str, Path] = {}
    processes: dict[str, dict[str, Any]] = {}
    for backend, exe in PYTHON_EXE.items():
        raw_path = RESULTS_DIR / f"fuzz-raw-{backend}.json"
        raw_paths[backend] = raw_path
        print(f"[fuzz] running run.py with the {backend} interpreter ...")
        process = run_command(
            [
                str(exe),
                str(TOOLS_FUZZ / "run.py"),
                "--corpus",
                str(corpus_dir),
                "--out",
                str(raw_path),
                "--timeout",
                str(args.timeout),
            ],
            timeout=args.global_timeout,
            label=backend,
        )
        processes[backend] = process
        payload = load_json(raw_path) if raw_path.exists() else None
        stats = side_stats(
            backend=backend, process=process, payload=payload, raw_path=raw_path
        )
        backends[backend] = stats
        print(
            "[fuzz] {}: {} file(s), ok={}, raised={}, timeouts={}, panics={}, "
            "crash={}, wall={}s".format(
                backend,
                stats["files"],
                stats["ok"],
                stats["errors"],
                stats["timeouts"],
                stats["panics"],
                stats["crash"],
                stats["wall_s"],
            )
        )
    fingerprints_after = {side: package_fingerprint(side) for side in PYTHON_EXE}
    environment_stable = fingerprints_before == fingerprints_after
    if not environment_stable:
        print(
            "[fuzz] WARNING: a pypdf package changed during the fuzz run "
            "(concurrent rebuild) - this run is torn and must be repeated",
            file=sys.stderr,
        )

    oracle_payload = load_json(raw_paths["oracle"])
    port_payload = load_json(raw_paths["port"])
    comparison = compare_files(oracle_payload, port_payload, max_diffs=args.max_diffs)
    comparison["environment_stable"] = environment_stable
    oracle_timeouts = set(backends["oracle"].get("timeout_files") or [])
    port_timeouts = set(backends["port"].get("timeout_files") or [])
    comparison["timeouts"] = {
        "oracle": sorted(oracle_timeouts),
        "port": sorted(port_timeouts),
        "same_files": sorted(oracle_timeouts & port_timeouts),
        "only_oracle": sorted(oracle_timeouts - port_timeouts),
        "only_port": sorted(port_timeouts - oracle_timeouts),
    }

    payload = {
        "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "git_commit": git_commit(),
        "corpus": {
            "count": args.count,
            "seed": args.seed,
            "sources": count_sources(),
            "per_file_timeout_s": args.timeout,
            "dir": str(corpus_dir),
            "kept": bool(args.keep_corpus),
        },
        "environment": {
            "fingerprint_before": fingerprints_before,
            "fingerprint_after": fingerprints_after,
            "stable": environment_stable,
        },
        "backends": backends,
        "comparison": comparison,
        "overall": "PASS" if comparison["identical"] else "FAIL",
    }
    comparison_out = RESULTS_DIR / "fuzz-comparison.json"
    comparison_out.write_text(
        json.dumps(payload, indent=1, sort_keys=True) + "\n", encoding="utf-8"
    )
    report_out = RESULTS_DIR / "fuzz-report.md"
    report_out.write_text(render_report(payload), encoding="utf-8")

    if args.keep_corpus:
        kept = RESULTS_DIR / "fuzz-corpus"
        if kept.exists():
            shutil.rmtree(kept)
        shutil.copytree(corpus_dir, kept)
        print(f"[fuzz] corpus copied to {kept}")

    print(
        "[fuzz] comparison: {} ({} missing, {} decision, {} message, {} result diff(s)) -> {}".format(
            "IDENTICAL" if comparison["identical"] else "DIFFERENT",
            len(comparison["missing_files"]),
            len(comparison["decision_diffs"]),
            len(comparison["message_diffs"]),
            len(comparison["result_diffs"]),
            comparison_out,
        )
    )
    print(f"[fuzz] report written to {report_out}")

    if temporary:
        shutil.rmtree(work_dir, ignore_errors=True)

    failing = (
        not comparison["identical"]
        or not environment_stable
        or any(backends[side]["crash"] for side in PYTHON_EXE)
        or any(backends[side]["panics"] for side in PYTHON_EXE)
        or any(backends[side]["timeouts"] for side in PYTHON_EXE)
    )
    return 1 if failing else 0


if __name__ == "__main__":
    raise SystemExit(main())
