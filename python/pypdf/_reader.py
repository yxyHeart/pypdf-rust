# Copyright (c) 2006, Mathieu Fenniak
# Copyright (c) 2007, Ashish Kulkarni <kulkarni.ashish@gmail.com>
#
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

"""Rust-backed port of ``pypdf/_reader.py`` (upstream pypdf 7.1.1).

The class structure is kept verbatim from upstream -- the ``PdfReader`` class,
its ``@property``/``@staticmethod``/``@classmethod`` decorators, the class
attributes and the module-level re-exports -- so that MRO, ``isinstance``,
subclassing and ``mock.patch`` targets are unchanged.  Every method body is a
one-line delegation to the Rust implementation in
``crates/pypdf-py/src/modules/reader_mod.rs`` (reached through
``pypdf._pypdf``), which contains all of the parsing/decryption logic.
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
from typing import (
    TYPE_CHECKING,
    Any,
    cast,
)

if sys.version_info >= (3, 11):
    from typing import Self
else:
    from typing_extensions import Self

from ._configuration import apply_legacy_configuration
from ._doc_common import PdfDocCommon, convert_to_int
from ._encryption import Encryption, PasswordType
from ._utils import (
    WHITESPACES_AS_BYTES,
    StrByteType,
    StreamType,
    _TraversalState,
    get_stream_data,
    get_stream_size,
    logger_warning,
    read_non_whitespace,
    read_previous_line,
    read_until_whitespace,
    skip_over_comment,
    skip_over_whitespace,
)
from .constants import TrailerKeys
from .errors import (
    EmptyFileError,
    FileNotDecryptedError,
    LimitReachedError,
    PdfReadError,
    PdfStreamError,
    WrongPasswordError,
)
from .generic import (
    ArrayObject,
    ByteStringObject,
    Destination,
    DictionaryObject,
    EncodedStreamObject,
    IndirectObject,
    NameObject,
    NullObject,
    NumberObject,
    PdfObject,
    StreamObject,
    TextStringObject,
    is_null_or_none,
    read_object,
)
from .xmp import XmpInformation

from pypdf._pypdf import (
    reader_add_form_topname as _add_form_topname,
    reader_basic_validation as _basic_validation,
    reader_cache_get_indirect_object as _cache_get_indirect_object,
    reader_cache_indirect_object as _cache_indirect_object,
    reader_close as _close,
    reader_decrypt as _decrypt,
    reader_enter as _enter,
    reader_exit as _exit,
    reader_find_eof_marker as _find_eof_marker,
    reader_find_pdf_objects as _find_pdf_objects,
    reader_find_pdf_trailers as _find_pdf_trailers,
    reader_find_previous_startxref_pos as _find_previous_startxref_pos,
    reader_find_startxref_pos as _find_startxref_pos,
    reader_get_named_destinations as _get_named_destinations,
    reader_get_object as _get_object,
    reader_get_object_from_stream as _get_object_from_stream,
    reader_get_page_number_by_indirect as _get_page_number_by_indirect,
    reader_get_xref_issues as _get_xref_issues,
    reader_handle_encryption as _handle_encryption,
    reader_id as _id,
    reader_info as _info,
    reader_init as _init,
    reader_initialize_stream as _initialize_stream,
    reader_is_encrypted as _is_encrypted,
    reader_is_valid_startxref_value as _is_valid_startxref_value,
    reader_load_recovery_cache as _load_recovery_cache,
    reader_pairs as _pairs,
    reader_pdf_header as _pdf_header,
    reader_process_xref_stream as _process_xref_stream,
    reader_read as _read,
    reader_read_object_header as _read_object_header,
    reader_read_pdf15_xref_stream as _read_pdf15_xref_stream,
    reader_read_standard_xref_table as _read_standard_xref_table,
    reader_read_standard_xref_table_handle_size as _read_standard_xref_table__handle_size,
    reader_read_xref as _read_xref,
    reader_read_xref_other_error as _read_xref_other_error,
    reader_read_xref_subsections as _read_xref_subsections,
    reader_read_xref_tables_and_trailers as _read_xref_tables_and_trailers,
    reader_rebuild_xref_table as _rebuild_xref_table,
    reader_rename_form_topname as _rename_form_topname,
    reader_replace_object as _replace_object,
    reader_repr_mimebundle as _repr_mimebundle,
    reader_root_object as _root_object,
    reader_sanitize_pdf15_xref_stream_index_pairs as _sanitize_pdf15_xref_stream_index_pairs,
    reader_xmp_metadata as _xmp_metadata,
)

if TYPE_CHECKING:
    from ._page import PageObject


class PdfReader(PdfDocCommon):
    """
    Initialize a PdfReader object.

    This operation can take some time, as the PDF stream's cross-reference
    tables are read into memory.

    Args:
        stream: A File object or an object that supports the standard read
            and seek methods similar to a File object. Could also be a
            string representing a path to a PDF file.
        strict: Determines whether user should be warned of all
            problems and also causes some correctable problems to be fatal.
            Defaults to ``False``.
        password: Decrypt PDF file at initialization. If the
            password is None, the file will not be decrypted.
            Defaults to ``None``.
        root_object_recovery_limit: The maximum number of objects to query
            for recovering the Root object in non-strict mode. To disable
            this security measure, pass ``None``.
        xref_stream_maximum_entries: The maximum number of entries which are
            allowed inside a stream-based cross-reference stream.

    """

    xref: dict[int, dict[Any, Any]]
    trailer: DictionaryObject

    def __init__(
        self,
        stream: StrByteType | Path,
        strict: bool = False,
        password: str | bytes | None = None,
        *,
        root_object_recovery_limit: int | None = 10_000,
        xref_stream_maximum_entries: int = 2_000_000,
    ) -> None:
        return _init(
            self,
            stream,
            strict,
            password,
            root_object_recovery_limit,
            xref_stream_maximum_entries,
        )

    def _initialize_stream(self, stream: StrByteType | Path) -> None:
        return _initialize_stream(self, stream)

    def _handle_encryption(self, password: str | bytes | None) -> None:
        return _handle_encryption(self, password)

    def __enter__(self) -> Self:
        return _enter(self)

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        return _exit(self, exc_type, exc_val, exc_tb)

    def close(self) -> None:
        """Close the stream if opened in __init__ and clear memory."""
        return _close(self)

    @property
    def root_object(self) -> DictionaryObject:
        """Provide access to "/Root". Standardized with PdfWriter."""
        return _root_object(self)

    @property
    def _info(self) -> DictionaryObject | None:
        """
        Provide access to "/Info". Standardized with PdfWriter.

        Returns:
            /Info Dictionary; None if the entry does not exist

        """
        return _info(self)

    @property
    def _ID(self) -> ArrayObject | None:
        """
        Provide access to "/ID". Standardized with PdfWriter.

        Returns:
            /ID array; None if the entry does not exist

        """
        return _id(self)

    @property
    def pdf_header(self) -> str:
        """
        The first 8 bytes of the file.

        This is typically something like ``'%PDF-1.6'`` and can be used to
        detect if the file is actually a PDF file and which version it is.
        """
        return _pdf_header(self)

    @property
    def xmp_metadata(self) -> XmpInformation | None:
        """XMP (Extensible Metadata Platform) data."""
        return _xmp_metadata(self)

    def _get_page_number_by_indirect(
        self, indirect_reference: int | NullObject | IndirectObject | None
    ) -> int | None:
        """
        Retrieve the page number from an indirect reference.

        Args:
            indirect_reference: The indirect reference to locate.

        Returns:
            Page number or None.

        """
        return _get_page_number_by_indirect(self, indirect_reference)

    def _get_object_from_stream(
        self, indirect_reference: IndirectObject
    ) -> PdfObject:
        return _get_object_from_stream(self, indirect_reference)

    def get_object(
        self, indirect_reference: int | IndirectObject
    ) -> PdfObject | None:
        return _get_object(self, indirect_reference)

    def read_object_header(self, stream: StreamType) -> tuple[int, int]:
        return _read_object_header(self, stream)

    def cache_get_indirect_object(
        self, generation: int, idnum: int
    ) -> PdfObject | None:
        return _cache_get_indirect_object(self, generation, idnum)

    def cache_indirect_object(
        self, generation: int, idnum: int, obj: PdfObject | None
    ) -> PdfObject | None:
        return _cache_indirect_object(self, generation, idnum, obj)

    def _replace_object(self, indirect_reference: IndirectObject, obj: PdfObject) -> PdfObject:
        return _replace_object(self, indirect_reference, obj)

    def read(self, stream: StreamType) -> None:
        """
        Read and process the PDF stream, extracting necessary data.

        Args:
            stream: The PDF file stream.

        """
        return _read(self, stream)

    def _basic_validation(self, stream: StreamType) -> None:
        """Ensure the stream is valid and not empty."""
        return _basic_validation(self, stream)

    def _find_eof_marker(self, stream: StreamType) -> None:
        """
        Jump to the %%EOF marker.

        According to the specs, the %%EOF marker should be at the very end of
        the file. Hence for standard-compliant PDF documents this function will
        read only the last part (DEFAULT_BUFFER_SIZE).

        Producers that omit the line break before the marker (``startxref
        256%%EOF`` or ``256%%EOF``) are accepted as well (#4127).
        """
        return _find_eof_marker(self, stream)

    def _find_startxref_pos(self, stream: StreamType) -> int:
        """
        Find startxref entry - the location of the xref table.

        Args:
            stream:

        Returns:
            The bytes offset

        """
        return _find_startxref_pos(self, stream)

    # Upper bound on the number of lines _find_previous_startxref_pos scans
    # backwards while recovering a corrupt trailing startxref pointer. Kept
    # fixed and non-configurable so a crafted file cannot trigger an unbounded
    # backwards scan.
    _MAX_STARTXREF_RECOVERY_LINES = 1000

    @staticmethod
    def _is_valid_startxref_value(value: bytes) -> bool:
        return _is_valid_startxref_value(value)

    @classmethod
    def _find_previous_startxref_pos(cls, stream: StreamType) -> int:
        """
        Recover the most recent intact ``startxref`` pointer by scanning
        backwards from the current position.

        This is used as a fallback when the ``startxref`` keyword belonging to
        the final ``%%EOF`` is corrupt (#3238). The offset always appears on
        the line directly below the keyword, so the value read immediately
        before encountering ``startxref`` (while moving backwards) is returned.
        At most ``_MAX_STARTXREF_RECOVERY_LINES`` lines are inspected.

        Args:
            stream: The PDF byte stream, positioned just above the corrupt
                trailing pointer.

        Returns:
            The bytes offset of the recovered ``startxref``.

        """
        return _find_previous_startxref_pos(cls, stream)

    def _load_recovery_cache(self, data: bytes) -> dict[int, tuple[int, int]]:
        return _load_recovery_cache(self, data)

    def _read_standard_xref_table__handle_size(self, stream: StreamType) -> int | None:
        return _read_standard_xref_table__handle_size(self, stream)

    def _read_standard_xref_table(self, stream: StreamType) -> None:
        return _read_standard_xref_table(self, stream)

    def _read_xref_tables_and_trailers(
        self, stream: StreamType, startxref: int | None, xref_issue_nr: int
    ) -> None:
        """Read the cross-reference tables and trailers in the PDF stream."""
        return _read_xref_tables_and_trailers(self, stream, startxref, xref_issue_nr)

    # The trailer keys a PDF 1.5+ cross-reference stream carries in place of a
    # `trailer` keyword (PDF 2.0 specification, table 17).
    _XREF_STREAM_TRAILER_KEYS = (
        TrailerKeys.ROOT, TrailerKeys.ENCRYPT, TrailerKeys.INFO, TrailerKeys.ID, TrailerKeys.SIZE
    )

    def _process_xref_stream(self, xrefstream: DictionaryObject) -> None:
        """Process and handle the xref stream."""
        return _process_xref_stream(self, xrefstream)

    def _read_xref(self, stream: StreamType) -> int | None:
        return _read_xref(self, stream)

    def _read_xref_other_error(
        self, stream: StreamType, startxref: int
    ) -> int | None:
        return _read_xref_other_error(self, stream, startxref)

    def _sanitize_pdf15_xref_stream_index_pairs(
            self, index_pairs: list[int], entry_sizes: list[int], xref_stream: StreamObject
    ) -> list[int]:
        return _sanitize_pdf15_xref_stream_index_pairs(
            self, index_pairs, entry_sizes, xref_stream
        )

    def _read_pdf15_xref_stream(self, stream: StreamType) -> StreamObject:
        """Read the cross-reference stream for PDF 1.5+."""
        return _read_pdf15_xref_stream(self, stream)

    @staticmethod
    def _get_xref_issues(stream: StreamType, startxref: int) -> int:
        """
        Return an int which indicates an issue. 0 means there is no issue.

        Args:
            stream:
            startxref:

        Returns:
            0 means no issue, other values represent specific issues.

        """
        return _get_xref_issues(stream, startxref)

    @classmethod
    def _find_pdf_objects(cls, data: bytes) -> Iterable[tuple[int, int, int]]:
        return _find_pdf_objects(cls, data)

    @classmethod
    def _find_pdf_trailers(cls, data: bytes) -> Iterable[int]:
        return _find_pdf_trailers(cls, data)

    def _rebuild_xref_table(self, stream: StreamType) -> None:
        return _rebuild_xref_table(self, stream)

    def _read_xref_subsections(
        self,
        idx_pairs: list[int],
        get_entry: Callable[[int], int | tuple[int, ...]],
        used_before: Callable[[int, int | tuple[int, ...]], bool],
    ) -> None:
        """Read and process the subsections of the xref."""
        return _read_xref_subsections(self, idx_pairs, get_entry, used_before)

    def _pairs(self, array: list[int]) -> Iterable[tuple[int, int]]:
        """Iterate over pairs in the array."""
        return _pairs(self, array)

    def decrypt(self, password: str | bytes) -> PasswordType:
        """
        When using an encrypted / secured PDF file with the PDF Standard
        encryption handler, this function will allow the file to be decrypted.
        It checks the given password against the document's user password and
        owner password, and then stores the resulting decryption key if either
        password is correct.

        It does not matter which password was matched. Both passwords provide
        the correct decryption key that will allow the document to be used with
        this library.

        Args:
            password: The password to match.

        Returns:
            An indicator if the document was decrypted and whether it was the
            owner password or the user password.

        """
        return _decrypt(self, password)

    @property
    def is_encrypted(self) -> bool:
        """
        Read-only boolean property showing whether this PDF file is encrypted.

        Note that this property, if true, will remain true even after the
        :meth:`decrypt()<pypdf.PdfReader.decrypt>` method is called.
        """
        return _is_encrypted(self)

    def add_form_topname(self, name: str) -> DictionaryObject | None:
        """
        Add a top level form that groups all form fields below it.

        Args:
            name: text string of the "/T" Attribute of the created object

        Returns:
            The created object. ``None`` means no object was created.

        """
        return _add_form_topname(self, name)

    def rename_form_topname(self, name: str) -> DictionaryObject | None:
        """
        Rename top level form field that all form fields below it.

        Args:
            name: text string of the "/T" field of the created object

        Returns:
            The modified object. ``None`` means no object was modified.

        """
        return _rename_form_topname(self, name)

    def _repr_mimebundle_(
        self,
        include: Iterable[str] | None = None,
        exclude: Iterable[str] | None = None,
    ) -> dict[str, Any]:
        """
        Integration into Jupyter Notebooks.

        This method returns a dictionary that maps a mime-type to its
        representation.

        .. seealso::

            https://ipython.readthedocs.io/en/stable/config/integrating.html
        """
        return _repr_mimebundle(self, include, exclude)

    def _get_named_destinations(
        self,
        *,
        tree: DictionaryObject | None = None,
        retval: dict[str, Destination] | None = None,
        visited: set[int] | None = None,
        depth: int = 0,
        traversal_state: _TraversalState | None = None,
    ) -> dict[str, Destination]:
        """Override from PdfDocCommon. In the reader we can assume this is
        static, but not in the writer.
        """
        return _get_named_destinations(
            self,
            tree,
            retval,
            visited,
            depth,
            traversal_state,
        )
