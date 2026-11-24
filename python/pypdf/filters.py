# Copyright (c) 2006, Mathieu Fenniak
# All rights reserved.
#
# Redistribution and use in source and binary forms, with or without
# modification, are permitted provided that the following conditions are
# met:
#
# * Redistributions of source code must retain the above copyright notice,
# this list of conditions and the following disclaimer.
# * Redistributions in binary form must reproduce the above copyright notice,
# this list of conditions and the following disclaimer in the documentation
# and/or other materials provided with the distribution.
# * The name of the author may not be used to endorse or promote products
# derived from this software without specific prior written permission.
#
# THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
# AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
# IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
# ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE
# LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
# CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
# SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
# INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
# CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
# ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
# POSSIBILITY OF SUCH DAMAGE.


"""
Implementation of stream filters; §7.4 Filters of the PDF 2.0 specification.

§8.9.7 Inline images of the PDF 2.0 specification has abbreviations that can be
used for the names of filters in an inline image object.
"""

# Rust-backed module: the classes (and their class attributes) are defined here
# exactly as upstream; every method body delegates to
# ``crates/pypdf-py/src/modules/filters_mod.rs``.
__author__ = "Mathieu Fenniak"
__author_email__ = "biziqe@mathieu.fenniak.net"

import binascii  # noqa: F401  (kept for module parity; logic lives in Rust)
import math  # noqa: F401
import os  # noqa: F401
import shutil
import struct  # noqa: F401
import subprocess
import zlib  # noqa: F401
from base64 import a85decode  # noqa: F401
from dataclasses import dataclass
from functools import lru_cache
from importlib.util import find_spec
from pathlib import Path  # noqa: F401
from tempfile import TemporaryDirectory  # noqa: F401
from typing import Any, cast  # noqa: F401

from pypdf._pypdf import (
    filters_ascii85_decode as _ascii85_decode,
    filters_asciihex_decode as _asciihex_decode,
    filters_brotli_check_available as _brotli_check_available,
    filters_brotli_decode as _brotli_decode,
    filters_brotli_encode as _brotli_encode,
    filters_ccitt_decode as _ccitt_decode,
    filters_ccitt_get_parameters as _ccitt_get_parameters,
    filters_ccitt_parameters_group as _ccitt_parameters_group,
    filters_dct_decode as _dct_decode,
    filters_decode_stream_data as _decode_stream_data,
    filters_decompress as decompress,
    filters_decompress_with_limit as _decompress_with_limit,
    filters_deprecate_inline_image_filters as _deprecate_inline_image_filters,
    filters_flate_decode as _flate_decode,
    filters_flate_decode_png_prediction as _flate_decode_png_prediction,
    filters_flate_encode as _flate_encode,
    filters_flate_get_parameters as _flate_get_parameters,
    filters_jbig2_decode as _jbig2_decode,
    filters_jbig2_is_binary_compatible as _jbig2_is_binary_compatible,
    filters_jpx_decode as _jpx_decode,
    filters_lzw_decode as _lzw_decode,
    filters_lzw_decoder_decode as _lzw_decoder_decode,
    filters_runlength_decode as _runlength_decode,
)

from ._codecs._codecs import LzwCodec as _LzwCodec  # noqa: F401
from ._configuration import get_configuration  # noqa: F401
from ._utils import (
    WHITESPACES_AS_BYTES,  # noqa: F401
    deprecation,  # noqa: F401
    logger_warning,  # noqa: F401
)
from .constants import (
    CcittFaxDecodeParameters,  # noqa: F401
    FilterTypeAbbreviations,  # noqa: F401
    FilterTypes,  # noqa: F401
    ImageAttributes,  # noqa: F401
    LzwFilterParameters,  # noqa: F401
    StreamAttributes,  # noqa: F401
)
from .errors import (  # noqa: F401
    DependencyError,
    LimitReachedError,
    PdfReadError,
    PdfStreamError,
)
from .generic import (  # noqa: F401
    ArrayObject,
    BooleanObject,
    DictionaryObject,
    IndirectObject,
    NumberObject,
    PdfObject,
    StreamObject,
    is_null_or_none,
)

