"""Markup annotations.

Rust port of ``pypdf/annotations/_markup_annotations.py`` (upstream pypdf 7.1.1).
The class hierarchy, keyword-only signatures and class-level type aliases are
kept verbatim; every ``__init__`` delegates to
``crates/pypdf-py/src/modules/annotations_mod.rs``.
"""

import uuid
from abc import ABC
from typing import Any, Literal, TypeAlias

import pypdf._pypdf as _rust

from ..constants import AnnotationFlag
from ..generic import ArrayObject, DictionaryObject
from ..generic._base import (
    BooleanObject,
    FloatObject,
    IndirectObject,
    NameObject,
    NumberObject,
    TextStringObject,
)
from ..generic._rectangle import RectangleObject
from ..generic._utils import hex_to_rgb
from ._base import NO_FLAGS, AnnotationDictionary

Vertex: TypeAlias = tuple[float, float]


def _get_bounding_rectangle(vertices: list[Vertex]) -> RectangleObject:
    return _rust.annotationsmod_get_bounding_rectangle(vertices)


class MarkupAnnotation(AnnotationDictionary, ABC):
    """
    Base class for all markup annotations.

    Args:
        title_bar: Text to be displayed in the title bar of the annotation;
            by convention this is the name of the author
        in_reply_to: The annotation that this annotation is "in reply to"
            (PDF 1.5). Can be either an annotation (previously added using
            :meth:`~pypdf.PdfWriter.add_annotation`) or a reference to the
            target annotation.
        reply_type: The relationship between this annotation and the one
            specified by ``in_reply_to``. Either ``"R"`` (a reply, default)
            or ``"Group"`` (grouped with the parent annotation). Raises
            ``ValueError`` if a non-default value is provided without
            ``in_reply_to``.
        annotation_name: A text string uniquely identifying this annotation
            among all annotations on its page. Automatically generated when
            ``in_reply_to`` is set and no name is provided. Raises
            ``ValueError`` if provided without ``in_reply_to``.

    """

    def __init__(
        self,
        *,
        title_bar: str | None = None,
        in_reply_to: DictionaryObject | IndirectObject | None = None,
        reply_type: Literal["R", "Group"] = "R",
        annotation_name: str | None = None,
    ) -> None:
        _rust.annotationsmod_markup_annotation_init(
            self, title_bar, in_reply_to, reply_type, annotation_name
        )


class Text(MarkupAnnotation):
    """
    A text annotation.

    Args:
        rect: array of four integers ``[xLL, yLL, xUR, yUR]``
            specifying the clickable rectangular area
        text: The text that is added to the document
        open:
        flags:

    """

    def __init__(
        self,
        *,
        rect: RectangleObject | tuple[float, float, float, float],
        text: str,
        open: bool = False,
        flags: int = NO_FLAGS,
        **kwargs: Any,
    ) -> None:
        _rust.annotationsmod_text_init(self, rect, text, open, flags, kwargs)


class FreeText(MarkupAnnotation):
    """A FreeText annotation"""

    def __init__(
        self,
        *,
        text: str,
        rect: RectangleObject | tuple[float, float, float, float],
        font: str = "Helvetica",
        bold: bool = False,
        italic: bool = False,
        font_size: str = "14pt",
        font_color: str = "000000",
        border_color: str | None = "000000",
        background_color: str | None = "ffffff",
        **kwargs: Any,
    ) -> None:
        _rust.annotationsmod_free_text_init(
            self,
            text,
            rect,
            font,
            bold,
            italic,
            font_size,
            font_color,
            border_color,
            background_color,
            kwargs,
        )


class Line(MarkupAnnotation):
    def __init__(
        self,
        p1: Vertex,
        p2: Vertex,
        rect: RectangleObject | tuple[float, float, float, float],
        text: str = "",
        **kwargs: Any,
    ) -> None:
        _rust.annotationsmod_line_init(self, p1, p2, rect, text, kwargs)


class PolyLine(MarkupAnnotation):
    def __init__(
        self,
        vertices: list[Vertex],
        **kwargs: Any,
    ) -> None:
        _rust.annotationsmod_polyline_init(self, vertices, kwargs)


class Rectangle(MarkupAnnotation):
    def __init__(
        self,
        rect: RectangleObject | tuple[float, float, float, float],
        *,
        interior_color: str | None = None,
        border_color: str | None = None,
        **kwargs: Any,
    ) -> None:
        _rust.annotationsmod_rectangle_init(
            self, rect, interior_color, border_color, kwargs
        )


class Highlight(MarkupAnnotation):
    def __init__(
        self,
        *,
        rect: RectangleObject | tuple[float, float, float, float],
        quad_points: ArrayObject,
        highlight_color: str = "ff0000",
        printing: bool = False,
        **kwargs: Any,
    ) -> None:
        _rust.annotationsmod_highlight_init(
            self, rect, quad_points, highlight_color, printing, kwargs
        )


class Ellipse(MarkupAnnotation):
    def __init__(
        self,
        rect: RectangleObject | tuple[float, float, float, float],
        *,
        interior_color: str | None = None,
        **kwargs: Any,
    ) -> None:
        _rust.annotationsmod_ellipse_init(self, rect, interior_color, kwargs)


class Polygon(MarkupAnnotation):
    def __init__(
        self,
        vertices: list[tuple[float, float]],
        **kwargs: Any,
    ) -> None:
        _rust.annotationsmod_polygon_init(self, vertices, kwargs)

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
