#!/usr/bin/env python3
"""Shared helpers for the §9 round-trip and independent-tool interop checks.

Everything here is deliberately *semantic*: the digests contain page geometry,
resolved page numbers for outlines/destinations, decoded content-stream hashes,
extracted text, image payload hashes and metadata -- never raw PDF bytes.

Recorded normalisations (they are also reported in the JSON that is written):

* floats are rounded to ``NUM_DIGITS`` decimals (``NUM_DIGITS == 6``);
* page text is kept verbatim but truncated to ``TEXT_CAP`` characters to keep
  the result files small -- ``text_sha256``/``text_len`` always describe the
  *full* text, and ``text_truncated`` says whether truncation happened;
* indirect object references are resolved where possible; unresolvable ones
  never leak ``id()`` values into the comparison (addresses are masked);
* workdir / repository paths in error messages are replaced by
  ``<workdir>`` / ``<root>`` so that the two backends, which use different
  temporary directories, can be compared.

The module intentionally does not import ``pypdf`` at module level: callers
must install deterministic randomness (``prng.install_deterministic_secrets``)
*first*, and several helpers (``digest_reader``) operate on reader objects.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "validation" / "fixtures" / "manifest.csv"
RESULTS = ROOT / "validation" / "results"
ORACLE_TOOLS = ROOT / "tools" / "oracle"

if str(ORACLE_TOOLS) not in sys.path:
    sys.path.insert(0, str(ORACLE_TOOLS))

from prng import DEFAULT_SEED, MASK, install_deterministic_secrets  # noqa: E402

try:  # The password map for resources/encryption/* lives in the audit helper.
    from audit import PASSWORDS as _AUDIT_PASSWORDS  # noqa: E402
except Exception:  # pragma: no cover - audit.py is a dev helper, not a runtime dep
    _AUDIT_PASSWORDS = {}

NUM_DIGITS = 6
TEXT_CAP = 4000
MAX_IMAGES_PER_PAGE = 8
MAX_DEPTH = 12

NORMALIZATIONS = {
    "numbers": f"floats rounded to {NUM_DIGITS} decimals",
    "text": (
        f"page text stored verbatim, truncated to {TEXT_CAP} chars; "
        "text_sha256/text_len always describe the full text"
    ),
    "paths": "workdir/root paths in messages replaced with <workdir>/<root>",
    "addresses": "hex addresses (0x...) in messages masked as 0xADDR",
    "indirect": "resolved to page numbers / names; raw object ids never compared",
}

# Passwords for the encrypted corpus documents (see tests/test_encryption.py,
# tests/test_reader.py and tests/test_workflows.py).
PASSWORDS: dict[str, str] = {
    "crazyones-encrypted-256.pdf": "password",
    "encrypted-file.pdf": "test",
    "encrypted_doc_no_id.pdf": "",
    "libreoffice-writer-password.pdf": "openpassword",
}
PASSWORDS.update({k: v for k, v in _AUDIT_PASSWORDS.items() if k not in PASSWORDS})

PASSWORD_CANDIDATES = [
    "",
    "asdfzxcv",
    "password",
    "test",
    "openpassword",
    "foo",
    "user",
    "owner",
]


# ---------------------------------------------------------------------------
# hashing / number / path helpers
# ---------------------------------------------------------------------------
def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def num(value: Any) -> Any:
    """Round floats so that both backends print the same JSON number."""
    try:
        number = float(value)
    except (TypeError, ValueError):
        return str(value)
    if number != number or number in (float("inf"), float("-inf")):
        return repr(number)
    rounded = round(number, NUM_DIGITS)
    if rounded == 0:
        rounded = 0.0
    return rounded


def box(value: Any) -> Any:
    """Rectangle-ish object -> list of rounded floats (or an error marker)."""
    if value is None:
        return None
    try:
        return [num(item) for item in value]
    except Exception as exc:  # noqa: BLE001 - broken /MediaBox is data
        return {"error": f"{type(exc).__name__}: {exc}"}


def sanitize_message(text: Any, workdir: Path | None = None) -> str:
    """Mask volatile parts of exception messages (paths, object addresses)."""
    result = str(text)
    if workdir is not None:
        result = result.replace(str(workdir), "<workdir>")
    result = result.replace(str(ROOT), "<root>")
    result = re.sub(r"0x[0-9a-fA-F]+", "0xADDR", result)
    return result


def safe_name(case_id: str, limit: int = 120) -> str:
    return re.sub(r"[^A-Za-z0-9._-]+", "_", case_id)[:limit]


def seed_for(case_id: str, base_seed: int = DEFAULT_SEED) -> int:
    """Per-case deterministic seed; both backends derive it identically."""
    import zlib

    return (base_seed + zlib.crc32(case_id.encode("utf-8"))) & MASK


def install_deterministic(seed: int) -> Any:
    return install_deterministic_secrets(seed)


def detect_backend() -> str:
    """Identify which pypdf implementation the current interpreter imports."""
    try:
        import pypdf
    except Exception:  # pragma: no cover - broken environment
        return "unknown"
    module = str(Path(pypdf.__file__).resolve())
    if "/.venvs/oracle/" in module:
        return "oracle"
    if module.startswith(str(ROOT)) or getattr(pypdf, "_pypdf", None) is not None:
        return "dev"
    return "unknown"


def backend_info(backend: str | None = None) -> dict[str, Any]:
    import pypdf

    info: dict[str, Any] = {
        "version": pypdf.__version__,
        "module": str(Path(pypdf.__file__).resolve()),
        "python": sys.version.split()[0],
        "executable": sys.executable,
    }
    try:
        import pypdf._pypdf as _ext  # noqa: PLC0415

        info["rust_extension"] = getattr(_ext, "__file__", None)
    except ImportError:
        info["rust_extension"] = None
    if backend:
        info["backend"] = backend
    return info


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(
        json.dumps(payload, indent=1, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    os.replace(tmp, path)


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def password_candidates(path: Path | str) -> list[str]:
    name = Path(path).name
    candidates: list[str] = []
    if name in PASSWORDS:
        candidates.append(PASSWORDS[name])
    candidates.extend(PASSWORD_CANDIDATES)
    seen: set[str] = set()
    ordered: list[str] = []
    for item in candidates:
        if item not in seen:
            seen.add(item)
            ordered.append(item)
    return ordered


class OpenFailure(RuntimeError):
    """Raised when a PDF cannot be opened/decrypted with any known password."""

    def __init__(self, path: Path | str, attempts: list[dict[str, Any]]) -> None:
        super().__init__(f"cannot open/decrypt {path} (tried {len(attempts)} passwords)")
        self.path = str(path)
        self.attempts = attempts


def open_reader(path: Path | str, candidates: list[str] | None = None):
    """Open a PDF, trying the known passwords until the page tree is readable.

    Returns ``(reader, password_used, failed_attempts)``.  Wrong passwords in
    pypdf surface lazily, hence the ``len(reader.pages)`` probe.  pypdf raises
    ``PdfReadError("Not an encrypted file")`` if a password is passed for an
    unencrypted file, so the password-less open is attempted first.
    """
    from pypdf import PdfReader  # noqa: PLC0415  (import after prng patching)

    attempts: list[dict[str, Any]] = []
    try:
        reader = PdfReader(str(path), strict=False)
        len(reader.pages)  # forces decryption of the whole page tree
        return reader, None, attempts
    except Exception as exc:  # noqa: BLE001 - encrypted/malformed, try passwords
        attempts.append(
            {"password": None, "type": type(exc).__name__, "msg": str(exc)[:300]}
        )

    for candidate in candidates if candidates is not None else password_candidates(path):
        try:
            reader = PdfReader(str(path), strict=False, password=candidate)
            len(reader.pages)  # forces decryption of the whole page tree
            return reader, candidate, attempts
        except Exception as exc:  # noqa: BLE001 - try the next password
            attempts.append(
                {
                    "password": candidate,
                    "type": type(exc).__name__,
                    "msg": str(exc)[:300],
                }
            )
    raise OpenFailure(path, attempts)


# ---------------------------------------------------------------------------
# semantic digests
# ---------------------------------------------------------------------------
def _simple(value: Any, depth: int = 0) -> Any:
    """Reduce a PDF object to a JSON-safe, address-free value."""
    if depth > MAX_DEPTH:
        return "<depth-limit>"
    if value is None or isinstance(value, (bool, int)):
        return value
    if isinstance(value, float):
        return num(value)
    if isinstance(value, (bytes, bytearray)):
        return {"bytes_len": len(value), "bytes_sha256": sha256_bytes(bytes(value))}
    if isinstance(value, str):
        return str(value)
    if isinstance(value, (list, tuple)):
        return [_simple(item, depth + 1) for item in value]
    if isinstance(value, dict):
        out: dict[str, Any] = {}
        for key, item in value.items():
            out[str(key)] = _simple(item, depth + 1)
        return out
    # IndirectObject and friends: keep only stable fields.
    idnum = getattr(value, "idnum", None)
    generation = getattr(value, "generation", None)
    if idnum is not None and generation is not None:
        return {"indirect": [int(idnum), int(generation)]}
    return sanitize_message(value)


def _text_value(value: Any) -> Any:
    """Text values inside annotations may be TextStringObject or bytes."""
    if value is None:
        return None
    if isinstance(value, (bytes, bytearray)):
        try:
            return bytes(value).decode("utf-8", "replace")
        except Exception:  # noqa: BLE001
            return {"bytes_sha256": sha256_bytes(bytes(value))}
    return str(value)


def dest_page_index(reader, target: Any, _depth: int = 0) -> Any:
    """Resolve an outline/destination target to a 0-based page index or None."""
    if target is None or _depth > 6:
        return None
    try:
        obj = target.get_object() if hasattr(target, "get_object") else target
        if isinstance(obj, (list, tuple)):
            if not obj:
                return None
            first = obj[0]
            page = first.get_object() if hasattr(first, "get_object") else first
            return int(reader.get_page_number(page))
        if hasattr(obj, "get") and not isinstance(obj, str):
            if "/Page" in obj:
                page_ref = obj["/Page"]
                page = page_ref.get_object() if hasattr(page_ref, "get_object") else page_ref
                return int(reader.get_page_number(page))
            if "/D" in obj:
                return dest_page_index(reader, obj["/D"], _depth + 1)
            return None
        if isinstance(obj, str):
            named = reader.named_destinations
            if obj in named:
                return dest_page_index(reader, named[obj], _depth + 1)
            return {"name": str(obj)}
    except Exception:  # noqa: BLE001 - unresolvable destination is data
        return None
    return None


def annotation_digest(reader, annotation: Any) -> dict[str, Any]:
    try:
        obj = annotation.get_object() if hasattr(annotation, "get_object") else annotation
    except Exception as exc:  # noqa: BLE001
        return {"error": f"{type(exc).__name__}: {exc}"}
    if not hasattr(obj, "get"):
        return {"value": _simple(obj)}

    entry: dict[str, Any] = {
        "subtype": _simple(obj.get("/Subtype")),
        "rect": box(obj.get("/Rect")),
        "contents": _text_value(obj.get("/Contents")),
        "nm": _simple(obj.get("/NM")),
        "field": _simple(obj.get("/T")),
        "flags": _simple(obj.get("/F")),
        "quadpoints": _simple(obj.get("/QuadPoints")),
        "dest_page": dest_page_index(reader, obj.get("/Dest")),
    }
    action = obj.get("/A")
    if action is not None:
        try:
            action_obj = action.get_object() if hasattr(action, "get_object") else action
            entry["action"] = {
                "s": _simple(action_obj.get("/S")) if hasattr(action_obj, "get") else None,
                "uri": _text_value(action_obj.get("/URI"))
                if hasattr(action_obj, "get")
                else None,
            }
        except Exception as exc:  # noqa: BLE001
            entry["action"] = {"error": f"{type(exc).__name__}: {exc}"}
    return entry


def page_annotations(reader, page) -> list[dict[str, Any]]:
    try:
        raw = page.get("/Annots") or []
        if hasattr(raw, "get_object"):
            raw = raw.get_object()
        return [annotation_digest(reader, item) for item in raw]
    except Exception as exc:  # noqa: BLE001
        return [{"error": f"{type(exc).__name__}: {exc}"}]


def page_images(page) -> list[dict[str, Any]]:
    images: list[dict[str, Any]] = []
    try:
        candidates = page.images
    except Exception as exc:  # noqa: BLE001
        return [{"error": f"{type(exc).__name__}: {exc}"}]
    for image in candidates[:MAX_IMAGES_PER_PAGE]:
        entry: dict[str, Any] = {"name": _simple(getattr(image, "name", None))}
        try:
            data = image.data
            size = image.image.size
            mode = image.image.mode
            entry.update(
                {
                    "width": int(size[0]),
                    "height": int(size[1]),
                    "mode": str(mode),
                    "data_len": len(data),
                    "data_sha256": sha256_bytes(data),
                }
            )
        except Exception as exc:  # noqa: BLE001
            entry["error"] = f"{type(exc).__name__}: {exc}"
        images.append(entry)
    return images


def page_digest(
    reader,
    index: int,
    page,
    *,
    text: str | None = "full",
    layout: bool = False,
) -> dict[str, Any]:
    """Semantic digest of one page.  ``text`` is ``full``, ``sha`` or ``None``."""
    entry: dict[str, Any] = {"index": index}
    try:
        entry["mediabox"] = box(page.mediabox)
    except Exception as exc:  # noqa: BLE001
        entry["mediabox"] = {"error": f"{type(exc).__name__}: {exc}"}
    try:
        entry["cropbox"] = box(page.cropbox)
    except Exception as exc:  # noqa: BLE001
        entry["cropbox"] = {"error": f"{type(exc).__name__}: {exc}"}
    try:
        entry["rotation"] = int(page.rotation)
    except Exception as exc:  # noqa: BLE001
        entry["rotation"] = {"error": f"{type(exc).__name__}: {exc}"}

    try:
        contents = page.get_contents()
        data = contents.get_data()
        entry["contents"] = {"len": len(data), "sha256": sha256_bytes(data)}
    except Exception as exc:  # noqa: BLE001
        entry["contents"] = {"error": f"{type(exc).__name__}: {exc}"}

    entry["annotations"] = page_annotations(reader, page)
    entry["images"] = page_images(page)

    if text is not None:
        try:
            raw = page.extract_text()
            entry["text_len"] = len(raw)
            entry["text_sha256"] = sha256_text(raw)
            if text == "full":
                entry["text"] = raw[:TEXT_CAP]
                entry["text_truncated"] = len(raw) > TEXT_CAP
        except Exception as exc:  # noqa: BLE001
            entry["text_error"] = f"{type(exc).__name__}: {sanitize_message(exc)}"

    if layout:
        try:
            layout_text = page.extract_text(extraction_mode="layout")
            entry["layout_text_len"] = len(layout_text)
            entry["layout_text_sha256"] = sha256_text(layout_text)
        except Exception as exc:  # noqa: BLE001
            entry["layout_text_error"] = f"{type(exc).__name__}: {sanitize_message(exc)}"
    return entry


def metadata_digest(reader) -> Any:
    try:
        metadata = reader.metadata
    except Exception as exc:  # noqa: BLE001
        return {"error": f"{type(exc).__name__}: {sanitize_message(exc)}"}
    if metadata is None:
        return {}
    try:
        return {str(key): str(value) for key, value in metadata.items()}
    except Exception as exc:  # noqa: BLE001
        return {"error": f"{type(exc).__name__}: {sanitize_message(exc)}"}


def page_labels_digest(reader) -> Any:
    try:
        return [str(label) for label in reader.page_labels]
    except Exception as exc:  # noqa: BLE001
        return {"error": f"{type(exc).__name__}: {sanitize_message(exc)}"}


def outline_digest(reader) -> Any:
    """Flatten the outline to (level, title, resolved page) -- no object ids."""
    try:
        outline = reader.outline
    except Exception as exc:  # noqa: BLE001
        return {"error": f"{type(exc).__name__}: {sanitize_message(exc)}"}

    flat: list[dict[str, Any]] = []

    def walk(items: Any, level: int) -> None:
        if not isinstance(items, (list, tuple)):
            return
        for item in items:
            if isinstance(item, (list, tuple)):
                walk(item, level + 1)
                continue
            if not hasattr(item, "get"):
                flat.append({"level": level, "value": _simple(item)})
                continue
            try:
                title = item.get("/Title")
            except Exception as exc:  # noqa: BLE001
                title = f"<error: {type(exc).__name__}>"
            entry: dict[str, Any] = {
                "level": level,
                "title": _text_value(title),
                "page": dest_page_index(reader, item),
            }
            flat.append(entry)

    walk(outline, 0)
    return flat


def named_destinations_digest(reader) -> Any:
    try:
        destinations = reader.named_destinations
    except Exception as exc:  # noqa: BLE001
        return {"error": f"{type(exc).__name__}: {sanitize_message(exc)}"}
    out: dict[str, Any] = {}
    for name in sorted(destinations):
        target = destinations[name]
        out[str(name)] = {
            "page": dest_page_index(reader, target),
            "type": _simple(target.get("/Type")) if hasattr(target, "get") else None,
        }
    return out


def attachments_digest(reader) -> Any:
    try:
        attachments = reader.attachments
    except Exception as exc:  # noqa: BLE001
        return {"error": f"{type(exc).__name__}: {sanitize_message(exc)}"}
    out: dict[str, Any] = {}
    for name in sorted(attachments):
        payloads = []
        try:
            for payload in attachments[name]:
                payloads.append(
                    {"len": len(payload), "sha256": sha256_bytes(payload)}
                )
        except Exception as exc:  # noqa: BLE001
            payloads.append({"error": f"{type(exc).__name__}: {exc}"})
        out[str(name)] = payloads
    return out


def acroform_digest(reader) -> Any:
    try:
        fields = reader.get_fields()
    except Exception as exc:  # noqa: BLE001
        return {"error": f"{type(exc).__name__}: {sanitize_message(exc)}"}
    if not fields:
        return {"count": 0, "names": []}
    return {"count": len(fields), "names": sorted(str(name) for name in fields)}


def xmp_digest(reader) -> Any:
    try:
        xmp = reader.xmp_metadata
    except Exception as exc:  # noqa: BLE001
        return {"error": f"{type(exc).__name__}: {sanitize_message(exc)}"}
    if xmp is None:
        return None
    try:
        xml = xmp.rdf_root.toxml()
        return {"len": len(xml), "sha256": sha256_text(xml)}
    except Exception as exc:  # noqa: BLE001
        return {"error": f"{type(exc).__name__}: {sanitize_message(exc)}"}


def digest_reader(
    reader,
    *,
    text: str | None = "full",
    text_pages: int | None = None,
    layout_first_page: bool = False,
    images: bool = True,
) -> dict[str, Any]:
    """Build the semantic digest of a document (see the module docstring).

    ``text_pages`` limits *text extraction* to the first N pages (page digests
    beyond that point are still present, with ``text_skipped: true``).  Text is
    never stripped or whitespace-normalised.
    """
    pages_obj = reader.pages
    page_count = len(pages_obj)
    pages: list[dict[str, Any]] = []
    for index in range(page_count):
        extract_text = text_pages is None or index < text_pages
        try:
            page = pages_obj[index]
            entry = page_digest(
                reader,
                index,
                page,
                text=text if extract_text else None,
                layout=layout_first_page and index == 0,
            )
        except Exception as exc:  # noqa: BLE001 - report, never abort the digest
            entry = {
                "index": index,
                "error": f"{type(exc).__name__}: {sanitize_message(exc)}",
            }
        if not extract_text:
            entry["text_skipped"] = True
        pages.append(entry)

    digest: dict[str, Any] = {
        "page_count": page_count,
        "pdf_header": getattr(reader, "pdf_header", None),
        "metadata": metadata_digest(reader),
        "page_labels": page_labels_digest(reader),
        "outline": outline_digest(reader),
        "named_destinations": named_destinations_digest(reader),
        "attachments": attachments_digest(reader),
        "acroform": acroform_digest(reader),
        "xmp": xmp_digest(reader),
        "pages": pages,
        "text_pages_covered": page_count if text_pages is None else min(text_pages, page_count),
    }
    return digest


# ---------------------------------------------------------------------------
# digest accessors used for round-trip invariants
# ---------------------------------------------------------------------------
def text_shas(digest: dict[str, Any], limit: int | None = None) -> list[Any]:
    values = [page.get("text_sha256") for page in digest.get("pages", [])]
    return values if limit is None else values[:limit]


def contents_shas(digest: dict[str, Any]) -> list[Any]:
    return [
        (page.get("contents") or {}).get("sha256")
        for page in digest.get("pages", [])
    ]


def image_shas(digest: dict[str, Any]) -> list[Any]:
    return [
        [image.get("data_sha256") for image in page.get("images", [])]
        for page in digest.get("pages", [])
    ]


def intersection_equal(left: list[Any], right: list[Any]) -> bool:
    """Compare the common prefix, tolerating digests with fewer extracted pages."""
    if not left or not right:
        return left == right
    limit = min(len(left), len(right))
    return left[:limit] == right[:limit]


def all_checks(checks: dict[str, Any]) -> tuple[bool, list[str]]:
    failures = [name for name, value in checks.items() if value is not True]
    return (not failures, failures)
