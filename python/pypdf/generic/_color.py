"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.generic._color``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass, field
from typing import ClassVar, Union
class Color:
    """Stub for pypdf.generic._color.Color."""
    color_operator = None
    _ordered_fields = None
    @classmethod
    def from_normalized_values(cls, *args, **kwargs):
        return None  # inert stub
    def as_operator(self, *args, **kwargs):
        return None  # inert stub
class DeviceGray:
    """Stub for pypdf.generic._color.DeviceGray."""
    gray = None
    color_operator = 'g'
    _ordered_fields = ()
class DeviceRGB:
    """Stub for pypdf.generic._color.DeviceRGB."""
    red = None
    green = None
    blue = None
    color_operator = 'rg'
    _ordered_fields = ()
class DeviceCMYK:
    """Stub for pypdf.generic._color.DeviceCMYK."""
    cyan = None
    magenta = None
    yellow = None
    black = None
    color_operator = 'k'
    _ordered_fields = ()
