"""Implementation of generic PDF objects (dictionary, number, string, ...).

Rust port of ``pypdf/generic/_base.py`` (upstream pypdf 7.1.1).  The Python class
hierarchy, the class attributes and the method surface are kept verbatim; every
method body delegates to the Rust implementation in
``crates/pypdf-py/src/modules/base_mod.rs`` (reached through the
``pypdf._pypdf`` extension module).

Keeping the classes (and their methods) as Python objects is intentional: the
official test suite observes the Python object model directly
(``isinstance(x, PdfObject)``, ``type(PdfObject) is type``, subclassing, and
``sys.settrace``-based Python frame counts).  All logic lives in Rust.
"""

__author__ = "Mathieu Fenniak"
__author_email__ = "biziqe@mathieu.fenniak.net"

import hashlib
import re
from collections.abc import Callable
from typing import Any, ClassVar, Optional, TypeGuard, cast

from pypdf._pypdf import (
    basemod_boolean_clone as _boolean_clone,
    basemod_boolean_eq as _boolean_eq,
    basemod_boolean_hash as _boolean_hash,
    basemod_boolean_hash_bin as _boolean_hash_bin,
    basemod_boolean_read_from_stream as _boolean_read_from_stream,
    basemod_boolean_repr as _boolean_repr,
    basemod_boolean_write_to_stream as _boolean_write_to_stream,
    basemod_bytestring_clone as _bytestring_clone,
    basemod_bytestring_hash_bin as _bytestring_hash_bin,
    basemod_bytestring_original_bytes as _bytestring_original_bytes,
    basemod_bytestring_str as _bytestring_str,
    basemod_bytestring_write_to_stream as _bytestring_write_to_stream,
    basemod_encode_pdfdocencoding as _encode_pdfdocencoding,
    basemod_float_as_numeric as _float_as_numeric,
    basemod_float_clone as _float_clone,
    basemod_float_hash_bin as _float_hash_bin,
    basemod_float_myrepr as _float_myrepr,
    basemod_float_new as _float_new,
    basemod_float_repr as _float_repr,
    basemod_float_write_to_stream as _float_write_to_stream,
    basemod_indirect_clone as _indirect_clone,
    basemod_indirect_contains as _indirect_contains,
    basemod_indirect_deepcopy as _indirect_deepcopy,
    basemod_indirect_eq as _indirect_eq,
    basemod_indirect_float as _indirect_float,
    basemod_indirect_get_object as _indirect_get_object,
    basemod_indirect_get_object_with_check as _indirect_get_object_with_check,
    basemod_indirect_getattr as _indirect_getattr,
    basemod_indirect_getitem as _indirect_getitem,
    basemod_indirect_hash as _indirect_hash,
    basemod_indirect_hash_bin as _indirect_hash_bin,
    basemod_indirect_int as _indirect_int,
    basemod_indirect_iter as _indirect_iter,
    basemod_indirect_ne as _indirect_ne,
    basemod_indirect_read_from_stream as _indirect_read_from_stream,
    basemod_indirect_replicate as _indirect_replicate,
    basemod_indirect_repr as _indirect_repr,
    basemod_indirect_str as _indirect_str,
    basemod_indirect_write_to_stream as _indirect_write_to_stream,
    basemod_is_null_or_none as _is_null_or_none,
    basemod_name_clone as _name_clone,
    basemod_name_hash_bin as _name_hash_bin,
    basemod_name_read_from_stream as _name_read_from_stream,
    basemod_name_renumber as _name_renumber,
    basemod_name_sanitize as _name_sanitize,
    basemod_name_unnumber as _name_unnumber,
    basemod_name_write_to_stream as _name_write_to_stream,
    basemod_null_clone as _null_clone,
    basemod_null_eq as _null_eq,
    basemod_null_hash as _null_hash,
    basemod_null_hash_bin as _null_hash_bin,
    basemod_null_read_from_stream as _null_read_from_stream,
    basemod_null_repr as _null_repr,
    basemod_null_write_to_stream as _null_write_to_stream,
    basemod_number_as_numeric as _number_as_numeric,
    basemod_number_clone as _number_clone,
    basemod_number_hash_bin as _number_hash_bin,
    basemod_number_new as _number_new,
    basemod_number_read_from_stream as _number_read_from_stream,
    basemod_number_write_to_stream as _number_write_to_stream,
    basemod_pdfobject_clone as _pdfobject_clone,
    basemod_pdfobject_get_object as _pdfobject_get_object,
    basemod_pdfobject_hash_bin as _pdfobject_hash_bin,
    basemod_pdfobject_hash_value as _pdfobject_hash_value,
    basemod_pdfobject_hash_value_data as _pdfobject_hash_value_data,
    basemod_pdfobject_reference_clone as _pdfobject_reference_clone,
    basemod_pdfobject_replicate as _pdfobject_replicate,
    basemod_pdfobject_write_to_stream as _pdfobject_write_to_stream,
    basemod_textstring_clone as _textstring_clone,
    basemod_textstring_get_encoded_bytes as _textstring_get_encoded_bytes,
    basemod_textstring_get_original_bytes as _textstring_get_original_bytes,
    basemod_textstring_hash_bin as _textstring_hash_bin,
    basemod_textstring_new as _textstring_new,
    basemod_textstring_original_bytes as _textstring_original_bytes,
    basemod_textstring_write_to_stream as _textstring_write_to_stream,
)


