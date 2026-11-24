#!/usr/bin/env python3
"""§2/§5/§6/§7: deep semantic differential (reference vs port).

Run the same scenario code in both venvs; compare canonical JSON.  Never
compares raw PDF bytes (PDF serialization legitimately differs), only semantic
digests of read results and of re-opened written documents.

Usage: <venv>/bin/python validation/differential.py --out validation/results/diff-<backend>.json
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import sys
import traceback
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools" / "oracle"))

from canon import canon  # noqa: E402
from prng import install_deterministic_secrets  # noqa: E402

SAMPLE_LIMIT = 24


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def digest_reader(reader) -> dict:
    import pypdf

    info: dict = {
        "pdf_header": str(reader.pdf_header),
        "is_encrypted": bool(reader.is_encrypted),
        "num_pages": len(reader.pages),
        "metadata": canon(dict(reader.metadata or {})),
        "page_labels": canon(reader.page_labels),
        "outline": canon(reader.outline),
        "attachments": {name: [ _sha(item) for item in files ] for name, files in reader.attachments.items()},
        "fields": {name: {"type": str(field.get("/FT")), "value": canon(field.get("/V"))}
                   for name, field in (reader.get_fields() or {}).items()},
        "named_destinations": {name: canon(dest) for name, dest in reader.named_destinations.items()},
    }
    try:
        xmp = reader.xmp_metadata
        info["xmp"] = hashlib.sha256(xmp.rdf_root.toxml().encode() if xmp is not None else b"").hexdigest()
    except Exception as exc:  # noqa: BLE001
        info["xmp"] = f"error:{type(exc).__name__}"
    pages = []
    for index, page in enumerate(reader.pages):
        if index >= 8:
            break
        page_info = {
            "mediabox": canon(page.mediabox),
            "cropbox": canon(page.cropbox),
            "trimbox": canon(page.trimbox),
            "bleedbox": canon(page.bleedbox),
            "artbox": canon(page.artbox),
            "rotation": page.rotation,
            "labels": page.label if hasattr(page, "label") else None,
        }
        try:
            page_info["text"] = _sha(page.extract_text().encode("utf-8", "surrogatepass"))
        except Exception as exc:  # noqa: BLE001
            page_info["text"] = f"error:{type(exc).__name__}"
        try:
            page_info["images"] = [
                {"name": img.name, "sha": _sha(img.data), "mode": img.image.mode if hasattr(img, "image") else None,
                 "size": list(img.image.size) if hasattr(img, "image") else None}
                for img in page.images
            ]
        except Exception as exc:  # noqa: BLE001
            page_info["images"] = f"error:{type(exc).__name__}"
        try:
            annots = page.get("/Annots") or []
            page_info["annotations"] = [
                {"subtype": str(annot.get("/Subtype")), "rect": canon(annot.get("/Rect")),
                 "flags": canon(annot.get("/F"))}
                for annot in annots
            ]
        except Exception as exc:  # noqa: BLE001
            page_info["annotations"] = f"error:{type(exc).__name__}"
        pages.append(page_info)
    info["pages"] = pages
    return info


def run_scenarios(paths: list[Path]) -> dict:
    from pypdf import PdfReader, PdfWriter

    results: dict[str, dict] = {}

    for path in paths:
        case = path.relative_to(ROOT).as_posix()
        data = path.read_bytes()
        entry: dict = {}

        # §4.4 / §3.3: input forms must behave identically
        forms = {
            "path_str": lambda: PdfReader(str(path)),
            "path_obj": lambda: PdfReader(path),
            "bytes_io": lambda: PdfReader(io.BytesIO(data)),
            "file_obj": lambda: PdfReader(open(path, "rb")),  # noqa: SIM115
        }
        for name, factory in forms.items():
            try:
                reader = factory()
                entry[f"input:{name}"] = {"ok": True, "pages": len(reader.pages)}
            except Exception as exc:  # noqa: BLE001
                entry[f"input:{name}"] = {"ok": False, "error": type(exc).__name__, "msg": str(exc)}

        # §5: full semantic digest, strict True/False
        for strict in (False, True):
            key = f"read:strict={strict}"
            try:
                reader = PdfReader(io.BytesIO(data), strict=strict)
                entry[key] = {"ok": True, "digest": digest_reader(reader)}
            except Exception as exc:  # noqa: BLE001
                entry[key] = {"ok": False, "error": type(exc).__name__, "msg": str(exc),
                              "cause": type(exc.__cause__).__name__ if exc.__cause__ else None}

        # §6: write -> reopen semantic round-trip
        try:
            reader = PdfReader(io.BytesIO(data))
            writer = PdfWriter(clone_from=reader)
            buffer = io.BytesIO()
            writer.write(buffer)
            buffer.seek(0)
            entry["rw"] = {"ok": True, "digest": digest_reader(PdfReader(buffer))}
        except Exception as exc:  # noqa: BLE001
            entry["rw"] = {"ok": False, "error": type(exc).__name__, "msg": str(exc)}

        results[case] = entry
    return results


def select_paths(limit: int) -> list[Path]:
    paths = sorted((ROOT / "resources").rglob("*.pdf"))
    preferred = [p for p in paths if p.stat().st_size < 2_000_000]
    return preferred[:limit]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=SAMPLE_LIMIT)
    args = parser.parse_args()
    install_deterministic_secrets()
    results = run_scenarios(select_paths(args.limit))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(results, indent=1, sort_keys=True, ensure_ascii=False), encoding="utf-8")
    print(f"differential: {len(results)} documents -> {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
