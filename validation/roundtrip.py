#!/usr/bin/env python3
"""§9.1 Round-trip validation for the pypdf Rust port.

For a deterministic sample of ``validation/fixtures/manifest.csv`` this script
runs the *same* workflow in the reference implementation (``.venvs/oracle``) and
in the port (``.venvs/dev``) and records a **semantic digest** of the reopened
result (never raw PDF bytes, except for pass-through promises such as the
incremental-mode prefix and the deterministic ``/ID``).

Workflows
---------
``RW``            open -> unmodified clone -> reopen -> digest (source digest is
                  kept too, so reader and writer effects can be told apart)
``MERGE``         merge two documents at a deterministic position -> reopen
``TRANSFORM``     rotate 90 / crop / ``Transformation().scale`` on a
                  writer-owned page -> reopen
``EDIT``          outline item, named destination, page label, annotation,
                  attachment (EmbeddedFile) and metadata -> reopen
``ENCRYPT``       write with AES-256 -> reopen with the password -> digest
``INCREMENTAL``   incremental write (append-only) -> reopen -> digest

Every case runs in its own subprocess (``--run-case``), so a crash in one case
cannot abort the run, and has a per-case timeout (``--timeout``, default 120 s).
Per-case results are written to ``validation/results/roundtrip-<backend>.json``.

Determinism
-----------
The document sample is drawn with ``random.Random(seed).sample`` over the
case-id sorted manifest; each subprocess installs
``tools/oracle/prng.install_deterministic_secrets`` with a seed derived from the
case id before importing pypdf, so ``/ID``, encryption salts and uuid4-based
names are reproducible and identical in both backends.  ``PYTHONHASHSEED=0`` is
forced for the case subprocesses.

Exit status
-----------
0 when every case ran and a result was recorded (per-case failures are reported
in the JSON and printed in the summary; ``compare_roundtrip.py`` is the
parity gate), 2 for infrastructure errors (wrong interpreter, missing
manifest).  ``--strict`` turns per-case failures into exit status 1.

Usage
-----
    .venvs/oracle/bin/python validation/roundtrip.py --backend oracle
    .venvs/dev/bin/python    validation/roundtrip.py --backend dev --limit 10
    .venvs/dev/bin/python    validation/roundtrip.py --backend dev --match RW::
"""

from __future__ import annotations

import argparse
import csv
import json
import logging
import os
import random
import subprocess
import sys
import tempfile
import time
import traceback
import warnings
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import rt_common as rc  # noqa: E402

WORKFLOWS = ("RW", "MERGE", "TRANSFORM", "EDIT", "ENCRYPT", "INCREMENTAL")
DEFAULT_LIMIT = 25
DEFAULT_TIMEOUT = 120.0
MERGE_POSITIONS = ("append", "middle", "prepend")
PRODUCER_TAG = "pypdf-rust roundtrip"
ATTACHMENT_NAME = "rt-case.txt"
ATTACHMENT_PAYLOAD = b"roundtrip attachment payload\n"


# ---------------------------------------------------------------------------
# manifest / case selection
# ---------------------------------------------------------------------------
def load_manifest(path: Path = rc.MANIFEST) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        rows = [row for row in csv.DictReader(handle)]
    return sorted(rows, key=lambda row: row["case_id"])


def select_documents(rows: list[dict[str, str]], limit: int, seed: int) -> list[dict[str, str]]:
    """Deterministic sample: seeded shuffle, then sorted by case id."""
    ordered = sorted(rows, key=lambda row: row["case_id"])
    if limit and 0 < limit < len(ordered):
        picked = random.Random(seed).sample(ordered, limit)
    else:
        picked = ordered
    return sorted(picked, key=lambda row: row["case_id"])


def build_cases(documents: list[dict[str, str]], workflows: list[str]) -> list[dict[str, Any]]:
    cases: list[dict[str, Any]] = []
    for index, row in enumerate(documents):
        for workflow in workflows:
            if workflow == "MERGE":
                partner = documents[(index + 1) % len(documents)]
                position = MERGE_POSITIONS[index % len(MERGE_POSITIONS)]
                cases.append(
                    {
                        "case_id": (
                            f"MERGE@{position}::{row['case_id']}__WITH__{partner['case_id']}"
                        ),
                        "workflow": workflow,
                        "doc": row["path"],
                        "partner": partner["path"],
                        "merge_position": position,
                        "manifest_case_id": row["case_id"],
                        "partner_case_id": partner["case_id"],
                    }
                )
            else:
                cases.append(
                    {
                        "case_id": f"{workflow}::{row['case_id']}",
                        "workflow": workflow,
                        "doc": row["path"],
                        "manifest_case_id": row["case_id"],
                    }
                )
    return sorted(cases, key=lambda spec: spec["case_id"])


