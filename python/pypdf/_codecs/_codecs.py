"""This module is for codecs only.

While the codec implementation can contain details of the PDF specification,
the module should not do any PDF parsing.

Rust-backed module: the implementation lives in
``crates/pypdf-py/src/modules/codecs_mod.rs``.  The public classes are kept as
Python classes (matching upstream's `ABC`-based hierarchy exactly) and delegate
every operation to the Rust implementation.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import ClassVar

from pypdf._pypdf import _LzwCodecImpl

__all__ = ["Codec", "LzwCodec"]


class Codec(ABC):
    """
    Generic codec class.

    It uses an encoder and a decoder which must have the same names as the
    subclass.

    The standard can be found in the PDF Reference 1.7, in the section about
    stream filters.
    """

    @abstractmethod
    def encode(self, data: bytes) -> bytes:
        """
        Encode the input data.

        Args:
            data: Data to encode.

        Returns:
            Encoded data.

        """

    @abstractmethod
    def decode(self, data: bytes) -> bytes:
        """
        Decode the input data.

        Args:
            data: Data to decode.

        Returns:
            Decoded data.

        """


class LzwCodec(Codec):
    """Lempel-Ziv-Welch (LZW) adaptive compression codec."""

    CLEAR_TABLE_MARKER = 256  # Special code to indicate table reset
    EOD_MARKER = 257  # End-of-data marker
    INITIAL_BITS_PER_CODE = 9  # Initial code bit width
    MAX_BITS_PER_CODE = 12  # Maximum code bit width

    _AND_TABLE: ClassVar[list[int]] = [511, 1023, 2047, 4095]

    def __init__(self, max_output_length: int = 75_000_000) -> None:
        self._impl = _LzwCodecImpl(max_output_length)
        self.max_output_length = max_output_length
        self._table_index = -1
        self._next_bits = -1
        self._next_data = -1
        self._byte_pointer = -1
        self.decoding_table: list[bytes] = []

    _STATE_ATTRS = (
        "max_output_length",
        "_table_index",
        "_next_bits",
        "_next_data",
        "_byte_pointer",
        "decoding_table",
        "encoding_table",
        "next_code",
        "bits_per_code",
        "max_code_value",
        "_bits_to_get",
    )

    def _sync_state(self) -> None:
        """Mirror the Rust implementation's mutable state onto Python attributes.

        Upstream keeps this state as plain instance attributes; tests and user
        code read them (e.g. ``codec.decoding_table``), so the wrapper mirrors
        it after every state-changing call.
        """
        impl = self._impl
        for attr in self._STATE_ATTRS:
            try:
                value = getattr(impl, attr)
            except AttributeError:
                continue
            setattr(self, attr, value)

    def _initialize_encoding_table(self) -> None:
        """Initialize the encoding table and state to initial conditions."""
        result = self._impl._initialize_encoding_table()
        self._sync_state()
        return result

    def _increase_next_code(self) -> None:
        """Update bits_per_code and max_code_value if necessary."""
        result = self._impl._increase_next_code()
        self._sync_state()
        return result

    def _pack_codes_into_bytes(self, codes: list[int]) -> bytes:
        """Pack the variable-width codes into a byte string."""
        result = self._impl._pack_codes_into_bytes(codes)
        self._sync_state()
        return result

    def _initialize_decoding_table(self) -> None:
        """Initialize the decoding table and state to initial conditions."""
        result = self._impl._initialize_decoding_table()
        self._sync_state()
        return result

    def _next_code_decode(self, data: bytes) -> int:
        """Read the next variable-width code from the data."""
        result = self._impl._next_code_decode(data)
        self._sync_state()
        return result

    def _add_entry_decode(self, old_string: bytes, new_char: int) -> None:
        """Add a new entry to the decoding table."""
        result = self._impl._add_entry_decode(old_string, new_char)
        self._sync_state()
        return result

    def encode(self, data: bytes) -> bytes:
        """
        Encode data using the LZW compression algorithm.

        Taken from PDF 1.7 specs, "7.4.4.2 Details of LZW Encoding".
        """
        result = self._impl.encode(data)
        self._sync_state()
        return result

    def decode(self, data: bytes) -> bytes:
        """
        Decode data using the LZW decompression algorithm.

        Taken from PDF 1.7 specs, "7.4.4.3 Details of LZW Decoding".
        """
        result = self._impl.decode(data)
        self._sync_state()
        return result


# --- upstream module-namespace parity (validation §4.1) -----------------------
from pypdf._pypdf import logger_warning as logger_warning  # noqa: E402,F401
from pypdf.errors import LimitReachedError as LimitReachedError  # noqa: E402,F401
from pypdf.errors import PdfStreamError as PdfStreamError  # noqa: E402,F401

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations  # noqa: E402

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
