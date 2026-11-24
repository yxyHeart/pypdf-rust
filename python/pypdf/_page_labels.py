"""Page labels are shown by PDF viewers as "the page number".

A page has a numeric index, starting at 0. Additionally, the page
has a label. In the most simple case:

    label = index + 1

However, the title page and the table of contents might have Roman numerals as
page labels. This makes things more complicated.

Example 1
---------

>>> reader.root_object["/PageLabels"]["/Nums"]
[0, IndirectObject(18, 0, 139929798197504),
 8, IndirectObject(19, 0, 139929798197504)]
>>> reader.get_object(reader.root_object["/PageLabels"]["/Nums"][1])
{'/S': '/r'}
>>> reader.get_object(reader.root_object["/PageLabels"]["/Nums"][3])
{'/S': '/D'}

Example 2
---------
The following is a document with pages labeled
i, ii, iii, iv, 1, 2, 3, A-8, A-9, ...

1 0 obj
    << /Type /Catalog
       /PageLabels << /Nums [
                        0 << /S /r >>
                        4 << /S /D >>
                        7 << /S /D
                             /P ( A- )
                             /St 8
                        >>
                        % A number tree containing
                        % three page label dictionaries
                        ]
                   >>
    ...
    >>
endobj


§12.4.2 PDF Specification 1.7 and 2.0
=====================================
Entries in a page label dictionary
----------------------------------
The /S key:
D       Decimal Arabic numerals
R       Uppercase Roman numerals
r       Lowercase Roman numerals
A       Uppercase letters (A to Z for the first 26 pages,
                           AA to ZZ for the next 26, and so on)
a       Lowercase letters (a to z for the first 26 pages,
                           aa to zz for the next 26, and so on)

Rust-backed module: the implementation lives in
``crates/pypdf-py/src/modules/page_labels_mod.rs``.  The module constants, the
``_NUMBERING_STYLES`` table, the ``typing.NamedTuple`` containers and the pure
attribute-assignment ``_FirstKidFinder.__init__`` stay Python definitions; every
function body and method body delegates to Rust.
"""

import math
import string
from bisect import bisect_left, bisect_right
from collections.abc import Callable, Iterable
from typing import Any, NamedTuple, cast

from pypdf._pypdf import (
    pagelabels_firstkidfinder_cover as _firstkidfinder_cover,
    pagelabels_firstkidfinder_find as _firstkidfinder_find,
    pagelabels_firstkidfinder_find_preceding as _firstkidfinder_find_preceding,
    pagelabels_firstkidfinder_read_kid as _firstkidfinder_read_kid,
    pagelabels_get_label as _rust_get_label,
    pagelabels_get_label_from_nums as _rust_get_label_from_nums,
    pagelabels_get_labels_for_page_indexes as _rust_get_labels_for_page_indexes,
    pagelabels_get_labels_from_kids as _rust_get_labels_from_kids,
    pagelabels_get_page_range as _rust_get_page_range,
    pagelabels_index2label as _rust_index2label,
    pagelabels_index2labels as _rust_index2labels,
    pagelabels_index2labels_page_by_page as _rust_index2labels_page_by_page,
    pagelabels_number2decimal_arabic_numeral as _rust_number2decimal_arabic_numeral,
    pagelabels_number2lowercase_letter as _rust_number2lowercase_letter,
    pagelabels_number2lowercase_roman_numeral as _rust_number2lowercase_roman_numeral,
    pagelabels_number2uppercase_letter as _rust_number2uppercase_letter,
    pagelabels_number2uppercase_roman_numeral as _rust_number2uppercase_roman_numeral,
    pagelabels_nums_clear_range as _rust_nums_clear_range,
    pagelabels_nums_insert as _rust_nums_insert,
    pagelabels_nums_next as _rust_nums_next,
)

from ._protocols import PdfCommonDocProtocol
from ._utils import logger_warning
from .generic import (
    ArrayObject,
    DictionaryObject,
    NullObject,
    NumberObject,
    is_null_or_none,
)

