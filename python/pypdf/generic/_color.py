"""Colour factory classes (Rust port of ``pypdf/generic/_color.py``, pypdf 7.1.1).

The dataclasses, their fields, defaults, class attributes and (auto-generated)
``__init__``/``__eq__``/``__repr__`` are kept verbatim from upstream: they are
observable Python API (``DeviceGray(0) == DeviceGray(0)``, ``repr``,
``isinstance``).  All behaviour lives in ``crates/pypdf-py/src/modules/
genbasic_mod.rs``; the method bodies below only delegate.
"""

from collections.abc import Sequence
from dataclasses import dataclass, field
from typing import ClassVar, Union

from pypdf._pypdf import (
    genbasic_color_as_operator as _color_as_operator,
    genbasic_color_from_normalized_values as _color_from_normalized_values,
)


@dataclass
class Color:
    """
    A factory class to generate one of class DeviceGray, DeviceRGB or DeviceCMYK. Call with
    Color.from_normalized_values() on a tuple of length 1 for grayscale, 3 for RGB, or 4 for CMYK.
    """
    color_operator: ClassVar[str] = field(init=False)
    _ordered_fields: ClassVar[tuple[str, ...]]

    @classmethod
    def from_normalized_values(
        cls,
        color: Sequence[float] | None,
        operator: str | None = None,
    ) -> Union["Color", None]:
        """
        Method to instantiate a color class. Can be called with value of None for cases where an appearance
        characteristics dictionary contains an empty "/BG" or "/BC" value,  which returns None and signifies
        transparent color. See Table 189, "Entries in an appearance characteristics dictionary" of the PDF
        specification 1.7.

        Args:
            color: A sequence of 1 (for DeviceGray), 3 (for DeviceRGB) or 4 (for DeviceCMYK) float values
                in the range of 0.0 to 1.0 (representing normalized color channel values), or None to return None.
            operator: The non-stroking color operator the values are the operands of, that is, "g", "rg" or "k"
                (Table 73, "Colour operators" of the PDF specification 2.0), as in a default appearance string.
                If given, None is returned unless it is the operator of the color class the values result in.
        """
        return _color_from_normalized_values(color, operator)

    def as_operator(self, stroke: bool = False) -> str:
        """
        Returns the PDF color operator as a string.

        Args:
            stroke: Returns stroke (i.e., uppercase) color operator if True
        """
        return _color_as_operator(self, stroke)


@dataclass
class DeviceGray(Color):
    gray: float = 0.0

    color_operator = "g"
    _ordered_fields = ("gray",)


@dataclass
class DeviceRGB(Color):
    red: float = 0.0
    green: float = 0.0
    blue: float = 0.0

    color_operator = "rg"
    _ordered_fields = ("red", "green", "blue")


@dataclass
class DeviceCMYK(Color):
    cyan: float = 0.0
    magenta: float = 0.0
    yellow: float = 0.0
    black: float = 1.0

    color_operator = "k"
    _ordered_fields = ("cyan", "magenta", "yellow", "black")
