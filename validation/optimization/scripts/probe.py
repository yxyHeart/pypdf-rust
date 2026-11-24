#!/usr/bin/env python3
"""P0 instrumentation probes for the pypdf-rust optimization plan (OPT-000/OPT-010).

Read-only: this script never modifies library code, tests or inputs.  It only
monkeypatches *runtime* objects to count calls and to split wall time into
stages, and writes JSON under ``validation/optimization/baseline/raw/``.

Modes
-----
``stages``      W1-W8 per-file / per-stage wall-time split (plan S3.1, S3.4).
``w3``          W3 (text extraction) stage split + token/operation counters.
``w4``          W4 (image extraction) per-file decode split.
``w57``         W5/W6/W7 clone/object-graph vs serialization split.
``issue301``    resources/issue-301.pdf call counters (read_object /
                _check_limits) with recursion accounting.

Every mode runs on whichever implementation the interpreter imported, so the
same command under ``.venvs/dev`` (port) and ``.venvs/oracle`` (reference)
produces directly comparable numbers.

Instrumentation caveats (reported in the JSON as ``notes``):

* ``read_object`` and ``ContentStream._check_limits`` counters work by replacing
  the *module/class* attribute with a Python wrapper.  The port detects that
  patched attribute (``is_default_shim_function`` identity check) and therefore
  falls back to the Python entry point for those tokens.  The counts are exact
  logical call counts, but the instrumented wall time is an upper bound.
* PyO3 calls made from Rust to Rust (``call_method1`` fast paths) are invisible
  from Python; they cannot be counted without editing the extension, so the
  boundary-call numbers here are Python-visible calls only.
"""
from __future__ import annotations

import argparse
import json
import statistics
import sys
import time
import traceback
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PERF_DIR = ROOT / "validation" / "perf"
OUT_DIR = ROOT / "validation" / "optimization" / "baseline" / "raw"

NOTES = [
    "read_object/_check_limits counts come from monkeypatching the module/class "
    "attribute, which forces the Python fallback path of the port; the counts are "
    "exact logical call counts but instrumented timings are an upper bound.",
    "PyO3 calls issued from Rust to Rust cannot be observed from Python without "
    "editing the extension, so boundary-call counts here are Python-visible only.",
    "peak RSS is not re-sampled here; see validation/results/perf-*.json.",
]


# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------
def backend() -> tuple[str, str | None]:
    import pypdf

    try:
        import pypdf._pypdf as ext

        return "rust", getattr(ext, "__file__", None)
    except ImportError:
        return "reference", None


def load_workloads() -> dict:
    with open(PERF_DIR / "workloads.json", encoding="utf-8") as handle:
        return json.load(handle)


def wl(workloads: dict, wid: str) -> dict:
    for entry in workloads["workloads"]:
        if entry["id"] == wid:
            return entry
    raise KeyError(wid)


def files_of(workload: dict) -> list[Path]:
    return [ROOT / rel for rel in workload["files"]]


def timeit(fn, repeat: int = 1) -> float:
    """Best (min) wall time of ``repeat`` runs, in seconds."""
    best = float("inf")
    for _ in range(repeat):
        t0 = time.perf_counter()
        fn()
        best = min(best, time.perf_counter() - t0)
    return best


def stats(values: list[float]) -> dict:
    if not values:
        return {"n": 0}
    return {
        "n": len(values),
        "median": statistics.median(values),
        "min": min(values),
        "max": max(values),
        "sum": sum(values),
    }


