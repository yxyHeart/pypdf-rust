#!/usr/bin/env python3
"""Per-stage / per-call diagnostics for the Reader (W1, W2) and Writer
(W5, W6, W7) workloads of the pypdf Rust port vs. reference pypdf 7.1.1.

Runs in the interpreter of one backend (the venv decides which implementation is
imported; ``--backend`` is only a guard so results cannot be filed wrongly).

    .venvs/dev/bin/python validation/optimization/scripts/rw_diag.py \
        --backend rust --workloads W1,W2,W5,W6,W7 --mode all \
        --out validation/optimization/raw/rw_diag_rust.json

Modes (each measurement is a separate pass so the instrumentation of one mode
never perturbs another):

``layers``   layered wall time (open / +pages / +metadata-or-traverse) so the
             big stages can be differenced without any monkeypatch overhead.
``stages``   inclusive wall time + call count of the shim methods that make up
             each stage (patched with a ~0.2 us wrapper; overhead reported).
``counts``   Python -> Rust boundary calls and PDF-object materialisations, per
             workload phase (open / flatten / traverse / ...).  Wall time of
             this pass is perturbed and explicitly marked as such.
``profile``  cProfile attribution, classified Rust builtin vs Python.

The JSON written here is consumed by ``render_tables.py``.
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
import time
import traceback
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE))

import rw_instrument as inst  # noqa: E402

SCHEMA = "rw-diag-v1"


# ---------------------------------------------------------------------------
# environment / backend detection
# ---------------------------------------------------------------------------


def detect_backend():
    import pypdf

    try:
        import pypdf._pypdf as ext

        return "rust", getattr(ext, "__file__", None)
    except ImportError:
        return "reference", None


def file_identity(path):
    if not path:
        return None
    candidate = Path(path)
    if not candidate.is_file():
        return {"path": path, "exists": False}
    digest = hashlib.sha256()
    with candidate.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return {
        "path": str(candidate),
        "sha256": digest.hexdigest(),
        "bytes": candidate.stat().st_size,
    }


def loadavg():
    try:
        return [round(value, 2) for value in os.getloadavg()]
    except OSError:
        return None


def environment():
    import pypdf

    backend, ext_path = detect_backend()
    return {
        "backend": backend,
        "pypdf_file": pypdf.__file__,
        "pypdf_version": getattr(pypdf, "__version__", None),
        "extension": file_identity(ext_path),
        "python": sys.version.split()[0],
        "executable": sys.executable,
        "platform": platform.platform(),
        "machine": platform.machine(),
        "cpu_count": os.cpu_count(),
        "loadavg_start": loadavg(),
    }


# ---------------------------------------------------------------------------
# workload operations (mirror validation/perf/run.py; phase hooks added)
# ---------------------------------------------------------------------------


class Phase:
    """Marks a named phase on the optional BoundaryCounter."""

    def __init__(self, counter, name):
        self.counter = counter
        self.name = name

    def __enter__(self):
        if self.counter is not None:
            self.counter.start_phase(self.name)
        return self

    def __exit__(self, *exc):
        if self.counter is not None:
            self.counter.end_phase()
        return False


def load_workloads():
    with open(ROOT / "validation" / "perf" / "workloads.json", encoding="utf-8") as handle:
        data = json.load(handle)
    return {item["id"]: item for item in data["workloads"]}


def resolve_files(workload):
    return [ROOT / name for name in workload["files"]]


def op_reader_init(files, params, counter=None, facts=None):
    """W1: open every file, read the page count and selected metadata fields."""
    from pypdf import PdfReader

    fields = params.get("metadata_fields") or ["/Title"]
    rows = []
    for path in files:
        with Phase(counter, "open"):
            reader = PdfReader(str(path))
        with Phase(counter, "pages"):
            num_pages = len(reader.pages)
        with Phase(counter, "metadata"):
            metadata = reader.metadata or {}
            values = [None if metadata.get(name) is None else str(metadata.get(name)) for name in fields]
        rows.append([str(path.relative_to(ROOT)), num_pages, values])
        if facts is not None:
            facts.append(reader_facts(path, reader))
    return {"workload": "reader_init", "files": rows}


def op_page_traverse(files, params, counter=None, facts=None):
    """W2: walk every page of the page tree and touch all boxes + rotation."""
    from pypdf import PdfReader

    box_names = params.get("boxes") or ["mediabox", "cropbox"]
    max_pages = int(params.get("max_pages") or 0)
    include_rotation = bool(params.get("include_rotation", True))
    rows = []
    for path in files:
        with Phase(counter, "open"):
            reader = PdfReader(str(path))
        with Phase(counter, "flatten"):
            pages = list(reader.pages)
        if max_pages:
            pages = pages[:max_pages]
        page_rows = []
        with Phase(counter, "traverse"):
            for page in pages:
                boxes = []
                for name in box_names:
                    box = getattr(page, name)
                    boxes.append([round(float(value), 3) for value in box])
                rotation = int(page.rotation) if include_rotation else None
                page_rows.append([rotation, boxes])
        rows.append([str(path.relative_to(ROOT)), len(page_rows), page_rows])
        if facts is not None:
            facts.append(reader_facts(path, reader, pages=len(page_rows)))
    return {"workload": "page_traverse", "files": rows}


def reader_facts(path, reader, pages=None):
    facts = {"path": str(path.relative_to(ROOT))}
    try:
        facts["file_bytes"] = path.stat().st_size
    except OSError:
        pass
    try:
        facts["xref_entries"] = sum(len(gen) for gen in getattr(reader, "xref", {}).values())
    except Exception:  # noqa: BLE001
        pass
    try:
        facts["object_streams"] = len(getattr(reader, "xref_objStm", {}))
    except Exception:  # noqa: BLE001
        pass
    try:
        facts["resolved_objects"] = len(getattr(reader, "resolved_objects", {}))
    except Exception:  # noqa: BLE001
        pass
    try:
        facts["cached_objects"] = len(getattr(reader, "_objects", {}) or {})
    except Exception:  # noqa: BLE001
        pass
    if pages is not None:
        facts["pages"] = pages
    return facts


def _write_io(writer):
    buffer = io.BytesIO()
    writer.write(buffer)
    return buffer.getvalue()


def writer_build(wid, files, params, counter=None, facts=None):
    """Construct the output writer (source readers + page clone + edits).

    Shared by the workload operations and by the layered timing, so both use
    exactly the same code path.
    """
    from pypdf import PdfReader, PdfWriter

    writer = PdfWriter()
    if wid == "W5":
        with Phase(counter, "append"):
            for path in files:
                writer.append(str(path))
        if facts is not None:
            facts.append(writer_facts(writer, b"", "merge_append_build"))
        return writer

    cycle = params.get("rotate_cycle") or [90, 180, 270]
    lower_left = tuple(params.get("crop_lower_left") or [10.0, 10.0])
    upper_right = tuple(params.get("crop_upper_right") or [500.0, 700.0])
    index = 0
    with Phase(counter, "read_source"):
        sources = [(path, PdfReader(str(path))) for path in files]
    with Phase(counter, "add_page"):
        for _path, reader in sources:
            for page in reader.pages:
                new_page = writer.add_page(page)
                if wid == "W6":
                    rotation = int(cycle[index % len(cycle)])
                    if rotation:
                        new_page.rotate(rotation)
                    new_page.cropbox.lower_left = lower_left
                    new_page.cropbox.upper_right = upper_right
                index += 1
    if facts is not None:
        facts.append(writer_facts(writer, b"", f"{wid}_build"))
    return writer


def op_merge_append(files, params, counter=None, facts=None):
    """W5: merge 5 small PDFs with PdfWriter.append and serialize."""
    writer = writer_build("W5", files, params, counter=counter)
    with Phase(counter, "write"):
        data = _write_io(writer)
    if facts is not None:
        facts.append(writer_facts(writer, data, "merge_append"))
    return {
        "workload": "merge_append",
        "input_files": [str(path.relative_to(ROOT)) for path in files],
        "pages": len(writer.pages),
        "pdf_bytes": len(data),
    }


def op_page_ops_write(files, params, counter=None, facts=None):
    """W6: clone pages, rotate + replace cropbox, then serialize."""
    writer = writer_build("W6", files, params, counter=counter)
    with Phase(counter, "write"):
        data = _write_io(writer)
    if facts is not None:
        facts.append(writer_facts(writer, data, "page_ops_write"))
    return {
        "workload": "page_ops_write",
        "input_files": [str(path.relative_to(ROOT)) for path in files],
        "pages": len(writer.pages),
        "pdf_bytes": len(data),
    }


def op_clone_write(files, params, counter=None, facts=None):
    """W7: clone every page of the file set into a fresh writer and write."""
    writer = writer_build("W7", files, params, counter=counter)
    with Phase(counter, "write"):
        data = _write_io(writer)
    if facts is not None:
        facts.append(writer_facts(writer, data, "clone_write"))
    return {
        "workload": "clone_write",
        "input_files": [str(path.relative_to(ROOT)) for path in files],
        "pages": len(writer.pages),
        "pdf_bytes": len(data),
    }


def writer_facts(writer, data, workload):
    import hashlib

    facts = {
        "workload": workload,
        "out_bytes": len(data),
        "out_sha256": hashlib.sha256(data).hexdigest(),
        "pages": len(writer.pages),
    }
    try:
        facts["objects_in_writer"] = len(writer._objects)
    except Exception:  # noqa: BLE001
        pass
    try:
        facts["id_translated_pdfs"] = len(writer._id_translated)
        facts["id_translated_pages"] = sum(len(value) for value in writer._id_translated.values())
    except Exception:  # noqa: BLE001
        pass
    try:
        facts["root_pages_kids"] = len(writer.root_object["/Pages"]["/Kids"])
    except Exception:  # noqa: BLE001
        pass
    try:
        facts["declared_xref_size"] = int(writer._objects[-1].idnum) if writer._objects else None
    except Exception:  # noqa: BLE001
        pass
    return facts


OPS = {
    "W1": op_reader_init,
    "W2": op_page_traverse,
    "W5": op_merge_append,
    "W6": op_page_ops_write,
    "W7": op_clone_write,
}


# ---------------------------------------------------------------------------
# stage definitions
# ---------------------------------------------------------------------------


def stage_patches(timer, patches):
    """Patch the shim methods that correspond to one named stage each."""
    from pypdf import PdfReader, PdfWriter
    from pypdf._doc_common import PdfDocCommon
    from pypdf._page import PageObject
    import pypdf.generic._data_structures as ds

    applied = []

    def mark(label, ok):
        applied.append((label, bool(ok)))

    # --- Reader open -------------------------------------------------------
    mark("PdfReader.__init__", timer.patch_method(patches, PdfReader, "__init__", "open:__init__"))
    for attr in [
        "_initialize_stream",
        "read",
        "_basic_validation",
        "_find_eof_marker",
        "_find_startxref_pos",
        "_get_xref_issues",
        "_read_xref_tables_and_trailers",
        "_read_xref",
        "_read_standard_xref_table",
        "_read_pdf15_xref_stream",
        "_process_xref_stream",
        "_handle_encryption",
        "read_object_header",
        "get_object",
        "_get_object_from_stream",
        "cache_get_indirect_object",
        "cache_indirect_object",
    ]:
        if attr in PdfReader.__dict__:
            mark(f"PdfReader.{attr}", timer.patch_method(patches, PdfReader, attr, f"open:{attr}"))

    # --- page tree ---------------------------------------------------------
    for attr in [
        "get_num_pages",
        "_flatten",
        "_enter_page_tree_node",
        "_pop_next_page_tree_kid",
        "_flatten_leaf_page",
    ]:
        if attr in PdfDocCommon.__dict__:
            mark(f"PdfDocCommon.{attr}", timer.patch_method(patches, PdfDocCommon, attr, f"tree:{attr}"))
    mark("PageObject.__init__", timer.patch_method(patches, PageObject, "__init__", "tree:PageObject.__init__"))
    mark("PageObject.rotation", timer.patch_property(patches, PageObject, "rotation", "traverse:rotation"))
    if "rotate" in PageObject.__dict__:
        mark("PageObject.rotate", timer.patch_method(patches, PageObject, "rotate", "write:rotate"))

    # --- metadata ----------------------------------------------------------
    if "metadata" in PdfDocCommon.__dict__:
        mark("PdfDocCommon.metadata", timer.patch_property(patches, PdfDocCommon, "metadata", "meta:metadata"))
    for attr in ["_info"]:
        if attr in PdfDocCommon.__dict__:
            mark(f"PdfDocCommon.{attr}", timer.patch_method(patches, PdfDocCommon, attr, f"meta:{attr}"))

    # --- boxes -------------------------------------------------------------
    mark("_page._get_rectangle", timer.patch_module_function(patches, "pypdf._page", "_get_rectangle", "traverse:_get_rectangle"))
    mark("_page._set_rectangle", timer.patch_module_function(patches, "pypdf._page", "_set_rectangle", "traverse:_set_rectangle"))

    # --- writer ------------------------------------------------------------
    for attr in [
        "_add_object",
        "_add_page",
        "add_page",
        "append",
        "merge",
        "append_pages_from_reader",
        "clone_document_from_reader",
        "write_stream",
        "write",
        "_write_pdf_structure",
        "_write_xref_table",
        "_write_trailer",
        "get_reference",
        "_resolve_links",
        "compress_identical_objects",
        "_get_page_in_node",
    ]:
        if attr in PdfWriter.__dict__:
            mark(f"PdfWriter.{attr}", timer.patch_method(patches, PdfWriter, attr, f"write:{attr}"))
    for attr in ["_get_page_in_node"]:
        if attr in PdfDocCommon.__dict__:
            mark(f"PdfDocCommon.{attr}", timer.patch_method(patches, PdfDocCommon, attr, f"write:{attr}"))

    # --- object graph ------------------------------------------------------
    for cls in (ds.DictionaryObject, ds.ArrayObject, ds.StreamObject, ds.ContentStream):
        for attr in ("clone", "_clone", "write_to_stream"):
            if attr in cls.__dict__:
                mark(f"{cls.__name__}.{attr}", timer.patch_method(patches, cls, attr, f"graph:{cls.__name__}.{attr}"))
    from pypdf.generic._base import IndirectObject, NameObject, FloatObject, NumberObject, ByteStringObject, TextStringObject

    for cls in (IndirectObject, NameObject, FloatObject, NumberObject, ByteStringObject, TextStringObject):
        if "write_to_stream" in cls.__dict__:
            mark(f"{cls.__name__}.write_to_stream", timer.patch_method(patches, cls, "write_to_stream", f"graph:{cls.__name__}.write_to_stream"))
        if "clone" in cls.__dict__:
            mark(f"{cls.__name__}.clone", timer.patch_method(patches, cls, "clone", f"graph:{cls.__name__}.clone"))
    return applied


def count_patches(counter, patches):
    """Boundary + materialisation counters plus a few Python-level counters."""
    from pypdf import PdfReader, PdfWriter
    from pypdf._doc_common import PdfDocCommon
    from pypdf._page import PageObject
    import pypdf.generic._data_structures as ds
    from pypdf.generic._base import IndirectObject

    applied = []
    ok = counter.patch_boundary(patches)
    applied.append(("boundary", ok))
    applied.append(("materialization", True))
    counter.patch_materialization(patches)

    counted = [
        (PdfReader, "__init__"),
        (PdfReader, "get_object"),
        (PdfReader, "_get_object_from_stream"),
        (PdfReader, "cache_get_indirect_object"),
        (PdfDocCommon, "_flatten"),
        (PdfDocCommon, "_flatten_leaf_page"),
        (PdfDocCommon, "_enter_page_tree_node"),
        (PdfDocCommon, "_pop_next_page_tree_kid"),
        (PageObject, "__init__"),
        (ds.DictionaryObject, "clone"),
        (ds.DictionaryObject, "write_to_stream"),
        (ds.ArrayObject, "clone"),
        (ds.StreamObject, "clone"),
        (IndirectObject, "clone"),
        (PdfWriter, "_add_object"),
        (PdfWriter, "_add_page"),
        (PdfWriter, "add_page"),
        (PdfWriter, "get_reference"),
        (PdfWriter, "write_stream"),
        (PdfWriter, "_write_pdf_structure"),
        (PdfWriter, "_write_xref_table"),
        (PdfWriter, "_write_trailer"),
        (PdfWriter, "append"),
        (PdfWriter, "merge"),
    ]
    for cls, attr in counted:
        if attr in cls.__dict__:
            applied.append(
                (f"{cls.__name__}.{attr}", counter.patch_class_method(patches, cls, attr))
            )
    applied.append(
        (
            "PageObject.rotation",
            counter.patch_property_counted(patches, PageObject, "rotation"),
        )
    )
    for mod, attr in [("pypdf._page", "_get_rectangle"), ("pypdf._page", "_set_rectangle")]:
        applied.append((f"{mod}.{attr}", counter.patch_module_counted(patches, mod, attr)))
    return applied


# ---------------------------------------------------------------------------
# measurement passes
# ---------------------------------------------------------------------------


def warmup(wid, workload, files):
    """One untimed run so import/caches do not leak into the first sample."""
    OPS[wid](files, workload.get("params") or {})


def run_layers(wid, workload, files, repeats):
    """Layered wall time: each layer adds one stage, differences give the cost."""
    from pypdf import PdfReader

    params = workload.get("params") or {}
    fields = params.get("metadata_fields") or ["/Title", "/Author"]
    box_names = params.get("boxes") or ["mediabox"]
    max_pages = int(params.get("max_pages") or 0)

    if wid == "W1":
        layers = ("open", "open+pages", "open+pages+metadata")
    elif wid == "W2":
        layers = ("open", "open+pages", "open+pages+traverse")
    else:
        layers = None

    if layers is None:
        # writer workloads: open sources / +page tree / build writer / +write
        layers = ("open", "open+pages", "build", "build+write")

    samples = {name: [] for name in layers}
    order = list(layers)
    from pypdf import PdfWriter

    if wid in {"W5", "W6", "W7"}:
        for _ in range(repeats):
            for layer in order:
                t0 = inst.PERF()
                if layer == "open":
                    readers = [PdfReader(str(path)) for path in files]
                elif layer == "open+pages":
                    readers = [PdfReader(str(path)) for path in files]
                    for reader in readers:
                        list(reader.pages)
                elif layer == "build":
                    writer_build(wid, files, params)
                else:
                    writer = writer_build(wid, files, params)
                    _write_io(writer)
                samples[layer].append(inst.PERF() - t0)
        return {
            layer: {"samples_ms": [value / 1e6 for value in values], "describe": inst.describe([v / 1e6 for v in values])}
            for layer, values in samples.items()
        }

    # reader workloads: each layer adds one stage to the previous one
    for _ in range(repeats):
        for layer in order:
            t0 = inst.PERF()
            for path in files:
                reader = PdfReader(str(path))
                if layer != "open":
                    pages = list(reader.pages)
                    if max_pages:
                        pages = pages[:max_pages]
                if layer == "open+pages+metadata":
                    metadata = reader.metadata or {}
                    for name in fields:
                        metadata.get(name)
                if layer == "open+pages+traverse":
                    for page in pages:
                        for name in box_names:
                            getattr(page, name)
                        page.rotation
            samples[layer].append(inst.PERF() - t0)
    return {
        layer: {"samples_ms": [value / 1e6 for value in values], "describe": inst.describe([v / 1e6 for v in values])}
        for layer, values in samples.items()
    }


def run_wall_phases(wid, workload, files, repeats):
    """Direct wall-clock timing of each stage, with no monkeypatching at all.

    Each stage is bracketed by ``perf_counter_ns`` inside one repetition, so the
    per-phase numbers sum exactly to the workload wall time and no layer
    differencing (and no timer wrappers) is involved.
    """
    from pypdf import PdfReader

    params = workload.get("params") or {}
    fields = params.get("metadata_fields") or ["/Title"]
    box_names = params.get("boxes") or ["mediabox", "cropbox"]
    max_pages = int(params.get("max_pages") or 0)
    samples = {}
    totals = []

    def add(name, t0, t1=None):
        t1 = t1 if t1 is not None else inst.PERF()
        samples.setdefault(name, []).append(t1 - t0)

    repeat_phase = {}
    for _ in range(repeats):
        phase = {}
        started = inst.PERF()
        if wid == "W1":
            for path in files:
                t0 = inst.PERF()
                reader = PdfReader(str(path))
                t1 = inst.PERF()
                len(reader.pages)
                t2 = inst.PERF()
                metadata = reader.metadata or {}
                for name in fields:
                    metadata.get(name)
                t3 = inst.PERF()
                phase["open"] = phase.get("open", 0) + (t1 - t0)
                phase["pages_flatten"] = phase.get("pages_flatten", 0) + (t2 - t1)
                phase["metadata"] = phase.get("metadata", 0) + (t3 - t2)
        elif wid == "W2":
            for path in files:
                t0 = inst.PERF()
                reader = PdfReader(str(path))
                t1 = inst.PERF()
                pages = list(reader.pages)
                if max_pages:
                    pages = pages[:max_pages]
                t2 = inst.PERF()
                for page in pages:
                    for name in box_names:
                        getattr(page, name)
                t3 = inst.PERF()
                for page in pages:
                    page.rotation
                t4 = inst.PERF()
                phase["open"] = phase.get("open", 0) + (t1 - t0)
                phase["pages_flatten"] = phase.get("pages_flatten", 0) + (t2 - t1)
                phase["boxes"] = phase.get("boxes", 0) + (t3 - t2)
                phase["rotation"] = phase.get("rotation", 0) + (t4 - t3)
        else:
            t0 = inst.PERF()
            writer = writer_build(wid, files, params)
            t1 = inst.PERF()
            _write_io(writer)
            t2 = inst.PERF()
            phase["build"] = t1 - t0
            phase["write"] = t2 - t1
        phase["total"] = inst.PERF() - started
        totals.append(phase["total"])
        repeat_phase = phase
        for name, value in phase.items():
            if name == "total":
                continue
            samples.setdefault(name, []).append(value)

    return {
        "phases": {
            name: {
                "samples_ms": [value / 1e6 for value in values],
                "describe": inst.describe([value / 1e6 for value in values]),
                "median_share_of_total": (
                    statistics.median(values) / statistics.median(totals) if totals else None
                ),
            }
            for name, values in samples.items()
        },
        "total_ms": {"describe": inst.describe([value / 1e6 for value in totals])},
        "repeat": repeats,
        "last_repeat_phases_ms": {name: value / 1e6 for name, value in repeat_phase.items()},
    }


def run_stages(wid, workload, files, repeats):
    timer = inst.StageTimer()
    patches = inst.PatchSet()
    applied = stage_patches(timer, patches)
    per_repeat = []
    wall = []
    try:
        for _ in range(repeats):
            timer.begin()
            t0 = inst.PERF()
            OPS[wid](files, workload.get("params") or {})
            wall.append(inst.PERF() - t0)
            per_repeat.append(timer.snapshot())
    finally:
        patches.restore()

    stages = {}
    names = sorted({name for snapshot in per_repeat for name in snapshot})
    for name in names:
        totals = [snapshot.get(name, {}).get("total_ms", 0.0) for snapshot in per_repeat]
        calls = [snapshot.get(name, {}).get("calls", 0) for snapshot in per_repeat]
        stages[name] = {
            "calls_median": statistics.median(calls),
            "total_ms_median": statistics.median(totals),
            "total_ms_samples": totals,
            "max_ms_median": statistics.median(
                snapshot.get(name, {}).get("max_ms", 0.0) for snapshot in per_repeat
            ),
        }
    return {
        "applied": applied,
        "patch_failures": patches.failures,
        "wall_ms": {"samples": [v / 1e6 for v in wall], "describe": inst.describe([v / 1e6 for v in wall])},
        "stages": dict(sorted(stages.items(), key=lambda kv: -kv[1]["total_ms_median"])),
    }


def run_counts(wid, workload, files, repeats=1, restore=False):
    """Counting pass.

    ``restore=False`` is the default and is *required* on CPython 3.14: the
    instance-creation counters patch ``cls.__new__``, and deleting that class
    attribute again does not restore the original ``tp_new`` slot (the class then
    raises ``object.__new__() takes exactly one argument`` on the next
    instantiation).  The counting pass therefore runs in its own process and
    leaves the patches applied until the interpreter exits; everything runs
    normally while the patches are *applied*.
    """
    counter = inst.BoundaryCounter()
    patches = inst.PatchSet()
    applied = count_patches(counter, patches)
    facts = []
    wall = []
    try:
        for _ in range(repeats):
            counter.counts.clear()
            counter.by_phase.clear()
            t0 = inst.PERF()
            OPS[wid](files, workload.get("params") or {}, counter=counter, facts=facts)
            wall.append(inst.PERF() - t0)
    finally:
        if restore:
            patches.restore()
    payload = counter.to_json()
    payload["applied"] = applied
    payload["patch_failures"] = patches.failures
    payload["facts"] = facts
    payload["perturbed_wall_ms"] = [value / 1e6 for value in wall]
    return payload


def run_profile(wid, workload, files, repeats):
    OPS[wid](files, workload.get("params") or {})  # warm-up outside the profile
    stats = inst.profile_callable(
        lambda: OPS[wid](files, workload.get("params") or {}), repeats=repeats
    )
    rows = inst.profile_rows(stats)
    return {
        "repeats": repeats,
        "summary": inst.stats_summary(stats),
        "rows": rows[:40],
        "classification": inst.classify_totals(rows),
        "_stats": stats,
    }


def run_per_file_stages(wid, workload, files, repeats):
    """Per-input-file stage timing (no instance-creation counters)."""
    out = {}
    for path in files:
        rel = str(path.relative_to(ROOT))
        record = {"file_bytes": path.stat().st_size}
        try:
            record["stages"] = run_stages(wid, workload, [path], repeats)
        except Exception:  # noqa: BLE001
            record["stages_error"] = traceback.format_exc()
        out[rel] = record
        print(f"  [{rel}] stages done", file=sys.stderr, flush=True)
    return out


def run_per_file_counts(wid, workload, files):
    """Per-input-file call counts; the counting patches are applied once and kept."""
    counter = inst.BoundaryCounter()
    patches = inst.PatchSet()
    applied = count_patches(counter, patches)
    out = {}
    for path in files:
        rel = str(path.relative_to(ROOT))
        counter.counts.clear()
        counter.by_phase.clear()
        record = {"file_bytes": path.stat().st_size}
        try:
            OPS[wid](files=[path], params=workload.get("params") or {}, counter=counter, facts=None)
        except Exception:  # noqa: BLE001
            record["error"] = traceback.format_exc()
        payload = counter.to_json()
        payload.pop("applied", None)
        payload.pop("patch_failures", None)
        record["counts"] = payload
        out[rel] = record
        print(f"  [{rel}] counts done", file=sys.stderr, flush=True)
    return {"applied": applied, "patch_failures": patches.failures, "files": out}


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=["rust", "reference"], required=True)
    parser.add_argument("--workloads", default="W1,W2,W5,W6,W7")
    parser.add_argument(
        "--mode",
        default="all",
        choices=["all", "layers", "phases", "stages", "counts", "profile", "perfile"],
    )
    parser.add_argument("--repeat", type=int, default=0, help="override repeat count")
    parser.add_argument("--out", required=True)
    parser.add_argument("--skip-profile", action="store_true")
    parser.add_argument(
        "--no-per-file",
        action="store_true",
        help="skip the per-input-file pass (it repeats stages+counts once per file)",
    )
    args = parser.parse_args(argv)

    backend, _ = detect_backend()
    if backend != args.backend:
        raise SystemExit(f"backend mismatch: interpreter is {backend!r}, --backend {args.backend!r}")

    workloads = load_workloads()
    result = {
        "schema": SCHEMA,
        "workloads_source": str(ROOT / "validation" / "perf" / "workloads.json"),
        "created_local": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "env": environment(),
        "workloads": {},
    }

    ordered = [wid.strip() for wid in args.workloads.split(",") if wid.strip()]
    for wid in ordered:
        workload = workloads[wid]
        files = resolve_files(workload)
        missing = [str(path) for path in files if not path.is_file()]
        if missing:
            raise SystemExit(f"{wid}: missing input files: {missing}")
        entry = {
            "name": workload["name"],
            "type": workload["type"],
            "files": workload["files"],
            "params": workload.get("params") or {},
            "file_bytes": {str(path.relative_to(ROOT)): path.stat().st_size for path in files},
        }
        default_repeat = 7 if wid in {"W1", "W5", "W6", "W7"} else 5
        repeats = args.repeat or default_repeat
        mode = args.mode
        started = time.time()

        if mode in {"all", "layers"}:
            print(f"[{wid}] warm-up", file=sys.stderr, flush=True)
            warmup(wid, workload, files)

        if mode in {"all", "phases"}:
            print(f"[{wid}] wall phases (repeat={repeats})", file=sys.stderr, flush=True)
            try:
                entry["wall_phases"] = run_wall_phases(wid, workload, files, repeats)
            except Exception:  # noqa: BLE001
                entry["wall_phases_error"] = traceback.format_exc()

        if mode in {"all", "layers"}:
            print(f"[{wid}] layers (repeat={repeats})", file=sys.stderr, flush=True)
            try:
                entry["layers"] = run_layers(wid, workload, files, repeats)
            except Exception:  # noqa: BLE001
                entry["layers_error"] = traceback.format_exc()

        if mode in {"all", "stages"}:
            print(f"[{wid}] stages (repeat={repeats})", file=sys.stderr, flush=True)
            try:
                entry["stages"] = run_stages(wid, workload, files, repeats)
            except Exception:  # noqa: BLE001
                entry["stages_error"] = traceback.format_exc()

        if mode in {"all", "stages", "perfile"} and not args.no_per_file:
            print(f"[{wid}] per-file stages (repeat={max(3, repeats // 2)})", file=sys.stderr, flush=True)
            try:
                entry.setdefault("per_file", {})
                entry["per_file"].update(
                    run_per_file_stages(wid, workload, files, max(3, repeats // 2))
                )
            except Exception:  # noqa: BLE001
                entry["per_file_stages_error"] = traceback.format_exc()

        if mode in {"all", "profile"}:
            print(f"[{wid}] profile", file=sys.stderr, flush=True)
            try:
                prof = run_profile(wid, workload, files, repeats=3)
            except Exception:  # noqa: BLE001
                entry["profile_error"] = traceback.format_exc()
            else:
                prof.pop("_stats", None)
                entry["profile"] = prof

        # The counting passes come last: they install instance-creation counters
        # that cannot be removed again on CPython 3.14, so this process must not
        # be used for further measurement afterwards.
        if mode in {"all", "counts"}:
            print(f"[{wid}] counts", file=sys.stderr, flush=True)
            try:
                entry["counts"] = run_counts(wid, workload, files)
            except Exception:  # noqa: BLE001
                entry["counts_error"] = traceback.format_exc()

        if mode in {"all", "counts", "perfile"} and not args.no_per_file:
            print(f"[{wid}] per-file counts", file=sys.stderr, flush=True)
            try:
                entry["per_file_counts"] = run_per_file_counts(wid, workload, files)
            except Exception:  # noqa: BLE001
                entry["per_file_counts_error"] = traceback.format_exc()

        entry["seconds"] = round(time.time() - started, 3)
        result["workloads"][wid] = entry

    result["env"]["loadavg_end"] = loadavg()
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8") as handle:
        json.dump(result, handle, indent=1, sort_keys=False)
    print(f"[rw_diag] wrote {out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
