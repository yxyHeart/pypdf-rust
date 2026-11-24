"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._cmap``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

from binascii import Error as BinasciiError
from binascii import unhexlify
from collections.abc import Callable
from functools import partial
from hashlib import sha256
from io import BytesIO
from typing import Any, cast
from ._codecs import adobe_glyphs, charset_encoding
from ._utils import logger_error, logger_warning
from .errors import LimitReachedError
from .generic import ArrayObject, DecodedStreamObject, DictionaryObject, NullObject, StreamObject
from .generic._data_structures import _FontFileCharacterMap
_predefined_cmap = None
def get_encoding(*args, **kwargs):
    return None  # inert stub: pypdf._cmap.get_encoding
def _parse_encoding(*args, **kwargs):
    return None  # inert stub: pypdf._cmap._parse_encoding
def _parse_to_unicode(*args, **kwargs):
    return None  # inert stub: pypdf._cmap._parse_to_unicode
def prepare_cm(*args, **kwargs):
    return None  # inert stub: pypdf._cmap.prepare_cm
def process_cm_line(*args, **kwargs):
    return None  # inert stub: pypdf._cmap.process_cm_line
MAPPING_DICTIONARY_SIZE_LIMIT = 100000
MAX_CMAP_CODE_BYTES = 8
MAX_CMAP_STRING_BYTES = 512
MAX_CMAP_CODE_BYTES_LIMIT = None
MAX_CMAP_STRING_BYTES_LIMIT = None
def _check_mapping_size(*args, **kwargs):
    return None  # inert stub: pypdf._cmap._check_mapping_size
def _check_token_length(*args, **kwargs):
    return None  # inert stub: pypdf._cmap._check_token_length
def __parse_bfrange__decode(*args, **kwargs):
    return None  # inert stub: pypdf._cmap.__parse_bfrange__decode
def parse_bfrange(*args, **kwargs):
    return None  # inert stub: pypdf._cmap.parse_bfrange
def parse_bfchar(*args, **kwargs):
    return None  # inert stub: pypdf._cmap.parse_bfchar
def _glyph_name_to_unicode(*args, **kwargs):
    return None  # inert stub: pypdf._cmap._glyph_name_to_unicode
def _derive_character_map_from_font_file(*args, **kwargs):
    return None  # inert stub: pypdf._cmap._derive_character_map_from_font_file
def _derive_character_map_from_cff_type1_font_file(*args, **kwargs):
    return None  # inert stub: pypdf._cmap._derive_character_map_from_cff_type1_font_file
def _derive_character_map_from_type1_font_file(*args, **kwargs):
    return None  # inert stub: pypdf._cmap._derive_character_map_from_type1_font_file
