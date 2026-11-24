"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.generic._files``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import bisect
from functools import cached_property
from typing import TYPE_CHECKING, cast
from pypdf._utils import format_iso8824_date, logger_warning, parse_iso8824_date
from pypdf.constants import CatalogAttributes, FileSpecificationDictionaryEntries, PageAttributes
from pypdf.errors import PdfReadError, PyPdfError
from pypdf.generic import ArrayObject, ByteStringObject, DecodedStreamObject, DictionaryObject, NameObject, NullObject, NumberObject, StreamObject, TextStringObject, is_null_or_none
if TYPE_CHECKING:
    import datetime
    from collections.abc import Iterator
    from pypdf._writer import PdfWriter
class EmbeddedFile:
    """Stub for pypdf.generic._files.EmbeddedFile."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    @property
    def name(self):
        return None  # inert stub
    @classmethod
    def _create_new(cls, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def _get_names_array(cls, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def _get_insertion_index(cls, *args, **kwargs):
        return None  # inert stub
    @property
    def alternative_name(self):
        return None  # inert stub
    def alternative_name(self, *args, **kwargs):
        return None  # inert stub
    @property
    def description(self):
        return None  # inert stub
    def description(self, *args, **kwargs):
        return None  # inert stub
    @property
    def associated_file_relationship(self):
        return None  # inert stub
    def associated_file_relationship(self, *args, **kwargs):
        return None  # inert stub
    @property
    def _embedded_file(self):
        return None  # inert stub
    @property
    def _params(self):
        return None  # inert stub
    def _ensure_params(self, *args, **kwargs):
        return None  # inert stub
    @property
    def subtype(self):
        return None  # inert stub
    def subtype(self, *args, **kwargs):
        return None  # inert stub
    @property
    def content(self):
        return None  # inert stub
    def content(self, *args, **kwargs):
        return None  # inert stub
    @property
    def size(self):
        return None  # inert stub
    def size(self, *args, **kwargs):
        return None  # inert stub
    @property
    def creation_date(self):
        return None  # inert stub
    def creation_date(self, *args, **kwargs):
        return None  # inert stub
    @property
    def modification_date(self):
        return None  # inert stub
    def modification_date(self, *args, **kwargs):
        return None  # inert stub
    @property
    def checksum(self):
        return None  # inert stub
    def checksum(self, *args, **kwargs):
        return None  # inert stub
    @property
    def _names(self):
        return None  # inert stub
    def delete(self, *args, **kwargs):
        return None  # inert stub
    def __repr__(self, *args, **kwargs):
        return ""  # inert stub
    @staticmethod
    def _report_malformed(*args, **kwargs):
        return None  # inert stub
    @classmethod
    def _load_from_names(cls, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def _load(cls, *args, **kwargs):
        return None  # inert stub
