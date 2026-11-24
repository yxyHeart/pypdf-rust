"""Container holding the information on an embedded file.

Rust-backed port of ``pypdf/generic/_files.py`` (upstream pypdf 7.1.1).  The
``EmbeddedFile`` class, its property/classmethod/method surface, the class
attributes and the pure attribute-assignment ``__init__`` stay Python
definitions (the class object is part of the observable Python API); every
method body delegates to ``crates/pypdf-py/src/modules/filesimages_mod.rs``
through the ``pypdf._pypdf`` extension module.

The generator shape of ``_names``, ``_load_from_names`` and ``_load`` is kept
here with ``yield from`` (a structural detail, not business logic): upstream
returns generators and callers rely on lazy iteration.
"""

from __future__ import annotations

import bisect
from functools import cached_property
from typing import TYPE_CHECKING, cast

from pypdf._pypdf import (
    files_embeddedfile_alternative_name_get as _alternative_name_get,
    files_embeddedfile_alternative_name_set as _alternative_name_set,
    files_embeddedfile_associated_file_relationship_get as _associated_file_relationship_get,
    files_embeddedfile_associated_file_relationship_set as _associated_file_relationship_set,
    files_embeddedfile_checksum_get as _checksum_get,
    files_embeddedfile_checksum_set as _checksum_set,
    files_embeddedfile_content_get as _content_get,
    files_embeddedfile_content_set as _content_set,
    files_embeddedfile_create_new as _create_new_impl,
    files_embeddedfile_creation_date_get as _creation_date_get,
    files_embeddedfile_creation_date_set as _creation_date_set,
    files_embeddedfile_delete as _delete,
    files_embeddedfile_description_get as _description_get,
    files_embeddedfile_description_set as _description_set,
    files_embeddedfile_embedded_file as _embedded_file,
    files_embeddedfile_ensure_params as _ensure_params_impl,
    files_embeddedfile_get_insertion_index as _get_insertion_index_impl,
    files_embeddedfile_get_names_array as _get_names_array_impl,
    files_embeddedfile_load as _load_impl,
    files_embeddedfile_load_from_names as _load_from_names_impl,
    files_embeddedfile_modification_date_get as _modification_date_get,
    files_embeddedfile_modification_date_set as _modification_date_set,
    files_embeddedfile_names as _names_impl,
    files_embeddedfile_params as _params,
    files_embeddedfile_report_malformed as _report_malformed_impl,
    files_embeddedfile_repr as _repr,
    files_embeddedfile_size_get as _size_get,
    files_embeddedfile_size_set as _size_set,
    files_embeddedfile_subtype_get as _subtype_get,
    files_embeddedfile_subtype_set as _subtype_set,
)

from pypdf._utils import format_iso8824_date, logger_warning, parse_iso8824_date
from pypdf.constants import CatalogAttributes, FileSpecificationDictionaryEntries, PageAttributes
from pypdf.errors import PdfReadError, PyPdfError
from pypdf.generic import (
    ArrayObject,
    ByteStringObject,
    DecodedStreamObject,
    DictionaryObject,
    NameObject,
    NullObject,
    NumberObject,
    StreamObject,
    TextStringObject,
    is_null_or_none,
)

if TYPE_CHECKING:
    import datetime
    from collections.abc import Iterator

    from pypdf._writer import PdfWriter


