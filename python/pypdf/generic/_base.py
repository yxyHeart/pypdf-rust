"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.generic._base``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import binascii
import codecs
import hashlib
import re
import sys
from collections.abc import Callable, Sequence
from math import log10
from struct import iter_unpack
from typing import Any, ClassVar, Optional, TypeGuard, Union, cast
if sys.version_info >= (3, 11):
    from typing import Self
else:
    from typing_extensions import Self
from .._codecs import _pdfdoc_encoding_rev
from .._protocols import PdfWriterProtocol
from .._utils import WHITESPACES, StreamType, logger_warning, read_non_whitespace, read_until_regex
from ..errors import STREAM_TRUNCATED_PREMATURELY, PdfReadError, PdfStreamError
__author__ = 'Mathieu Fenniak'
__author_email__ = 'biziqe@mathieu.fenniak.net'
class PdfObject:
    """Stub for pypdf.generic._base.PdfObject."""
    hash_func = None
    indirect_reference = None
    def hash_bin(self, *args, **kwargs):
        return None  # inert stub
    def hash_value_data(self, *args, **kwargs):
        return None  # inert stub
    def hash_value(self, *args, **kwargs):
        return None  # inert stub
    def replicate(self, *args, **kwargs):
        return None  # inert stub
    def clone(self, *args, **kwargs):
        return None  # inert stub
    def _reference_clone(self, *args, **kwargs):
        return None  # inert stub
    def get_object(self, *args, **kwargs):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
class NullObject(PdfObject):
    """Stub for pypdf.generic._base.NullObject."""
    def clone(self, *args, **kwargs):
        return None  # inert stub
    def hash_bin(self, *args, **kwargs):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def read_from_stream(*args, **kwargs):
        return None  # inert stub
    def __repr__(self, *args, **kwargs):
        return ""  # inert stub
    def __eq__(self, *args, **kwargs):
        return None  # inert stub
    def __hash__(self, *args, **kwargs):
        return None  # inert stub
class BooleanObject(PdfObject):
    """Stub for pypdf.generic._base.BooleanObject."""
    value = None
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def clone(self, *args, **kwargs):
        return None  # inert stub
    def hash_bin(self, *args, **kwargs):
        return None  # inert stub
    def __eq__(self, *args, **kwargs):
        return None  # inert stub
    def __hash__(self, *args, **kwargs):
        return None  # inert stub
    def __repr__(self, *args, **kwargs):
        return ""  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def read_from_stream(*args, **kwargs):
        return None  # inert stub
class IndirectObject(PdfObject):
    """Stub for pypdf.generic._base.IndirectObject."""
    _MAXIMUM_PART_LENGTH = 64
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def __hash__(self, *args, **kwargs):
        return None  # inert stub
    def hash_bin(self, *args, **kwargs):
        return None  # inert stub
    def replicate(self, *args, **kwargs):
        return None  # inert stub
    def clone(self, *args, **kwargs):
        return None  # inert stub
    @property
    def indirect_reference(self):
        return None  # inert stub
    def get_object(self, *args, **kwargs):
        return None  # inert stub
    def __deepcopy__(self, *args, **kwargs):
        return None  # inert stub
    def _get_object_with_check(self, *args, **kwargs):
        return None  # inert stub
    def __getattr__(self, *args, **kwargs):
        return None  # inert stub
    def __getitem__(self, *args, **kwargs):
        return None  # inert stub
    def __contains__(self, *args, **kwargs):
        return None  # inert stub
    def __iter__(self, *args, **kwargs):
        return None  # inert stub
    def __float__(self, *args, **kwargs):
        return None  # inert stub
    def __int__(self, *args, **kwargs):
        return None  # inert stub
    def __str__(self, *args, **kwargs):
        return ""  # inert stub
    def __repr__(self, *args, **kwargs):
        return ""  # inert stub
    def __eq__(self, *args, **kwargs):
        return None  # inert stub
    def __ne__(self, *args, **kwargs):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def read_from_stream(*args, **kwargs):
        return None  # inert stub
FLOAT_WRITE_PRECISION = 8
class FloatObject(float, PdfObject):
    """Stub for pypdf.generic._base.FloatObject."""
    def __new__(cls, *args, **kwargs):
        return super().__new__(cls)  # inert stub
    def clone(self, *args, **kwargs):
        return None  # inert stub
    def hash_bin(self, *args, **kwargs):
        return None  # inert stub
    def myrepr(self, *args, **kwargs):
        return None  # inert stub
    def __repr__(self, *args, **kwargs):
        return ""  # inert stub
    def as_numeric(self, *args, **kwargs):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
class NumberObject(int, PdfObject):
    """Stub for pypdf.generic._base.NumberObject."""
    NumberPattern = None
    _LENGTH_LIMIT = 64
    def __new__(cls, *args, **kwargs):
        return super().__new__(cls)  # inert stub
    def clone(self, *args, **kwargs):
        return None  # inert stub
    def hash_bin(self, *args, **kwargs):
        return None  # inert stub
    def as_numeric(self, *args, **kwargs):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def read_from_stream(*args, **kwargs):
        return None  # inert stub
class ByteStringObject(bytes, PdfObject):
    """Stub for pypdf.generic._base.ByteStringObject."""
    def clone(self, *args, **kwargs):
        return None  # inert stub
    def hash_bin(self, *args, **kwargs):
        return None  # inert stub
    @property
    def original_bytes(self):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
    def __str__(self, *args, **kwargs):
        return ""  # inert stub
class TextStringObject(str, PdfObject):
    """Stub for pypdf.generic._base.TextStringObject."""
    autodetect_pdfdocencoding = None
    autodetect_utf16 = None
    utf16_bom = None
    _original_bytes = None
    def __new__(cls, *args, **kwargs):
        return super().__new__(cls)  # inert stub
    def clone(self, *args, **kwargs):
        return None  # inert stub
    def hash_bin(self, *args, **kwargs):
        return None  # inert stub
    @property
    def original_bytes(self):
        return None  # inert stub
    def get_original_bytes(self, *args, **kwargs):
        return None  # inert stub
    def get_encoded_bytes(self, *args, **kwargs):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
class NameObject(str, PdfObject):
    """Stub for pypdf.generic._base.NameObject."""
    delimiter_pattern = None
    prefix = b'/'
    renumber_table = None
    _LENGTH_LIMIT = 4096
    def clone(self, *args, **kwargs):
        return None  # inert stub
    def hash_bin(self, *args, **kwargs):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
    def renumber(self, *args, **kwargs):
        return None  # inert stub
    def _sanitize(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def unnumber(*args, **kwargs):
        return None  # inert stub
    CHARSETS = ()
    @staticmethod
    def read_from_stream(*args, **kwargs):
        return None  # inert stub
def encode_pdfdocencoding(*args, **kwargs):
    return None  # inert stub: pypdf.generic._base.encode_pdfdocencoding
def is_null_or_none(*args, **kwargs):
    return None  # inert stub: pypdf.generic._base.is_null_or_none