# ---------------------------------------------------------------------------
# runtime counters
# ---------------------------------------------------------------------------
class Counters:
    """Patch read_object / *_check_limits to count calls and nesting."""

    def __init__(self) -> None:
        self.read_object = Counter()
        self.check_limits = Counter()
        self.depth = 0
        self.max_depth = 0
        self.types: Counter = Counter()
        self._orig_read = None
        self._orig_cs = None
        self._orig_arr = None

    def install(self) -> None:
        import pypdf.generic._data_structures as ds

        self._ds = ds
        self._orig_read = ds.read_object
        self._orig_cs = ds.ContentStream._check_limits
        self._orig_arr = ds.ArrayObject._check_limits
        outer = self

        def read_object(stream, pdf, forced_encoding=None, *, parsing_budget=None):
            depth = outer.depth
            outer.read_object["calls"] += 1
            if depth == 0:
                outer.read_object["top_level"] += 1
            else:
                outer.read_object["recursive"] += 1
            if depth > outer.max_depth:
                outer.max_depth = depth
            outer.depth = depth + 1
            try:
                value = outer._orig_read(
                    stream, pdf, forced_encoding, parsing_budget=parsing_budget
                )
            finally:
                outer.depth = depth
            outer.types[type(value).__name__] += 1
            return value

        def cs_check_limits(*, remaining_elements=None, parsing_budget, token_decrement):
            outer.check_limits["contentstream"] += 1
            outer.check_limits["token_decrement_total"] += int(token_decrement)
            return outer._orig_cs(
                remaining_elements=remaining_elements,
                parsing_budget=parsing_budget,
                token_decrement=token_decrement,
            )

        def arr_check_limits(*, remaining_elements, parsing_budget=None):
            outer.check_limits["arrayobject"] += 1
            return outer._orig_arr(
                remaining_elements=remaining_elements, parsing_budget=parsing_budget
            )

        ds.read_object = read_object
        ds.ContentStream._check_limits = staticmethod(cs_check_limits)
        ds.ArrayObject._check_limits = staticmethod(arr_check_limits)

    def uninstall(self) -> None:
        if self._orig_read is None:
            return
        self._ds.read_object = self._orig_read
        self._ds.ContentStream._check_limits = self._orig_cs
        self._ds.ArrayObject._check_limits = self._orig_arr

    def reset(self) -> None:
        self.read_object.clear()
        self.check_limits.clear()
        self.types.clear()
        self.max_depth = 0
        self.depth = 0

    def as_dict(self) -> dict:
        return {
            "read_object": dict(self.read_object),
            "read_object_return_types": dict(self.types.most_common()),
            "read_object_max_recursion_depth": self.max_depth,
            "check_limits": dict(self.check_limits),
        }


class OpTimers:
    """Time TextExtraction.process_operation from Python (per-operator boundary)."""

    def __init__(self) -> None:
        self.calls = 0
        self.total = 0.0
        self.per_operator: Counter = Counter()
        self._orig = None

    def install(self) -> None:
        from pypdf._text_extraction._text_extractor import TextExtraction

        self._cls = TextExtraction
        self._orig = TextExtraction.process_operation
        outer = self

        def process_operation(self, operator, operands):
            t0 = time.perf_counter()
            try:
                return outer._orig(self, operator, operands)
            finally:
                outer.total += time.perf_counter() - t0
                outer.calls += 1
                outer.per_operator[operator] += 1

        TextExtraction.process_operation = process_operation

    def uninstall(self) -> None:
        if self._orig is not None:
            self._cls.process_operation = self._orig

    def as_dict(self) -> dict:
        return {
            "process_operation_calls": self.calls,
            "process_operation_seconds": self.total,
            "operator_histogram": {
                (k.decode("latin-1") if isinstance(k, bytes) else str(k)): v
                for k, v in self.per_operator.most_common(30)
            },
        }


def content_stream_info(reader, max_pages: int = 0) -> dict:
    """Sizes/counters over the decoded content streams of a document."""
    from pypdf.generic import ContentStream

    pages = list(reader.pages)
    if max_pages:
        pages = pages[:max_pages]
    decoded_bytes = 0
    n_ops = 0
    n_tokens = 0
    operator_hist: Counter = Counter()
    per_page = []
    t_decode = 0.0
    t_parse = 0.0
    for page in pages:
        contents = page.get_contents()
        if not isinstance(contents, ContentStream):
            continue
        # decoded bytes as produced by the filter chain
        t0 = time.perf_counter()
        data = contents.get_data()
        t_decode += time.perf_counter() - t0
        decoded_bytes += len(data)
        t0 = time.perf_counter()
        operations = contents.operations  # triggers _parse_content_stream
        t_parse += time.perf_counter() - t0
        n_ops += len(operations)
        tokens = 0
        for operands, operator in operations:
            tokens += len(operands)
            operator_hist[operator] += 1
        n_tokens += tokens
        per_page.append(
            {
                "decoded_bytes": len(data),
                "operations": len(operations),
                "tokens": tokens,
                "decode_s": t_decode,
                "parse_s": t_parse,
            }
        )
    return {
        "pages": len(pages),
        "decoded_bytes": decoded_bytes,
        "operations": n_ops,
        "tokens": n_tokens,
        "decode_seconds": t_decode,
        "parse_seconds": t_parse,
        "operator_histogram": {
            (k.decode("latin-1") if isinstance(k, bytes) else str(k)): v
            for k, v in operator_hist.most_common(15)
        },
        "per_page": per_page,
    }


