"""Font objects (Rust port of ``pypdf/generic/_font.py``, pypdf 7.1.1).

The dataclasses (``FontDescriptor``, ``CoreFontMetrics``, ``Font``), their
fields/defaults/decorators and the module constants are kept verbatim from
upstream: they are observable Python API (``dataclasses.asdict(font)``,
``FontDescriptor._DEFAULT_BBOX``, ``isinstance``, ``mock.patch`` targets).
All behaviour lives in ``crates/pypdf-py/src/modules/font_mod.rs``; the method
bodies below only delegate.

fontTools is only *read* by pypdf, so the Rust port calls the installed
``fontTools`` package through PyO3 exactly where upstream does instead of
reimplementing it; ``HAS_FONTTOOLS`` keeps upstream's detection semantics.
"""

from __future__ import annotations

import unicodedata
from collections.abc import Sequence
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, ClassVar, cast

from pypdf._pypdf import (
    font_add_default_width as _rust_add_default_width,
    font_add_space_width as _rust_add_space_width,
    font_add_to_writer as _rust_add_to_writer,
    font_as_font_resource as _rust_as_font_resource,
    font_can_encode as _rust_can_encode,
    font_check_entry_count as _rust_check_entry_count,
    font_check_range_length as _rust_check_range_length,
    font_collect_cid_character_widths as _rust_collect_cid_character_widths,
    font_collect_tt_t1_character_widths as _rust_collect_tt_t1_character_widths,
    font_create_widths_list_and_unicode_stream as _rust_create_widths_list_and_unicode_stream,
    font_font_descriptor_as_font_descriptor_resource as _rust_font_descriptor_as_font_descriptor_resource,
    font_from_core_font_name as _rust_from_core_font_name,
    font_from_font_resource as _rust_from_font_resource,
    font_from_truetype_font_file as _rust_from_truetype_font_file,
    font_get_font_flags_from_truetype_font_tables as _rust_get_font_flags_from_truetype_font_tables,
    font_get_space_char as _rust_get_space_char,
    font_get_text_width as _rust_get_text_width,
    font_get_typographic_maps as _rust_get_typographic_maps,
    font_load_fonttools_table as _rust_load_fonttools_table,
    font_parse_bbox as _rust_parse_bbox,
    font_parse_font_descriptor as _rust_parse_font_descriptor,
)
from pypdf.generic import (
    ArrayObject,
    DictionaryObject,
    FloatObject,
    IndirectObject,
    NameObject,
    NumberObject,
    PdfObject,
    StreamObject,
    TextStringObject,
)

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


# Limits.
MAX_CID_WIDTH_ENTRY_COUNT = 65_536
MAX_WIDTH_ENTRY_COUNT = 100_000
# For a simple font, character codes are one-byte values, 0-255, so /Widths can have at most 256 meaningful entries.
MAX_SIMPLE_FONT_CHARACTER_CODE = 255


# Some constants from truetype font tables that we use:
HEADER_MACSTYLE_ITALIC = 0x02
OS2_FSSELECTION_ITALIC = 0x01
OS2_PANOSE_BFAMILYTYPE_SCRIPT = 3
OS2_PANOSE_BFAMILYTYPE_DECORATIVE = 4
OS2_PANOSE_BFAMILYTYPE_PICTORIAL = 5
OS2_PANOSE_BPROPORTION_MONOSPACED = 9
OS2_SFAMILYSCLASS_SCRIPTS = 10
OS2_SFAMILYSCLASS_SYMBOLIC = 12


# Groups of CMap mappings cannot exceed 100 entries (Adobe CMap and CIDFont Files Specification 1.0, p. 49).
CMAP_MAX_ENTRIES_PER_GROUP = 100