class PdfObject:
    # function for calculating a hash value
    hash_func: Callable[..., Any] = hashlib.sha1
    indirect_reference: Optional["IndirectObject"]

    def hash_bin(self) -> int:
        """
        Used to detect modified object.

        Returns:
            Hash considering type and value.

        """
        return _pdfobject_hash_bin(self)

    def hash_value_data(self) -> bytes:
        return _pdfobject_hash_value_data(self)

    def hash_value(self) -> bytes:
        return _pdfobject_hash_value(self)

    def replicate(self, pdf_dest: Any) -> "PdfObject":
        """
        Clone object into pdf_dest (PdfWriterProtocol which is an interface for PdfWriter)
        without ensuring links. This is used in clone_document_from_root with incremental = True.

        Args:
          pdf_dest: Target to clone to.

        Returns:
          The cloned PdfObject

        """
        return _pdfobject_replicate(self, pdf_dest)

    def clone(
        self,
        pdf_dest: Any,
        force_duplicate: bool = False,
        ignore_fields: Any = (),
    ) -> "PdfObject":
        """
        Clone object into pdf_dest (PdfWriterProtocol which is an interface for PdfWriter).

        By default, this method will call ``_reference_clone`` (see ``_reference``).


        Args:
          pdf_dest: Target to clone to.
          force_duplicate: By default, if the object has already been cloned and referenced,
            the copy will be returned; when ``True``, a new copy will be created.
            (Default value = ``False``)
          ignore_fields: List/tuple of field names (for dictionaries) that will be ignored
            during cloning (applies to children duplication as well). If fields are to be
            considered for a limited number of levels, you have to add it as integer, for
            example ``[1,"/B","/TOTO"]`` means that ``"/B"`` will be ignored at the first
            level only but ``"/TOTO"`` on all levels.

        Returns:
          The cloned PdfObject

        """
        return _pdfobject_clone(self, pdf_dest, force_duplicate, ignore_fields)

    def _reference_clone(
        self, clone: "PdfObject", pdf_dest: Any, force_duplicate: bool = False
    ) -> "PdfObject":
        """
        Reference the object within the _objects of pdf_dest only if
        indirect_reference attribute exists (which means the objects was
        already identified in xref/xobjstm) if object has been already
        referenced do nothing.

        Args:
          clone:
          pdf_dest:

        Returns:
          The clone

        """
        return _pdfobject_reference_clone(self, clone, pdf_dest, force_duplicate)

    def get_object(self) -> Optional["PdfObject"]:
        """Resolve indirect references."""
        return _pdfobject_get_object(self)

    def write_to_stream(self, stream: Any) -> None:
        return _pdfobject_write_to_stream(self, stream)


