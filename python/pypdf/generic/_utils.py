"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.generic._utils``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import codecs
from dataclasses import dataclass
from typing import Union
from .._codecs import _pdfdoc_encoding
from .._utils import StreamType, logger_warning, read_non_whitespace
from ..errors import STREAM_TRUNCATED_PREMATURELY, PdfStreamError
from ._base import ByteStringObject, TextStringObject
_PDFDOC_DECODE_MAP = None
def hex_to_rgb(*args, **kwargs):
    return None  # inert stub: pypdf.generic._utils.hex_to_rgb
def read_hex_string_from_stream(*args, **kwargs):
    return None  # inert stub: pypdf.generic._utils.read_hex_string_from_stream
__ESCAPE_DICT__ = {}
__BACKSLASH_CODE__ = 92
def read_string_from_stream(*args, **kwargs):
    return None  # inert stub: pypdf.generic._utils.read_string_from_stream
def create_string_object(*args, **kwargs):
    return None  # inert stub: pypdf.generic._utils.create_string_object
def _create_unicode_string_object(*args, **kwargs):
    return None  # inert stub: pypdf.generic._utils._create_unicode_string_object
def decode_pdfdocencoding(*args, **kwargs):
    return None  # inert stub: pypdf.generic._utils.decode_pdfdocencoding
class StreamParsingBudget:
    """Stub for pypdf.generic._utils.StreamParsingBudget."""
    remaining_budget = None
