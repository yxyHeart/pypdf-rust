"""
Code related to text extraction.

Some parts are still in _page.py. In doubt, they will stay there.

Rust port of ``pypdf/_text_extraction/__init__.py`` (upstream pypdf 7.1.1): the
module constants, the ``OrientationNotFoundError`` class and the module-global
``CUSTOM_RTL_*`` state are kept verbatim (they are observable Python API: tests
mutate them through ``set_custom_rtl`` and read them back), while every function
body delegates to ``crates/pypdf-py/src/modules/text_extraction_mod``.
"""

import math
import re
from collections.abc import Callable, Mapping
from typing import Any, Literal

from pypdf._pypdf import (
    text_extraction_crlf_space_check as _crlf_space_check,
    text_extraction_get_content_stream_for_text as _get_content_stream_for_text_impl,
    text_extraction_get_display_str as _get_display_str,
    text_extraction_get_text_operands as _get_text_operands,
    text_extraction_has_text_operator as _has_text_operator_impl,
    text_extraction_mult as _mult,
    text_extraction_orient as _orient,
    text_extraction_set_custom_rtl as _set_custom_rtl,
)

from .._utils import RTL_CHARACTER_RANGES, is_char_neutral, is_char_rtl
from ..generic import ContentStream, DictionaryObject, TextStringObject, encode_pdfdocencoding
from ..generic._font import Font

CUSTOM_RTL_MIN: str = ""
CUSTOM_RTL_MAX: str = ""
CUSTOM_RTL_SPECIAL_CHARS: str = ""
LAYOUT_NEW_BT_GROUP_SPACE_WIDTHS: int = 5
UNICODE_LOWER_LIMIT = 0
UNICODE_UPPER_LIMIT = 0x10FFFF
RTL_CHARACTER_PATTERN = re.compile("[" + "".join(f"{start}-{end}" for start, end in RTL_CHARACTER_RANGES) + "]")

# The operators that can add text: the text-showing operators (ISO 32000-2:2020,
# 9.4.3, Table 107) and Do, which can paint a form XObject with text (8.8.1, Table 86).
# The bytes may also occur in strings, names or inline image data; that only
# means that the content stream is parsed.
_TEXT_OPERATORS = (b"Tj", b"TJ", b"Do", b"'", b'"')
# Searching for their first bytes and comparing the next byte is faster than
# searching for the alternation Tj|TJ|Do|'|" in content streams with many graphics.
_TEXT_OPERATOR_START = re.compile(rb"[TD'\"]")


class OrientationNotFoundError(Exception):
    pass


def set_custom_rtl(
    _min: str | int | None = "",
    _max: str | int | None = "",
    specials: str | list[int] | None = None,
) -> tuple[str, str, str]:
    """
    Change the Right-To-Left and special characters custom parameters.

    Args:
        _min: The new minimum value for the range of custom characters that
            will be written right to left.
            If set to ``None``, the value will not be changed.
            If set to a valid integer, it will be converted to its corresponding character.
            The default value is "", which sets no additional range to be converted.
        _max: The new maximum value for the range of custom characters that will
            be written right to left.
            If set to ``None``, the value will not be changed.
            If set to a valid integer, it will be converted to its corresponding character.
            The default value is "", which sets no additional range to be converted.
        specials: The new list of special characters to be inserted in the
            current insertion order.
            If set to ``None``, the current value will not be changed.
            If set to a string, it will be converted to a list of ASCII codes.
            The default value is an empty list.

    Returns:
        A tuple containing the new values for ``CUSTOM_RTL_MIN``,
        ``CUSTOM_RTL_MAX``, and ``CUSTOM_RTL_SPECIAL_CHARS``.

    """
    return _set_custom_rtl(_min, _max, specials)


def mult(
    m: list[float],
    n: (
        list[float] |
        Mapping[int | Literal["is_text", "is_render"], float | bool]
    ),
) -> list[float]:
    return _mult(m, n)


def orient(m: list[float]) -> int:
    return _orient(m)


def crlf_space_check(  # noqa: PLR0913, PLR0917
    text: str,
    cmtm_prev: tuple[list[float], list[float]],
    cmtm_matrix: tuple[list[float], list[float]],
    memo_cmtm: tuple[list[float], list[float]],
    font_resource: DictionaryObject | None,
    orientations: tuple[int, ...],
    output: str,
    font_size: float,
    visitor_text: Callable[[Any, Any, Any, Any, Any], None] | None,
    str_widths: float,
    spacewidth: float,
    str_height: float,
    line_span: tuple[int, float, float] | None,
) -> tuple[str, str, list[float], list[float], tuple[int, float, float] | None]:
    return _crlf_space_check(
        text,
        cmtm_prev,
        cmtm_matrix,
        memo_cmtm,
        font_resource,
        orientations,
        output,
        font_size,
        visitor_text,
        str_widths,
        spacewidth,
        str_height,
        line_span,
    )


def get_text_operands(
    operands: list[str | TextStringObject],
    cm_matrix: list[float],
    tm_matrix: list[float],
    font: Font,
    orientations: tuple[int, ...]
) -> tuple[str, bool, float]:
    return _get_text_operands(operands, cm_matrix, tm_matrix, font, orientations)


def get_display_str(
    text: str,
    cm_matrix: list[float],
    tm_matrix: list[float],
    font_resource: DictionaryObject | None,
    font: Font,
    text_operands: str,
    font_size: float,
    rtl_dir: bool,
    visitor_text: Callable[[Any, Any, Any, Any, Any], None] | None
) -> tuple[str, bool, str]:
    """
    Add the characters of ``text_operands`` to ``text`` in display order.

    Returns:
        A tuple containing the current text run, its direction, and any text runs completed
        because of a direction change. The completed runs precede the current run in display order.

    """
    return _get_display_str(
        text,
        cm_matrix,
        tm_matrix,
        font_resource,
        font,
        text_operands,
        font_size,
        rtl_dir,
        visitor_text,
    )


def _has_text_operator(data: bytes) -> bool:
    """Return whether the bytes contain an operator that can add text."""
    return _has_text_operator_impl(data)


def _get_content_stream_for_text(
    obj: DictionaryObject, pdf: Any, content_key: str | None, *, has_visitor: bool
) -> ContentStream | None:
    """
    Return the content stream to extract text from, or None if it cannot give any text.

    Content without text, e.g. a figure, gives no text either. It is only parsed if
    a visitor has to see its operators.
    """
    return _get_content_stream_for_text_impl(obj, pdf, content_key, has_visitor)