class NullObject(PdfObject):
    def clone(
        self,
        pdf_dest: Any,
        force_duplicate: bool = False,
        ignore_fields: Any = (),
    ) -> "NullObject":
        """Clone object into pdf_dest."""
        if not hasattr(self, "indirect_reference"):
            # Direct object: shortcut, as some arrays consist mostly of nulls
            return NullObject()
        return cast("NullObject", _null_clone(self, pdf_dest, force_duplicate))

    def hash_bin(self) -> int:
        """
        Used to detect modified object.

        Returns:
            Hash considering type and value.

        """
        return _null_hash_bin(self)

    def write_to_stream(self, stream: Any) -> None:
        return _null_write_to_stream(self, stream)

    @staticmethod
    def read_from_stream(stream: Any) -> "NullObject":
        return _null_read_from_stream(stream)

    def __repr__(self) -> str:
        return _null_repr(self)

    def __eq__(self, other: object) -> bool:
        return _null_eq(self, other)

    def __hash__(self) -> int:
        return _null_hash(self)


class BooleanObject(PdfObject):
    value: bool

    def __init__(self, value: Any) -> None:
        self.value = value

    def clone(
        self,
        pdf_dest: Any,
        force_duplicate: bool = False,
        ignore_fields: Any = (),
    ) -> "BooleanObject":
        """Clone object into pdf_dest."""
        return cast(
            "BooleanObject", _boolean_clone(self, pdf_dest, force_duplicate)
        )

    def hash_bin(self) -> int:
        """
        Used to detect modified object.

        Returns:
            Hash considering type and value.

        """
        return _boolean_hash_bin(self)

    def __eq__(self, o: object, /) -> bool:
        return _boolean_eq(self, o)

    def __hash__(self) -> int:
        return _boolean_hash(self)

    def __repr__(self) -> str:
        return _boolean_repr(self)

    def write_to_stream(self, stream: Any) -> None:
        return _boolean_write_to_stream(self, stream)

    @staticmethod
    def read_from_stream(stream: Any) -> "BooleanObject":
        return _boolean_read_from_stream(stream)


class IndirectObject(PdfObject):
    _MAXIMUM_PART_LENGTH = 64

    def __init__(self, idnum: int, generation: int, pdf: Any) -> None:  # PdfReader
        self.idnum = idnum
        self.generation = generation
        self.pdf = pdf

    def __hash__(self) -> int:
        return _indirect_hash(self)

    def hash_bin(self) -> int:
        """
        Used to detect modified object.

        Returns:
            Hash considering type and value.

        """
        return _indirect_hash_bin(self)

    def replicate(
        self,
        pdf_dest: Any,
    ) -> "PdfObject":
        return _indirect_replicate(self, pdf_dest)

    def clone(
        self,
        pdf_dest: Any,
        force_duplicate: bool = False,
        ignore_fields: Any = (),
    ) -> "IndirectObject":
        """Clone object into pdf_dest."""
        return _indirect_clone(self, pdf_dest, force_duplicate, ignore_fields)

    @property
    def indirect_reference(self) -> "IndirectObject":
        return self

    def get_object(self) -> Optional["PdfObject"]:
        return _indirect_get_object(self)

    def __deepcopy__(self, memo: Any) -> "IndirectObject":
        return _indirect_deepcopy(self, memo)

    def _get_object_with_check(self) -> Optional["PdfObject"]:
        return _indirect_get_object_with_check(self)

    def __getattr__(self, name: str) -> Any:
        # Attribute not found in object: look in pointed object
        return _indirect_getattr(self, name)

    def __getitem__(self, key: Any) -> Any:
        # items should be extracted from pointed Object
        return _indirect_getitem(self, key)

    def __contains__(self, key: Any) -> bool:
        return _indirect_contains(self, key)

    def __iter__(self) -> Any:
        return _indirect_iter(self)

    def __float__(self) -> str:
        # in this case we are looking for the pointed data
        return _indirect_float(self)

    def __int__(self) -> int:
        # in this case we are looking for the pointed data
        return _indirect_int(self)

    def __str__(self) -> str:
        # in this case we are looking for the pointed data
        return _indirect_str(self)

    def __repr__(self) -> str:
        return _indirect_repr(self)

    def __eq__(self, other: object) -> bool:
        return _indirect_eq(self, other)

    def __ne__(self, other: object) -> bool:
        return _indirect_ne(self, other)

    def write_to_stream(self, stream: Any) -> None:
        return _indirect_write_to_stream(self, stream)

    @staticmethod
    def read_from_stream(stream: Any, pdf: Any) -> "IndirectObject":  # PdfReader
        return _indirect_read_from_stream(stream, pdf)


