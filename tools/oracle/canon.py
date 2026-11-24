"""Canonical representation of pypdf objects for differential comparison.

The canonical form is JSON serialisable, deterministic, and covers every type
that can appear in the pypdf object model.  It is also the format written by the
oracle runner and by the Rust ``pypdf-oracle`` binary, so both implementations
must agree on it byte-for-byte.

Rules
-----
* Every value is ``{"t": <python type name>, "v": <payload>}``.
* ``bytes``   -> lowercase hex.
* ``int``     -> decimal string (arbitrary precision safe).
* ``float``   -> ``repr()`` (shortest round-trip representation).
* ``str``     -> the string itself (JSON escapes handled by the writer).
* ``dict``    -> list of ``[key, value]`` pairs in insertion order.
* ``list`` / ``tuple`` -> list of values (``t`` keeps them apart).
* Streams get an extra ``data`` field with the sha256 of ``get_data()``.
* Cycles are cut with ``{"t": "cycle", "v": <id>}``.
"""

from __future__ import annotations

import hashlib
import json
import re
from typing import Any

MAX_DEPTH = 64


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canon(obj: Any, _depth: int = 0, _seen: dict[int, int] | None = None) -> Any:
    if _seen is None:
        _seen = {}
    if _depth > MAX_DEPTH:
        return {"t": "depth-limit", "v": _depth}

    if obj is None:
        return {"t": "NoneType", "v": None}
    if isinstance(obj, bool):
        return {"t": "bool", "v": obj}
    if isinstance(obj, (bytes, bytearray)):
        return {"t": type(obj).__name__, "v": bytes(obj).hex()}
    if isinstance(obj, int):
        return {"t": type(obj).__name__, "v": str(int(obj))}
    if isinstance(obj, float):
        return {"t": type(obj).__name__, "v": repr(float(obj))}
    if isinstance(obj, str):
        return {"t": type(obj).__name__, "v": str(obj)}

    if isinstance(obj, (set, frozenset)):
        # Sets have hash-order-dependent iteration; sort by canonical form so
        # that PYTHONHASHSEED does not leak into the comparison.
        items = sorted((canon(x, _depth + 1, _seen) for x in obj), key=lambda v: json.dumps(v, sort_keys=True))
        return {"t": type(obj).__name__, "v": items}

    if isinstance(obj, (dict, list, tuple)):
        ident = id(obj)
        if ident in _seen:
            return {"t": "cycle", "v": _seen[ident]}
        _seen[ident] = len(_seen)

        if isinstance(obj, dict):
            pairs = [
                [canon(k, _depth + 1, _seen), canon(v, _depth + 1, _seen)]
                for k, v in obj.items()
            ]
            result: dict[str, Any] = {"t": type(obj).__name__, "v": pairs}
        else:
            result = {
                "t": type(obj).__name__,
                "v": [canon(x, _depth + 1, _seen) for x in obj],
            }

        # Streams are dictionaries at heart; capture the decoded payload too.
        if hasattr(obj, "get_data") and hasattr(obj, "flate_encode") is False:
            try:
                data = obj.get_data()
                result["data"] = sha256_hex(data)
            except Exception as exc:  # noqa: BLE001 - the oracle records failures
                result["data"] = {"error": type(exc).__name__, "msg": str(exc)}
        return result

    # IndirectObject: repr() embeds id(self.pdf); keep only stable fields.
    if hasattr(obj, "idnum") and hasattr(obj, "generation") and type(obj).__name__ == "IndirectObject":
        return {
            "t": "IndirectObject",
            "v": [int(obj.idnum), int(obj.generation)],
        }

    # Unknown objects: record the class and a stable-ish textual form with
    # memory addresses stripped (repr of pypdf objects embeds id() values).
    text = None
    try:
        text = repr(obj)
    except Exception:  # noqa: BLE001
        text = None
    if isinstance(text, str):
        text = re.sub(r"0x[0-9a-fA-F]+", "0xADDR", text)
        text = re.sub(r"(?<=\s)\d{6,}(?=[)\s,]|$)", "ADDR", text)
    return {"t": type(obj).__name__, "v": text}
