"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.generic._appearance_stream``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import copy
from dataclasses import dataclass, field
from enum import IntEnum
from io import BytesIO
from operator import attrgetter
from typing import TYPE_CHECKING, Any, NamedTuple, cast
from .._codecs import encoding_dict_from_named_encoding
from .._codecs.core_font_metrics import CORE_FONT_METRICS
from .._page import Transformation
from .._utils import is_char_rtl, logger_warning
from ..constants import AnnotationDictionaryAttributes, BorderStyles, FieldDictionaryAttributes, PageAttributes
from ..errors import PdfReadError
from ..generic import ArrayObject, ContentStream, DecodedStreamObject, DictionaryObject, FloatObject, IndirectObject, NameObject, NumberObject, RectangleObject, StreamObject
from ..generic._base import ByteStringObject, TextStringObject
from ._color import Color, DeviceGray
from ._font import Font
if TYPE_CHECKING:
    from pypdf._writer import PdfWriter
    from .._page import PageObject
try:
    import arabic_reshaper
    from bidi.algorithm import get_display
    HAS_RTL_SUPPORT = True
except ImportError:
    HAS_RTL_SUPPORT = False
DEFAULT_FONT_SIZE_IN_MULTILINE = 12
TEXT_SPACE_TO_GLYPH_SPACE_FACTOR = 1000
class BaseStreamConfig:
    """Stub for pypdf.generic._appearance_stream.BaseStreamConfig."""
    rectangle = None
    border_width = None
    border_style = None
    border_color = None
    background_color = None
    rotation = None
class BaseStreamAppearance(DecodedStreamObject):
    """Stub for pypdf.generic._appearance_stream.BaseStreamAppearance."""
    def _add_matrix(self, *args, **kwargs):
        return None  # inert stub
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
class TextAlignment:
    """Stub for pypdf.generic._appearance_stream.TextAlignment."""
    LEFT = 0
    CENTER = 1
    RIGHT = 2
class WidthWordGlyphs:
    """Stub for pypdf.generic._appearance_stream.WidthWordGlyphs."""
    width = None
    word = None
    glyphs = None
class TextStreamAppearance(BaseStreamAppearance):
    """Stub for pypdf.generic._appearance_stream.TextStreamAppearance."""
    def _scale_text(self, *args, **kwargs):
        return None  # inert stub
    def _generate_appearance_stream_data(self, *args, **kwargs):
        return None  # inert stub
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    @staticmethod
    def _find_annotation_font_resource(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _sync_appearance_stream_font_resources(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _parse_default_appearance(*args, **kwargs):
        return None  # inert stub
    @classmethod
    def from_text_annotation(cls, *args, **kwargs):
        return None  # inert stub
def transform_annotation_appearance(*args, **kwargs):
    return None  # inert stub: pypdf.generic._appearance_stream.transform_annotation_appearance