# The largest number one can represent using the usual symbols
# and subtractive notation without additional conventions.
# Aligns with https://github.com/AA-Turner/roman-numerals/blob/master/python/roman_numerals/__init__.py
MAXIMUM_ROMAN_NUMERAL = 3_999

# Allow values of up to 512 * 26 = 13_312 for the `/A` and `/a`.
# Please note that this limit is somehow arbitrary and some trade-off
# between rather restrictive <= 256 output bytes per label and the possibly
# unrealistic 1000 output bytes per label.
MAXIMUM_PAGE_LABEL_LENGTH = 512


def number2decimal_arabic_numeral(number: int) -> str:
    """ ``pypdf._page_labels.number2decimal_arabic_numeral``, delegated to Rust."""
    return _rust_number2decimal_arabic_numeral(number)


def number2uppercase_roman_numeral(num: int) -> str:
    """ ``pypdf._page_labels.number2uppercase_roman_numeral``, delegated to Rust."""
    return _rust_number2uppercase_roman_numeral(num)


def number2lowercase_roman_numeral(number: int) -> str:
    """ ``pypdf._page_labels.number2lowercase_roman_numeral``, delegated to Rust."""
    return _rust_number2lowercase_roman_numeral(number)


def number2uppercase_letter(number: int) -> str:
    """ ``pypdf._page_labels.number2uppercase_letter``, delegated to Rust."""
    return _rust_number2uppercase_letter(number)


def number2lowercase_letter(number: int) -> str:
    """ ``pypdf._page_labels.number2lowercase_letter``, delegated to Rust."""
    return _rust_number2lowercase_letter(number)


_NUMBERING_STYLES: dict[str | None, Callable[[int], str]] = {
    None: lambda _: "",
    "/D": number2decimal_arabic_numeral,
    "/R": number2uppercase_roman_numeral,
    "/r": number2lowercase_roman_numeral,
    "/A": number2uppercase_letter,
    "/a": number2lowercase_letter,
}


def _get_label(value: Any, start_index: Any, index: int) -> str:
    """ ``pypdf._page_labels._get_label``, delegated to Rust."""
    return _rust_get_label(value, start_index, index)


def get_label_from_nums(dictionary_object: DictionaryObject, index: int) -> str:
    """ ``pypdf._page_labels.get_label_from_nums``, delegated to Rust."""
    return _rust_get_label_from_nums(dictionary_object, index)


def index2label(reader: PdfCommonDocProtocol, index: int) -> str:
    """
    See 7.9.7 "Number Trees".

    Args:
        reader: The PdfReader
        index: The index of the page

    Returns:
        The label of the page, e.g. "iv" or "4".

    """
    return _rust_index2label(reader, index)


def _get_labels_for_page_indexes(nums: ArrayObject, indexes: Iterable[int]) -> list[str]:
    """
    Labels for increasing page indexes, as ``get_label_from_nums`` would return them.

    ``get_label_from_nums`` stops at the first pair whose next key is above the page index.
    As that pair can only move forward when the page index grows, the array is read once
    for all pages instead of from the start for every page, and each value is resolved
    when its pair is first reached.
    """
    return _rust_get_labels_for_page_indexes(nums, indexes)


def _get_page_range(limits: list[Any], page_count: int) -> tuple[int, int] | None:
    """Get the first and last page index between the two ``/Limits`` values, if any."""
    return _rust_get_page_range(limits, page_count)


