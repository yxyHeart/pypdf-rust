"""Annotation base class.

Rust port of ``pypdf/annotations/_base.py`` (upstream pypdf 7.1.1).  The class
hierarchy is kept verbatim; the logic lives in
``crates/pypdf-py/src/modules/annotations_mod.rs``.
"""

from abc import ABC

from pypdf._pypdf import (
    annotationsmod_annotation_dictionary_flags_get as _annotation_dictionary_flags_get,
    annotationsmod_annotation_dictionary_flags_set as _annotation_dictionary_flags_set,
    annotationsmod_annotation_dictionary_init as _annotation_dictionary_init,
    annotationsmod_no_flags as _no_flags,
)

from ..constants import AnnotationFlag
from ..generic import NameObject, NumberObject
from ..generic._data_structures import DictionaryObject


class AnnotationDictionary(DictionaryObject, ABC):
    def __init__(self) -> None:
        _annotation_dictionary_init(self)
        # /Rect should not be added here as Polygon and PolyLine can automatically set it
        # The flags were NOT added to the constructor on purpose:
        # We expect that most users don't want to change the default.
        # If they do, they can use the property. The default is 0.

    @property
    def flags(self) -> AnnotationFlag:
        return _annotation_dictionary_flags_get(self)

    @flags.setter
    def flags(self, value: AnnotationFlag) -> None:
        _annotation_dictionary_flags_set(self, value)


NO_FLAGS = _no_flags()

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
