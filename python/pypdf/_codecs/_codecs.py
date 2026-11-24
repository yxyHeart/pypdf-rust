"""This module is for codecs only.

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._codecs._codecs``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import io
from abc import ABC, abstractmethod
from typing import ClassVar
from pypdf._utils import logger_warning
from pypdf.errors import LimitReachedError, PdfStreamError
class Codec:
    """Stub for pypdf._codecs._codecs.Codec."""
    def encode(self, *args, **kwargs):
        return None  # inert stub
    def decode(self, *args, **kwargs):
        return None  # inert stub
class LzwCodec(Codec):
    """Stub for pypdf._codecs._codecs.LzwCodec."""
    CLEAR_TABLE_MARKER = 256
    EOD_MARKER = 257
    INITIAL_BITS_PER_CODE = 9
    MAX_BITS_PER_CODE = 12
    _AND_TABLE = None
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def _initialize_encoding_table(self, *args, **kwargs):
        return None  # inert stub
    def _increase_next_code(self, *args, **kwargs):
        return None  # inert stub
    def encode(self, *args, **kwargs):
        return None  # inert stub
    def _pack_codes_into_bytes(self, *args, **kwargs):
        return None  # inert stub
    def _initialize_decoding_table(self, *args, **kwargs):
        return None  # inert stub
    def _next_code_decode(self, *args, **kwargs):
        return None  # inert stub
    def decode(self, *args, **kwargs):
        return None  # inert stub
    def _add_entry_decode(self, *args, **kwargs):
        return None  # inert stub