class _FirstKidFinder:
    """
    Find the kid ``index2label`` picks for a page index: the first kid in order whose
    ``/Limits`` contain it.

    Like ``index2label``, kids are read in order and only as far as needed. The pages
    covered by the kids read so far are kept as sorted ranges, so that each kid is read
    once instead of once per page. A kid which cannot be read is returned as the
    exception, which ``index2label`` raises for every page that reaches this kid.
    """

    def __init__(self, kids: ArrayObject, page_count: int) -> None:
        self._kids = kids
        self._kid_count = len(kids)
        self._page_count = page_count
        self._next_kid = 0
        # Pages covered by any kid read so far, as merged ranges.
        self._covered_starts: list[int] = []
        self._covered_ends: list[int] = []
        # Ranges of pages with the same first kid.
        self._starts: list[int] = []
        self._ends: list[int] = []
        self._first_kids: list[DictionaryObject | Exception] = []
        # Kids read so far with valid /Limits and their lower limits, in order.
        self._read_kids: list[DictionaryObject] = []
        self._read_lows: list[float] = []

    def find(self, index: int) -> DictionaryObject | Exception | None:
        """ ``pypdf._page_labels._FirstKidFinder.find``, delegated to Rust."""
        return _firstkidfinder_find(self, index)

    def find_preceding(self, index: int) -> DictionaryObject | None:
        """
        Find the kid ``index2label`` picks for a page index no kid contains: the last
        kid in order whose ``/Limits`` start before it. Only valid once ``find`` has
        returned ``None`` for the index, which means that all kids have been read.
        """
        return _firstkidfinder_find_preceding(self, index)

    def _read_kid(self, kid: Any) -> tuple[int, int, DictionaryObject | Exception] | None:
        """ ``pypdf._page_labels._FirstKidFinder._read_kid``, delegated to Rust."""
        return _firstkidfinder_read_kid(self, kid)

    def _cover(self, first: int, last: int, kid: DictionaryObject | Exception) -> None:
        """Make ``kid`` the first kid of the pages from ``first`` to ``last`` which no earlier kid covers."""
        return _firstkidfinder_cover(self, first, last, kid)


class _TreeNode(NamedTuple):
    """A node of a ``/Kids`` number tree with the page indexes reaching it."""

    node: DictionaryObject
    indexes: list[int]
    level: int


class _KidPages(NamedTuple):
    """A kid with the page indexes it is picked for."""

    kid: DictionaryObject
    indexes: list[int]


class _LeafPages(NamedTuple):
    """A leaf with its ``/Nums`` array, if it is one, and the page indexes it labels."""

    nums: ArrayObject | None
    leaf: DictionaryObject
    indexes: list[int]


def _get_labels_from_kids(number_tree: DictionaryObject, page_count: int) -> list[str]:
    """
    Labels of the first ``page_count`` pages of a ``/Kids`` number tree, as ``index2label`` would return them.

    Instead of walking the tree from the root for every page, each ``/Kids`` array is
    indexed once and each ``/Nums`` array is read once for all of its pages. Warnings
    about malformed kids are logged once per array instead of once per page.
    """
    return _rust_get_labels_from_kids(number_tree, page_count)


def _index2labels_page_by_page(pdf: PdfCommonDocProtocol, page_count: int) -> list[str]:
    """ ``pypdf._page_labels._index2labels_page_by_page``, delegated to Rust."""
    return _rust_index2labels_page_by_page(pdf, page_count)


def index2labels(pdf: PdfCommonDocProtocol, page_count: int) -> list[str]:
    """
    Labels of the first ``page_count`` pages, as ``index2label`` would return them.

    Number trees are read once for all pages instead of from the start for every page.
    """
    return _rust_index2labels(pdf, page_count)


def nums_insert(
    key: NumberObject,
    value: DictionaryObject,
    nums: ArrayObject,
) -> None:
    """
    Insert a key, value pair in a Nums array.

    See 7.9.7 "Number Trees".

    Args:
        key: number key of the entry
        value: value of the entry
        nums: Nums array to modify

    """
    return _rust_nums_insert(key, value, nums)


def nums_clear_range(
    key: NumberObject,
    page_index_to: int,
    nums: ArrayObject,
) -> None:
    """
    Remove all entries in a number tree in a range after an entry.

    See 7.9.7 "Number Trees".

    Args:
        key: number key of the entry before the range
        page_index_to: The page index of the upper limit of the range
        nums: Nums array to modify

    """
    return _rust_nums_clear_range(key, page_index_to, nums)


def nums_next(
    key: NumberObject,
    nums: ArrayObject,
) -> tuple[NumberObject | None, DictionaryObject | None]:
    """
    Return the (key, value) pair of the entry after the given one.

    See 7.9.7 "Number Trees".

    Args:
        key: number key of the entry
        nums: Nums array

    """
    return _rust_nums_next(key, nums)

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
