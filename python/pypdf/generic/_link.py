"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.generic._link``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, cast
from .._utils import logger_warning
from . import ArrayObject, DictionaryObject, IndirectObject, PdfObject, TextStringObject, is_null_or_none
if TYPE_CHECKING:
    from .._page import PageObject
    from .._reader import PdfReader
    from .._writer import PdfWriter
class NamedReferenceLink:
    """Stub for pypdf.generic._link.NamedReferenceLink."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def find_referenced_page(self, *args, **kwargs):
        return None  # inert stub
    def patch_reference(self, *args, **kwargs):
        return None  # inert stub
class DirectReferenceLink:
    """Stub for pypdf.generic._link.DirectReferenceLink."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def find_referenced_page(self, *args, **kwargs):
        return None  # inert stub
    def patch_reference(self, *args, **kwargs):
        return None  # inert stub
ReferenceLink = None
def extract_links(*args, **kwargs):
    return None  # inert stub: pypdf.generic._link.extract_links
def _build_link(*args, **kwargs):
    return None  # inert stub: pypdf.generic._link._build_link
def _create_link(*args, **kwargs):
    return None  # inert stub: pypdf.generic._link._create_link
