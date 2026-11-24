"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.generic._font``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import unicodedata
from collections.abc import Sequence
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, ClassVar, cast
from pypdf.generic import ArrayObject, DictionaryObject, FloatObject, IndirectObject, NameObject, NumberObject, PdfObject, StreamObject, TextStringObject
from .._cmap import get_encoding
from .._codecs import encoding_dict_from_named_encoding
from .._codecs.adobe_glyphs import adobe_glyphs
from .._utils import logger_warning
from ..constants import FontFlags
from ..errors import LimitReachedError, PdfReadError
if TYPE_CHECKING:
    from fontTools.ttLib.tables._h_e_a_d import table__h_e_a_d
    from fontTools.ttLib.tables._p_o_s_t import table__p_o_s_t
    from fontTools.ttLib.tables.DefaultTable import DefaultTable
    from fontTools.ttLib.tables.O_S_2f_2 import table_O_S_2f_2
    from .._writer import PdfWriter
try:
    from io import BytesIO
    from fontTools.ttLib import TTFont
    HAS_FONTTOOLS = True
except ImportError:
    HAS_FONTTOOLS = False
MAX_CID_WIDTH_ENTRY_COUNT = 65536
MAX_WIDTH_ENTRY_COUNT = 100000
MAX_SIMPLE_FONT_CHARACTER_CODE = 255
HEADER_MACSTYLE_ITALIC = 2
OS2_FSSELECTION_ITALIC = 1
OS2_PANOSE_BFAMILYTYPE_SCRIPT = 3
OS2_PANOSE_BFAMILYTYPE_DECORATIVE = 4
OS2_PANOSE_BFAMILYTYPE_PICTORIAL = 5
OS2_PANOSE_BPROPORTION_MONOSPACED = 9
OS2_SFAMILYSCLASS_SCRIPTS = 10
OS2_SFAMILYSCLASS_SYMBOLIC = 12
CMAP_MAX_ENTRIES_PER_GROUP = 100
class FontDescriptor:
    """Stub for pypdf.generic._font.FontDescriptor."""
    _DEFAULT_BBOX = None
    name = None
    family = None
    weight = None
    ascent = None
    descent = None
    cap_height = None
    x_height = None
    italic_angle = None
    flags = None
    bbox = None
    font_file = None
    def as_font_descriptor_resource(self, *args, **kwargs):
        return None  # inert stub
class CoreFontMetrics:
    """Stub for pypdf.generic._font.CoreFontMetrics."""
    font_descriptor = None
    character_widths = None
class Font:
    """Stub for pypdf.generic._font.Font."""
    name = None
    encoding = None
    character_map = None
    sub_type = None
    font_descriptor = None
    character_widths = None
    space_width = None
    space_char = None
    interpretable = None
    @staticmethod
    def _collect_tt_t1_character_widths(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def __check_range_length(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def __check_entry_count(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _collect_cid_character_widths(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _get_space_char(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _add_default_width(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _add_space_width(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _parse_bbox(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _parse_font_descriptor(*args, **kwargs):
        return None  # inert stub
    @classmethod
    def from_font_resource(cls, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _load_fonttools_table(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _get_font_flags_from_truetype_font_tables(*args, **kwargs):
        return None  # inert stub
    @classmethod
    def from_truetype_font_file(cls, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def from_core_font_name(cls, *args, **kwargs):
        return None  # inert stub
    def _get_typographic_maps(self, *args, **kwargs):
        return None  # inert stub
    def _create_widths_list_and_unicode_stream(self, *args, **kwargs):
        return None  # inert stub
    def as_font_resource(self, *args, **kwargs):
        return None  # inert stub
    def _add_to_writer(self, *args, **kwargs):
        return None  # inert stub
    def get_text_width(self, *args, **kwargs):
        return None  # inert stub
    def can_encode(self, *args, **kwargs):
        return None  # inert stub
