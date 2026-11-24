# Rust port of ``pypdf/generic/_utils.py`` (upstream pypdf 7.1.1).
#
# All logic lives in ``crates/pypdf-py/src/modules/genutils_mod.rs`` (reached
# through the ``pypdf._pypdf`` extension module).  This module keeps the
# upstream module surface: the module-level constants (``_PDFDOC_DECODE_MAP``,
# ``__ESCAPE_DICT__``, ``__BACKSLASH_CODE__``), the ``StreamParsingBudget``
# dataclass, and the function definitions with their docstrings/signatures
# delegating to Rust.  Upstream has no module docstring, so neither does this
# shim.

import codecs
from dataclasses import dataclass
from typing import Union

from pypdf._pypdf import (
    genutils_create_string_object as _create_string_object_rust,
    genutils_create_unicode_string_object as _create_unicode_string_object_rust,
    genutils_decode_pdfdocencoding as _decode_pdfdocencoding_rust,
    genutils_hex_to_rgb as _hex_to_rgb_rust,
    genutils_read_hex_string_from_stream as _read_hex_string_from_stream_rust,
    genutils_read_string_from_stream as _read_string_from_stream_rust,
)

from .._codecs import _pdfdoc_encoding
from .._utils import StreamType, logger_warning, read_non_whitespace
from ..errors import STREAM_TRUNCATED_PREMATURELY, PdfStreamError
from ._base import ByteStringObject, TextStringObject

_PDFDOC_DECODE_MAP = dict(enumerate(_pdfdoc_encoding))


def hex_to_rgb(value: str) -> tuple[float, float, float]:
    return _hex_to_rgb_rust(value)


def read_hex_string_from_stream(
    stream: StreamType,
    forced_encoding: str | list[str] | dict[int, str] | None = None,
) -> Union["TextStringObject", "ByteStringObject"]:
    return _read_hex_string_from_stream_rust(stream, forced_encoding)


__ESCAPE_DICT__ = {
    b"n": ord(b"\n"),
    b"r": ord(b"\r"),
    b"t": ord(b"\t"),
    b"b": ord(b"\b"),
    b"f": ord(b"\f"),
    b"(": ord(b"("),
    b")": ord(b")"),
    b"/": ord(b"/"),
    b"\\": ord(b"\\"),
    b" ": ord(b" "),
    b"%": ord(b"%"),
    b"<": ord(b"<"),
    b">": ord(b">"),
    b"[": ord(b"["),
    b"]": ord(b"]"),
    b"#": ord(b"#"),
    b"_": ord(b"_"),
    b"&": ord(b"&"),
    b"$": ord(b"$"),
}
__BACKSLASH_CODE__ = 92


def read_string_from_stream(
    stream: StreamType,
    forced_encoding: str | list[str] | dict[int, str] | None = None,
) -> Union["TextStringObject", "ByteStringObject"]:
    return _read_string_from_stream_rust(stream, forced_encoding)


def create_string_object(
    string: str | bytes,
    forced_encoding: str | list[str] | dict[int, str] | None = None,
) -> TextStringObject | ByteStringObject:
    """
    Create a ByteStringObject or a TextStringObject from a string to represent the string.

    Args:
        string: The data being used
        forced_encoding: Typically None, or an encoding string

    Returns:
        A ByteStringObject

    Raises:
        TypeError: If string is not of type str or bytes.

    """
    return _create_string_object_rust(string, forced_encoding)


def _create_unicode_string_object(data: bytes) -> TextStringObject | None:
    """
    Decode a text string which is marked or detected as UTF-8 or UTF-16.

    Args:
        data: The data being used

    Returns:
        A TextStringObject, or None if the data should be decoded as PDFDocEncoding.

    Raises:
        UnicodeDecodeError: If the data cannot be decoded as UTF-16.

    """
    return _create_unicode_string_object_rust(data)


def decode_pdfdocencoding(byte_array: bytes) -> str:
    return _decode_pdfdocencoding_rust(byte_array)


@dataclass
class StreamParsingBudget:
    """
    Shared, mutable budget for limiting parsing work.

    Pass the same instance to recursive parsing calls to enforce a
    cumulative budget across nested arrays and dictionaries.
    """
    remaining_budget: int

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
