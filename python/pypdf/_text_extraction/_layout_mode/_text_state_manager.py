"""manage the PDF transform stack during "layout" mode text extraction

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._text_extraction._layout_mode._text_state_manager``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

from collections import ChainMap, Counter
from collections import ChainMap as ChainMapType
from collections import Counter as CounterType
from collections.abc import MutableMapping
from typing import Any, Literal
from ...errors import PdfReadError
from ...generic._font import Font
from .. import mult
from ._text_state_params import TextStateParams
TextStateManagerKeyType = None
TextStateManagerChainMapType = None
TextStateManagerDictType = None
class TextStateManager:
    """Stub for pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def set_state_param(self, *args, **kwargs):
        return None  # inert stub
    def set_font(self, *args, **kwargs):
        return None  # inert stub
    def text_state_params(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _as_float(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def raw_transform(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def new_transform(*args, **kwargs):
        return None  # inert stub
    def reset_tm(self, *args, **kwargs):
        return None  # inert stub
    def reset_trm(self, *args, **kwargs):
        return None  # inert stub
    def remove_q(self, *args, **kwargs):
        return None  # inert stub
    def add_q(self, *args, **kwargs):
        return None  # inert stub
    def add_cm(self, *args, **kwargs):
        return None  # inert stub
    def _complete_matrix(self, *args, **kwargs):
        return None  # inert stub
    def add_tm(self, *args, **kwargs):
        return None  # inert stub
    def add_trm(self, *args, **kwargs):
        return None  # inert stub
    @property
    def effective_transform(self):
        return None  # inert stub