MAX_DECLARED_STREAM_LENGTH = 75_000_000  # DEPRECATED: Use pypdf.Configuration.
MAX_ARRAY_BASED_STREAM_OUTPUT_LENGTH = 75_000_000  # DEPRECATED: Use pypdf.Configuration.

JBIG2_MAX_OUTPUT_LENGTH = 75_000_000  # DEPRECATED: Use pypdf.Configuration.
LZW_MAX_OUTPUT_LENGTH = 75_000_000  # DEPRECATED: Use pypdf.Configuration.
RUN_LENGTH_MAX_OUTPUT_LENGTH = 75_000_000  # DEPRECATED: Use pypdf.Configuration.
ZLIB_MAX_OUTPUT_LENGTH = 75_000_000  # DEPRECATED: Use pypdf.Configuration.
ZLIB_MAX_RECOVERY_INPUT_LENGTH = 5_000_000  # DEPRECATED: Use pypdf.Configuration.
FLATE_MAX_COLUMNS = 250_000  # DEPRECATED: Use pypdf.Configuration.
FLATE_MAX_ROW_LENGTH = 4_000_000  # DEPRECATED: Use pypdf.Configuration.
FLATE_MAX_BUFFER_SIZE = 75_000_000  # DEPRECATED: Use pypdf.Configuration.

# Reuse cached 1-byte values in the fallback loop to avoid per-byte allocations.
_SINGLE_BYTES = tuple(bytes((i,)) for i in range(256))


class FlateDecode:
    @staticmethod
    def decode(
        data: bytes,
        decode_parms: DictionaryObject | IndirectObject | None = None,
        **kwargs: Any,
    ) -> bytes:
        """Decode data which is flate-encoded."""
        return _flate_decode(data, decode_parms)

    @staticmethod
    def _get_parameters(parameters: DictionaryObject) -> tuple[int, int, int]:
        # For details, see table 8 of ISO 32000-2:2020.
        return _flate_get_parameters(parameters)

    @staticmethod
    def _decode_png_prediction(data: bytes, columns: int, row_length: int) -> bytes:
        # PNG prediction can vary from row to row
        return _flate_decode_png_prediction(data, columns, row_length)

    @staticmethod
    def encode(data: bytes, level: int = -1) -> bytes:
        """Compress the input data using zlib."""
        return _flate_encode(data, level)


class ASCIIHexDecode:
    """
    The ASCIIHexDecode filter decodes data that has been encoded in ASCII
    hexadecimal form into a base-7 ASCII format.
    """

    @staticmethod
    def decode(
        data: str | bytes,
        decode_parms: DictionaryObject | None = None,
        **kwargs: Any,
    ) -> bytes:
        """Decode an ASCII-Hex encoded data stream."""
        return _asciihex_decode(data)


class RunLengthDecode:
    """
    The RunLengthDecode filter decodes data that has been encoded in a
    simple byte-oriented format based on run length.
    The encoded data is a sequence of runs, where each run consists of
    a length byte followed by 1 to 128 bytes of data. If the length byte is
    in the range 0 to 127,
    the following length + 1 (1 to 128) bytes are copied literally during
    decompression.
    If length is in the range 129 to 255, the following single byte is to be
    copied 257 − length (2 to 128) times during decompression. A length value
    of 128 denotes EOD.
    """

    @staticmethod
    def decode(
        data: bytes,
        decode_parms: DictionaryObject | None = None,
        **kwargs: Any,
    ) -> bytes:
        """Decode a run length encoded data stream."""
        return _runlength_decode(data)


class LZWDecode:
    class Decoder:
        STOP = 257
        CLEARDICT = 256

        def __init__(self, data: bytes) -> None:
            self.data = data

        def decode(self) -> bytes:
            return _lzw_decoder_decode(self.data)

    @staticmethod
    def decode(
        data: bytes,
        decode_parms: DictionaryObject | None = None,
        **kwargs: Any,
    ) -> bytes:
        """Decode an LZW encoded data stream."""
        # decode_parms is unused here
        return _lzw_decode(data)