@dataclass(frozen=True)
class FontDescriptor:
    """
    Represents the FontDescriptor dictionary as defined in the PDF specification.
    This contains both descriptive and metric information.

    The defaults are derived from the mean values of the 14 core fonts, rounded
    to 100.
    """

    _DEFAULT_BBOX: ClassVar[tuple[float, float, float, float]] = (-100.0, -200.0, 1000.0, 900.0)

    name: str = "Unknown"
    family: str = "Unknown"
    weight: str = "Unknown"

    ascent: float = 700.0
    descent: float = -200.0
    cap_height: float = 600.0
    x_height: float = 500.0
    italic_angle: float = 0.0  # Non-italic
    flags: int = 32  # Non-serif, non-symbolic, not fixed width
    bbox: tuple[float, float, float, float] = _DEFAULT_BBOX
    font_file: StreamObject | None = None

    def as_font_descriptor_resource(self) -> DictionaryObject:
        """ ``pypdf.generic._font.FontDescriptor.as_font_descriptor_resource``, delegated to Rust."""
        return _rust_font_descriptor_as_font_descriptor_resource(self)


@dataclass(frozen=True)
class CoreFontMetrics:
    font_descriptor: FontDescriptor
    character_widths: dict[str, float]


@dataclass
class Font:
    """
    A font object for use during text extraction and for producing
    text appearance streams.

    Attributes:
        name: Font name, derived from ``font["/BaseFont"]``
        character_map: The font's character map
        encoding: Font encoding. Must be a dict for a simple font, and string otherwise.
        sub_type: The font type, such as Type1, TrueType, or Type3.
        font_descriptor: Font metrics, including a mapping of characters to widths
        character_widths: A mapping of characters to widths
        space_width: The width of a space, or an approximation
        interpretable: Default True. If False, the font glyphs cannot
            be translated to characters, e.g. Type3 fonts that do not define
            a ``/ToUnicode`` mapping.

    """

    name: str
    encoding: str | dict[int, str]
    character_map: dict[Any, Any] = field(default_factory=dict)
    sub_type: str = "Unknown"
    font_descriptor: FontDescriptor = field(default_factory=FontDescriptor)
    character_widths: dict[str, float] = field(default_factory=lambda: {"default": 500})
    space_width: float | int = 250
    space_char: str = " "
    interpretable: bool = True

    @staticmethod
    def _collect_tt_t1_character_widths(
        pdf_font_dict: DictionaryObject,
        char_map: dict[Any, Any],
        encoding: str | dict[int, str],
        current_widths: dict[str, float]
    ) -> None:
        """Parses a TrueType or Type1 font's /Widths array from a font dictionary and updates character widths"""
        return _rust_collect_tt_t1_character_widths(pdf_font_dict, char_map, encoding, current_widths)

    @staticmethod
    def __check_range_length(start: int, end: int) -> None:
        """ ``pypdf.generic._font.Font.__check_range_length``, delegated to Rust."""
        return _rust_check_range_length(start, end)

    @staticmethod
    def __check_entry_count(count: int, limit: int = MAX_WIDTH_ENTRY_COUNT) -> None:
        """ ``pypdf.generic._font.Font.__check_entry_count``, delegated to Rust."""
        return _rust_check_entry_count(count, limit)

    @staticmethod
    def _collect_cid_character_widths(d_font: DictionaryObject, current_widths: dict[str, float]) -> None:
        """Parses the /W array from a DescendantFont dictionary and updates character widths."""
        return _rust_collect_cid_character_widths(d_font, current_widths)

    @staticmethod
    def _get_space_char(
        encoding: str | dict[int, str],
        character_map: dict[Any, Any],
    ) -> str:
        """ ``pypdf.generic._font.Font._get_space_char``, delegated to Rust."""
        return _rust_get_space_char(encoding, character_map)

    @staticmethod
    def _add_default_width(current_widths: dict[str, float], flags: int, space_char: str) -> None:
        """ ``pypdf.generic._font.Font._add_default_width``, delegated to Rust."""
        return _rust_add_default_width(current_widths, flags, space_char)

    @staticmethod
    def _add_space_width(
        character_widths: dict[str, float],
        flags: int,
        space_char: str
    ) -> float:
        """ ``pypdf.generic._font.Font._add_space_width``, delegated to Rust."""
        return _rust_add_space_width(character_widths, flags, space_char)

    @staticmethod
    def _parse_bbox(raw_bbox: Any) -> tuple[float, float, float, float] | None:
        """
        Convert a raw /FontBBox value into four floats.

        Args:
            raw_bbox: The raw /FontBBox value read from the PDF.

        Returns:
            The four bounding box values, or ``None`` when the value is not
            a sequence of exactly four numbers, so that a malformed entry
            falls back to the default bounding box rather than raising.

        """
        return _rust_parse_bbox(raw_bbox)

    @staticmethod
    def _parse_font_descriptor(font_descriptor_obj: DictionaryObject) -> dict[str, Any]:
        """ ``pypdf.generic._font.Font._parse_font_descriptor``, delegated to Rust."""
        return _rust_parse_font_descriptor(font_descriptor_obj)

    @classmethod
    def from_font_resource(
        cls,
        pdf_font_dict: DictionaryObject,
    ) -> Font:
        """ ``pypdf.generic._font.Font.from_font_resource``, delegated to Rust."""
        return _rust_from_font_resource(cls, pdf_font_dict)

    @staticmethod
    def _load_fonttools_table(table_name: str, is_critical: bool, tt_font_object: TTFont) -> DefaultTable | None:
        """Once we have a TTFont object, we can try to get the necessary font tables from
        which we collect the information to instantiate a Font class. However, some of
        these tables might be missing from the embedded font file, and others might be
        corrupt in some form and then throw a struct.error on decompilation. We decompile
        all tables we need using this method to guard against fontTools exceptions.

        This method distinguishes between optional tables and critical tables without which
        we cannot instantiate a Font. For critical tables, raises PdfReadError if missing
        OR if binary data is corrupt. For optional tables, it logs warnings instead.
        """
        return _rust_load_fonttools_table(table_name, is_critical, tt_font_object)

    @staticmethod
    def _get_font_flags_from_truetype_font_tables(
            header_table: table__h_e_a_d,
            postscript_table: table__p_o_s_t,
            os2_table: table_O_S_2f_2
        ) -> int:
        """ ``pypdf.generic._font.Font._get_font_flags_from_truetype_font_tables``, delegated to Rust."""
        return _rust_get_font_flags_from_truetype_font_tables(header_table, postscript_table, os2_table)

    @classmethod
    def from_truetype_font_file(cls, font_file: BytesIO) -> Font:
        """ ``pypdf.generic._font.Font.from_truetype_font_file``, delegated to Rust."""
        return _rust_from_truetype_font_file(cls, font_file)

    @classmethod
    def from_core_font_name(cls, core_font_name: str = "/Helvetica") -> Font:
        """ ``pypdf.generic._font.Font.from_core_font_name``, delegated to Rust."""
        return _rust_from_core_font_name(cls, core_font_name)

    def _get_typographic_maps(self) -> tuple[dict[str, str], dict[str, bytes]]:
        """
        Generates maps to translate input unicode text to bytes in two steps:
        Unicode code point -> raw_character (reverse cmap) -> PDF bytes (encoding cmap).
        """
        return _rust_get_typographic_maps(self)

    def _create_widths_list_and_unicode_stream(self) -> tuple[list[PdfObject], StreamObject]:
        """ ``pypdf.generic._font.Font._create_widths_list_and_unicode_stream``, delegated to Rust."""
        return _rust_create_widths_list_and_unicode_stream(self)

    def as_font_resource(self) -> DictionaryObject:
        """ ``pypdf.generic._font.Font.as_font_resource``, delegated to Rust."""
        return _rust_as_font_resource(self)

    def _add_to_writer(
        self,
        writer: PdfWriter,
        target_resource_dict: DictionaryObject,
        font_resource_name: NameObject
    ) -> IndirectObject:
        """
        Some objects in a font resource need to be indirect objects. This method
        ensures that ToUnicode, FontDescriptor, FontFile, and, ultimately, the font
        resource itself, are registered with the PdfWriter instance as indirect objects.
        """
        return _rust_add_to_writer(self, writer, target_resource_dict, font_resource_name)

    def get_text_width(self, text: str = "") -> float:
        """Sum of character widths specified in PDF font for the supplied text."""
        return _rust_get_text_width(self, text)

    def can_encode(self, text: str) -> bool:
        """Check whether the font is able to encode a text string."""
        return _rust_can_encode(self, text)
