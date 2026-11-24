"""Non-markup annotations.

Rust port of ``pypdf/annotations/_non_markup_annotations.py`` (upstream pypdf
7.1.1).  The class hierarchy and keyword-only signatures are kept verbatim;
every ``__init__`` delegates to
``crates/pypdf-py/src/modules/annotations_mod.rs``.
"""

from collections.abc import Sequence
from typing import TYPE_CHECKING, Any

import pypdf._pypdf as _rust

from ..generic._base import (
    BooleanObject,
    NameObject,
    NumberObject,
    TextStringObject,
)
from ..generic._data_structures import ArrayObject, DictionaryObject
from ..generic._fit import DEFAULT_FIT, Fit
from ..generic._rectangle import RectangleObject
from ._base import AnnotationDictionary


class Link(AnnotationDictionary):
    def __init__(
        self,
        *,
        rect: RectangleObject | tuple[float, float, float, float],
        border: Sequence[Any] | None = None,
        url: str | None = None,
        target_page_index: int | None = None,
        fit: Fit = DEFAULT_FIT,
        **kwargs: Any,
    ) -> None:
        _rust.annotationsmod_link_init(
            self, rect, border, url, target_page_index, fit, kwargs
        )


class Popup(AnnotationDictionary):
    def __init__(
        self,
        *,
        rect: RectangleObject | tuple[float, float, float, float],
        parent: DictionaryObject | None = None,
        open: bool = False,
        **kwargs: Any,
    ) -> None:
        _rust.annotationsmod_popup_init(self, rect, parent, open, kwargs)