# ---------------------------------------------------------------------------
# modes
# ---------------------------------------------------------------------------
def mode_issue301(args) -> dict:
    import pypdf

    path = ROOT / "resources" / "issue-301.pdf"
    result = {"file": str(path.relative_to(ROOT)), "bytes": path.stat().st_size}

    # 1. uninstrumented baseline
    reader = pypdf.PdfReader(str(path))
    page = reader.pages[0]
    contents = page.get_contents()
    data = contents.get_data()
    result["decoded_bytes"] = len(data)
    t0 = time.perf_counter()
    operations = contents.operations
    result["parse_seconds_uninstrumented"] = time.perf_counter() - t0
    result["operations"] = len(operations)
    tokens = sum(len(operands) for operands, _ in operations)
    result["tokens"] = tokens
    hist = Counter(operator for _, operator in operations)
    result["operator_histogram"] = {
        (k.decode("latin-1") if isinstance(k, bytes) else str(k)): v
        for k, v in hist.most_common(20)
    }

    # fresh document: extract_text (full page) uninstrumented
    reader2 = pypdf.PdfReader(str(path))
    t0 = time.perf_counter()
    text = reader2.pages[0].extract_text() or ""
    result["extract_text_seconds_uninstrumented"] = time.perf_counter() - t0
    result["extracted_text_len"] = len(text)

    # 2. instrumented: read_object / _check_limits counters
    counters = Counters()
    counters.install()
    try:
        reader3 = pypdf.PdfReader(str(path))
        page3 = reader3.pages[0]
        contents3 = page3.get_contents()
        counters.reset()
        t0 = time.perf_counter()
        operations3 = contents3.operations
        result["parse_seconds_instrumented"] = time.perf_counter() - t0
        result["counters"] = counters.as_dict()
        result["operations_instrumented"] = len(operations3)

        # reader-side (xref / indirect object) read_object calls for comparison
        counters.reset()
        pypdf.PdfReader(str(path))
        result["reader_open_counters"] = counters.as_dict()

        # 3. instrumented: extract_text with per-operator timer
        timers = OpTimers()
        timers.install()
        try:
            reader4 = pypdf.PdfReader(str(path))
            counters.reset()
            t0 = time.perf_counter()
            text4 = reader4.pages[0].extract_text() or ""
            result["extract_text_seconds_instrumented"] = time.perf_counter() - t0
        finally:
            timers.uninstall()
        result["extract_text_text_len_equal"] = len(text4) == len(text)
        result["text_extract_counters"] = counters.as_dict()
        result["operator_timers"] = timers.as_dict()
    finally:
        counters.uninstall()

    # 4. counter consistency checks
    counts = result["counters"]["read_object"]
    result["consistency"] = {
        "top_level_equals_tokens": counts.get("top_level") == result["tokens"],
        "top_level": counts.get("top_level"),
        "tokens": result["tokens"],
        "calls_over_top_level": counts.get("calls", 0) - counts.get("top_level", 0),
        "check_limits_contentstream": result["counters"]["check_limits"].get("contentstream"),
        "operations_plus_tokens": result["operations"] + result["tokens"],
    }
    return result


def mode_stages(args) -> dict:
    workloads = load_workloads()
    out = {}
    for workload in workloads["workloads"]:
        wid = workload["id"]
        files = files_of(workload)
        entry = {"name": workload["name"], "files": []}
        for path in files:
            row = {"path": str(path.relative_to(ROOT)), "bytes": path.stat().st_size}
            row["open_s"] = timeit(lambda: __import__("pypdf").PdfReader(str(path)), 2)
            entry["files"].append(row)
        out[wid] = entry
    return out


