"""Page boxes (Rust port of ``pypdf/generic/_rectangle.py``, pypdf 7.1.1).

``RectangleObject`` subclasses the *Python* ``ArrayObject`` class from
``pypdf.generic._data_structures`` exactly like upstream; every method and
property body delegates to ``crates/pypdf-py/src/modules/genbasic_mod.rs``.
"""

from collections.abc import Sequence
from typing import Any

from pypdf._pypdf import (
    genbasic_rectangleobject_ensure_is_number as _rectangleobject_ensure_is_number,
    genbasic_rectangleobject_get_bottom as _rectangleobject_get_bottom,
    genbasic_rectangleobject_get_left as _rectangleobject_get_left,
    genbasic_rectangleobject_get_lower_left as _rectangleobject_get_lower_left,
    genbasic_rectangleobject_get_lower_right as _rectangleobject_get_lower_right,
    genbasic_rectangleobject_get_right as _rectangleobject_get_right,
    genbasic_rectangleobject_get_top as _rectangleobject_get_top,
    genbasic_rectangleobject_get_upper_left as _rectangleobject_get_upper_left,
    genbasic_rectangleobject_get_upper_right as _rectangleobject_get_upper_right,
    genbasic_rectangleobject_height as _rectangleobject_height,
    genbasic_rectangleobject_new as _rectangleobject_new,
    genbasic_rectangleobject_repr as _rectangleobject_repr,
    genbasic_rectangleobject_scale as _rectangleobject_scale,
    genbasic_rectangleobject_set_bottom as _rectangleobject_set_bottom,
    genbasic_rectangleobject_set_left as _rectangleobject_set_left,
    genbasic_rectangleobject_set_lower_left as _rectangleobject_set_lower_left,
    genbasic_rectangleobject_set_lower_right as _rectangleobject_set_lower_right,
    genbasic_rectangleobject_set_right as _rectangleobject_set_right,
    genbasic_rectangleobject_set_top as _rectangleobject_set_top,
    genbasic_rectangleobject_set_upper_left as _rectangleobject_set_upper_left,
    genbasic_rectangleobject_set_upper_right as _rectangleobject_set_upper_right,
    genbasic_rectangleobject_width as _rectangleobject_width,
)

from ._base import FloatObject, NumberObject
from ._data_structures import ArrayObject


class RectangleObject(ArrayObject):
    """
    This class is used to represent *page boxes* in pypdf.

    These boxes include:

    * :attr:`artbox <pypdf._page.PageObject.artbox>`
    * :attr:`bleedbox <pypdf._page.PageObject.bleedbox>`
    * :attr:`cropbox <pypdf._page.PageObject.cropbox>`
    * :attr:`mediabox <pypdf._page.PageObject.mediabox>`
    * :attr:`trimbox <pypdf._page.PageObject.trimbox>`
    """

    def __init__(self, arr: Sequence[Any]) -> None:
        return _rectangleobject_new(self, arr)

    def _ensure_is_number(self, value: Any) -> FloatObject | NumberObject:
        return _rectangleobject_ensure_is_number(self, value)

    def scale(self, sx: float, sy: float) -> "RectangleObject":
        return _rectangleobject_scale(self, sx, sy)

    def __repr__(self) -> str:
        return _rectangleobject_repr(self)

    @property
    def left(self) -> FloatObject:
        return _rectangleobject_get_left(self)

    @left.setter
    def left(self, f: float) -> None:
        return _rectangleobject_set_left(self, f)

    @property
    def bottom(self) -> FloatObject:
        return _rectangleobject_get_bottom(self)

    @bottom.setter
    def bottom(self, f: float) -> None:
        return _rectangleobject_set_bottom(self, f)

    @property
    def right(self) -> FloatObject:
        return _rectangleobject_get_right(self)

    @right.setter
    def right(self, f: float) -> None:
        return _rectangleobject_set_right(self, f)

    @property
    def top(self) -> FloatObject:
        return _rectangleobject_get_top(self)

    @top.setter
    def top(self, f: float) -> None:
        return _rectangleobject_set_top(self, f)

    @property
    def lower_left(self) -> tuple[float, float]:
        """
        Property to read and modify the lower left coordinate of this box
        in (x,y) form.
        """
        return _rectangleobject_get_lower_left(self)

    @lower_left.setter
    def lower_left(self, value: tuple[float, float]) -> None:
        return _rectangleobject_set_lower_left(self, value)

    @property
    def lower_right(self) -> tuple[float, float]:
        """
        Property to read and modify the lower right coordinate of this box
        in (x,y) form.
        """
        return _rectangleobject_get_lower_right(self)

    @lower_right.setter
    def lower_right(self, value: tuple[float, float]) -> None:
        return _rectangleobject_set_lower_right(self, value)

    @property
    def upper_left(self) -> tuple[float, float]:
        """
        Property to read and modify the upper left coordinate of this box
        in (x,y) form.
        """
        return _rectangleobject_get_upper_left(self)

    @upper_left.setter
    def upper_left(self, value: tuple[float, float]) -> None:
        return _rectangleobject_set_upper_left(self, value)

    @property
    def upper_right(self) -> tuple[float, float]:
        """
        Property to read and modify the upper right coordinate of this box
        in (x,y) form.
        """
        return _rectangleobject_get_upper_right(self)

    @upper_right.setter
    def upper_right(self, value: tuple[float, float]) -> None:
        return _rectangleobject_set_upper_right(self, value)

    @property
    def width(self) -> float:
        return _rectangleobject_width(self)

    @property
    def height(self) -> float:
        return _rectangleobject_height(self)

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
