"""A dataclass that captures the CTM and Text State for a tj operation

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._text_extraction._layout_mode._text_state_params``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Any
from ...generic._font import Font
from .. import mult, orient
class TextStateParams:
    """Stub for pypdf._text_extraction._layout_mode._text_state_params.TextStateParams."""
    value = None
    font = None
    font_size = None
    Tc = None
    Tw = None
    Tz = None
    TL = None
    Ts = None
    transform = None
    tx = None
    ty = None
    displaced_tx = None
    space_tx = None
    font_height = None
    flip_vertical = None
    rotated = None
    text = None
    _raw_chars = None
    def __post_init__(self, *args, **kwargs):
        return None  # inert stub
    def font_size_matrix(self, *args, **kwargs):
        return None  # inert stub
    def displaced_transform(self, *args, **kwargs):
        return None  # inert stub
    def render_transform(self, *args, **kwargs):
        return None  # inert stub
    def displacement_matrix(self, *args, **kwargs):
        return None  # inert stub
    def word_tx(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def to_dict(*args, **kwargs):
        return None  # inert stub