def mode_w3(args) -> dict:
    import pypdf

    workloads = load_workloads()
    workload = wl(workloads, "W3")
    max_pages = int(workload["params"].get("max_pages") or 0)
    rows = []
    totals = Counter()
    for path in files_of(workload):
        row = {"path": str(path.relative_to(ROOT)), "bytes": path.stat().st_size}
        try:
            t0 = time.perf_counter()
            reader = pypdf.PdfReader(str(path))
            row["open_s"] = time.perf_counter() - t0

            # stage 1: page + resources/font lookup
            t0 = time.perf_counter()
            pages = list(reader.pages)
            if max_pages:
                pages = pages[:max_pages]
            for page in pages:
                page.get("/Resources")
            row["pages_and_resources_s"] = time.perf_counter() - t0
            row["pages"] = len(pages)

            info = content_stream_info(reader, max_pages)
            row["content_streams"] = info

            # stage 2: full extract_text on the same pages
            reader2 = pypdf.PdfReader(str(path))
            pages2 = list(reader2.pages)[: (max_pages or None)]
            t0 = time.perf_counter()
            text = "".join((page.extract_text() or "") for page in pages2)
            row["extract_text_s"] = time.perf_counter() - t0
            row["text_len"] = len(text)

            # residual: everything extract_text does beyond decoding + parsing
            row["interpret_residual_s"] = (
                row["extract_text_s"]
                - info["decode_seconds"]
                - info["parse_seconds"]
            )
            row["parse_share_of_extract"] = (
                info["parse_seconds"] / row["extract_text_s"] if row["extract_text_s"] else None
            )
            totals["bytes"] += row["bytes"]
            totals["decoded_bytes"] += info["decoded_bytes"]
            totals["operations"] += info["operations"]
            totals["tokens"] += info["tokens"]
            totals["decode_seconds"] += info["decode_seconds"]
            totals["parse_seconds"] += info["parse_seconds"]
            totals["extract_text_s"] += row["extract_text_s"]
            totals["pages"] += row["pages"]
        except Exception as error:  # noqa: BLE001
            row["error"] = f"{type(error).__name__}: {error}"
            row["traceback"] = traceback.format_exc()[-1500:]
        rows.append(row)

    per_mb = {}
    decoded_mb = totals["decoded_bytes"] / (1024 * 1024)
    if decoded_mb:
        per_mb = {
            "decoded_mb": decoded_mb,
            "operations_per_mb": totals["operations"] / decoded_mb,
            "tokens_per_mb": totals["tokens"] / decoded_mb,
            "parse_ms_per_mb": 1000 * totals["parse_seconds"] / decoded_mb,
            "ops_per_token": totals["tokens"] / totals["operations"]
            if totals["operations"]
            else None,
        }
    return {"files": rows, "totals": dict(totals), "per_mb": per_mb}


def mode_w4(args) -> dict:
    import pypdf

    workloads = load_workloads()
    workload = wl(workloads, "W4")
    rows = []
    totals = Counter()
    for path in files_of(workload):
        row = {"path": str(path.relative_to(ROOT)), "bytes": path.stat().st_size}
        try:
            t0 = time.perf_counter()
            reader = pypdf.PdfReader(str(path))
            row["open_s"] = time.perf_counter() - t0
            pages = list(reader.pages)
            row["pages"] = len(pages)

            t0 = time.perf_counter()
            images = []
            for page in pages:
                images.extend(list(page.images))
            row["enumerate_s"] = time.perf_counter() - t0
            row["image_count"] = len(images)

            t_data = 0.0
            t_pil = 0.0
            filters: Counter = Counter()
            raw_total = 0
            for image in images:
                t = time.perf_counter()
                data = image.data
                t_data += time.perf_counter() - t
                raw_total += len(data)
                try:
                    name = getattr(image, "name", "?")
                    src = image.indirect_reference  # noqa: F841
                except Exception:  # noqa: BLE001
                    name = "?"
                t = time.perf_counter()
                pil = image.image
                t_pil += time.perf_counter() - t
                row.setdefault("image_rows", []).append(
                    {
                        "name": str(name),
                        "raw_bytes": len(data),
                        "pil": [list(pil.size), getattr(pil, "mode", None)] if pil is not None else None,
                    }
                )
                try:
                    filt = image.indirect_reference.get_object().get("/Filter")
                    for item in (filt if isinstance(filt, list) else [filt]):
                        if item is not None:
                            filters[str(item)] += 1
                except Exception:  # noqa: BLE001
                    pass
            row["image_data_s"] = t_data
            row["image_pil_decode_s"] = t_pil
            row["raw_bytes"] = raw_total
            row["filters"] = dict(filters)
            totals["enumerate_s"] += row["enumerate_s"]
            totals["image_data_s"] += t_data
            totals["image_pil_decode_s"] += t_pil
            totals["raw_bytes"] += raw_total
            totals["images"] += len(images)
        except Exception as error:  # noqa: BLE001
            row["error"] = f"{type(error).__name__}: {error}"
            row["traceback"] = traceback.format_exc()[-1500:]
        rows.append(row)
    return {"files": rows, "totals": dict(totals)}


