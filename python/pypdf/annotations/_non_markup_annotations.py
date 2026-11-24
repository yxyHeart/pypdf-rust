"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.annotations._non_markup_annotations``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

from collections.abc import Sequence
from typing import TYPE_CHECKING, Any
from ..generic._base import BooleanObject, NameObject, NumberObject, TextStringObject
from ..generic._data_structures import ArrayObject, DictionaryObject
from ..generic._fit import DEFAULT_FIT, Fit
from ..generic._rectangle import RectangleObject
from ._base import AnnotationDictionary
class Link(AnnotationDictionary):
    """Stub for pypdf.annotations._non_markup_annotations.Link."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
class Popup(AnnotationDictionary):
    """Stub for pypdf.annotations._non_markup_annotations.Popup."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