class EmbeddedFile:
    """
    Container holding the information on an embedded file.

    Attributes are evaluated lazily if possible.

    Further information on embedded files can be found in section 7.11 of the PDF 2.0 specification.
    """
    def __init__(self, name: str, pdf_object: DictionaryObject, parent: ArrayObject | None = None) -> None:
        """
        Args:
            name: The (primary) name as provided in the name tree.
            pdf_object: The corresponding PDF object to allow retrieving further data.
            parent: The parent list.
        """
        self._name = name
        self.pdf_object = pdf_object
        self._parent = parent

    @property
    def name(self) -> str:
        """
        The (primary) name of the embedded file as provided in the name tree.

        .. warning::

            This value can contain arbitrary characters. Please make sure to sanitize it before
            using it to write the file content to the disk for example.
        """
        return self._name

    @classmethod
    def _create_new(cls, writer: PdfWriter, name: str, content: str | bytes) -> EmbeddedFile:
        """
        Create a new embedded file and add it to the PdfWriter.

        Args:
            writer: The PdfWriter instance to add the embedded file to.
            name: The filename to display.
            content: The data in the file.

        Returns:
            EmbeddedFile instance for the newly created embedded file.
        """
        return _create_new_impl(cls, writer, name, content)

    @classmethod
    def _get_names_array(cls, writer: PdfWriter) -> ArrayObject:
        """Get the names array for embedded files, possibly creating and flattening it."""
        return _get_names_array_impl(cls, writer)

    @classmethod
    def _get_insertion_index(cls, names_array: ArrayObject, name: str) -> int:
        return _get_insertion_index_impl(cls, names_array, name)

    @property
    def alternative_name(self) -> str | None:
        """
        Retrieve the alternative name (as per the file specification dictionary).

        .. warning::

            This value can contain arbitrary characters. Please make sure to sanitize it before
            using it to write the file content to the disk for example.
        """
        return _alternative_name_get(self)

    @alternative_name.setter
    def alternative_name(self, value: TextStringObject | None) -> None:
        """Set the alternative name (as per the file specification dictionary)."""
        _alternative_name_set(self, value)

    @property
    def description(self) -> str | None:
        """Retrieve the description."""
        return _description_get(self)

    @description.setter
    def description(self, value: TextStringObject | None) -> None:
        """Set the description."""
        _description_set(self, value)

    @property
    def associated_file_relationship(self) -> str:
        """Retrieve the relationship of the referring document to this embedded file."""
        return _associated_file_relationship_get(self)

    @associated_file_relationship.setter
    def associated_file_relationship(self, value: NameObject) -> None:
        """Set the relationship of the referring document to this embedded file."""
        _associated_file_relationship_set(self, value)

    @property
    def _embedded_file(self) -> StreamObject:
        """Retrieve the actual embedded file stream."""
        return _embedded_file(self)

    @property
    def _params(self) -> DictionaryObject:
        """Retrieve the file-specific parameters."""
        return _params(self)

    @cached_property
    def _ensure_params(self) -> DictionaryObject:
        """Ensure the /Params dictionary exists and return it."""
        return _ensure_params_impl(self)

    @property
    def subtype(self) -> str | None:
        """Retrieve the subtype. This is a MIME media type, prefixed by a slash."""
        return _subtype_get(self)

    @subtype.setter
    def subtype(self, value: NameObject | None) -> None:
        """Set the subtype. This should be a MIME media type, prefixed by a slash."""
        _subtype_set(self, value)

    @property
    def content(self) -> bytes:
        """Retrieve the actual file content."""
        return _content_get(self)

    @content.setter
    def content(self, value: str | bytes) -> None:
        """Set the file content."""
        _content_set(self, value)

    @property
    def size(self) -> int | None:
        """Retrieve the size of the uncompressed file in bytes."""
        return _size_get(self)

    @size.setter
    def size(self, value: NumberObject | None) -> None:
        """Set the size of the uncompressed file in bytes."""
        _size_set(self, value)

    @property
    def creation_date(self) -> datetime.datetime | None:
        """Retrieve the file creation datetime."""
        return _creation_date_get(self)

    @creation_date.setter
    def creation_date(self, value: datetime.datetime | None) -> None:
        """Set the file creation datetime."""
        _creation_date_set(self, value)

    @property
    def modification_date(self) -> datetime.datetime | None:
        """Retrieve the datetime of the last file modification."""
        return _modification_date_get(self)

    @modification_date.setter
    def modification_date(self, value: datetime.datetime | None) -> None:
        """Set the datetime of the last file modification."""
        _modification_date_set(self, value)

    @property
    def checksum(self) -> bytes | None:
        """Retrieve the MD5 checksum of the (uncompressed) file."""
        return _checksum_get(self)

    @checksum.setter
    def checksum(self, value: ByteStringObject | None) -> None:
        """Set the MD5 checksum of the (uncompressed) file."""
        _checksum_set(self, value)

    @property
    def _names(self) -> Iterator[str]:
        yield from _names_impl(self)

    def delete(self) -> None:
        """Delete the file from the document."""
        _delete(self)

    def __repr__(self) -> str:
        return _repr(self)

    @staticmethod
    def _report_malformed(message: str, strict: bool) -> None:
        """Raise an exception in strict mode, warn otherwise."""
        _report_malformed_impl(message, strict)

    @classmethod
    def _load_from_names(cls, names: ArrayObject, strict: bool = False) -> Iterator[EmbeddedFile]:
        """
        Convert the given name tree into class instances.

        Args:
            names: The name tree to load the data from.
            strict: Whether to raise an exception on a file specification that is not a dictionary
                instead of issuing a warning and skipping it.

        Returns:
            Iterable of class instances for the files found.
        """
        yield from _load_from_names_impl(cls, names, strict)

    @classmethod
    def _load(cls, catalog: DictionaryObject, strict: bool = False) -> Iterator[EmbeddedFile]:
        """
        Load the embedded files for the given document catalog.

        This method and its signature are considered internal API and thus not exposed publicly for now.

        Args:
            catalog: The document catalog to load from.
            strict: Whether to raise an exception on malformed name trees
                instead of issuing a warning and skipping them.

        Returns:
            Iterable of class instances for the files found.
        """
        yield from _load_impl(cls, catalog, strict)