def mode_w57(args) -> dict:
    import io

    import pypdf

    workloads = load_workloads()
    out = {}
    for wid, kind in (("W5", "append"), ("W6", "page_ops"), ("W7", "clone")):
        workload = wl(workloads, wid)
        files = files_of(workload)
        cycle = (workload.get("params") or {}).get("rotate_cycle") or [90, 180, 270]
        lower = tuple((workload.get("params") or {}).get("crop_lower_left") or (10.0, 10.0))
        upper = tuple((workload.get("params") or {}).get("crop_upper_right") or (500.0, 700.0))
        row = {"kind": kind, "files": [str(p.relative_to(ROOT)) for p in files]}

        t0 = time.perf_counter()
        readers = [pypdf.PdfReader(str(p)) for p in files]
        pages = [list(reader.pages) for reader in readers]
        row["read_s"] = time.perf_counter() - t0

        writer = pypdf.PdfWriter()
        t0 = time.perf_counter()
        if kind == "append":
            for path in files:
                writer.append(str(path))
            row["copy_s"] = time.perf_counter() - t0
        elif kind == "clone":
            n = 0
            for page_list in pages:
                for page in page_list:
                    writer.add_page(page)
                    n += 1
            row["copy_s"] = time.perf_counter() - t0
            row["pages"] = n
        else:
            index = 0
            for page_list in pages:
                for page in page_list:
                    new_page = writer.add_page(page)
                    rotation = int(cycle[index % len(cycle)])
                    if rotation:
                        new_page.rotate(rotation)
                    new_page.cropbox.lower_left = lower
                    new_page.cropbox.upper_right = upper
                    index += 1
            row["copy_s"] = time.perf_counter() - t0
            row["pages"] = index

        buffer = io.BytesIO()
        t0 = time.perf_counter()
        writer.write(buffer)
        row["write_s"] = time.perf_counter() - t0
        row["output_bytes"] = len(buffer.getvalue())
        row["total_s"] = row["read_s"] + row["copy_s"] + row["write_s"]
        row["copy_share"] = row["copy_s"] / row["total_s"] if row["total_s"] else None
        row["write_share"] = row["write_s"] / row["total_s"] if row["total_s"] else None
        out[wid] = row
    return out


class NamedTimers:
    """Time a set of ``TextExtraction`` methods through the Python boundary.

    The Rust text loop reaches these methods through ``call_method1`` on the
    instance, so a class-level patch is observed by the port as well as by the
    reference implementation.
    """

    HOOKS = ("process_operation", "_handle_tj", "_handle_tj_operation", "_post_process_text_operation")

    def __init__(self) -> None:
        self.data: dict[str, dict] = {}
        self._orig: dict[str, object] = {}

    def install(self) -> None:
        from pypdf._text_extraction._text_extractor import TextExtraction

        self._cls = TextExtraction
        outer = self
        for name in self.HOOKS:
            original = getattr(TextExtraction, name, None)
            if original is None:
                continue
            self._orig[name] = original
            self.data[name] = {"calls": 0, "seconds": 0.0}

            def make(name: str, original):
                def wrapper(self, *args, **kwargs):
                    slot = outer.data[name]
                    t0 = time.perf_counter()
                    try:
                        return original(self, *args, **kwargs)
                    finally:
                        slot["seconds"] += time.perf_counter() - t0
                        slot["calls"] += 1

                return wrapper

            setattr(TextExtraction, name, make(name, original))

    def uninstall(self) -> None:
        for name, original in self._orig.items():
            setattr(self._cls, name, original)

    def as_dict(self) -> dict:
        return {k: dict(v) for k, v in self.data.items()}


