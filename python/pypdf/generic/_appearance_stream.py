"""Appearance streams for annotations (Rust port of ``pypdf/generic/_appearance_stream.py``, pypdf 7.1.1).

The dataclass (``BaseStreamConfig``), the ``IntEnum`` (``TextAlignment``), the
named tuple (``WidthWordGlyphs``) and the ``DecodedStreamObject`` subclasses are
kept verbatim from upstream: they are observable Python API (dataclass field
defaults, ``IntEnum`` identity, named-tuple unpacking, MRO and ``isinstance``).
All behaviour lives in ``crates/pypdf-py/src/modules/appearance_mod.rs``; the
method bodies below only delegate.

``arabic-reshaper`` and ``python-bidi`` are optional dependencies that upstream
imports here; the Rust implementation calls them through PyO3 at exactly the
same points, and ``HAS_RTL_SUPPORT`` keeps upstream's detection semantics
(``ImportError`` -> ``False``).  The module-level names imported below are the
globals the Rust port resolves dynamically, so ``mock.patch`` of e.g.
``HAS_RTL_SUPPORT`` behaves exactly as with upstream.
"""

from __future__ import annotations

import copy
from dataclasses import dataclass, field
from enum import IntEnum
from io import BytesIO
from operator import attrgetter
from typing import TYPE_CHECKING, Any, NamedTuple, cast

from pypdf._pypdf import (
    appearance_base_stream_appearance_add_matrix as _base_stream_appearance_add_matrix,
    appearance_base_stream_appearance_init as _base_stream_appearance_init,
    appearance_text_stream_appearance_find_annotation_font_resource as _find_annotation_font_resource,
    appearance_text_stream_appearance_from_text_annotation as _from_text_annotation,
    appearance_text_stream_appearance_generate_appearance_stream_data as _generate_appearance_stream_data,
    appearance_text_stream_appearance_init as _text_stream_appearance_init,
    appearance_text_stream_appearance_parse_default_appearance as _parse_default_appearance,
    appearance_text_stream_appearance_scale_text as _scale_text,
    appearance_text_stream_appearance_sync_appearance_stream_font_resources as _sync_appearance_stream_font_resources,
    appearance_transform_annotation_appearance as _transform_annotation_appearance,
)

from .._codecs import encoding_dict_from_named_encoding
from .._codecs.core_font_metrics import CORE_FONT_METRICS
from .._page import Transformation
from .._utils import is_char_rtl, logger_warning
from ..constants import AnnotationDictionaryAttributes, BorderStyles, FieldDictionaryAttributes, PageAttributes
from ..errors import PdfReadError
from ..generic import (
    ArrayObject,
    ContentStream,
    DecodedStreamObject,
    DictionaryObject,
    FloatObject,
    IndirectObject,
    NameObject,
    NumberObject,
    RectangleObject,
    StreamObject,
)
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

# "The glyph widths shall be measured in units in which 1000 units correspond to 1 unit in text space"
# (Table 111, PDF Specification 2.0)
TEXT_SPACE_TO_GLYPH_SPACE_FACTOR = 1000


@dataclass
class BaseStreamConfig:
    """A container representing the basic layout of an appearance stream."""
    rectangle: RectangleObject = field(default_factory=lambda: RectangleObject((0.0, 0.0, 0.0, 0.0)))
    border_width: int = 1  # The width of the border in points
    border_style: str = BorderStyles.SOLID
    border_color: Color | None = None
    background_color: Color | None = None
    rotation: int = 0


class BaseStreamAppearance(DecodedStreamObject):
    """A class representing the very base of an appearance stream, that is, a rectangle and a border."""

    def _add_matrix(self, rotation: int) -> None:
        """Rotate the appearance stream, keeping its origin at ``(0, 0)`` (delegated to Rust)."""
        return _base_stream_appearance_add_matrix(self, rotation)

    def __init__(self, layout: BaseStreamConfig | None) -> None:
        """
        Takes the appearance stream layout as an argument.

        Args:
            layout: The basic layout parameters.

        """
        return _base_stream_appearance_init(self, layout)