FLOAT_WRITE_PRECISION = 8  # shall be min 5 digits max, allow user adj


class FloatObject(float, PdfObject):
    def __new__(
        cls, value: Any = "0.0", context: Any | None = None
    ) -> "FloatObject":
        return _float_new(cls, value, context)

    def clone(
        self,
        pdf_dest: Any,
        force_duplicate: bool = False,
        ignore_fields: Any = (),
    ) -> "FloatObject":
        """Clone object into pdf_dest."""
        return cast("FloatObject", _float_clone(self, pdf_dest, force_duplicate))

    def hash_bin(self) -> int:
        """
        Used to detect modified object.

        Returns:
            Hash considering type and value.

        """
        return _float_hash_bin(self)

    def myrepr(self) -> str:
        return _float_myrepr(self, FLOAT_WRITE_PRECISION)

    def __repr__(self) -> str:
        return _float_repr(self)

    def as_numeric(self) -> float:
        return _float_as_numeric(self)

    def write_to_stream(self, stream: Any) -> None:
        return _float_write_to_stream(self, stream)


class NumberObject(int, PdfObject):
    NumberPattern = re.compile(b"[^+-.0-9]")
    _LENGTH_LIMIT = 64

    def __new__(cls, value: Any) -> "NumberObject":
        return _number_new(cls, value)

    def clone(
        self,
        pdf_dest: Any,
        force_duplicate: bool = False,
        ignore_fields: Any = (),
    ) -> "NumberObject":
        """Clone object into pdf_dest."""
        return cast("NumberObject", _number_clone(self, pdf_dest, force_duplicate))

    def hash_bin(self) -> int:
        """
        Used to detect modified object.

        Returns:
            Hash considering type and value.

        """
        return _number_hash_bin(self)

    def as_numeric(self) -> int:
        return _number_as_numeric(self)

    def write_to_stream(self, stream: Any) -> None:
        return _number_write_to_stream(self, stream)

    @staticmethod
    def read_from_stream(stream: Any) -> Any:
        return _number_read_from_stream(stream)


class ByteStringObject(bytes, PdfObject):
    """
    Represents a string object where the text encoding could not be determined.

    This occurs quite often, as the PDF spec doesn't provide an alternate way to
    represent strings -- for example, the encryption data stored in files (like
    /O) is clearly not text, but is still stored in a "String" object.
    """

    def clone(
        self,
        pdf_dest: Any,
        force_duplicate: bool = False,
        ignore_fields: Any = (),
    ) -> "ByteStringObject":
        """Clone object into pdf_dest."""
        return cast(
            "ByteStringObject", _bytestring_clone(self, pdf_dest, force_duplicate)
        )

    def hash_bin(self) -> int:
        """
        Used to detect modified object.

        Returns:
            Hash considering type and value.

        """
        return _bytestring_hash_bin(self)

    @property
    def original_bytes(self) -> bytes:
        """For compatibility with TextStringObject.original_bytes."""
        return _bytestring_original_bytes(self)

    def write_to_stream(self, stream: Any) -> None:
        return _bytestring_write_to_stream(self, stream)

    def __str__(self) -> str:
        return _bytestring_str(self)