def mode_w3ops(args) -> dict:
    """W3: measured split parse / dispatch / glyph+string / remaining (font-CMap)."""
    import pypdf

    workloads = load_workloads()
    workload = wl(workloads, "W3")
    max_pages = int(workload["params"].get("max_pages") or 0)
    rows = []
    totals = Counter()
    for path in files_of(workload):
        row = {"path": str(path.relative_to(ROOT))}
        try:
            reader = pypdf.PdfReader(str(path))
            info = content_stream_info(reader, max_pages)

            timers = NamedTimers()
            timers.install()
            try:
                reader2 = pypdf.PdfReader(str(path))
                pages2 = list(reader2.pages)
                if max_pages:
                    pages2 = pages2[:max_pages]
                t0 = time.perf_counter()
                text = "".join((page.extract_text() or "") for page in pages2)
                row["extract_text_instrumented_s"] = time.perf_counter() - t0
            finally:
                timers.uninstall()
            row["hooks"] = timers.as_dict()
            row["text_len"] = len(text)
            row["parse_s"] = info["parse_seconds"]
            row["decode_s"] = info["decode_seconds"]
            row["operations"] = info["operations"]
            row["tokens"] = info["tokens"]
            row["decoded_bytes"] = info["decoded_bytes"]
            row["operator_histogram"] = info["operator_histogram"]

            hooks = row["hooks"]
            dispatch = hooks.get("process_operation", {}).get("seconds", 0.0)
            tj = hooks.get("_handle_tj", {}).get("seconds", 0.0)
            tj_op = hooks.get("_handle_tj_operation", {}).get("seconds", 0.0)
            total = row["extract_text_instrumented_s"]
            row["split"] = {
                "extract_text_total_s": total,
                "parse_s": row["parse_s"],
                "decode_s": row["decode_s"],
                "process_operation_dispatch_s": dispatch,
                "handle_tj_glyph_and_string_s": tj,
                "handle_tj_operation_s": tj_op,
                "remaining_font_cmap_pagesetup_s": max(
                    0.0, total - row["parse_s"] - row["decode_s"] - dispatch
                ),
                "parse_share": row["parse_s"] / total if total else None,
                "dispatch_share": dispatch / total if total else None,
                "font_cmap_share": (max(0.0, total - row["parse_s"] - dispatch) / total)
                if total
                else None,
            }
            for key in ("parse_s", "extract_text_instrumented_s"):
                totals[key] += row[key]
            totals["dispatch_s"] += dispatch
            totals["tj_s"] += tj
            totals["tj_op_s"] += tj_op
            totals["operations"] += row["operations"]
            totals["tokens"] += row["tokens"]
            totals["decoded_bytes"] += row["decoded_bytes"]
        except Exception as error:  # noqa: BLE001
            row["error"] = f"{type(error).__name__}: {error}"
            row["traceback"] = traceback.format_exc()[-1200:]
        rows.append(row)
    totals["remaining_s"] = max(
        0.0, totals["extract_text_instrumented_s"] - totals["parse_s"] - totals["dispatch_s"]
    )
    return {"files": rows, "totals": dict(totals)}


