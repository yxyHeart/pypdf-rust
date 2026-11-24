#!/usr/bin/env python3
"""Build the frozen P0 baseline assets for validation/optimization/baseline/.

Sources (read-only, never rewritten by this script):

* ``validation/results/perf-reference.json`` / ``perf-rust.json`` -- the recorded
  10-repetition W1-W8 runs (plan S3.1/S3.2) that this baseline freezes.
* ``validation/optimization/baseline/raw/quick/*.json`` -- the repeat=2 harness
  re-confirmation run recorded during the P0 audit.

Outputs:

* ``baseline/environment.txt``   machine/interpreters/versions/load
* ``baseline/git-revision.txt``  commit, worktree state, extension identity
* ``baseline/inputs.sha256``     sha256 of every perf corpus input
* ``baseline/raw/W1.csv``..``W8.csv``  one row per measured repetition
* ``baseline/reports/baseline-report.md``  medians and speedup table
"""
from __future__ import annotations

import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
BASE = ROOT / "validation" / "optimization" / "baseline"
RESULTS = ROOT / "validation" / "results"
RAW = BASE / "raw"
REPORTS = BASE / "reports"


def run(cmd: list[str]) -> str:
    env = dict(os.environ)
    cargo_bin = str(Path.home() / ".cargo" / "bin")
    if cargo_bin not in env.get("PATH", "").split(os.pathsep):
        env["PATH"] = cargo_bin + os.pathsep + env.get("PATH", "")
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, timeout=120, env=env)
        return (out.stdout or "").strip() or (out.stderr or "").strip()
    except Exception as error:  # noqa: BLE001
        return f"<unavailable: {type(error).__name__}: {error}>"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_environment() -> None:
    py_ref = ROOT / ".venvs" / "oracle" / "bin" / "python"
    py_port = ROOT / ".venvs" / "dev" / "bin" / "python"
    lines = [
        "# P0 baseline environment (validation/optimization/baseline/environment.txt)",
        f"captured_at_local={run(['date', '+%Y-%m-%dT%H:%M:%S%z'])}",
        f"captured_at_utc={run(['date', '-u', '+%Y-%m-%dT%H:%M:%SZ'])}",
        "",
        "## host",
        f"hostname={platform.node()}",
        f"uname={platform.uname()}",
        f"platform={platform.platform()}",
        f"machine={platform.machine()}",
        f"macos={run(['sw_vers', '-productVersion'])}",
        f"kernel={run(['sysctl', '-n', 'kern.osrelease'])}",
        f"cpu_brand={run(['sysctl', '-n', 'machdep.cpu.brand_string'])}",
        f"cpu_cores_logical={run(['sysctl', '-n', 'hw.ncpu'])}",
        f"cpu_cores_physical={run(['sysctl', '-n', 'hw.physicalcpu'])}",
        f"mem_bytes={run(['sysctl', '-n', 'hw.memsize'])}",
        f"arch_reported={run(['arch'])}",
        f"loadavg_at_capture={os.getloadavg()}",
        f"uptime={run(['uptime'])}",
        "",
        "## interpreters",
        f"python_port={py_port} :: {run([str(py_port), '-c', 'import sys,platform;print(sys.version.replace(chr(10),\" \"), platform.python_implementation())'])}",
        f"python_reference={py_ref} :: {run([str(py_ref), '-c', 'import sys,platform;print(sys.version.replace(chr(10),\" \"), platform.python_implementation())'])}",
        f"pypdf_reference={run([str(py_ref), '-c', 'import pypdf;print(pypdf.__version__, pypdf.__file__)'])}",
        f"pypdf_port={run([str(py_port), '-c', 'import pypdf;print(pypdf.__version__, pypdf.__file__)'])}",
        f"extension={run([str(py_port), '-c', 'import pypdf._pypdf as m;print(m.__file__)'])}",
        f"extension_sha256={sha256(Path(run([str(py_port), '-c', 'import pypdf._pypdf as m;print(m.__file__)'])))}",
        "",
        "## toolchain (as reported; the P0 stage does not rebuild)",
        f"rustc={run(['rustc', '--version'])}",
        f"cargo={run(['cargo', '--version'])}",
        f"rustup_show={run(['rustup', 'show', 'active-toolchain'])}",
        f"rust_targets={run(['rustup', 'target', 'list', '--installed'])}",
        f"maturin={run([str(py_port), '-c', 'import maturin, sys;print(maturin.__version__)']) if shutil.which('maturin') else run([str(py_port), '-m', 'maturin', '--version'])}",
        f"pyo3_cargo_toml={run(['grep', '-n', 'pyo3', str(ROOT / 'Cargo.toml')])}",
        f"cc={run(['clang', '--version'])}",
        "",
        "## constraints",
        "offline=yes (no network in this environment; cargo/vendor: "
        f"{'vendor/ present' if (ROOT / 'vendor').is_dir() else 'no vendor/ dir'})",
        "measurement_notes=loadavg is high (shared laptop); absolute numbers move by "
        "tens of percent between runs. Compare medians of the recorded 10-repetition "
        "run; the repeat=2 re-check in raw/quick/ only confirms the harness still runs.",
    ]
    (BASE / "environment.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_git_revision() -> None:
    def git(*args: str) -> str:
        return run(["git", "-C", str(ROOT), *args])

    status = git("status", "--porcelain")
    port_py = ROOT / ".venvs" / "dev" / "bin" / "python"
    ext = run([str(port_py), "-c", "import pypdf._pypdf as m;print(m.__file__)"])
    ref_py = ROOT / ".venvs" / "oracle" / "bin" / "python"
    lines = [
        "# P0 baseline revision record",
        f"repo={ROOT}",
        f"head={git('rev-parse', 'HEAD')}",
        f"branch={git('rev-parse', '--abbrev-ref', 'HEAD')}",
        f"describe={git('describe', '--tags', '--always', '--dirty')}",
        f"head_commit_date={git('show', '-s', '--format=%cI', 'HEAD')}",
        f"commit_subject={git('show', '-s', '--format=%s', 'HEAD')}",
        f"worktree_clean={'yes' if not status else 'no'}",
        f"worktree_status_porcelain={status or '<clean>'}",
        f"untracked_plan_file={'present' if (ROOT / 'pypdf-rust-optimization-plan.md').exists() else 'absent'}",
        "",
        "## implementation versions",
        f"cargo_workspace_version={run(['grep', '-n', 'version = ', str(ROOT / 'Cargo.toml')])}",
        f"python_project_version={run(['grep', '-n', '^version', str(ROOT / 'pyproject.toml')])}",
        f"pypdf_tracked_version={run([str(port_py), '-c', 'import pypdf._pypdf as m;print(m.__version__)'])}",
        f"pypdf_reference_version={run([str(ref_py), '-c', 'import pypdf;print(pypdf.__version__)'])}",
        f"extension_path={ext}",
        f"extension_sha256={sha256(Path(ext))}" if ext and Path(ext).exists() else "extension_sha256=<missing>",
        "",
        "## recorded perf artifacts used by this baseline",
        *[
            f"{path.relative_to(ROOT)} sha256={sha256(path)}"
            for path in sorted(RESULTS.glob("perf-*.json"))
        ],
    ]
    (BASE / "git-revision.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")


def workload_files() -> list[Path]:
    doc = json.loads((ROOT / "validation" / "perf" / "workloads.json").read_text())
    seen: list[Path] = []
    for workload in doc["workloads"]:
        for rel in workload["files"]:
            path = ROOT / rel
            if path not in seen:
                seen.append(path)
    return seen


def write_inputs() -> None:
    lines = ["# sha256 of every W1-W8 perf input (validation/perf/workloads.json)"]
    for path in workload_files():
        if not path.exists():
            lines.append(f"<missing>  {path.relative_to(ROOT)}")
            continue
        lines.append(f"{sha256(path)}  {path.relative_to(ROOT)}")
    (BASE / "inputs.sha256").write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_csvs() -> dict:
    ref = json.loads((RESULTS / "perf-reference.json").read_text())
    rust = json.loads((RESULTS / "perf-rust.json").read_text())
    per_workload = {}
    for side, document in (("reference", ref), ("rust", rust)):
        for workload in document["workloads"]:
            wid = workload["id"]
            rows = per_workload.setdefault(wid, [])
            for index, rep in enumerate(workload["repetitions"], start=1):
                rows.append(
                    {
                        "backend": side,
                        "workload": wid,
                        "workload_name": workload["name"],
                        "rep": index,
                        "wall_s": rep.get("wall_s"),
                        "cpu_s": rep.get("cpu_s"),
                        "peak_rss_bytes": rep.get("peak_rss_bytes"),
                        "output_bytes": rep.get("output_bytes"),
                        "output_sha256": rep.get("output_sha256"),
                        "success": rep.get("success"),
                        "timeout": rep.get("timeout"),
                        "error": (rep.get("error") or "").replace("\n", " ")[:200],
                    }
                )
    header = [
        "backend",
        "workload",
        "workload_name",
        "rep",
        "wall_s",
        "cpu_s",
        "peak_rss_bytes",
        "output_bytes",
        "output_sha256",
        "success",
        "timeout",
        "error",
    ]
    summary = {}
    for wid, rows in sorted(per_workload.items()):
        rows.sort(key=lambda row: (row["backend"], row["rep"]))
        out = RAW / f"{wid}.csv"
        with open(out, "w", encoding="utf-8") as handle:
            handle.write(",".join(header) + "\n")
            for row in rows:
                handle.write(",".join(str(row[key]) for key in header) + "\n")
        summary[wid] = rows
    return summary


def write_report(summary: dict) -> None:
    import statistics

    ref = json.loads((RESULTS / "perf-reference.json").read_text())
    rust = json.loads((RESULTS / "perf-rust.json").read_text())
    ref_map = {w["id"]: w for w in ref["workloads"]}
    rust_map = {w["id"]: w for w in rust["workloads"]}
    lines = [
        "# P0 baseline report (frozen from validation/results/perf-{reference,rust}.json)",
        "",
        "- recorded runs: reference `perf-reference.json` (generated "
        f"`{ref.get('generated_at_local')}`), port `perf-rust.json` (generated "
        f"`{rust.get('generated_at_local')}`)",
        f"- repetitions per workload: {ref['config']['repeat']} counted + "
        f"{ref['config']['warmup']} warm-up, one process per backend",
        "- raw per-repetition rows: `baseline/raw/W1.csv` ... `W8.csv`",
        "- this file is generated by `validation/optimization/scripts/make_baseline.py`",
        "",
        "| W | workload | ref median (ms) | port median (ms) | speedup | latency change | ref p95 (ms) | port p95 (ms) | output bytes | output equal |",
        "|---|---|---:|---:|---:|---:|---:|---:|---:|---|",
    ]
    for wid in sorted(ref_map):
        a, b = ref_map[wid]["summary"], rust_map[wid]["summary"]
        rm = a["wall"]["median"] * 1000
        pm = b["wall"]["median"] * 1000
        lines.append(
            f"| {wid} | {ref_map[wid]['name']} | {rm:.2f} | {pm:.2f} | "
            f"{rm / pm:.3f}× | {(pm / rm - 1) * 100:+.1f}% | "
            f"{a['wall']['p95'] * 1000:.2f} | {b['wall']['p95'] * 1000:.2f} | "
            f"{a['output_bytes']} | "
            f"{'yes' if a['output_sha256_values'][0] == b['output_sha256_values'][0] else 'NO'} |"
        )
    lines += [
        "",
        "## harness re-confirmation (this audit, `--repeat 2`)",
        "",
        "Recorded in `baseline/raw/quick/`. It exists only to prove the harness still runs",
        "and reproduces the direction of the recorded run; it is not a new baseline.",
        "",
        "| W | ref median (ms) | port median (ms) | speedup |",
        "|---|---:|---:|---:|",
    ]
    quick_ref = json.loads((BASE / "raw" / "quick" / "perf-reference-repeat2.json").read_text())
    quick_rust = json.loads((BASE / "raw" / "quick" / "perf-rust-repeat2.json").read_text())
    qr = {w["id"]: w for w in quick_ref["workloads"]}
    qp = {w["id"]: w for w in quick_rust["workloads"]}
    for wid in sorted(qr):
        rm = qr[wid]["summary"]["wall"]["median"] * 1000
        pm = qp[wid]["summary"]["wall"]["median"] * 1000
        lines.append(f"| {wid} | {rm:.2f} | {pm:.2f} | {rm / pm:.3f}× |")
    lines += [
        "",
        "## caveat",
        "",
        "The host is a shared laptop (`loadavg` 8-30 during these runs). Absolute",
        "numbers vary by tens of percent between runs; only the direction and the",
        "large ratios are stable. Any optimisation claim must re-measure both sides",
        "back-to-back, ideally with the ABBA ordering required by plan S3.3.",
    ]
    (REPORTS / "baseline-report.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    RAW.mkdir(parents=True, exist_ok=True)
    REPORTS.mkdir(parents=True, exist_ok=True)
    write_environment()
    write_git_revision()
    write_inputs()
    summary = write_csvs()
    write_report(summary)
    print(f"[make_baseline] wrote environment.txt, git-revision.txt, inputs.sha256, "
          f"{len(summary)} CSVs, reports/baseline-report.md under {BASE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
