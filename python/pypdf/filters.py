"""Implementation of stream filters; §7.4 Filters of the PDF 2.0 specification.

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.filters``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

__author__ = 'Mathieu Fenniak'
__author_email__ = 'biziqe@mathieu.fenniak.net'
import binascii
import math
import os
import shutil
import struct
import subprocess
import zlib
from base64 import a85decode
from dataclasses import dataclass
from functools import lru_cache
from importlib.util import find_spec
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import Any, cast
from ._codecs._codecs import LzwCodec as _LzwCodec
from ._configuration import get_configuration
from ._utils import WHITESPACES_AS_BYTES, deprecation, logger_warning
from .constants import CcittFaxDecodeParameters, FilterTypeAbbreviations, FilterTypes, ImageAttributes, LzwFilterParameters, StreamAttributes
from .errors import DependencyError, LimitReachedError, PdfReadError, PdfStreamError
from .generic import ArrayObject, BooleanObject, DictionaryObject, IndirectObject, NumberObject, PdfObject, StreamObject, is_null_or_none
MAX_DECLARED_STREAM_LENGTH = 75000000
MAX_ARRAY_BASED_STREAM_OUTPUT_LENGTH = 75000000
JBIG2_MAX_OUTPUT_LENGTH = 75000000
LZW_MAX_OUTPUT_LENGTH = 75000000
RUN_LENGTH_MAX_OUTPUT_LENGTH = 75000000
ZLIB_MAX_OUTPUT_LENGTH = 75000000
ZLIB_MAX_RECOVERY_INPUT_LENGTH = 5000000
FLATE_MAX_COLUMNS = 250000
FLATE_MAX_ROW_LENGTH = 4000000
FLATE_MAX_BUFFER_SIZE = 75000000
_SINGLE_BYTES = None
def _decompress_with_limit(*args, **kwargs):
    return None  # inert stub: pypdf.filters._decompress_with_limit
def decompress(*args, **kwargs):
    return None  # inert stub: pypdf.filters.decompress
class FlateDecode:
    """Stub for pypdf.filters.FlateDecode."""
    @staticmethod
    def decode(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _get_parameters(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _decode_png_prediction(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def encode(*args, **kwargs):
        return None  # inert stub
class ASCIIHexDecode:
    """Stub for pypdf.filters.ASCIIHexDecode."""
    @staticmethod
    def decode(*args, **kwargs):
        return None  # inert stub
class RunLengthDecode:
    """Stub for pypdf.filters.RunLengthDecode."""
    @staticmethod
    def decode(*args, **kwargs):
        return None  # inert stub
class LZWDecode:
    """Stub for pypdf.filters.LZWDecode."""
    class Decoder:
        """Stub for pypdf.filters.Decoder."""
        STOP = 257
        CLEARDICT = 256
        def __init__(self, *args, **kwargs):
            try:
                super().__init__(*args, **kwargs)
            except TypeError:
                pass
        def decode(self, *args, **kwargs):
            return None  # inert stub
    @staticmethod
    def decode(*args, **kwargs):
        return None  # inert stub
class ASCII85Decode:
    """Stub for pypdf.filters.ASCII85Decode."""
    @staticmethod
    def decode(*args, **kwargs):
        return None  # inert stub
class DCTDecode:
    """Stub for pypdf.filters.DCTDecode."""
    @staticmethod
    def decode(*args, **kwargs):
        return None  # inert stub
class JPXDecode:
    """Stub for pypdf.filters.JPXDecode."""
    @staticmethod
    def decode(*args, **kwargs):
        return None  # inert stub
class BrotliDecode:
    """Stub for pypdf.filters.BrotliDecode."""
    @staticmethod
    def _check_brotli_available(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def decode(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def encode(*args, **kwargs):
        return None  # inert stub
class CCITTParameters:
    """Stub for pypdf.filters.CCITTParameters."""
    K = None
    columns = None
    rows = None
    EndOfLine = None
    EncodedByteAlign = None
    EndOfBlock = None
    BlackIs1 = None
    DamagedRowsBeforeError = None
    @property
    def group(self):
        return None  # inert stub
class CCITTFaxDecode:
    """Stub for pypdf.filters.CCITTFaxDecode."""
    _MAXIMUM_UNSIGNED_LONG = 4294967295
    @staticmethod
    def _get_parameters(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def decode(*args, **kwargs):
        return None  # inert stub
JBIG2DEC_BINARY = None
class JBIG2Decode:
    """Stub for pypdf.filters.JBIG2Decode."""
    @staticmethod
    def decode(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _is_binary_compatible(*args, **kwargs):
        return None  # inert stub
def _deprecate_inline_image_filters(*args, **kwargs):
    return None  # inert stub: pypdf.filters._deprecate_inline_image_filters
def decode_stream_data(*args, **kwargs):
    return None  # inert stub: pypdf.filters.decode_stream_data