class ASCII85Decode:
    """Decodes string ASCII85-encoded data into a byte format."""

    @staticmethod
    def decode(
        data: str | bytes,
        decode_parms: DictionaryObject | None = None,
        **kwargs: Any,
    ) -> bytes:
        """Decode an Ascii85 encoded data stream."""
        return _ascii85_decode(data)


class DCTDecode:
    @staticmethod
    def decode(
        data: bytes,
        decode_parms: DictionaryObject | None = None,
        **kwargs: Any,
    ) -> bytes:
        """Decompress data encoded using a DCT-based JPEG standard."""
        return _dct_decode(data)


class JPXDecode:
    @staticmethod
    def decode(
        data: bytes,
        decode_parms: DictionaryObject | None = None,
        **kwargs: Any,
    ) -> bytes:
        """Decompress data encoded using the wavelet-based JPEG 2000 standard."""
        return _jpx_decode(data)


class BrotliDecode:
    @staticmethod
    @lru_cache(maxsize=1)
    def _check_brotli_available() -> None:
        return _brotli_check_available()

    @staticmethod
    def decode(
        data: bytes,
        decode_parms: DictionaryObject | None = None,
        **kwargs: Any,
    ) -> bytes:
        """Decompress data encoded using the Brotli compression method."""
        return _brotli_decode(data)

    @staticmethod
    def encode(data: bytes, **kwargs: Any) -> bytes:
        """Compress data using the Brotli compression method."""
        return _brotli_encode(data)


@dataclass
class CCITTParameters:
    """§7.4.6, optional parameters for the CCITTFaxDecode filter."""

    K: int = 0
    columns: int = 1728
    rows: int = 0
    EndOfLine: bool | None = False
    EncodedByteAlign: bool | None = False
    EndOfBlock: bool | None = True
    BlackIs1: bool = False
    DamagedRowsBeforeError: int | None = 0

    @property
    def group(self) -> int:
        return _ccitt_parameters_group(self.K)


class CCITTFaxDecode:
    """
    §7.4.6, CCITTFaxDecode filter (ISO 32000).

    Either Group 3 or Group 4 CCITT facsimile (fax) encoding.
    CCITT encoding is bit-oriented, not byte-oriented.

    §7.4.6, optional parameters for the CCITTFaxDecode filter.
    """

    # We use the `L` with standard size, thus have 4 bytes, which corresponds to an upper limit of
    # 2 ** (struct.calcsize("<L") * 8) - 1 = 2 ** (4 * 8) - 1 = 2 ** 32 - 1 = 4_294_967_295
    _MAXIMUM_UNSIGNED_LONG = 0xFFFFFFFF

    @staticmethod
    def _get_parameters(
        parameters: ArrayObject | DictionaryObject | IndirectObject | None,
        rows: int | IndirectObject,
    ) -> CCITTParameters:
        return _ccitt_get_parameters(parameters, rows)

    @staticmethod
    def decode(
        data: bytes,
        decode_parms: DictionaryObject | None = None,
        height: int = 0,
        **kwargs: Any,
    ) -> bytes:
        return _ccitt_decode(data, decode_parms, height)


JBIG2DEC_BINARY = shutil.which("jbig2dec")  # DEPRECATED: Use pypdf.Configuration.


class JBIG2Decode:
    @staticmethod
    def decode(
        data: bytes,
        decode_parms: DictionaryObject | None = None,
        **kwargs: Any,
    ) -> bytes:
        return _jbig2_decode(data, decode_parms)

    @staticmethod
    def _is_binary_compatible() -> bool:
        return _jbig2_is_binary_compatible()


def decode_stream_data(stream: StreamObject) -> bytes:
    """
    Decode the stream data based on the specified filters.

    This function decodes the stream data using the filters provided in the
    stream.

    Args:
        stream: The input stream object containing the data and filters.

    Returns:
        The decoded stream data.

    Raises:
        NotImplementedError: If an unsupported filter type is encountered.

    """
    return _decode_stream_data(stream)

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