# ---------------------------------------------------------------------------
# child-side context
# ---------------------------------------------------------------------------
@dataclass
class CaseContext:
    spec: dict[str, Any]
    backend: str
    workdir: Path
    seed: int
    artifacts: dict[str, str] = field(default_factory=dict)
    diagnostics: dict[str, Any] = field(default_factory=dict)

    @property
    def case_id(self) -> str:
        return self.spec["case_id"]

    @property
    def workflow(self) -> str:
        return self.spec["workflow"]

    @property
    def doc_path(self) -> Path:
        return rc.ROOT / self.spec["doc"]

    @property
    def partner_path(self) -> Path:
        return rc.ROOT / self.spec["partner"]

    @property
    def case_dir(self) -> Path:
        return self.workdir / "cases" / rc.safe_name(self.case_id)

    def artifact(self, label: str, filename: str) -> Path:
        path = self.case_dir / filename
        path.parent.mkdir(parents=True, exist_ok=True)
        self.artifacts[label] = str(path)
        return path


def _write_output(ctx: CaseContext, writer, label: str, filename: str) -> Path:
    path = ctx.artifact(label, filename)
    writer.write(str(path))
    data = path.read_bytes()
    ctx.diagnostics.setdefault("outputs", {})[label] = {
        "sha256": rc.sha256_bytes(data),
        "len": len(data),
    }
    return path


def _target(
    label: str,
    digest_variant: str,
    path: Path,
    *,
    password: str | None = None,
    checks: list[str] | str = "all",
    render: bool = False,
) -> dict[str, Any]:
    return {
        "variant": label,
        "digest_variant": digest_variant,
        "file": str(path),
        "password": password,
        "checks": checks,
        "render": render,
    }


# ---------------------------------------------------------------------------
# workflows
# ---------------------------------------------------------------------------
def workflow_rw(ctx: CaseContext) -> dict[str, Any]:
    from pypdf import PdfWriter  # noqa: PLC0415

    reader, password, _ = rc.open_reader(ctx.doc_path)
    source = rc.digest_reader(reader, text="sha", layout_first_page=True)

    writer = PdfWriter(clone_from=reader)
    path = _write_output(ctx, writer, "clone", "clone.pdf")

    reopened, reopening_password, _ = rc.open_reader(path)
    output = rc.digest_reader(reopened, text="full", layout_first_page=True)

    checks = {
        "page_count_preserved": source["page_count"] == output["page_count"],
        "text_preserved": rc.text_shas(source) == rc.text_shas(output),
        "layout_text_preserved": _first_page_value(source, "layout_text_sha256")
        == _first_page_value(output, "layout_text_sha256"),
        "contents_preserved": rc.contents_shas(source) == rc.contents_shas(output),
        "images_preserved": rc.image_shas(source) == rc.image_shas(output),
        "metadata_preserved": source["metadata"] == output["metadata"],
        "page_labels_preserved": source["page_labels"] == output["page_labels"],
        "outline_preserved": source["outline"] == output["outline"],
        "named_destinations_preserved": source["named_destinations"]
        == output["named_destinations"],
        "attachments_preserved": source["attachments"] == output["attachments"],
        "acroform_preserved": source["acroform"] == output["acroform"],
        "xmp_preserved": source["xmp"] == output["xmp"],
    }
    return {
        "variants": {
            "clone": {
                "digest": output,
                "source_digest": source,
                "checks": checks,
                "open_password": password,
            }
        },
        "interop_targets": [_target("clone", "clone", path)],
        "shared": {
            "source_page_count": source["page_count"],
            "source_sha256": rc.sha256_bytes(ctx.doc_path.read_bytes()),
        },
    }


