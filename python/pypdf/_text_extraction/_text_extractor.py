"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._text_extraction._text_extractor``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Any
from .._codecs import encoding_dict_from_named_encoding
from ..generic import DictionaryObject, TextStringObject
from ..generic._font import Font, FontDescriptor
from . import OrientationNotFoundError, crlf_space_check, get_display_str, get_text_operands, mult
def _get_number_operand(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction._text_extractor._get_number_operand
class TextExtraction:
    """Stub for pypdf._text_extraction._text_extractor.TextExtraction."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def initialize_extraction(self, *args, **kwargs):
        return None  # inert stub
    def compute_str_widths(self, *args, **kwargs):
        return None  # inert stub
    def process_operation(self, *args, **kwargs):
        return None  # inert stub
    def process_tj_offset(self, *args, **kwargs):
        return None  # inert stub
    def _post_process_text_operation(self, *args, **kwargs):
        return None  # inert stub
    def _handle_tj(self, *args, **kwargs):
        return None  # inert stub
    def _flush_text(self, *args, **kwargs):
        return None  # inert stub
    def _handle_bt(self, *args, **kwargs):
        return None  # inert stub
    def _handle_et(self, *args, **kwargs):
        return None  # inert stub
    def _handle_save_graphics_state(self, *args, **kwargs):
        return None  # inert stub
    def _handle_restore_graphics_state(self, *args, **kwargs):
        return None  # inert stub
    def _handle_cm(self, *args, **kwargs):
        return None  # inert stub
    def _handle_tz(self, *args, **kwargs):
        return None  # inert stub
    def _handle_tc(self, *args, **kwargs):
        return None  # inert stub
    def _handle_tw(self, *args, **kwargs):
        return None  # inert stub
    def _handle_tl(self, *args, **kwargs):
        return None  # inert stub
    def _handle_tf(self, *args, **kwargs):
        return None  # inert stub
    def _handle_td(self, *args, **kwargs):
        return None  # inert stub
    def _handle_tm(self, *args, **kwargs):
        return None  # inert stub
    def _handle_t_star(self, *args, **kwargs):
        return None  # inert stub
    def _handle_tj_operation(self, *args, **kwargs):
        return None  # inert stub
