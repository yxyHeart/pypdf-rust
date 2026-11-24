"""Layout mode text extraction extension for pypdf"""
from ...generic._font import Font
from ._fixed_width_page import (
    fixed_char_width,
    fixed_width_page,
    text_show_operations,
    y_coordinate_groups,
)

__all__ = [
    "Font",
    "fixed_char_width",
    "fixed_width_page",
    "text_show_operations",
    "y_coordinate_groups",
]

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
