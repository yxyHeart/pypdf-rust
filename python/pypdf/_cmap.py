# Rust-backed module: the implementation lives in
# ``crates/pypdf-py/src/modules/cmap_mod.rs``.  The module constants, the
# ``_predefined_cmap`` table and the imported helpers below stay Python
# definitions; every function body delegates to Rust.
from binascii import Error as BinasciiError
from binascii import unhexlify
from collections.abc import Callable
from functools import partial
from hashlib import sha256
from io import BytesIO
from typing import Any, cast

from pypdf._pypdf import (
    cmap_check_mapping_size as _rust_check_mapping_size,
    cmap_check_token_length as _rust_check_token_length,
    cmap_derive_character_map_from_cff_type1_font_file as _rust_derive_character_map_from_cff_type1_font_file,
    cmap_derive_character_map_from_font_file as _rust_derive_character_map_from_font_file,
    cmap_derive_character_map_from_type1_font_file as _rust_derive_character_map_from_type1_font_file,
    cmap_get_encoding as _rust_get_encoding,
    cmap_glyph_name_to_unicode as _rust_glyph_name_to_unicode,
    cmap_parse_bfchar as _rust_parse_bfchar,
    cmap_parse_bfrange as _rust_parse_bfrange,
    cmap_parse_bfrange_decode as __parse_bfrange__decode,
    cmap_parse_encoding as _rust_parse_encoding,
    cmap_parse_to_unicode as _rust_parse_to_unicode,
    cmap_prepare_cm as _rust_prepare_cm,
    cmap_process_cm_line as _rust_process_cm_line,
)

from ._codecs import adobe_glyphs, charset_encoding
from ._utils import logger_error, logger_warning
from .errors import LimitReachedError
from .generic import (
    ArrayObject,
    DecodedStreamObject,
    DictionaryObject,
    NullObject,
    StreamObject,
)
from .generic._data_structures import _FontFileCharacterMap

_predefined_cmap: dict[str, str] = {
    "/Identity-H": "utf-16-be",
    "/Identity-V": "utf-16-be",
    "/GB-EUC-H": "gbk",
    "/GB-EUC-V": "gbk",
    "/GBpc-EUC-H": "gb2312",
    "/GBpc-EUC-V": "gb2312",
    "/GBK-EUC-H": "gbk",
    "/GBK-EUC-V": "gbk",
    "/GBK2K-H": "gb18030",
    "/GBK2K-V": "gb18030",
    "/ETen-B5-H": "cp950",
    "/ETen-B5-V": "cp950",
    "/ETenms-B5-H": "cp950",
    "/ETenms-B5-V": "cp950",
    # Unicode-based CMaps: character codes are UTF-16BE, see
    # ISO 32000-2:2020, 9.7.5.2, Note 1 and Table 116.
    # (UniGB covers the GB18030-2000 character set but is not GB18030-encoded.)
    "/UniCNS-UTF16-H": "utf-16-be",
    "/UniCNS-UTF16-V": "utf-16-be",
    "/UniGB-UTF16-H": "utf-16-be",
    "/UniGB-UTF16-V": "utf-16-be",
    # Japanese CMaps (PDF Reference 1.7, Appendix H)
    "/90ms-RKSJ-H": "cp932",  # Shift-JIS (JIS X 0208-1990), horizontal
    "/90ms-RKSJ-V": "cp932",  # Shift-JIS (JIS X 0208-1990), vertical
    "/UniJIS-UTF16-H": "utf-16-be",  # Unicode UTF-16BE -> JIS, horizontal
    "/UniJIS-UTF16-V": "utf-16-be",  # Unicode UTF-16BE -> JIS, vertical
    # UCS2 in code
}


def get_encoding(
    ft: DictionaryObject
) -> tuple[str | dict[int, str], dict[Any, Any]]:
    """ ``pypdf._cmap.get_encoding``, delegated to Rust."""
    return _rust_get_encoding(ft)


def _parse_encoding(
    ft: DictionaryObject
) -> str | dict[int, str]:
    """ ``pypdf._cmap._parse_encoding``, delegated to Rust."""
    return _rust_parse_encoding(ft)


