"""Code related to text extraction.

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._text_extraction``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import math
import re
from collections.abc import Callable, Mapping
from typing import Any, Literal
from .._utils import RTL_CHARACTER_RANGES, is_char_neutral, is_char_rtl
from ..generic import ContentStream, DictionaryObject, TextStringObject, encode_pdfdocencoding
from ..generic._font import Font
CUSTOM_RTL_MIN = None
CUSTOM_RTL_MAX = None
CUSTOM_RTL_SPECIAL_CHARS = None
LAYOUT_NEW_BT_GROUP_SPACE_WIDTHS = None
UNICODE_LOWER_LIMIT = 0
UNICODE_UPPER_LIMIT = 1114111
RTL_CHARACTER_PATTERN = None
_TEXT_OPERATORS = ()
_TEXT_OPERATOR_START = None
class OrientationNotFoundError(Exception):
    """Stub for pypdf._text_extraction.OrientationNotFoundError."""
    pass
def set_custom_rtl(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction.set_custom_rtl
def mult(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction.mult
def orient(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction.orient
def crlf_space_check(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction.crlf_space_check
def get_text_operands(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction.get_text_operands
def get_display_str(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction.get_display_str
def _has_text_operator(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction._has_text_operator
def _get_content_stream_for_text(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction._get_content_stream_for_text
