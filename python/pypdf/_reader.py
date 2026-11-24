"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._reader``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import os
import re
import sys
from collections.abc import Callable, Iterable
from io import BytesIO, UnsupportedOperation
from operator import itemgetter
from pathlib import Path
from types import TracebackType
from typing import TYPE_CHECKING, Any, cast
if sys.version_info >= (3, 11):
    from typing import Self
else:
    from typing_extensions import Self
from ._configuration import apply_legacy_configuration
from ._doc_common import PdfDocCommon, convert_to_int
from ._encryption import Encryption, PasswordType
from ._utils import WHITESPACES_AS_BYTES, StrByteType, StreamType, _TraversalState, get_stream_data, get_stream_size, logger_warning, read_non_whitespace, read_previous_line, read_until_whitespace, skip_over_comment, skip_over_whitespace
from .constants import TrailerKeys
from .errors import EmptyFileError, FileNotDecryptedError, LimitReachedError, PdfReadError, PdfStreamError, WrongPasswordError
from .generic import ArrayObject, ByteStringObject, Destination, DictionaryObject, EncodedStreamObject, IndirectObject, NameObject, NullObject, NumberObject, PdfObject, StreamObject, TextStringObject, is_null_or_none, read_object
from .xmp import XmpInformation
if TYPE_CHECKING:
    from ._page import PageObject
class PdfReader(PdfDocCommon):
    """Stub for pypdf._reader.PdfReader."""
    xref = None
    trailer = None
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def _initialize_stream(self, *args, **kwargs):
        return None  # inert stub
    def _handle_encryption(self, *args, **kwargs):
        return None  # inert stub
    def __enter__(self, *args, **kwargs):
        return None  # inert stub
    def __exit__(self, *args, **kwargs):
        return None  # inert stub
    def close(self, *args, **kwargs):
        return None  # inert stub
    @property
    def root_object(self):
        return None  # inert stub
    @property
    def _info(self):
        return None  # inert stub
    @property
    def _ID(self):
        return None  # inert stub
    @property
    def pdf_header(self):
        return None  # inert stub
    @property
    def xmp_metadata(self):
        return None  # inert stub
    def _get_page_number_by_indirect(self, *args, **kwargs):
        return None  # inert stub
    def _get_object_from_stream(self, *args, **kwargs):
        return None  # inert stub
    def get_object(self, *args, **kwargs):
        return None  # inert stub
    def read_object_header(self, *args, **kwargs):
        return None  # inert stub
    def cache_get_indirect_object(self, *args, **kwargs):
        return None  # inert stub
    def cache_indirect_object(self, *args, **kwargs):
        return None  # inert stub
    def _replace_object(self, *args, **kwargs):
        return None  # inert stub
    def read(self, *args, **kwargs):
        return None  # inert stub
    def _basic_validation(self, *args, **kwargs):
        return None  # inert stub
    def _find_eof_marker(self, *args, **kwargs):
        return None  # inert stub
    def _find_startxref_pos(self, *args, **kwargs):
        return None  # inert stub
    _MAX_STARTXREF_RECOVERY_LINES = 1000
    @staticmethod
    def _is_valid_startxref_value(*args, **kwargs):
        return None  # inert stub
    @classmethod
    def _find_previous_startxref_pos(cls, *args, **kwargs):
        return None  # inert stub
    def _load_recovery_cache(self, *args, **kwargs):
        return None  # inert stub
    def _read_standard_xref_table__handle_size(self, *args, **kwargs):
        return None  # inert stub
    def _read_standard_xref_table(self, *args, **kwargs):
        return None  # inert stub
    def _read_xref_tables_and_trailers(self, *args, **kwargs):
        return None  # inert stub
    _XREF_STREAM_TRAILER_KEYS = ()
    def _process_xref_stream(self, *args, **kwargs):
        return None  # inert stub
    def _read_xref(self, *args, **kwargs):
        return None  # inert stub
    def _read_xref_other_error(self, *args, **kwargs):
        return None  # inert stub
    def _sanitize_pdf15_xref_stream_index_pairs(self, *args, **kwargs):
        return None  # inert stub
    def _read_pdf15_xref_stream(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _get_xref_issues(*args, **kwargs):
        return None  # inert stub
    @classmethod
    def _find_pdf_objects(cls, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def _find_pdf_trailers(cls, *args, **kwargs):
        return None  # inert stub
    def _rebuild_xref_table(self, *args, **kwargs):
        return None  # inert stub
    def _read_xref_subsections(self, *args, **kwargs):
        return None  # inert stub
    def _pairs(self, *args, **kwargs):
        return None  # inert stub
    def decrypt(self, *args, **kwargs):
        return None  # inert stub
    @property
    def is_encrypted(self):
        return None  # inert stub
    def add_form_topname(self, *args, **kwargs):
        return None  # inert stub
    def rename_form_topname(self, *args, **kwargs):
        return None  # inert stub
    def _repr_mimebundle_(self, *args, **kwargs):
        return None  # inert stub
    def _get_named_destinations(self, *args, **kwargs):
        return None  # inert stub
