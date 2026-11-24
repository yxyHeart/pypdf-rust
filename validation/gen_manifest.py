#!/usr/bin/env python3
"""§3.2: build validation/fixtures/manifest.csv from the local corpus."""
from __future__ import annotations

import csv
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "validation" / "fixtures" / "manifest.csv"

FEATURE_NAMES = ["acroform", "outline", "annotations", "xmp", "attachments", "images",
                 "page_labels", "to_unicode", "xref_stream", "object_stream", "linearized"]


def features(reader) -> list[str]:
    found: list[str] = []
    root = reader.trailer.get("/Root")
    try:
        if root is not None and "/AcroForm" in root:
            found.append("acroform")
        if reader.outline:
            found.append("outline")
        if reader.page_labels:
            found.append("page_labels")
        if reader.attachments:
            found.append("attachments")
        xmp = getattr(reader, "xmp_metadata", None)
        if xmp is not None:
            found.append("xmp")
        for page in reader.pages:
            if page.images:
                found.append("images")
                break
            if page.get("/Annots"):
                found.append("annotations")
                break
        for xref_type, (_, _, xref_type_field) in reader.xref.items():
            if xref_type == 1:
                continue
        # xref stream / object streams heuristics
        data = Path(reader.stream.name).read_bytes() if hasattr(reader.stream, "name") else b""
        if b"/Type /XRef" in data or b"/Type/XRef" in data:
            found.append("xref_stream")
        if b"/ObjStm" in data:
            found.append("object_stream")
        if b"/Linearized" in data:
            found.append("linearized")
        if b"/ToUnicode" in data:
            found.append("to_unicode")
    except Exception:
        pass
    return sorted(set(found))


def main() -> int:
    from pypdf import PdfReader

    rows = []
    for base in ("resources", "sample-files"):
        for path in sorted((ROOT / base).rglob("*.pdf")):
            rel = path.relative_to(ROOT).as_posix()
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            pages = encrypted = version = ""
            feats: list[str] = []
            try:
                reader = PdfReader(str(path))
                encrypted = str(reader.is_encrypted)
                if not reader.is_encrypted:
                    pages = str(len(reader.pages))
                    feats = features(reader)
                version = str(reader.pdf_header)
            except Exception:
                pass
            rows.append({
                "case_id": rel.replace("/", "__"),
                "path": rel,
                "sha256": digest,
                "size_bytes": path.stat().st_size,
                "pages": pages,
                "encrypted": encrypted,
                "pdf_version": version,
                "features": ";".join(feats),
                "source": "upstream-test-resources" if base == "resources" else "py-pdf/sample-files",
                "license": "BSD-3-Clause" if base == "resources" else "see upstream repo",
                "priority": "P0" if base == "resources" else "P1",
            })
    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w", newline="", encoding="utf-8") as fh:
        writer = csv.DictWriter(fh, fieldnames=list(rows[0].keys()))
        writer.writeheader()
        writer.writerows(rows)
    print(f"manifest: {len(rows)} cases -> {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