def workflow_merge(ctx: CaseContext) -> dict[str, Any]:
    from pypdf import PdfWriter  # noqa: PLC0415

    reader_a, password_a, _ = rc.open_reader(ctx.doc_path)
    reader_b, password_b, _ = rc.open_reader(ctx.partner_path)

    source_a = rc.digest_reader(reader_a, text="sha")
    source_b = rc.digest_reader(reader_b, text="sha")
    pages_a = list(reader_a.pages)
    pages_b = list(reader_b.pages)

    position = ctx.spec["merge_position"]
    insert_at = {"prepend": 0, "middle": len(pages_a) // 2, "append": len(pages_a)}[position]

    writer = PdfWriter()
    for page in pages_a:
        writer.add_page(page)
    for offset, page in enumerate(pages_b):
        writer.insert_page(page, insert_at + offset)

    path = _write_output(ctx, writer, "merge", "merge.pdf")
    reopened, _, _ = rc.open_reader(path)
    output = rc.digest_reader(reopened, text="full")

    expected_text = (
        rc.text_shas(source_a)[:insert_at]
        + rc.text_shas(source_b)
        + rc.text_shas(source_a)[insert_at:]
    )
    expected_contents = (
        rc.contents_shas(source_a)[:insert_at]
        + rc.contents_shas(source_b)
        + rc.contents_shas(source_a)[insert_at:]
    )
    checks = {
        "page_count_matches": output["page_count"] == len(pages_a) + len(pages_b),
        "text_order_matches": rc.text_shas(output) == expected_text,
        "contents_order_matches": rc.contents_shas(output) == expected_contents,
        "insert_position_matches": _page_sha_at(output, insert_at)
        == _page_sha_at(source_b, 0)
        if pages_b
        else True,
    }
    return {
        "variants": {
            "merge": {
                "digest": output,
                "checks": checks,
                "open_password": password_a,
                "merge": {
                    "position": position,
                    "insert_at": insert_at,
                    "pages_a": len(pages_a),
                    "pages_b": len(pages_b),
                },
            }
        },
        "interop_targets": [_target("merge", "merge", path)],
        "shared": {
            "source_a_pages": len(pages_a),
            "source_b_pages": len(pages_b),
            "partner_password": password_b,
        },
    }


def _crop_rect(mediabox: list[float]) -> list[float]:
    width = float(mediabox[2]) - float(mediabox[0])
    height = float(mediabox[3]) - float(mediabox[1])
    inset = min(36.0, max(width, 1.0) / 10.0, max(height, 1.0) / 10.0)
    return [
        rc.num(float(mediabox[0]) + inset),
        rc.num(float(mediabox[1]) + inset),
        rc.num(float(mediabox[2]) - inset),
        rc.num(float(mediabox[3]) - inset),
    ]


def workflow_transform(ctx: CaseContext) -> dict[str, Any]:
    from pypdf import PdfWriter, Transformation  # noqa: PLC0415
    from pypdf.generic import RectangleObject  # noqa: PLC0415

    reader, password, _ = rc.open_reader(ctx.doc_path)
    source = rc.digest_reader(reader, text="sha", text_pages=3, layout_first_page=True)
    source_page = source["pages"][0] if source["pages"] else {}

    variants: dict[str, Any] = {}
    targets: list[dict[str, Any]] = []
    for label in ("rotate90", "crop", "scale"):
        writer = PdfWriter()
        page = writer.add_page(reader.pages[0])
        expected: dict[str, Any] = {}
        if label == "rotate90":
            page.rotate(90)
            expected["rotation"] = 90
        elif label == "crop":
            rect = _crop_rect([float(value) for value in page.mediabox])
            page.cropbox = RectangleObject(rect)
            expected["cropbox"] = rect
        else:
            page.add_transformation(Transformation().scale(0.5, 0.5))
            expected["scale"] = [0.5, 0.5]

        path = _write_output(ctx, writer, label, f"{label}.pdf")
        reopened, _, _ = rc.open_reader(path)
        output = rc.digest_reader(
            reopened, text="full", text_pages=3, layout_first_page=True
        )
        output_page = output["pages"][0] if output["pages"] else {}
        checks = {
            "page_count_matches": output["page_count"] == 1,
            "text_preserved": output_page.get("text_sha256")
            == source_page.get("text_sha256"),
            "rotation_ok": output_page.get("rotation")
            == (90 if label == "rotate90" else source_page.get("rotation")),
            "cropbox_ok": output_page.get("cropbox") == expected["cropbox"]
            if label == "crop"
            else True,
            "content_ops_present": bool((output_page.get("contents") or {}).get("len")),
        }
        variants[label] = {
            "digest": output,
            "checks": checks,
            "open_password": password,
            "transform": label,
            "expected": expected,
        }
        targets.append(_target(label, label, path, render=True))

    return {
        "variants": variants,
        "interop_targets": targets,
        "shared": {"source_digest": source},
    }


def workflow_edit(ctx: CaseContext) -> dict[str, Any]:
    from pypdf import PdfWriter  # noqa: PLC0415
    from pypdf.annotations import FreeText  # noqa: PLC0415
    from pypdf.constants import PageLabelStyle  # noqa: PLC0415

    reader, password, _ = rc.open_reader(ctx.doc_path)
    page_count = len(reader.pages)
    source = rc.digest_reader(reader, text="sha", text_pages=2)

    variants: dict[str, Any] = {}
    targets: list[dict[str, Any]] = []

    def run_variant(
        label: str,
        apply_fn: Callable[[Any], None],
        check_fn: Callable[[Any, dict[str, Any]], bool],
    ) -> None:
        writer = PdfWriter(clone_from=reader)
        apply_fn(writer)
        path = _write_output(ctx, writer, label, f"{label}.pdf")
        reopened, _, _ = rc.open_reader(path)
        output = rc.digest_reader(reopened, text="full", text_pages=2)
        try:
            applied = bool(check_fn(reopened, output))
        except Exception as exc:  # noqa: BLE001 - a broken edit is a failed check
            applied = False
            ctx.diagnostics.setdefault("check_errors", {})[label] = (
                f"{type(exc).__name__}: {exc}"
            )
        checks = {
            "page_count_preserved": output["page_count"] == page_count,
            "text_preserved": rc.text_shas(source) == rc.text_shas(output),
            "edit_applied": applied,
        }
        variants[label] = {
            "digest": output,
            "checks": checks,
            "open_password": password,
        }

    def add_outline(writer) -> None:
        parent = writer.add_outline_item(title="RT Outline", page_number=0)
        writer.add_outline_item(
            title="RT Outline Child",
            page_number=min(1, page_count - 1),
            parent=parent,
        )

    def check_outline(_reader, digest: dict[str, Any]) -> bool:
        # Documents may already have an outline; the new item is appended at the
        # end of the top-level tree with its child directly after it, so look
        # for the pair instead of comparing the whole outline.
        outline = digest["outline"]
        if not isinstance(outline, list):
            return False
        titles = [entry.get("title") for entry in outline]
        if "RT Outline" not in titles:
            return False
        index = titles.index("RT Outline")
        if index + 1 >= len(outline):
            return False
        parent = outline[index]
        child = outline[index + 1]
        return (
            child.get("title") == "RT Outline Child"
            and child.get("level") == (parent.get("level") or 0) + 1
            and parent.get("page") == 0
            and child.get("page") == min(1, page_count - 1)
        )

    run_variant("outline", add_outline, check_outline)

    run_variant(
        "named_destination",
        lambda writer: writer.add_named_destination("RT Destination", 0),
        lambda _reader, digest: digest["named_destinations"]
        .get("RT Destination", {})
        .get("page")
        == 0,
    )

    run_variant(
        "page_label",
        lambda writer: writer.set_page_label(
            0,
            page_count - 1,
            style=PageLabelStyle.DECIMAL,
            prefix="RT-",
            start=1,
        ),
        lambda _reader, digest: digest["page_labels"]
        == [f"RT-{index + 1}" for index in range(page_count)],
    )

    run_variant(
        "annotation",
        lambda writer: writer.add_annotation(
            0, FreeText(text="RT annotation", rect=(100, 700, 300, 760))
        ),
        lambda _reader, digest: _has_annotation(
            digest, subtype="/FreeText", contents="RT annotation"
        ),
    )

    run_variant(
        "attachment",
        lambda writer: writer.add_attachment(ATTACHMENT_NAME, ATTACHMENT_PAYLOAD),
        lambda _reader, digest: digest["attachments"].get(ATTACHMENT_NAME)
        == [{"len": len(ATTACHMENT_PAYLOAD), "sha256": rc.sha256_bytes(ATTACHMENT_PAYLOAD)}],
    )

    run_variant(
        "metadata",
        lambda writer: writer.add_metadata(
            {"/Title": "RT Title", "/Subject": "Round-trip", "/Keywords": "pypdf validation"}
        ),
        lambda _reader, digest: digest["metadata"].get("/Title") == "RT Title",
    )

    # The attachment variant is structurally the richest; use it for the
    # independent-tool checks to keep interop runtime bounded.
    targets.append(
        _target("attachment", "attachment", Path(ctx.artifacts["attachment"]))
    )
    return {
        "variants": variants,
        "interop_targets": targets,
        "shared": {"page_count": page_count},
    }


def workflow_encrypt(ctx: CaseContext) -> dict[str, Any]:
    from pypdf import PdfReader, PdfWriter  # noqa: PLC0415

    reader, password, _ = rc.open_reader(ctx.doc_path)
    source = rc.digest_reader(reader, text="sha")

    writer = PdfWriter(clone_from=reader)
    writer.encrypt(user_password="user", owner_password="owner", algorithm="AES-256")
    encrypted_path = _write_output(ctx, writer, "aes256", "aes256.pdf")

    reopened, used_password, _ = rc.open_reader(encrypted_path, candidates=["user"])
    output = rc.digest_reader(reopened, text="full")

    probes: dict[str, str] = {}
    for label, wrong_password in (("wrong", "wrong-password"), ("empty", "")):
        try:
            probe = PdfReader(str(encrypted_path), password=wrong_password)
            len(probe.pages)
            probes[label] = "NO_ERROR"
        except Exception as exc:  # noqa: BLE001 - recorded as the observed type
            probes[label] = type(exc).__name__

    security: dict[str, Any] = {}
    try:
        encryption_dict = reopened.trailer["/Encrypt"].get_object()
        for key in ("/Filter", "/V", "/R", "/Length", "/P", "/StrF", "/StmF"):
            if key in encryption_dict:
                security[key] = rc._simple(encryption_dict[key])
        security["/CF"] = sorted(str(key) for key in (encryption_dict.get("/CF") or {}))
    except Exception as exc:  # noqa: BLE001
        security = {"error": f"{type(exc).__name__}: {exc}"}

    checks = {
        "encrypted_output": bool(reopened.is_encrypted),
        "page_count_preserved": output["page_count"] == source["page_count"],
        "text_preserved": rc.text_shas(source) == rc.text_shas(output),
        "metadata_preserved": source["metadata"] == output["metadata"],
        "aes256_params": (
            security.get("/V") == 5
            and security.get("/R") == 6
            and security.get("/Length") == 256
        ),
        "wrong_password_rejected": probes.get("wrong") == "WrongPasswordError",
        "empty_password_rejected": probes.get("empty") == "WrongPasswordError",
    }

    # A decrypted copy is needed by the independent tools (pdftotext/mutool do
    # not all accept the same password flags); it is still produced by the
    # backend under test and checked with the same semantic digest.
    plain_writer = PdfWriter(clone_from=reopened)
    plain_path = _write_output(ctx, plain_writer, "aes256_plain", "aes256-decrypted.pdf")

    targets = [
        _target("aes256_plain", "aes256", plain_path),
        _target(
            "aes256",
            "aes256",
            encrypted_path,
            password=used_password,
            checks=["qpdf_check"],
        ),
    ]
    return {
        "variants": {
            "aes256": {
                "digest": output,
                "checks": checks,
                "open_password": used_password,
                "security": security,
                "wrong_password_probes": probes,
            }
        },
        "interop_targets": targets,
        "shared": {
            "source_page_count": source["page_count"],
            "source_was_encrypted": bool(reader.is_encrypted),
            "source_open_password": password,
        },
    }


def workflow_incremental(ctx: CaseContext) -> dict[str, Any]:
    from pypdf import PdfWriter  # noqa: PLC0415

    reader, password, _ = rc.open_reader(ctx.doc_path)
    source = rc.digest_reader(reader, text="sha")

    if reader.is_encrypted:
        return {
            "status": "skipped",
            "skip_reason": "encrypted_source_not_supported_by_incremental_writer",
            "variants": {},
            "interop_targets": [],
            "shared": {"source_page_count": source["page_count"]},
        }

    original = ctx.doc_path.read_bytes()
    path = ctx.artifact("append", "incremental.pdf")
    path.write_bytes(original)

    writer = PdfWriter(str(path), incremental=True)
    writer.add_blank_page(200, 300)
    writer.add_metadata({"/Producer": PRODUCER_TAG})
    writer.write(str(path))
    updated = path.read_bytes()
    ctx.diagnostics.setdefault("outputs", {})["append"] = {
        "sha256": rc.sha256_bytes(updated),
        "len": len(updated),
    }

    reopened, _, _ = rc.open_reader(path)
    output = rc.digest_reader(reopened, text="full")
    last_page = output["pages"][-1] if output["pages"] else {}
    checks = {
        "page_count_incremented": output["page_count"] == source["page_count"] + 1,
        "original_prefix_preserved": updated.startswith(original),
        "text_preserved": rc.text_shas(output)[: len(rc.text_shas(source))]
        == rc.text_shas(source),
        "metadata_updated": output["metadata"].get("/Producer") == PRODUCER_TAG,
        "page_labels_preserved": _labels_prefix_preserved(source, output),
        "outline_preserved": output["outline"] == source["outline"],
        "appended_page_geometry": last_page.get("mediabox") == [0.0, 0.0, 200.0, 300.0],
    }
    return {
        "variants": {
            "append": {
                "digest": output,
                "checks": checks,
                "open_password": password,
                "bytes_before": len(original),
                "bytes_after": len(updated),
            }
        },
        "interop_targets": [_target("append", "append", path)],
        "shared": {
            "source_page_count": source["page_count"],
            "source_sha256": rc.sha256_bytes(original),
        },
    }


WORKFLOW_FUNCS: dict[str, Callable[[CaseContext], dict[str, Any]]] = {
    "RW": workflow_rw,
    "MERGE": workflow_merge,
    "TRANSFORM": workflow_transform,
    "EDIT": workflow_edit,
    "ENCRYPT": workflow_encrypt,
    "INCREMENTAL": workflow_incremental,
}


# ---------------------------------------------------------------------------
# small digest helpers
# ---------------------------------------------------------------------------
def _first_page_value(digest: dict[str, Any], key: str) -> Any:
    pages = digest.get("pages") or []
    return pages[0].get(key) if pages else None


def _page_sha_at(digest: dict[str, Any], index: int) -> Any:
    pages = digest.get("pages") or []
    if index < 0 or index >= len(pages):
        return None
    return pages[index].get("text_sha256")


def _has_annotation(digest: dict[str, Any], subtype: str, contents: str) -> bool:
    pages = digest.get("pages") or []
    for annotation in pages[0].get("annotations", []) if pages else []:
        if annotation.get("subtype") == subtype and annotation.get("contents") == contents:
            return True
    return False


def _labels_prefix_preserved(source: dict[str, Any], output: dict[str, Any]) -> bool:
    """Incremental append may extend default/auto page labels by one page.

    Only the labels of the pre-existing pages are required to be preserved;
    the appended page legitimately gets a new label (or none when an explicit
    /PageLabels tree does not cover it).
    """
    old = source.get("page_labels")
    new = output.get("page_labels")
    if not isinstance(old, list) or not isinstance(new, list):
        return old == new
    return new[: len(old)] == old


# ---------------------------------------------------------------------------
# diagnostics capture (child side)
# ---------------------------------------------------------------------------
class _LogCapture(logging.Handler):
    def __init__(self) -> None:
        super().__init__(level=logging.NOTSET)
        self.records: list[dict[str, Any]] = []

    def emit(self, record: logging.LogRecord) -> None:
        try:
            message = record.getMessage()
        except Exception:  # noqa: BLE001
            message = f"<unformattable: {record.msg!r}>"
        self.records.append(
            {"logger": record.name, "level": record.levelname, "msg": message[:1000]}
        )


def _warning_summary(caught: list[Any]) -> dict[str, Any]:
    by_category: dict[str, int] = {}
    for warning in caught:
        category = warning.category.__name__
        by_category[category] = by_category.get(category, 0) + 1
    return {"count": len(caught), "by_category": by_category}


def _flatten_checks(result: dict[str, Any]) -> dict[str, bool]:
    flat: dict[str, bool] = {}
    for label, variant in sorted(result.get("variants", {}).items()):
        for name, value in sorted((variant.get("checks") or {}).items()):
            flat[f"variants.{label}.checks.{name}"] = value is True
    return flat


# ---------------------------------------------------------------------------
# child entry point
# ---------------------------------------------------------------------------
def child_main(args: argparse.Namespace) -> int:
    backend = rc.detect_backend()
    spec = json.loads(Path(args.case_spec).read_text(encoding="utf-8"))
    if backend != args.backend:
        print(
            f"error: interpreter provides backend {backend!r}, "
            f"but --backend {args.backend!r} was requested "
            f"(pypdf module: {rc.backend_info().get('module')})",
            file=sys.stderr,
        )
        return 3

    workdir = Path(args.workdir)
    seed = rc.seed_for(spec["case_id"], args.seed)
    rc.install_deterministic(seed)

    ctx = CaseContext(spec=spec, backend=backend, workdir=workdir, seed=seed)
    capture = _LogCapture()
    root_logger = logging.getLogger()
    root_logger.addHandler(capture)
    root_logger.setLevel(logging.DEBUG)

    started = time.time()
    status = "ok"
    error: dict[str, Any] | None = None
    payload: dict[str, Any] = {}
    caught_warnings: list[Any] = []
    try:
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            payload = WORKFLOW_FUNCS[spec["workflow"]](ctx)
            caught_warnings = list(caught)
        status = payload.get("status", "ok")
    except rc.OpenFailure as exc:
        status = "error"
        error = {
            "type": type(exc).__name__,
            "msg": rc.sanitize_message(str(exc), ctx.case_dir),
            "attempts": exc.attempts,
        }
    except BaseException as exc:  # noqa: BLE001 - one case must not abort the run
        status = "error"
        error = {
            "type": type(exc).__name__,
            "msg": rc.sanitize_message(str(exc), ctx.case_dir),
            "traceback": rc.sanitize_message(
                traceback.format_exc().splitlines()[-12:], ctx.case_dir
            ),
        }
    finally:
        root_logger.removeHandler(capture)

    variants = payload.get("variants", {})
    result: dict[str, Any] = {
        "case_id": spec["case_id"],
        "workflow": spec["workflow"],
        "backend": backend,
        "status": status,
        "error": error,
        "skip_reason": payload.get("skip_reason"),
        "input": {
            "doc": spec["doc"],
            "manifest_case_id": spec.get("manifest_case_id"),
            "partner": spec.get("partner"),
            "partner_case_id": spec.get("partner_case_id"),
            "merge_position": spec.get("merge_position"),
        },
        "variants": variants,
        "shared": payload.get("shared"),
        "interop_targets": payload.get("interop_targets", []),
        "artifacts": ctx.artifacts,
        "warnings": _warning_summary(caught_warnings),
        "diagnostics": {
            **ctx.diagnostics,
            "log_records": capture.records[:40],
            "log_count": len(capture.records),
            "warning_messages": [
                {
                    "category": warning.category.__name__,
                    "message": rc.sanitize_message(str(warning.message), ctx.case_dir)[:1000],
                }
                for warning in caught_warnings[:40]
            ],
            "seed": seed,
            "workdir": str(workdir),
        },
        "duration_ms": int((time.time() - started) * 1000),
    }
    rc.write_json(Path(args.case_out), result)
    return 0


# ---------------------------------------------------------------------------
# parent entry point
# ---------------------------------------------------------------------------
def _case_failed(result: dict[str, Any]) -> tuple[bool, list[str]]:
    if result.get("status") != "ok":
        return True, []
    flat = _flatten_checks(result)
    failures = [path for path, ok in flat.items() if not ok]
    return bool(failures), failures


def run_case(
    spec: dict[str, Any],
    *,
    backend: str,
    workdir: Path,
    seed: int,
    timeout: float,
    case_spec_path: Path,
) -> dict[str, Any]:
    rc.write_json(case_spec_path, spec)
    case_out = workdir / "cases" / rc.safe_name(spec["case_id"]) / "case.json"
    case_out.parent.mkdir(parents=True, exist_ok=True)
    case_out.unlink(missing_ok=True)

    env = dict(os.environ)
    env["PYTHONHASHSEED"] = "0"
    env["PYTHONIOENCODING"] = "utf-8"

    started = time.time()
    timed_out = False
    try:
        completed = subprocess.run(
            [
                sys.executable,
                str(Path(__file__).resolve()),
                "--run-case",
                "--case-spec",
                str(case_spec_path),
                "--case-out",
                str(case_out),
                "--backend",
                backend,
                "--seed",
                str(seed),
                "--workdir",
                str(workdir),
            ],
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env,
        )
        stdout, stderr, returncode = completed.stdout, completed.stderr, completed.returncode
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        stdout = exc.stdout.decode() if isinstance(exc.stdout, bytes) else (exc.stdout or "")
        stderr = exc.stderr.decode() if isinstance(exc.stderr, bytes) else (exc.stderr or "")
        returncode = "timeout"

    wall_ms = int((time.time() - started) * 1000)
    stderr_tail = rc.sanitize_message(stderr[-4000:], case_out.parent)
    stdout_tail = stdout[-2000:]

    if case_out.exists():
        result = rc.read_json(case_out)
    else:
        result = {
            "case_id": spec["case_id"],
            "workflow": spec["workflow"],
            "backend": backend,
            "status": "error",
            "error": {
                "type": "ProcessTimeout" if timed_out else "ProcessCrash",
                "msg": (
                    f"case process exceeded {timeout:g}s"
                    if timed_out
                    else f"case process exited with {returncode} and wrote no result"
                ),
            },
            "skip_reason": None,
            "input": {"doc": spec["doc"]},
            "variants": {},
            "shared": None,
            "interop_targets": [],
            "artifacts": {},
            "warnings": {"count": 0, "by_category": {}},
            "diagnostics": {},
        }
    result["wall_ms"] = wall_ms
    result["process"] = {
        "returncode": returncode,
        "timed_out": timed_out,
        "stdout_tail": stdout_tail,
        "stderr_tail": stderr_tail,
    }
    return result


def summarize(results: list[dict[str, Any]]) -> dict[str, Any]:
    by_workflow: dict[str, dict[str, Any]] = {}
    failures: list[dict[str, Any]] = []
    for result in results:
        workflow = result.get("workflow", "?")
        bucket = by_workflow.setdefault(
            workflow,
            {"total": 0, "passed": 0, "failed": 0, "skipped": 0, "check_failures": 0},
        )
        bucket["total"] += 1
        if result.get("status") == "skipped":
            bucket["skipped"] += 1
            continue
        failed, check_failures = _case_failed(result)
        if failed:
            bucket["failed"] += 1
            bucket["check_failures"] += len(check_failures)
            entry: dict[str, Any] = {
                "case_id": result["case_id"],
                "workflow": workflow,
                "status": result.get("status"),
            }
            if result.get("error"):
                entry["error"] = {
                    "type": result["error"].get("type"),
                    "msg": result["error"].get("msg"),
                }
            if check_failures:
                entry["failed_checks"] = check_failures
            failures.append(entry)
        else:
            bucket["passed"] += 1
    total = len(results)
    passed = sum(bucket["passed"] for bucket in by_workflow.values())
    failed = sum(bucket["failed"] for bucket in by_workflow.values())
    skipped = sum(bucket["skipped"] for bucket in by_workflow.values())
    return {
        "total": total,
        "passed": passed,
        "failed": failed,
        "skipped": skipped,
        "by_workflow": by_workflow,
        "failures": failures,
    }


def print_summary(payload: dict[str, Any]) -> None:
    summary = payload["summary"]
    print()
    print(f"backend={payload['backend']} pypdf={payload['pypdf']['version']} "
          f"module={payload['pypdf']['module']}")
    print(f"corpus={payload['manifest_sha256'][:16]} seed={payload['seed']} "
          f"limit={payload['limit']} workdir={payload['workdir']}")
    print()
    print(f"{'workflow':<14}{'total':>7}{'passed':>8}{'failed':>8}{'skipped':>9}")
    for workflow in WORKFLOWS:
        bucket = summary["by_workflow"].get(workflow)
        if not bucket:
            continue
        print(
            f"{workflow:<14}{bucket['total']:>7}{bucket['passed']:>8}"
            f"{bucket['failed']:>8}{bucket['skipped']:>9}"
        )
    print(
        f"{'TOTAL':<14}{summary['total']:>7}{summary['passed']:>8}"
        f"{summary['failed']:>8}{summary['skipped']:>9}"
    )
    if summary["failures"]:
        print()
        print(f"FAILED cases ({len(summary['failures'])}):")
        for failure in summary["failures"][:60]:
            detail = ""
            if failure.get("error"):
                detail = f" {failure['error']['type']}: {str(failure['error']['msg'])[:160]}"
            elif failure.get("failed_checks"):
                detail = f" checks={failure['failed_checks'][:4]}"
            print(f"  - {failure['case_id']}{detail}")
        if len(summary["failures"]) > 60:
            print(f"  ... and {len(summary['failures']) - 60} more (see JSON)")


def parent_main(args: argparse.Namespace) -> int:
    backend = rc.detect_backend()
    if backend != args.backend:
        print(
            f"error: this interpreter provides backend {backend!r}, "
            f"but --backend {args.backend!r} was requested; run the script with "
            f".venvs/{args.backend}/bin/python",
            file=sys.stderr,
        )
        return 2
    if not rc.MANIFEST.exists():
        print(f"error: manifest not found: {rc.MANIFEST}", file=sys.stderr)
        return 2

    rows = load_manifest()
    documents = select_documents(rows, args.limit, args.seed)
    workflows = [item.strip().upper() for item in args.workflows.split(",") if item.strip()]
    unknown = [item for item in workflows if item not in WORKFLOWS]
    if unknown:
        print(f"error: unknown workflows {unknown}; choose from {WORKFLOWS}", file=sys.stderr)
        return 2
    cases = build_cases(documents, workflows)
    if args.match:
        cases = [spec for spec in cases if args.match in spec["case_id"]]
    if not cases:
        print("error: no cases selected", file=sys.stderr)
        return 2

    if args.workdir:
        workdir = Path(args.workdir).resolve()
    else:
        workdir = Path(tempfile.mkdtemp(prefix=f"pypdf-rt-{args.backend}-"))
    (workdir / "specs").mkdir(parents=True, exist_ok=True)
    (workdir / "cases").mkdir(parents=True, exist_ok=True)

    pypdf_info = rc.backend_info(backend)
    started = time.time()
    results: list[dict[str, Any]] = []
    print(
        f"roundtrip: backend={backend} cases={len(cases)} seed={args.seed} "
        f"timeout={args.timeout:g}s workdir={workdir}",
        flush=True,
    )
    for index, spec in enumerate(cases, 1):
        case_spec_path = workdir / "specs" / f"{rc.safe_name(spec['case_id'])}.spec.json"
        result = run_case(
            spec,
            backend=backend,
            workdir=workdir,
            seed=args.seed,
            timeout=args.timeout,
            case_spec_path=case_spec_path,
        )
        results.append(result)
        failed, check_failures = _case_failed(result)
        status = result.get("status", "?")
        marker = "FAIL" if failed else status.upper()
        detail = ""
        if result.get("error"):
            detail = f" {result['error'].get('type')}: {str(result['error'].get('msg'))[:120]}"
        elif check_failures:
            detail = f" checks={check_failures[:3]}"
        print(
            f"[{index:>3}/{len(cases)}] {marker:<7} {result['case_id']} "
            f"({result.get('wall_ms', 0)} ms){detail}",
            flush=True,
        )

    summary = summarize(results)
    payload = {
        "generator": "validation/roundtrip.py",
        "backend": backend,
        "pypdf": pypdf_info,
        "started_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "duration_s": round(time.time() - started, 3),
        "seed": args.seed,
        "limit": args.limit,
        "timeout_s": args.timeout,
        "workdir": str(workdir),
        "manifest": str(rc.MANIFEST.relative_to(rc.ROOT)),
        "manifest_sha256": rc.sha256_bytes(rc.MANIFEST.read_bytes()),
        "normalizations": rc.NORMALIZATIONS,
        "training_selection": [row["case_id"] for row in documents],
        "workflows": workflows,
        "selection": [spec["case_id"] for spec in cases],
        "cases": results,
        "summary": summary,
    }
    out_path = Path(args.out) if args.out else rc.RESULTS / f"roundtrip-{backend}.json"
    rc.write_json(out_path, payload)
    print_summary(payload)
    print(f"\nresults -> {out_path}")
    if args.strict and summary["failed"]:
        return 1
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--backend", required=True, choices=["oracle", "dev"])
    parser.add_argument("--limit", type=int, default=DEFAULT_LIMIT)
    parser.add_argument("--seed", type=int, default=rc.DEFAULT_SEED)
    parser.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT)
    parser.add_argument("--out", default=None)
    parser.add_argument("--workdir", default=None)
    parser.add_argument("--workflows", default=",".join(WORKFLOWS))
    parser.add_argument("--match", default=None)
    parser.add_argument("--strict", action="store_true")
    # Internal: run one case in this process.
    parser.add_argument("--run-case", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--case-spec", default=None, help=argparse.SUPPRESS)
    parser.add_argument("--case-out", default=None, help=argparse.SUPPRESS)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if args.run_case:
        if not args.case_spec or not args.case_out or not args.workdir:
            print("error: --run-case needs --case-spec/--case-out/--workdir", file=sys.stderr)
            return 2
        return child_main(args)
    return parent_main(args)


if __name__ == "__main__":
    raise SystemExit(main())