def mode_w4b(args) -> dict:
    """W4: split ``page.images`` into filter decode, imaging, list/ImageFile overhead."""
    import pypdf
    from pypdf.generic import StreamObject

    workloads = load_workloads()
    workload = wl(workloads, "W4")
    rows = []
    totals = Counter()
    for path in files_of(workload):
        row = {"path": str(path.relative_to(ROOT))}
        try:
            # (1) what the workload actually does: materialise every ImageFile
            reader = pypdf.PdfReader(str(path))
            pages = list(reader.pages)
            t0 = time.perf_counter()
            images = []
            for page in pages:
                images.extend(list(page.images))
            row["t_list_images_s"] = time.perf_counter() - t0
            row["image_count"] = len(images)
            row["raw_bytes"] = sum(len(image.data) for image in images)
            filters: Counter = Counter()
            for image in images:
                obj = image.indirect_reference.get_object()
                filt = obj.get("/Filter")
                for item in (filt if isinstance(filt, list) else [filt]):
                    if item is not None:
                        filters[str(item)] += 1
            row["filters"] = dict(filters)

            # (2) filter chain only (get_data), fresh reader
            reader2 = pypdf.PdfReader(str(path))
            xobjects = []
            for page in reader2.pages:
                resources = page.get("/Resources")
                xobj = resources.get("/XObject") if resources else None
                if not isinstance(xobj, dict):
                    continue
                for _, entry in xobj.items():
                    value = entry.get_object()
                    if isinstance(value, StreamObject) and str(value.get("/Subtype")) == "/Image":
                        xobjects.append(value)
            t0 = time.perf_counter()
            for xobj in xobjects:
                xobj.get_data()
            row["t_filter_decode_s"] = time.perf_counter() - t0
            row["xobject_count"] = len(xobjects)

            # (3) filter + Pillow imaging, fresh reader (decode_as_image)
            reader3 = pypdf.PdfReader(str(path))
            xobjects3 = []
            for page in reader3.pages:
                resources = page.get("/Resources")
                xobj = resources.get("/XObject") if resources else None
                if not isinstance(xobj, dict):
                    continue
                for _, entry in xobj.items():
                    value = entry.get_object()
                    if isinstance(value, StreamObject) and str(value.get("/Subtype")) == "/Image":
                        xobjects3.append(value)
            t0 = time.perf_counter()
            for xobj in xobjects3:
                xobj.decode_as_image()
            row["t_decode_as_image_s"] = time.perf_counter() - t0

            t_filter = row["t_filter_decode_s"]
            t_image = row["t_decode_as_image_s"]
            row["split"] = {
                "list_images_s": row["t_list_images_s"],
                "filter_decode_s": t_filter,
                "imaging_s": max(0.0, t_image - t_filter),
                "imagefile_list_overhead_s": max(0.0, row["t_list_images_s"] - t_image),
                "filter_share": t_filter / row["t_list_images_s"] if row["t_list_images_s"] else None,
            }
            totals["t_list_images_s"] += row["t_list_images_s"]
            totals["t_filter_decode_s"] += t_filter
            totals["t_decode_as_image_s"] += t_image
            totals["images"] += len(images)
            totals["raw_bytes"] += row["raw_bytes"]
        except Exception as error:  # noqa: BLE001
            row["error"] = f"{type(error).__name__}: {error}"
            row["traceback"] = traceback.format_exc()[-1200:]
        rows.append(row)
    totals["imaging_s"] = max(
        0.0, totals["t_decode_as_image_s"] - totals["t_filter_decode_s"]
    )
    totals["overhead_s"] = max(
        0.0, totals["t_list_images_s"] - totals["t_decode_as_image_s"]
    )
    return {"files": rows, "totals": dict(totals)}


def mode_cprofile(args) -> dict:
    """Show what cProfile can and cannot see in one W3 file / one W5 write."""
    import cProfile
    import io
    import pstats

    import pypdf

    workloads = load_workloads()
    w3 = wl(workloads, "W3")
    target = ROOT / "sample-files" / "004-pdflatex-4-pages" / "pdflatex-4-pages.pdf"
    reader = pypdf.PdfReader(str(target))
    pages = list(reader.pages)[:5]
    profiler = cProfile.Profile()
    profiler.enable()
    for page in pages:
        page.extract_text()
    profiler.disable()
    stream = io.StringIO()
    pstats.Stats(profiler, stream=stream).sort_stats("cumulative").print_stats(18)
    text_dump = stream.getvalue()

    reader2 = pypdf.PdfReader(str(target))
    writer = pypdf.PdfWriter()
    for page in list(reader2.pages):
        writer.add_page(page)
    profiler2 = cProfile.Profile()
    profiler2.enable()
    buffer = io.BytesIO()
    writer.write(buffer)
    profiler2.disable()
    stream2 = io.StringIO()
    pstats.Stats(profiler2, stream=stream2).sort_stats("cumulative").print_stats(18)

    return {
        "extract_text_profile_top": text_dump.splitlines()[-60:],
        "write_profile_top": stream2.getvalue().splitlines()[-60:],
        "note": "Only Python-level shims appear; work inside the extension is a single leaf entry.",
    }


MODES = {
    "issue301": mode_issue301,
    "stages": mode_stages,
    "w3": mode_w3,
    "w3ops": mode_w3ops,
    "w4": mode_w4,
    "w4b": mode_w4b,
    "w57": mode_w57,
    "cprofile": mode_cprofile,
}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=sorted(MODES))
    parser.add_argument("--out", type=Path, default=None)
    args = parser.parse_args(argv)

    name, ext = backend()
    payload = {
        "schema": "optimization-probe-v1",
        "backend": name,
        "backend_extension": ext,
        "mode": args.mode,
        "python": sys.version.split()[0],
        "argv": sys.argv,
        "notes": NOTES,
        "result": MODES[args.mode](args),
    }
    out = args.out or (OUT_DIR / f"probe-{args.mode}-{name}.json")
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    with open(out, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=1, ensure_ascii=False, default=str)
        handle.write("\n")
    print(f"[probe] wrote {out} (backend={name}, mode={args.mode})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