def _parse_to_unicode(
    ft: DictionaryObject
) -> tuple[dict[Any, Any], list[int]]:
    """ ``pypdf._cmap._parse_to_unicode``, delegated to Rust."""
    return _rust_parse_to_unicode(ft)


def prepare_cm(ft: DictionaryObject) -> bytes:
    """ ``pypdf._cmap.prepare_cm``, delegated to Rust."""
    return _rust_prepare_cm(ft)


def process_cm_line(
    line: bytes,
    process_rg: bool,
    process_char: bool,
    multiline_rg: tuple[int, int] | None,
    map_dict: dict[Any, Any],
    int_entry: list[int],
) -> tuple[bool, bool, tuple[int, int] | None]:
    """ ``pypdf._cmap.process_cm_line``, delegated to Rust."""
    return _rust_process_cm_line(
        line, process_rg, process_char, multiline_rg, map_dict, int_entry
    )


# Usual values should be up to 65_536.
MAPPING_DICTIONARY_SIZE_LIMIT = 100_000

# Typical /ToUnicode CMaps use 1-4 byte source codes.
# This is intentionally generous.
# The actual limit is doubled, as each byte is represented by two hex characters.
MAX_CMAP_CODE_BYTES = 8
MAX_CMAP_STRING_BYTES = 512
MAX_CMAP_CODE_BYTES_LIMIT = MAX_CMAP_CODE_BYTES * 2
MAX_CMAP_STRING_BYTES_LIMIT = MAX_CMAP_STRING_BYTES * 2


def _check_mapping_size(size: int) -> None:
    """ ``pypdf._cmap._check_mapping_size``, delegated to Rust."""
    return _rust_check_mapping_size(size)


def _check_token_length(token: bytes, limit: int) -> None:
    """ ``pypdf._cmap._check_token_length``, delegated to Rust."""
    return _rust_check_token_length(token, limit)


def parse_bfrange(
    line: bytes,
    map_dict: dict[Any, Any],
    int_entry: list[int],
    multiline_rg: tuple[int, int] | None,
) -> tuple[int, int] | None:
    """ ``pypdf._cmap.parse_bfrange``, delegated to Rust."""
    return _rust_parse_bfrange(line, map_dict, int_entry, multiline_rg)


def parse_bfchar(line: bytes, map_dict: dict[Any, Any], int_entry: list[int]) -> None:
    """ ``pypdf._cmap.parse_bfchar``, delegated to Rust."""
    return _rust_parse_bfchar(line, map_dict, int_entry)


def _glyph_name_to_unicode(glyph_name: str) -> str | None:
    """ ``pypdf._cmap._glyph_name_to_unicode``, delegated to Rust."""
    return _rust_glyph_name_to_unicode(glyph_name)


def _derive_character_map_from_font_file(
    font_file: StreamObject,
    font_data: bytes,
    font_file_processor: Callable[[bytes, dict[Any, Any], list[int]], tuple[dict[Any, Any], list[int]]],
    map_dict: dict[Any, Any],
    int_entry: list[int],
) -> tuple[dict[Any, Any], list[int]]:
    """ ``pypdf._cmap._derive_character_map_from_font_file``, delegated to Rust."""
    return _rust_derive_character_map_from_font_file(
        font_file, font_data, font_file_processor, map_dict, int_entry
    )


def _derive_character_map_from_cff_type1_font_file(
    font_data: bytes,
    map_dict: dict[Any, Any],
    int_entry: list[int],
) -> tuple[dict[Any, Any], list[int]]:
    """ ``pypdf._cmap._derive_character_map_from_cff_type1_font_file``, delegated to Rust."""
    return _rust_derive_character_map_from_cff_type1_font_file(
        font_data, map_dict, int_entry
    )


def _derive_character_map_from_type1_font_file(
    font_data: bytes,
    map_dict: dict[Any, Any],
    int_entry: list[int],
) -> tuple[dict[Any, Any], list[int]]:
    """ ``pypdf._cmap._derive_character_map_from_type1_font_file``, delegated to Rust."""
    return _rust_derive_character_map_from_type1_font_file(
        font_data, map_dict, int_entry
    )