class TextStringObject(str, PdfObject):  # noqa: SLOT000
    """
    A string object that has been decoded into a real unicode string.

    If read from a PDF document, this string appeared to match the
    PDFDocEncoding, or contained a UTF-16BE BOM mark to cause UTF-16 decoding
    to occur.
    """

    autodetect_pdfdocencoding: bool
    autodetect_utf16: bool
    utf16_bom: bytes
    _original_bytes: bytes | None = None

    def __new__(cls, value: Any) -> "TextStringObject":
        return _textstring_new(cls, value)

    def clone(
        self,
        pdf_dest: Any,
        force_duplicate: bool = False,
        ignore_fields: Any = (),
    ) -> "TextStringObject":
        """Clone object into pdf_dest."""
        return cast(
            "TextStringObject", _textstring_clone(self, pdf_dest, force_duplicate)
        )

    def hash_bin(self) -> int:
        """
        Used to detect modified object.

        Returns:
            Hash considering type and value.

        """
        return _textstring_hash_bin(self)

    @property
    def original_bytes(self) -> bytes:
        """
        It is occasionally possible that a text string object gets created where
        a byte string object was expected due to the autodetection mechanism --
        if that occurs, this "original_bytes" property can be used to
        back-calculate what the original encoded bytes were.
        """
        return _textstring_original_bytes(self)

    def get_original_bytes(self) -> bytes:
        # We're a text string object, but the library is trying to get our raw
        # bytes. This can happen if we auto-detected this string as text, but
        # we were wrong. It's pretty common. Return the original bytes that
        # would have been used to create this object, based upon the autodetect
        # method.
        return _textstring_get_original_bytes(self)

    def get_encoded_bytes(self) -> bytes:
        # Try to write the string out as a PDFDocEncoding encoded string. It's
        # nicer to look at in the PDF file. Sadly, we take a performance hit
        # here for trying...
        return _textstring_get_encoded_bytes(self)

    def write_to_stream(self, stream: Any) -> None:
        return _textstring_write_to_stream(self, stream)


class NameObject(str, PdfObject):  # noqa: SLOT000
    delimiter_pattern = re.compile(rb"\s+|[\(\)<>\[\]{}/%]")
    prefix = b"/"
    renumber_table: ClassVar[dict[str, bytes]] = {
        **{chr(i): f"#{i:02X}".encode() for i in b"#()<>[]{}/%"},
        **{chr(i): f"#{i:02X}".encode() for i in range(33)},
    }
    _LENGTH_LIMIT = 4096

    def clone(
        self,
        pdf_dest: Any,
        force_duplicate: bool = False,
        ignore_fields: Any = (),
    ) -> "NameObject":
        """Clone object into pdf_dest."""
        return cast("NameObject", _name_clone(self, pdf_dest, force_duplicate))

    def hash_bin(self) -> int:
        """
        Used to detect modified object.

        Returns:
            Hash considering type and value.

        """
        return _name_hash_bin(self)

    def write_to_stream(self, stream: Any) -> None:
        return _name_write_to_stream(self, stream)

    def renumber(self) -> bytes:
        return _name_renumber(self)

    def _sanitize(self) -> "NameObject":
        """
        Sanitize the NameObject's name to be a valid PDF name part
        (alphanumeric, underscore, hyphen). The _sanitize method replaces
        spaces and any non-alphanumeric/non-underscore/non-hyphen with
        underscores.

        Returns:
            NameObject with sanitized name.
        """
        return _name_sanitize(self)

    @staticmethod
    def unnumber(sin: bytes) -> bytes:
        return _name_unnumber(sin)

    CHARSETS = ("utf-8", "gbk", "latin1")

    @staticmethod
    def read_from_stream(stream: Any, pdf: Any) -> "NameObject":  # PdfReader
        return _name_read_from_stream(stream, pdf)


def encode_pdfdocencoding(unicode_string: str) -> bytes:
    return _encode_pdfdocencoding(unicode_string)


def is_null_or_none(x: Any) -> TypeGuard[NullObject | IndirectObject | None]:
    """
    Returns:
        True if x is None or NullObject.

    """
    return _is_null_or_none(x)


# --- upstream module-namespace parity (validation §4.1) -----------------------
from collections.abc import Sequence as Sequence  # noqa: F811 - upstream imports it from here
from math import log10 as log10
from struct import iter_unpack as iter_unpack
from typing import Self as Self, Union as Union

from pypdf._codecs import _pdfdoc_encoding_rev as _pdfdoc_encoding_rev
from pypdf._pypdf import (  # noqa: F401 - upstream module globals
    WHITESPACES as WHITESPACES,
    StreamType as StreamType,
    logger_warning as logger_warning,
    read_non_whitespace as read_non_whitespace,
    read_until_regex as read_until_regex,
)
from pypdf._protocols import PdfWriterProtocol as PdfWriterProtocol
from pypdf.errors import (
    PdfReadError as PdfReadError,
    PdfStreamError as PdfStreamError,
    STREAM_TRUNCATED_PREMATURELY as STREAM_TRUNCATED_PREMATURELY,
)

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
