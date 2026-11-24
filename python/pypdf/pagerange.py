"""Representation and utils for ranges of PDF file pages.

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.pagerange``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import re
from typing import Any, Union
from .errors import ParseError
_INT_RE = '(0|-?[1-9]\\d*)'
PAGE_RANGE_RE = None
class PageRange:
    """Stub for pypdf.pagerange.PageRange."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    @staticmethod
    def valid(*args, **kwargs):
        return None  # inert stub
    def to_slice(self, *args, **kwargs):
        return None  # inert stub
    def __str__(self, *args, **kwargs):
        return ""  # inert stub
    def __repr__(self, *args, **kwargs):
        return ""  # inert stub
    def indices(self, *args, **kwargs):
        return None  # inert stub
    def __eq__(self, *args, **kwargs):
        return None  # inert stub
    def __hash__(self, *args, **kwargs):
        return None  # inert stub
    def __add__(self, *args, **kwargs):
        return None  # inert stub
PAGE_RANGE_ALL = None
def parse_filename_page_ranges(*args, **kwargs):
    return None  # inert stub: pypdf.pagerange.parse_filename_page_ranges
PageRangeSpec = None
