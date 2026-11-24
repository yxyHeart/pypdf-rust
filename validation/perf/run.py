#!/usr/bin/env python3
"""§10.2–§10.4: measure ONE pypdf backend (reference or port) on the W1–W8 workloads.

The venv of the interpreter running this script decides which implementation is
measured; ``--backend`` is only a label guard so that results cannot be written
to the wrong file by accident.

For every workload this script runs one warm-up (not counted) and then
``--repeat`` counted repetitions in this single process.  Each repetition
records:

* wall time            -- ``time.perf_counter``
* CPU time             -- ``time.process_time`` (whole process, all threads)
* peak RSS             -- sampled resident-set size (macOS ``proc_pid_rusage`` /
                          Linux ``/proc/self/statm``), falling back to
                          ``resource.getrusage().ru_maxrss``
* output bytes         -- the produced PDF / extracted text / extracted image
                          data, never a hash-only stand-in
* success / timeout    -- failures are recorded and counted, never dropped

The JSON written here is consumed by ``compare.py``.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import platform
import statistics
import sys
import threading
import time
import traceback
from pathlib import Path
from typing import Any

PERF_DIR = Path(__file__).resolve().parent
ROOT = PERF_DIR.parents[1]
DEFAULT_WORKLOADS = PERF_DIR / "workloads.json"
RESULTS_DIR = ROOT / "validation" / "results"

BENCH_SCHEMA = "perf-run-v1"


# ---------------------------------------------------------------------------
# process / environment helpers
# ---------------------------------------------------------------------------
def ensure_hashseed() -> None:
    """PYTHONHASHSEED must be 0 on both sides; re-exec once if it is not."""
    if os.environ.get("PYTHONHASHSEED") != "0":
        env = dict(os.environ)
        env["PYTHONHASHSEED"] = "0"
        print(
            f"[run] re-exec with PYTHONHASHSEED=0 "
            f"(was {os.environ.get('PYTHONHASHSEED')!r})",
            file=sys.stderr,
            flush=True,
        )
        os.execve(sys.executable, [sys.executable, *sys.argv], env)


def detect_backend() -> tuple[str, str | None]:
    """Return ('rust'|'reference', extension_path). Mirrors validation/env_info.py."""
    import pypdf

    try:
        import pypdf._pypdf as _ext

        return "rust", getattr(_ext, "__file__", None)
    except ImportError:
        return "reference", None


def file_identity(path: str | None) -> dict | None:
    """sha256/mtime/size of the compiled extension, so a mid-run rebuild is visible."""
    if not path:
        return None
    candidate = Path(path)
    if not candidate.is_file():
        return {"path": path, "exists": False}
    digest = hashlib.sha256()
    with candidate.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    stat = candidate.stat()
    return {
        "path": str(candidate),
        "sha256": digest.hexdigest(),
        "bytes": stat.st_size,
        "mtime_local": time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(stat.st_mtime)),
    }


def ru_maxrss_bytes() -> int:
    import resource

    value = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    # macOS reports bytes, Linux reports KiB.
    return int(value) if sys.platform == "darwin" else int(value) * 1024


def _darwin_rss_reader():
    try:
        import ctypes
        import ctypes.util

        path = ctypes.util.find_library("proc")
        if not path:
            return None
        lib = ctypes.CDLL(path, use_errno=True)
    except Exception:  # noqa: BLE001 - sampling is best effort
        return None

    # struct rusage_info_v0 from <sys/resource.h>: 16-byte uuid + 10 uint64.
    class RUsageInfoV0(ctypes.Structure):
        _fields_ = [
            ("ri_uuid", ctypes.c_uint8 * 16),
            ("ri_user_time", ctypes.c_uint64),
            ("ri_system_time", ctypes.c_uint64),
            ("ri_pkg_idle_wkups", ctypes.c_uint64),
            ("ri_interrupt_wkups", ctypes.c_uint64),
            ("ri_pageins", ctypes.c_uint64),
            ("ri_wired_size", ctypes.c_uint64),
            ("ri_resident_size", ctypes.c_uint64),
            ("ri_phys_footprint", ctypes.c_uint64),
            ("ri_proc_start_abstime", ctypes.c_uint64),
            ("ri_proc_exit_abstime", ctypes.c_uint64),
        ]

    func = lib.proc_pid_rusage
    func.restype = ctypes.c_int
    func.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    info = RUsageInfoV0()
    pid = os.getpid()

    def read() -> int | None:
        if func(pid, 0, ctypes.byref(info)) == 0:  # RUSAGE_INFO_V0
            return int(info.ri_resident_size) or None
        return None

    return read


def _linux_rss_reader():
    path = "/proc/self/statm"
    page_size = os.sysconf("SC_PAGE_SIZE")

    def read() -> int | None:
        try:
            with open(path, "r", encoding="ascii") as handle:
                fields = handle.readline().split()
            return int(fields[1]) * page_size  # resident pages
        except Exception:  # noqa: BLE001
            return None

    return read


def make_rss_reader():
    if sys.platform == "darwin":
        return _darwin_rss_reader()
    if sys.platform.startswith("linux"):
        return _linux_rss_reader()
    return None


class PeakRSSSampler:
    """Sample the current resident set while a repetition runs and keep the peak."""

    interval_s = 0.0015

    def __init__(self) -> None:
        self._read = make_rss_reader()
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self.peak: int | None = None
        self.start_value: int | None = None

    @property
    def available(self) -> bool:
        return self._read is not None

    def start(self) -> None:
        if self._read is None:
            return
        self._stop.clear()
        self.start_value = self._read() or 0
        self.peak = self.start_value
        self._thread = threading.Thread(target=self._run, name="peak-rss", daemon=True)
        self._thread.start()

    def _run(self) -> None:
        read = self._read
        while not self._stop.is_set():
            try:
                value = read()
            except Exception:  # noqa: BLE001 - never let sampling kill the run
                value = None
            if value and value > (self.peak or 0):
                self.peak = value
            self._stop.wait(self.interval_s)

    def stop(self) -> int | None:
        if self._thread is not None:
            self._stop.set()
            self._thread.join(timeout=5.0)
            self._thread = None
        return self.peak


class TimeoutGuard:
    """Interrupt a repetition after ``seconds`` using SIGALRM (Unix only)."""

    def __init__(self, seconds: float | None) -> None:
        self.seconds = seconds
        self._old_handler = None
        self._armed = False

    def __enter__(self) -> "TimeoutGuard":
        if self.seconds and hasattr(__import__("signal"), "setitimer"):
            import signal

            self._old_handler = signal.signal(signal.SIGALRM, self._raise)
            signal.setitimer(signal.ITIMER_REAL, float(self.seconds))
            self._armed = True
        return self

    def _raise(self, signum, frame) -> None:  # noqa: ANN001
        raise TimeoutError(f"repetition exceeded timeout of {self.seconds:g}s")

    def __exit__(self, exc_type, exc, tb) -> bool:
        if self._armed:
            import signal

            signal.setitimer(signal.ITIMER_REAL, 0)
            if self._old_handler is not None:
                signal.signal(signal.SIGALRM, self._old_handler)
            self._armed = False
        return False


# ---------------------------------------------------------------------------
# statistics
# ---------------------------------------------------------------------------
def percentile(values: list[float], q: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    position = (len(ordered) - 1) * q
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    if lower == upper:
        return ordered[lower]
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def describe(values: list[float], digits: int = 6) -> dict[str, Any]:
    if not values:
        return {"count": 0, "median": None, "p95": None, "min": None, "max": None, "mean": None}
    rounded = lambda v: None if v is None else round(float(v), digits)  # noqa: E731
    return {
        "count": len(values),
        "median": rounded(statistics.median(values)),
        "p95": rounded(percentile(values, 0.95)),
        "min": rounded(min(values)),
        "max": rounded(max(values)),
        "mean": rounded(statistics.fmean(values)),
    }


# ---------------------------------------------------------------------------
# workload operations
# ---------------------------------------------------------------------------
class OpResult:
    """Small JSON summary plus the raw output material of one repetition."""

    __slots__ = ("payload", "raw_output", "per_file")

    def __init__(self, payload: dict, raw_output: Any, per_file: list[dict] | None = None) -> None:
        self.payload = payload
        self.raw_output = raw_output
        self.per_file = per_file or []


def _reader(path: Path):
    from pypdf import PdfReader

    return PdfReader(str(path))


def op_reader_init(files: list[Path], params: dict, per_file: bool) -> OpResult:
    fields = params.get("metadata_fields") or ["/Title"]
    rows = []
    timings = []
    for path in files:
        marker = time.perf_counter() if per_file else 0.0
        reader = _reader(path)
        num_pages = len(reader.pages)
        metadata = reader.metadata or {}
        values = []
        for name in fields:
            value = metadata.get(name)
            values.append(None if value is None else str(value))
        elapsed = time.perf_counter() - marker if per_file else 0.0
        rows.append([str(path.relative_to(ROOT)), num_pages, values])
        if per_file:
            timings.append({"path": str(path.relative_to(ROOT)), "seconds": round(elapsed, 6)})
    payload = {"workload": "reader_init", "files": rows}
    return OpResult(payload, None, timings)


def op_page_traverse(files: list[Path], params: dict, per_file: bool) -> OpResult:
    box_names = params.get("boxes") or ["mediabox", "cropbox", "trimbox"]
    max_pages = int(params.get("max_pages") or 0)
    include_rotation = bool(params.get("include_rotation", True))
    rows = []
    timings = []
    for path in files:
        marker = time.perf_counter() if per_file else 0.0
        reader = _reader(path)
        pages = list(reader.pages)
        if max_pages:
            pages = pages[:max_pages]
        page_rows = []
        for page in pages:
            boxes = []
            for name in box_names:
                box = getattr(page, name)
                boxes.append([round(float(value), 3) for value in box])
            rotation = int(page.rotation) if include_rotation else None
            page_rows.append([rotation, boxes])
        elapsed = time.perf_counter() - marker if per_file else 0.0
        rows.append([str(path.relative_to(ROOT)), len(page_rows), page_rows])
        if per_file:
            timings.append({"path": str(path.relative_to(ROOT)), "seconds": round(elapsed, 6)})
    payload = {"workload": "page_traverse", "box_names": box_names, "include_rotation": include_rotation, "files": rows}
    return OpResult(payload, None, timings)


def op_text_extract(files: list[Path], params: dict, per_file: bool) -> OpResult:
    max_pages = int(params.get("max_pages") or 0)
    rows = []
    texts: list[str] = []
    timings = []
    for path in files:
        marker = time.perf_counter() if per_file else 0.0
        reader = _reader(path)
        pages = list(reader.pages)
        if max_pages:
            pages = pages[:max_pages]
        chunks = [page.extract_text() or "" for page in pages]
        text = "".join(chunks)
        elapsed = time.perf_counter() - marker if per_file else 0.0
        texts.append(text)
        rows.append([str(path.relative_to(ROOT)), len(pages), len(text)])
        if per_file:
            timings.append({"path": str(path.relative_to(ROOT)), "seconds": round(elapsed, 6)})
    payload = {"workload": "text_extract", "files": rows}
    return OpResult(payload, texts, timings)


def op_image_extract(files: list[Path], params: dict, per_file: bool) -> OpResult:
    max_pages = int(params.get("max_pages") or 0)
    decode = bool(params.get("decode_with_pillow", True))
    rows = []
    chunks: list[bytes] = []
    timings = []
    for path in files:
        marker = time.perf_counter() if per_file else 0.0
        reader = _reader(path)
        pages = list(reader.pages)
        if max_pages:
            pages = pages[:max_pages]
        image_rows = []
        for page in pages:
            for image in page.images:
                data = image.data
                size = mode = None
                if decode:
                    pil = image.image
                    if pil is not None:
                        size = [int(pil.size[0]), int(pil.size[1])]
                        mode = getattr(pil, "mode", None)
                image_rows.append([image.name, len(data), size, mode])
                chunks.append(data)
        elapsed = time.perf_counter() - marker if per_file else 0.0
        rows.append([str(path.relative_to(ROOT)), len(image_rows), image_rows])
        if per_file:
            timings.append({"path": str(path.relative_to(ROOT)), "seconds": round(elapsed, 6)})
    payload = {"workload": "image_extract", "files": rows}
    return OpResult(payload, chunks, timings)


def _write_io(writer) -> bytes:
    buffer = io.BytesIO()
    writer.write(buffer)
    return buffer.getvalue()


def op_merge_append(files: list[Path], params: dict, per_file: bool) -> OpResult:
    from pypdf import PdfWriter

    writer = PdfWriter()
    for path in files:
        writer.append(str(path))
    data = _write_io(writer)
    payload = {
        "workload": "merge_append",
        "input_files": [str(path.relative_to(ROOT)) for path in files],
        "pages": len(writer.pages),
        "pdf_bytes": len(data),
    }
    return OpResult(payload, data)


def op_page_ops_write(files: list[Path], params: dict, per_file: bool) -> OpResult:
    from pypdf import PdfWriter

    cycle = params.get("rotate_cycle") or [90, 180, 270]
    lower_left = tuple(params.get("crop_lower_left") or [10.0, 10.0])
    upper_right = tuple(params.get("crop_upper_right") or [500.0, 700.0])
    writer = PdfWriter()
    index = 0
    for path in files:
        reader = _reader(path)
        for page in reader.pages:
            new_page = writer.add_page(page)
            rotation = int(cycle[index % len(cycle)])
            if rotation:
                new_page.rotate(rotation)
            new_page.cropbox.lower_left = lower_left
            new_page.cropbox.upper_right = upper_right
            index += 1
    data = _write_io(writer)
    payload = {
        "workload": "page_ops_write",
        "input_files": [str(path.relative_to(ROOT)) for path in files],
        "pages": index,
        "pdf_bytes": len(data),
    }
    return OpResult(payload, data)


def op_clone_write(files: list[Path], params: dict, per_file: bool) -> OpResult:
    from pypdf import PdfWriter

    writer = PdfWriter()
    pages = 0
    for path in files:
        reader = _reader(path)
        for page in reader.pages:
            writer.add_page(page)
            pages += 1
    data = _write_io(writer)
    payload = {
        "workload": "clone_write",
        "input_files": [str(path.relative_to(ROOT)) for path in files],
        "pages": pages,
        "pdf_bytes": len(data),
    }
    return OpResult(payload, data)


def op_end_to_end(files: list[Path], params: dict, per_file: bool) -> OpResult:
    from pypdf import PdfReader, PdfWriter

    text_pages = int(params.get("text_pages") or 0)
    writer = PdfWriter()
    rows = []
    timings = []
    for path in files:
        marker = time.perf_counter() if per_file else 0.0
        reader = _reader(path)
        pages = list(reader.pages)
        selected = pages[:text_pages] if text_pages else pages
        text_length = sum(len(page.extract_text() or "") for page in selected)
        writer.append(reader)
        elapsed = time.perf_counter() - marker if per_file else 0.0
        rows.append([str(path.relative_to(ROOT)), len(pages), text_length])
        if per_file:
            timings.append({"path": str(path.relative_to(ROOT)), "seconds": round(elapsed, 6)})
    data = _write_io(writer)
    reopened = PdfReader(io.BytesIO(data))
    payload = {
        "workload": "end_to_end",
        "files": rows,
        "pages": len(writer.pages),
        "out_pages": len(reopened.pages),
        "pdf_bytes": len(data),
    }
    return OpResult(payload, data, timings)


OPS = {
    "reader_init": op_reader_init,
    "page_traverse": op_page_traverse,
    "text_extract": op_text_extract,
    "image_extract": op_image_extract,
    "merge_append": op_merge_append,
    "page_ops_write": op_page_ops_write,
    "clone_write": op_clone_write,
    "end_to_end": op_end_to_end,
}


# ---------------------------------------------------------------------------
# output finalization
# ---------------------------------------------------------------------------
def finalize_output(kind: str, payload: dict, raw_output: Any) -> tuple[int, str]:
    """Return (output_bytes, output_sha256) for one repetition."""
    if kind == "derived_summary":
        data = json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
        return len(data), hashlib.sha256(data).hexdigest()
    if kind == "extracted_text":
        digest = hashlib.sha256()
        total = 0
        for text in raw_output or []:
            encoded = text.encode("utf-8")
            total += len(encoded)
            digest.update(encoded)
        return total, digest.hexdigest()
    if kind == "image_bytes":
        digest = hashlib.sha256()
        total = 0
        for chunk in raw_output or []:
            total += len(chunk)
            digest.update(chunk)
        return total, digest.hexdigest()
    if kind == "pdf_bytes":
        data = raw_output or b""
        return len(data), hashlib.sha256(data).hexdigest()
    raise ValueError(f"unknown output_kind {kind!r}")


# ---------------------------------------------------------------------------
# repetition / workload runner
# ---------------------------------------------------------------------------
def run_repetition(workload: dict, root: Path, timeout_s: float | None) -> dict:
    from prng import install_deterministic_secrets

    files = [root / rel for rel in workload["files"]]
    op = OPS[workload["type"]]
    per_file = bool(workload.get("per_file_timing"))
    params = workload.get("params") or {}

    proc_before = ru_maxrss_bytes()
    sampler = PeakRSSSampler()
    install_deterministic_secrets()
    cpu0 = time.process_time()
    sampler.start()
    t0 = time.perf_counter()
    try:
        with TimeoutGuard(timeout_s):
            result = op(files, params, per_file)
        wall = time.perf_counter() - t0
        cpu = time.process_time() - cpu0
        peak_sampled = sampler.stop()
    except BaseException as exc:  # noqa: BLE001 - record, never abort the run
        wall = time.perf_counter() - t0
        cpu = time.process_time() - cpu0
        sampler.stop()
        record = {
            "success": False,
            "timeout": isinstance(exc, TimeoutError),
            "wall_s": round(wall, 6),
            "cpu_s": round(cpu, 6),
            "peak_rss_bytes": None,
            "rss_start_bytes": sampler.start_value,
            "proc_maxrss_bytes": ru_maxrss_bytes(),
            "output_bytes": None,
            "output_sha256": None,
            "per_file": [],
            "error": {
                "type": type(exc).__name__,
                "message": str(exc),
                "traceback_tail": traceback.format_exc(limit=4).splitlines()[-4:],
            },
        }
        return record

    proc_after = ru_maxrss_bytes()
    output_bytes, output_sha = finalize_output(workload["output_kind"], result.payload, result.raw_output)
    record = {
        "success": True,
        "timeout": False,
        "wall_s": round(wall, 6),
        "cpu_s": round(cpu, 6),
        "peak_rss_bytes": peak_sampled if peak_sampled else proc_after,
        "rss_start_bytes": sampler.start_value,
        "rss_source": "sampled" if peak_sampled else "ru_maxrss",
        "proc_maxrss_bytes": proc_after,
        "proc_maxrss_before_bytes": proc_before,
        "output_bytes": output_bytes,
        "output_sha256": output_sha,
        "per_file": result.per_file,
        "summary": result.payload,
        "error": None,
    }
    return record


def analyze(reps: list[dict]) -> dict:
    ok = [rep for rep in reps if rep["success"]]
    wall = [rep["wall_s"] for rep in ok]
    cpu = [rep["cpu_s"] for rep in ok]
    rss = [rep["peak_rss_bytes"] for rep in ok if rep["peak_rss_bytes"]]
    rss_start = [rep["rss_start_bytes"] for rep in ok if rep.get("rss_start_bytes")]
    rss_growth = [
        rep["peak_rss_bytes"] - rep["rss_start_bytes"]
        for rep in ok
        if rep.get("peak_rss_bytes") and rep.get("rss_start_bytes")
    ]
    outputs = sorted({rep["output_bytes"] for rep in ok if rep["output_bytes"] is not None})
    per_file_medians: dict[str, float | None] = {}
    per_file_counts: dict[str, int] = {}
    for rep in ok:
        for entry in rep.get("per_file") or []:
            per_file_counts[entry["path"]] = per_file_counts.get(entry["path"], 0) + 1
    for rep in ok:
        for entry in rep.get("per_file") or []:
            per_file_medians.setdefault(entry["path"], None)
    for path in per_file_medians:
        samples = [
            entry["seconds"]
            for rep in ok
            for entry in (rep.get("per_file") or [])
            if entry["path"] == path
        ]
        per_file_medians[path] = round(statistics.median(samples), 6) if samples else None

    return {
        "n_total": len(reps),
        "n_success": len(ok),
        "n_failure": len(reps) - len(ok),
        "n_timeout": sum(1 for rep in reps if rep.get("timeout")),
        "wall": describe(wall),
        "cpu": describe(cpu),
        "peak_rss_bytes": {
            "max": max(rss) if rss else None,
            "median": round(statistics.median(rss)) if rss else None,
        },
        "rss_start_bytes": {
            "max": max(rss_start) if rss_start else None,
            "median": round(statistics.median(rss_start)) if rss_start else None,
        },
        "rss_growth_bytes": {
            "max": max(rss_growth) if rss_growth else None,
            "median": round(statistics.median(rss_growth)) if rss_growth else None,
        },
        "output_bytes_values": outputs,
        "output_bytes": outputs[0] if len(outputs) == 1 else None,
        "output_sha256_values": sorted({rep["output_sha256"] for rep in ok if rep["output_sha256"]}),
        "per_file_medians_s": per_file_medians,
        "per_file_samples": per_file_counts,
        "failure_indices": [index for index, rep in enumerate(reps) if not rep["success"]],
        "failures": [
            {"index": index, "timeout": rep.get("timeout"), "error": rep.get("error")}
            for index, rep in enumerate(reps)
            if not rep["success"]
        ],
    }


def run_workload(workload: dict, root: Path, repeat: int, warmup: int, timeout_s: float | None) -> dict:
    wid = workload["id"]
    file_info = []
    for rel in workload["files"]:
        path = root / rel
        if not path.is_file():
            raise FileNotFoundError(f"{wid}: missing corpus file {path}")
        file_info.append({"path": rel, "bytes": path.stat().st_size})

    warmups = []
    for index in range(warmup):
        record = run_repetition(workload, root, timeout_s)
        record["index"] = index
        record.pop("summary", None)
        warmups.append(record)
        status = "ok" if record["success"] else f"FAILED ({record['error']['type']})"
        print(f"  {wid} warm-up {index + 1}/{warmup}: {record['wall_s']:.4f}s {status}", flush=True)

    reps = []
    for index in range(repeat):
        record = run_repetition(workload, root, timeout_s)
        record["index"] = index
        reps.append(record)
        if record["success"]:
            print(
                f"  {wid} rep {index + 1}/{repeat}: wall={record['wall_s']:.4f}s "
                f"cpu={record['cpu_s']:.4f}s rss={record['peak_rss_bytes'] / 1e6:.1f}MB "
                f"out={record['output_bytes']}B",
                flush=True,
            )
        else:
            print(
                f"  {wid} rep {index + 1}/{repeat}: FAILED {record['error']['type']}: "
                f"{record['error']['message'][:160]}",
                flush=True,
            )

    sample_summary = None
    for record in reps:
        if sample_summary is None and record["success"]:
            sample_summary = record["summary"]
        record.pop("summary", None)
    analysis = analyze(reps)
    return {
        "id": wid,
        "name": workload.get("name", wid),
        "description": workload.get("description", ""),
        "type": workload["type"],
        "size_class": workload.get("size_class", "unknown"),
        "output_kind": workload["output_kind"],
        "files": file_info,
        "params": workload.get("params") or {},
        "expected_output_bytes": workload.get("expected_output_bytes"),
        "warmup": warmups,
        "repetitions": reps,
        "sample_summary": sample_summary,
        "summary": analysis,
    }


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------
def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--backend", choices=["reference", "rust"], default=None,
                        help="expected backend label (informational; the venv decides)")
    parser.add_argument("--repeat", type=int, default=7, help="counted repetitions per workload (default: 7)")
    parser.add_argument("--warmup", type=int, default=1, help="warm-up repetitions, not counted (default: 1)")
    parser.add_argument("--timeout", type=float, default=300.0,
                        help="per-repetition timeout in seconds, 0 disables (default: 300)")
    parser.add_argument("--workloads", type=Path, default=DEFAULT_WORKLOADS)
    parser.add_argument("--out", type=Path, default=None)
    parser.add_argument("--only", default=None, help="comma separated workload ids, e.g. W1,W3")
    parser.add_argument("--list", action="store_true", help="list workloads and exit")
    return parser


def main(argv: list[str] | None = None) -> int:
    ensure_hashseed()
    args = build_parser().parse_args(argv)
    if args.repeat < 1:
        print("[run] --repeat must be >= 1", file=sys.stderr)
        return 2
    if args.repeat < 5:
        print(f"[run] warning: --repeat {args.repeat} < plan minimum 5", file=sys.stderr, flush=True)
    timeout_s = args.timeout if args.timeout and args.timeout > 0 else None

    sys.path.insert(0, str(ROOT / "tools" / "oracle"))
    try:
        import prng  # noqa: F401 - fail early if the deterministic PRNG is missing
    except ImportError as exc:
        print(f"[run] cannot import tools/oracle/prng.py: {exc}", file=sys.stderr)
        return 2

    import pypdf

    detected, extension = detect_backend()
    if args.backend and args.backend != detected:
        print(
            f"[run] backend mismatch: --backend {args.backend} but this interpreter is the "
            f"{detected} implementation ({pypdf.__file__}). Refusing to write mislabeled results.",
            file=sys.stderr,
        )
        return 2
    backend = args.backend or detected

    definitions = json.loads(args.workloads.read_text(encoding="utf-8"))
    workloads = definitions["workloads"]
    if args.only:
        wanted = {item.strip() for item in args.only.split(",") if item.strip()}
        unknown = wanted - {workload["id"] for workload in workloads}
        if unknown:
            print(f"[run] unknown workload ids: {sorted(unknown)}", file=sys.stderr)
            return 2
        workloads = [workload for workload in workloads if workload["id"] in wanted]
    if args.list:
        for workload in workloads:
            print(f"{workload['id']} {workload.get('name', ''):32s} {len(workload['files']):2d} files  {workload['description']}")
        return 0

    out_path = args.out or (RESULTS_DIR / f"perf-{backend}.json")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    loadavg_start = list(os.getloadavg()) if hasattr(os, "getloadavg") else None
    extension_identity_start = file_identity(extension)

    per_file_types = {"reader_init", "page_traverse", "text_extract", "image_extract", "end_to_end"}

    run_workloads = []
    for workload in workloads:
        workload = dict(workload)
        workload["per_file_timing"] = workload["type"] in per_file_types
        print(f"[run] {workload['id']} {workload.get('name', '')} ({len(workload['files'])} files, "
              f"{workload['type']}, output={workload['output_kind']})", flush=True)
        record_loadavg = list(os.getloadavg()) if hasattr(os, "getloadavg") else None
        started = time.perf_counter()
        record = run_workload(workload, ROOT, args.repeat, args.warmup, timeout_s)
        record["elapsed_s"] = round(time.perf_counter() - started, 3)
        record["loadavg_before"] = record_loadavg
        run_workloads.append(record)
        summary = record["summary"]["wall"]
        print(
            f"[run] {workload['id']} done: median={summary['median']}s p95={summary['p95']}s "
            f"ok={record['summary']['n_success']}/{record['summary']['n_total']}",
            flush=True,
        )

    extension_identity_end = file_identity(extension)
    extension_changed = bool(
        extension_identity_start
        and extension_identity_end
        and extension_identity_start.get("sha256") != extension_identity_end.get("sha256")
    )
    if extension_changed:
        print(
            "[run] WARNING: the compiled extension changed on disk during this run "
            "(mixed-build measurements); the wrapper will re-run this backend once.",
            file=sys.stderr,
            flush=True,
        )

    document = {
        "schema": BENCH_SCHEMA,
        "plan_section": "§10.2-§10.4",
        "backend": backend,
        "backend_detected": detected,
        "generated_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "generated_at_local": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "environment": {
            "python": sys.version.split()[0],
            "executable": sys.executable,
            "prefix": sys.prefix,
            "platform": platform.platform(),
            "machine": platform.machine(),
            "pypdf_version": pypdf.__version__,
            "module": pypdf.__file__,
            "rust_extension": extension,
            "rust_extension_identity": extension_identity_start,
            "rust_extension_identity_end": extension_identity_end,
            "extension_changed_during_run": extension_changed,
            "cpu_count": os.cpu_count(),
            "loadavg_start": loadavg_start,
            "loadavg_end": list(os.getloadavg()) if hasattr(os, "getloadavg") else None,
            "hashseed": os.environ.get("PYTHONHASHSEED"),
        },
        "invocation": {"argv": sys.argv, "cwd": os.getcwd()},
        "config": {
            "repeat": args.repeat,
            "warmup": args.warmup,
            "timeout_s": timeout_s,
            "workloads_path": str(args.workloads),
            "large_file_threshold_bytes": definitions.get("large_file_threshold_bytes", 250000),
            "deterministic_secrets": True,
            "memory_metric": "sampled current RSS peak; falls back to ru_maxrss",
        },
        "workloads": run_workloads,
    }
    out_path.write_text(json.dumps(document, indent=1, ensure_ascii=False, sort_keys=False) + "\n", encoding="utf-8")
    print(f"[run] wrote {out_path}", flush=True)
    failures = sum(item["summary"]["n_failure"] for item in run_workloads)
    if failures:
        print(f"[run] warning: {failures} failed repetition(s) recorded in {out_path.name}", file=sys.stderr, flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
