"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.annotations._markup_annotations``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import uuid
from abc import ABC
from typing import Any, Literal, TypeAlias
from ..constants import AnnotationFlag
from ..generic import ArrayObject, DictionaryObject
from ..generic._base import BooleanObject, FloatObject, IndirectObject, NameObject, NumberObject, TextStringObject
from ..generic._rectangle import RectangleObject
from ..generic._utils import hex_to_rgb
from ._base import NO_FLAGS, AnnotationDictionary
Vertex = None
def _get_bounding_rectangle(*args, **kwargs):
    return None  # inert stub: pypdf.annotations._markup_annotations._get_bounding_rectangle
class MarkupAnnotation:
    """Stub for pypdf.annotations._markup_annotations.MarkupAnnotation."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
class Text(MarkupAnnotation):
    """Stub for pypdf.annotations._markup_annotations.Text."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
class FreeText(MarkupAnnotation):
    """Stub for pypdf.annotations._markup_annotations.FreeText."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
class Line(MarkupAnnotation):
    """Stub for pypdf.annotations._markup_annotations.Line."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
class PolyLine(MarkupAnnotation):
    """Stub for pypdf.annotations._markup_annotations.PolyLine."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
class Rectangle(MarkupAnnotation):
    """Stub for pypdf.annotations._markup_annotations.Rectangle."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
class Highlight(MarkupAnnotation):
    """Stub for pypdf.annotations._markup_annotations.Highlight."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
class Ellipse(MarkupAnnotation):
    """Stub for pypdf.annotations._markup_annotations.Ellipse."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
class Polygon(MarkupAnnotation):
    """Stub for pypdf.annotations._markup_annotations.Polygon."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