class TextAlignment(IntEnum):
    """Defines the alignment options for text within a form field's appearance stream."""

    LEFT = 0
    CENTER = 1
    RIGHT = 2


class WidthWordGlyphs(NamedTuple):
    """A tuple of the unscaled width of a word (unencoded text) and the font-encoded glyphs that represent it."""

    width: float
    word: str
    glyphs: str


class TextStreamAppearance(BaseStreamAppearance):
    """
    A class representing the appearance stream for a text-based form field.

    This class generates the content stream (the `ap_stream_data`) that dictates
    how text is rendered within a form field's bounding box. It handles properties
    like font, font size, color, multiline text, and text selection highlighting.
    """

    def _scale_text(
        self,
        font: Font,
        font_size: float,
        leading_factor: float,
        field_width: float,
        field_height: float,
        paragraphs: list[list[WidthWordGlyphs]],
        min_font_size: float,
        font_size_step: float = 0.2
    ) -> tuple[list[WidthWordGlyphs], float]:
        """
        Scale the text to ``field_width``/``field_height``, wrapping where necessary (delegated to Rust).

        Args:
            font: The font to be used.
            font_size: The font size in points.
            leading_factor: The line distance.
            field_width: The width of the field in which to fit the text.
            field_height: The height of the field in which to fit the text.
            paragraphs: The list of text paragraphs to fit with the field.
            min_font_size: The minimum font size at which to scale the text.
            font_size_step: The amount by which to decrement font size per step while scaling.

        Returns:
            The wrapped lines and the font size for these lines.

        """
        return _scale_text(
            self,
            font,
            font_size,
            leading_factor,
            field_width,
            field_height,
            paragraphs,
            min_font_size,
            font_size_step,
        )

    def _generate_appearance_stream_data(
        self,
        text: str,
        selection: list[str] | None,
        font: Font,
        font_name: str = "/Helv",
        font_size: float = 0.0,
        font_color: Color | None = None,
        is_multiline: bool = False,
        alignment: TextAlignment = TextAlignment.LEFT,
        is_comb: bool = False,
        max_length: int | None = None
    ) -> bytes:
        """
        Generate the raw bytes of the PDF appearance stream for a text field (delegated to Rust).

        Args:
            text: The text to be rendered in the form field.
            selection: An optional list of strings that should be highlighted as selected.
            font: The font to use.
            font_name: The name of the font resource to use (e.g., "/Helv").
            font_size: The font size. If 0, it is automatically calculated.
            font_color: The color to apply to the font.
            is_multiline: A boolean indicating if the text field is multiline.
            alignment: Text alignment, can be TextAlignment.LEFT, .RIGHT, or .CENTER.
            is_comb: Boolean that designates fixed-length fields.
            max_length: Used if is_comb is set. The maximum number of characters.

        Returns:
            A byte string containing the PDF content stream data.

        """
        return _generate_appearance_stream_data(
            self,
            text,
            selection,
            font,
            font_name,
            font_size,
            font_color,
            is_multiline,
            alignment,
            is_comb,
            max_length,
        )

    def __init__(
        self,
        layout: BaseStreamConfig | None = None,
        text: str = "",
        selection: list[str] | None = None,
        font: Font | None = None,
        font_resource: DictionaryObject | IndirectObject | None = None,
        font_name: str = "/Helv",
        font_size: float = 0.0,
        font_color: Color | None = None,
        is_multiline: bool = False,
        alignment: TextAlignment = TextAlignment.LEFT,
        is_comb: bool = False,
        max_length: int | None = None
    ) -> None:
        """
        Initializes a TextStreamAppearance object (delegated to Rust).

        Args:
            layout: The basic layout parameters.
            text: The text to be rendered in the form field.
            selection: An optional list of strings that should be highlighted as selected.
            font: A Font object. Falls back to Type 1 Helvetica if not given.
            font_resource: An optional variable that represents a PDF font dictionary.
            font_name: The name of the font resource, e.g., "/Helv".
            font_size: The font size. If 0, it's auto-calculated.
            font_color: The font color string.
            is_multiline: A boolean indicating if the text field is multiline.
            alignment: Text alignment, can be TextAlignment.LEFT, .RIGHT, or .CENTER.
            is_comb: Boolean that designates fixed-length fields.
            max_length: Used if is_comb is set. The maximum number of characters.

        """
        return _text_stream_appearance_init(
            self,
            layout,
            text,
            selection,
            font,
            font_resource,
            font_name,
            font_size,
            font_color,
            is_multiline,
            alignment,
            is_comb,
            max_length,
        )

    @staticmethod
    def _find_annotation_font_resource(
        font_name: str,
        annotation: DictionaryObject,
        acro_form: DictionaryObject,
        text: str
    ) -> tuple[str, Font]:
        """Find (or rebuild) the font resource holding the field's text (delegated to Rust)."""
        return _find_annotation_font_resource(font_name, annotation, acro_form, text)

    @staticmethod
    def _sync_appearance_stream_font_resources(
        writer: PdfWriter,
        font_name: str,
        font: Font,
        target_resource_dict: DictionaryObject,
        page: PageObject | None = None
    ) -> IndirectObject:
        """
        Unified helper to sync fonts from an AP stream to a target resource dictionary (delegated to Rust).

        Will sync to page resources as well when page is added to the arguments.
        """
        return _sync_appearance_stream_font_resources(writer, font_name, font, target_resource_dict, page)

    @staticmethod
    def _parse_default_appearance(
        default_appearance: TextStringObject | ByteStringObject,
    ) -> tuple[str | None, float, Color | None]:
        """
        Read the font name, font size and font color from a default appearance string (delegated to Rust).

        Args:
            default_appearance: The /DA string of a variable text field.

        Returns:
            The font name and font size of the last complete Tf operator, or None and 0 if there is none,
            and the color of the last complete color operator, or None if there is none.
        """
        return _parse_default_appearance(default_appearance)

    @classmethod
    def from_text_annotation(
        cls,
        writer: PdfWriter,
        page: PageObject,
        flatten: bool,
        acro_form: DictionaryObject,  # _root_object[CatalogAttributes.ACRO_FORM])
        field: DictionaryObject,
        annotation: DictionaryObject,
        user_font_name: str = "",
        user_font_size: float = -1,
    ) -> TextStreamAppearance:
        """
        Creates a TextStreamAppearance object from a text field annotation (delegated to Rust).

        Args:
            writer: The PdfWriter instance that we are creating text stream appearances for.
            page: The page that we are processing annotations for.
            flatten: Whether we flatten text annotations or not.
            acro_form: The root AcroForm dictionary from the PDF catalog.
            field: The field dictionary object.
            annotation: The widget annotation dictionary object associated with the field.
            user_font_name: An optional user-provided font name to override the default.
            user_font_size: An optional user-provided font size to override the default.

        Returns:
            A new `TextStreamAppearance` instance configured for the given field.

        """
        return _from_text_annotation(
            cls,
            writer,
            page,
            flatten,
            acro_form,
            field,
            annotation,
            user_font_name,
            user_font_size,
        )


def transform_annotation_appearance(annotation_obj: DictionaryObject, transformation: Transformation) -> None:
    """
    Compose `transformation` into an annotation's /AP /N appearance stream(s), in place (delegated to Rust).

    Repositioning/resizing an annotation's /Rect alone is not enough: per
    the appearance-stream algorithm (PDF 2.0, 12.5.5), a viewer fits the
    appearance's /BBox (as mapped by its own /Matrix) into /Rect using an
    axis-aligned scale, never a rotation. Composing the transform into /Matrix
    (rather than overwriting it) keeps the rendered content consistent with the
    rest of the transformed page while preserving whatever the annotation
    already had. Handles both a single appearance stream and the multi-state
    /AP /N dict used by checkbox/radio-button widgets.
    """
    return _transform_annotation_appearance(annotation_obj, transformation)
