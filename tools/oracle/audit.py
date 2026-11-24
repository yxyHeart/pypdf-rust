#!/usr/bin/env python3
"""Determinism audit / scenario matrix for the pypdf oracle.

Runs a fixed matrix of read and write scenarios against the reference
implementation (pypdf 7.1.1) and dumps canonical results.  Running the same
matrix twice with different ``PYTHONHASHSEED`` values must produce identical
output; any difference is a source of non-determinism that has to be normalised
(or eliminated) before writer output can be compared byte-for-byte.

The same scenario semantics are mirrored by the Rust ``pypdf-oracle`` binary.

Usage:
    .venvs/oracle/bin/python tools/oracle/audit.py --out .audit/a --seed 1
    PYTHONHASHSEED=7 .venvs/oracle/bin/python tools/oracle/audit.py --out .audit/b --seed 1
    .venvs/oracle/bin/python tools/oracle/compare.py .audit/a .audit/b
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import logging
import sys
import traceback
import warnings
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))

from canon import canon  # noqa: E402
from prng import DEFAULT_SEED, install_deterministic_secrets  # noqa: E402

RESOURCES = ROOT / "resources"
SAMPLE_ROOT = ROOT / "sample-files"

LOGGER_NAMES = ["pypdf"]


# ---------------------------------------------------------------------------
# capture helpers
# ---------------------------------------------------------------------------
class _LogCapture(logging.Handler):
    def __init__(self) -> None:
        super().__init__(level=logging.NOTSET)
        self.records: list[dict[str, object]] = []

    def emit(self, record: logging.LogRecord) -> None:
        try:
            msg = record.getMessage()
        except Exception:  # noqa: BLE001
            msg = f"<unformattable: {record.msg!r}>"
        self.records.append(
            {
                "logger": record.name,
                "level": record.levelname,
                "msg": msg,
                "args": canon(record.args) if record.args else None,
            }
        )


def _digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _resource_names() -> list[str]:
    names = sorted(p.name for p in RESOURCES.glob("*.pdf"))
    return names


def _encryption_names() -> list[str]:
    return sorted(p.name for p in (RESOURCES / "encryption").glob("*.pdf"))


PASSWORDS = {
    "r2-empty-password.pdf": "",
    "r2-owner-password.pdf": "asdfzxcv",
    "r2-user-password.pdf": "asdfzxcv",
    "r3-empty-password.pdf": "",
    "r3-user-password.pdf": "asdfzxcv",
    "r4-aes-user-password.pdf": "asdfzxcv",
    "r4-aes-v2-no-key-length.pdf": "asdfzxcv",
    "r4-owner-password.pdf": "asdfzxcv",
    "r4-user-password.pdf": "asdfzxcv",
    "r5-empty-password.pdf": "",
    "r5-owner-password.pdf": "asdfzxcv",
    "r5-user-password.pdf": "asdfzxcv",
    "r6-both-passwords.pdf": "foo",
    "r6-cleartext-metadata.pdf": "foo",
    "r6-empty-password.pdf": "",
    "r6-owner-password.pdf": "asdfzxcv",
    "r6-user-password.pdf": "asdfzxcv",
    "unencrypted.pdf": "",
}

TEXT_PDFS = [
    "crazyones.pdf",
    "example.pdf",
    "fontsampler.pdf",
    "AutoCad_Diagram.pdf",
    "AutoCad_Simple.pdf",
    "inkscape-abc.pdf",
    "GeoBase_NHNC1_Data_Model_UML_EN.pdf",
    "form.pdf",
    "form_evince.pdf",
    "attachment.pdf",
    "git.pdf",
    "hello-world.pdf",
]


# ---------------------------------------------------------------------------
# read scenarios
# ---------------------------------------------------------------------------
def read_document(path: Path, password: str | None = None, *, text: bool = False) -> dict:
    from pypdf import PdfReader

    reader = PdfReader(str(path), strict=False, password=password)
    info: dict = {
        "is_encrypted": reader.is_encrypted,
        "metadata": canon(dict(reader.metadata or {})),
        "trailer_keys": canon(sorted(str(k) for k in reader.trailer.keys())),
        "num_pages": len(reader.pages),
        "page_labels": canon(reader.page_labels) if len(reader.pages) else None,
        "outline": canon(reader.outline),
        "attachments": canon(list(reader.attachments.keys())),
        "xmp_meta": (
            canon(reader.xmp_metadata.rdf_root.toxml()) if reader.xmp_metadata else None
        ),
    }
    try:
        info["form_fields"] = canon(
            {k: canon(v) for k, v in reader.get_fields().items()}  # type: ignore[union-attr]
            if reader.get_fields()
            else None
        )
    except Exception as exc:  # noqa: BLE001
        info["form_fields"] = {"error": type(exc).__name__, "msg": str(exc)}

    pages = []
    for index, page in enumerate(reader.pages[:4]):
        entry = {
            "index": index,
            "mediabox": canon(page.mediabox),
            "cropbox": canon(page.cropbox),
            "rotation": canon(page.rotation),
            "annotations": canon(list(page.get("/Annots") or [])),
            "images": canon(
                [
                    (img.name, img.image.size, img.image.mode)
                    for img in page.images
                ]
            ),
        }
        if text:
            entry["text"] = page.extract_text()
            entry["text_layout"] = page.extract_text(extraction_mode="layout")
            entry["fonts"] = canon(page._get_fonts())  # noqa: SLF001
        pages.append(entry)
    info["pages"] = pages
    return info


# ---------------------------------------------------------------------------
# write scenarios
# ---------------------------------------------------------------------------
def writer_scenarios() -> list[tuple[str, object]]:
    """Return ``(name, callable)`` pairs; every callable returns output digests."""
    from pypdf import PdfReader, PdfWriter

    def dump(writer) -> dict:
        buf = io.BytesIO()
        writer.write(buf)
        data = buf.getvalue()
        return {
            "sha256": _digest(data),
            "length": len(data),
            "head": data[:32].hex(),
            "tail": data[-64:].hex(),
        }

    scenarios: list[tuple[str, object]] = []

    def scenario(name: str, func) -> None:
        scenarios.append((name, func))

    def clone_crazyones() -> dict:
        reader = PdfReader(str(RESOURCES / "crazyones.pdf"))
        return dump(PdfWriter(clone_from=reader))

    scenario("w01_clone_crazyones", clone_crazyones)

    def merge_two() -> dict:
        writer = PdfWriter()
        for name in ("crazyones.pdf", "example.pdf"):
            for page in PdfReader(str(RESOURCES / name)).pages:
                writer.add_page(page)
        return dump(writer)

    scenario("w02_merge_two", merge_two)

    def blank_outline_meta() -> dict:
        writer = PdfWriter()
        for _ in range(3):
            writer.add_blank_page(200, 300)
        for index, title in enumerate(["Cover", "Body", "End"]):
            writer.add_outline_item(title=title, page_number=index)
        writer.metadata = None
        writer.add_metadata({"/Title": "audit", "/Producer": "pypdf"})
        return dump(writer)

    scenario("w03_blank_outline_meta", blank_outline_meta)

    def compress_identical() -> dict:
        writer = PdfWriter(clone_from=PdfReader(str(RESOURCES / "crazyones.pdf")))
        writer.compress_identical_objects()
        return dump(writer)

    scenario("w04_compress_identical", compress_identical)

    for algo, kwargs in [
        ("r2", {"use_128bit": False}),
        ("r3", {"use_128bit": True}),
        ("r4_rc4", {"algorithm": "RC4-128"}),
        ("r4_aes", {"algorithm": "AES-128"}),
        ("r5", {"algorithm": "AES-256-R5"}),
        ("r6", {"algorithm": "AES-256"}),
    ]:
        def encrypt_case(kwargs=kwargs) -> dict:
            writer = PdfWriter(clone_from=PdfReader(str(RESOURCES / "crazyones.pdf")))
            writer.encrypt(user_password="user", owner_password="owner", **kwargs)
            return dump(writer)

        scenario(f"w05_encrypt_{algo}", encrypt_case)

    def extract_pages() -> dict:
        writer = PdfWriter()
        for page in PdfReader(str(RESOURCES / "crazyones.pdf")).pages[:2]:
            writer.add_page(page)
        return dump(writer)

    scenario("w06_extract_pages", extract_pages)

    def transform() -> dict:
        from pypdf import Transformation

        writer = PdfWriter()
        for page in PdfReader(str(RESOURCES / "crazyones.pdf")).pages[:2]:
            new_page = writer.add_page(page)
            new_page.add_transformation(Transformation().rotate(90).scale(0.5, 0.5))
        return dump(writer)

    scenario("w07_transform", transform)

    def incremental() -> dict:
        import tempfile

        with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as handle:
            path = Path(handle.name)
        try:
            source = path.with_suffix(".src.pdf")
            source.write_bytes((RESOURCES / "crazyones.pdf").read_bytes())
            writer = PdfWriter(str(source), incremental=True)
            writer.add_blank_page(100, 100)
            writer.write(str(path))
            data = path.read_bytes()
        finally:
            path.unlink(missing_ok=True)
            source.unlink(missing_ok=True)
        return {
            "sha256": _digest(data),
            "length": len(data),
            "head": data[:32].hex(),
            "tail": data[-64:].hex(),
        }

    scenario("w08_incremental", incremental)

    def attachment() -> dict:
        writer = PdfWriter(clone_from=PdfReader(str(RESOURCES / "attachment.pdf")))
        writer.add_attachment("audit.txt", b"deterministic audit payload")
        return dump(writer)

    scenario("w09_attachment", attachment)

    def form_fill() -> dict:
        writer = PdfWriter(clone_from=PdfReader(str(RESOURCES / "form.pdf")))
        for page in writer.pages:
            writer.update_page_form_field_values(page, {"field1": "value1"}, auto_regenerate=False)
        return dump(writer)

    scenario("w10_form_fill", form_fill)

    def js_and_date() -> dict:
        from pypdf.actions import JavaScript

        writer = PdfWriter()
        writer.add_blank_page(612, 792)
        writer.add_open_action(JavaScript("app.alert('audit')"))
        writer.add_metadata({"/CreationDate": "D:20260101000000Z"})
        return dump(writer)

    scenario("w11_js_creation_date", js_and_date)

    def remove_page() -> dict:
        writer = PdfWriter(clone_from=PdfReader(str(RESOURCES / "inkscape-abc.pdf")))
        if len(writer.pages) > 1:
            writer.remove_page(0)
        return dump(writer)

    scenario("w12_remove_page", remove_page)

    def rotate_contents() -> dict:
        writer = PdfWriter(clone_from=PdfReader(str(RESOURCES / "crazyones.pdf")))
        page = writer.pages[0]
        page.rotate(180)
        writer.add_page(page)
        return dump(writer)

    scenario("w13_rotate", rotate_contents)

    return scenarios


def page_label_and_outline_scenarios() -> dict[str, object]:
    from pypdf import PdfReader

    out: dict[str, object] = {}
    for name in _resource_names():
        path = RESOURCES / name
        if name in PASSWORDS:
            continue
        try:
            reader = PdfReader(str(path), strict=False)
            out[name] = {
                "labels": canon(reader.page_labels),
                "outline": canon(reader.outline),
            }
        except Exception as exc:  # noqa: BLE001
            out[name] = {"error": type(exc).__name__, "msg": str(exc)}
    return out


# ---------------------------------------------------------------------------
# driver
# ---------------------------------------------------------------------------
def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", required=True)
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED)
    parser.add_argument("--skip-text", action="store_true")
    parser.add_argument("--skip-writer", action="store_true")
    args = parser.parse_args()

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    install_deterministic_secrets(args.seed)

    import pypdf  # noqa: PLC0415  (import after secrets are patched)

    capture = _LogCapture()
    root_logger = logging.getLogger()
    root_logger.addHandler(capture)
    root_logger.setLevel(logging.DEBUG)

    results: dict[str, object] = {}

    def run(name: str, func) -> None:
        capture.records.clear()
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            try:
                value = func()
                error = None
            except BaseException as exc:  # noqa: BLE001
                value = None
                error = {
                    "type": type(exc).__name__,
                    "mod": type(exc).__module__,
                    "msg": str(exc),
                }
        results[name] = {
            "scenario": name,
            "error": error,
            "value": canon(value),
            "logs": list(capture.records),
            "warnings": [
                {"category": w.category.__name__, "message": str(w.message)}
                for w in caught
            ],
        }

    for name in _resource_names():
        path = RESOURCES / name
        run(f"read::{name}", lambda path=path: read_document(path))

    for name in _encryption_names():
        path = RESOURCES / "encryption" / name
        password = PASSWORDS.get(name)
        run(
            f"read_encrypted::{name}",
            lambda path=path, password=password: read_document(path, password),
        )

    if not args.skip_text:
        for name in TEXT_PDFS:
            path = RESOURCES / name
            if not path.exists():
                results[f"text::{name}"] = {
                    "scenario": f"text::{name}",
                    "error": {"type": "Missing", "msg": str(path)},
                    "value": None,
                    "logs": [],
                    "warnings": [],
                }
                continue
            run(f"text::{name}", lambda path=path: read_document(path, text=True))

        for txt in sorted(RESOURCES.glob("*.layout*.txt")):
            pass  # layout golden comparison lives in the pytest suite

    run("labels_outlines", page_label_and_outline_scenarios)

    if not args.skip_writer:
        for name, func in writer_scenarios():
            run(f"write::{name}", func)

    payload = {
        "pypdf_version": pypdf.__version__,
        "debug_versions": pypdf._debug_versions,
        "seed": args.seed,
        "python": sys.version,
        "scenarios": results,
    }
    # The PYTHONHASHSEED value itself is part of the audit input, not output.
    (out_dir / "audit.json").write_text(
        json.dumps(payload, indent=1, sort_keys=True, ensure_ascii=False),
        encoding="utf-8",
    )
    failed = sum(1 for r in results.values() if r["error"] is not None)
    print(f"scenarios={len(results)} errors={failed} -> {out_dir / 'audit.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
