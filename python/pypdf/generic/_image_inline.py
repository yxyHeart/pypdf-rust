"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.generic._image_inline``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import logging
from io import BytesIO
from math import ceil
from typing import IO, Any
from .._configuration import get_configuration
from .._utils import WHITESPACES, WHITESPACES_AS_BYTES, BinaryStreamType, StreamType, logger_warning, read_non_whitespace
from ..errors import LimitReachedError, PdfReadError, PdfStreamError
from ._data_structures import ContentStream, DictionaryObject, read_object
from ._utils import StreamParsingBudget
logger = None
BUFFER_SIZE = 8192
def _check_end_image_marker(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_inline._check_end_image_marker
def extract_inline__ascii_hex_decode(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_inline.extract_inline__ascii_hex_decode
def extract_inline__ascii85_decode(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_inline.extract_inline__ascii85_decode
def extract_inline__run_length_decode(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_inline.extract_inline__run_length_decode
def extract_inline__dct_decode(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_inline.extract_inline__dct_decode
def _is_preceded_by_whitespace(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_inline._is_preceded_by_whitespace
def _is_followed_by_closing_operator(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_inline._is_followed_by_closing_operator
def extract_inline_default(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_inline.extract_inline_default
def is_followed_by_binary_data(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_inline.is_followed_by_binary_data
def _read_settings_dictionary(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_inline._read_settings_dictionary
def extract_inline__not_set(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_inline.extract_inline__not_set
def _handle_filters(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_inline._handle_filters
def read_inline_image(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_inline.read_inline_image
