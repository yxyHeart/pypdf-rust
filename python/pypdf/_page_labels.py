"""Page labels are shown by PDF viewers as "the page number".

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._page_labels``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import math
import string
from bisect import bisect_left, bisect_right
from collections.abc import Callable, Iterable
from typing import Any, NamedTuple, cast
from ._protocols import PdfCommonDocProtocol
from ._utils import logger_warning
from .generic import ArrayObject, DictionaryObject, NullObject, NumberObject, is_null_or_none
MAXIMUM_ROMAN_NUMERAL = 3999
MAXIMUM_PAGE_LABEL_LENGTH = 512
def number2decimal_arabic_numeral(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels.number2decimal_arabic_numeral
def number2uppercase_roman_numeral(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels.number2uppercase_roman_numeral
def number2lowercase_roman_numeral(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels.number2lowercase_roman_numeral
def number2uppercase_letter(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels.number2uppercase_letter
def number2lowercase_letter(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels.number2lowercase_letter
_NUMBERING_STYLES = None
def _get_label(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels._get_label
def get_label_from_nums(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels.get_label_from_nums
def index2label(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels.index2label
def _get_labels_for_page_indexes(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels._get_labels_for_page_indexes
def _get_page_range(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels._get_page_range
class _FirstKidFinder:
    """Stub for pypdf._page_labels._FirstKidFinder."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def find(self, *args, **kwargs):
        return None  # inert stub
    def find_preceding(self, *args, **kwargs):
        return None  # inert stub
    def _read_kid(self, *args, **kwargs):
        return None  # inert stub
    def _cover(self, *args, **kwargs):
        return None  # inert stub
class _TreeNode:
    """Stub for pypdf._page_labels._TreeNode."""
    node = None
    indexes = None
    level = None
class _KidPages:
    """Stub for pypdf._page_labels._KidPages."""
    kid = None
    indexes = None
class _LeafPages:
    """Stub for pypdf._page_labels._LeafPages."""
    nums = None
    leaf = None
    indexes = None
def _get_labels_from_kids(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels._get_labels_from_kids
def _index2labels_page_by_page(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels._index2labels_page_by_page
def index2labels(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels.index2labels
def nums_insert(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels.nums_insert
def nums_clear_range(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels.nums_clear_range
def nums_next(*args, **kwargs):
    return None  # inert stub: pypdf._page_labels.nums_next
