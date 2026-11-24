"""PDF writer.

Rust port of ``pypdf/_writer.py`` (upstream pypdf 7.1.1).

The public class surface (``PdfWriter``, ``ObjectDeletionFlag``,
``_NamedDestinationIndex``) is defined here exactly as upstream so that
``isinstance``/subclassing/MRO behave identically; every method body delegates
to ``crates/pypdf-py/src/modules/writer_mod.rs``.
"""

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

import decimal
import enum
import re
import secrets
import struct
import sys
from collections.abc import Callable, Iterable, Mapping, Sequence
from io import BytesIO, FileIO, IOBase
from itertools import compress, pairwise
from pathlib import Path
from re import Pattern
from types import TracebackType
from typing import (
    IO,
    Any,
    Literal,
    NoReturn,
    cast,
)

if sys.version_info >= (3, 11):
    from typing import Self
else:
    from typing_extensions import Self

from ._doc_common import DocumentInformation, PdfDocCommon
from ._encryption import EncryptAlgorithm, Encryption
from ._page import PageObject, Transformation
from ._page_labels import nums_clear_range, nums_insert, nums_next
from ._reader import PdfReader
from ._utils import (
    StrByteType,
    StreamType,
    _get_max_pdf_version_header,
    deprecate_positional_arguments,
    deprecate_with_replacement,
    deprecation_with_replacement,
    logger_warning,
)
from .actions import Action
from .constants import (
    AnnotationDictionaryAttributes,
    CatalogAttributes,
    Core,
    FieldDictionaryAttributes,
    GoToActionArguments,
    ImageType,
    InteractiveFormDictEntries,
    OutlineFontFlag,
    PageAttributes,
    PageLabelStyle,
    PagesAttributes,
    TrailerKeys,
    UserAccessPermissions,
)
from .errors import LimitReachedError, PdfReadError, PyPdfError
from .generic import (
    PAGE_FIT,
    ArrayObject,
    BooleanObject,
    ByteStringObject,
    ContentStream,
    Destination,
    DictionaryObject,
    EmbeddedFile,
    Fit,
    FloatObject,
    IndirectObject,
    NamedReferenceLink,
    NameObject,
    NullObject,
    NumberObject,
    PdfObject,
    RectangleObject,
    ReferenceLink,
    StreamObject,
    TextStringObject,
    TreeObject,
    ViewerPreferences,
    create_string_object,
    extract_links,
    hex_to_rgb,
    is_null_or_none,
)
from .generic._appearance_stream import TextStreamAppearance
from .pagerange import PageRange, PageRangeSpec
from .types import (
    AnnotationSubtype,
    BorderArrayType,
    LayoutType,
    OutlineItemType,
    OutlineType,
    PagemodeType,
)
from .xmp import XmpInformation


from pypdf import _pypdf as _writermod
ALL_DOCUMENT_PERMISSIONS = UserAccessPermissions.all()
class ObjectDeletionFlag(enum.IntFlag):
    NONE = 0
    TEXT = enum.auto()
    LINKS = enum.auto()
    ATTACHMENTS = enum.auto()
    OBJECTS_3D = enum.auto()
    ALL_ANNOTATIONS = enum.auto()
    XOBJECT_IMAGES = enum.auto()
    INLINE_IMAGES = enum.auto()
    DRAWING_IMAGES = enum.auto()
    IMAGES = XOBJECT_IMAGES | INLINE_IMAGES | DRAWING_IMAGES
_NAMED_DESTINATION_INDEX_MINIMUM = 32
class _NamedDestinationIndex():
    '\n    Look up and insert named destinations in a ``/Names`` array without scanning it.\n\n    ``PdfWriter.merge`` adds the named destinations of a document one by one, and both\n    looking each one up in the growing array and inserting it there take quadratic time.\n    The string entries are kept in a set instead, as only strings compare equal to a title.\n    If the keys are sorted strings, the new destinations are collected and sorted into the\n    array once by ``flush``: inserting each one before the first greater key gives the same\n    order as this stable sort. Other arrays are scanned as before.\n    '
    def __init__(self, *, is_enabled: bool):
        return _writermod.writermod_nameddestinationindex_init(self, is_enabled)
    def _update(self, names: list[Any]):
        return _writermod.writermod_nameddestinationindex_private_update(self, names)
    def contains(self, container: Any, title: Any):
        return _writermod.writermod_nameddestinationindex_contains(self, container, title)
    def insert(self, names: list[Any], title: Any, destination: Any):
        'Insert like ``PdfWriter.add_named_destination_array``; return False if the array has to be scanned.'
        return _writermod.writermod_nameddestinationindex_insert(self, names, title, destination)
    def record_insertion(self, names: list[Any], title: Any):
        'Update the index after ``add_named_destination_array`` inserted ``title`` into ``names``.'
        return _writermod.writermod_nameddestinationindex_record_insertion(self, names, title)
    def flush(self):
        return _writermod.writermod_nameddestinationindex_flush(self)
class PdfWriter(PdfDocCommon):
    '\n    Write a PDF file out, given pages produced by another class or through\n    cloning a PDF file during initialization.\n\n    Typically data is added from a :class:`PdfReader<pypdf.PdfReader>`.\n\n    Args:\n        clone_from: identical to fileobj (for compatibility)\n\n        incremental: If true, loads the document and set the PdfWriter in incremental mode.\n\n            When writing incrementally, the original document is written first and new/modified\n            content is appended. To be used for signed document/forms to keep signature valid.\n            Encryption is not supported in this mode: calling ``encrypt`` or writing changes to an\n            encrypted document raises a ``NotImplementedError``.\n\n        full: If true, loads all the objects (always full if incremental = True).\n            This parameter may allow loading large PDFs.\n\n        strict: If true, pypdf will raise an exception if a PDF does not follow the specification.\n            If false, pypdf will try to be forgiving and do something reasonable, but it will log\n            a warning message. It is a best-effort approach.\n\n        keep_initial_header: If true, the PDF header of the cloned document is kept\n            when using ``clone_from`` in non-incremental mode.\n\n    '
    # Class-body declarations of the data members listed by
    # ``pypdf._protocols.PdfWriterProtocol`` (upstream declares them here as
    # well so that ``typing.get_type_hints`` / runtime protocol checks see them).
    incremental: bool
    """
    Returns if the PdfWriter object has been started in incremental mode.
    """

    _objects: list[PdfObject | None]
    """
    The indirect objects in the PDF.
    For the incremental case, it will be filled with None
    in clone_reader_document_root.
    """

    _id_translated: dict[int, dict[int | Literal["PreventGC"], Any]]
    """
    List of already translated IDs.
    dict[id(pdf)][(idnum, generation)]
    """

    _reader: PdfReader | None
    """
    The document being appended to, in incremental mode only.
    """

    def __init__(
        self,
        fileobj: PdfReader | StrByteType | Path | None = "",
        clone_from: PdfReader | StrByteType | Path | None = None,
        incremental: bool = False,
        full: bool = False,
        strict: bool = False,
        *,
        keep_initial_header: bool = False,
        incremental_clone_object_count_limit: int | None = 500_000,
        incremental_clone_object_id_limit: int | None = 1_000_000,
    ):
        return _writermod.writermod_pdfwriter_init(self, fileobj, clone_from, incremental, full, strict, keep_initial_header, incremental_clone_object_count_limit, incremental_clone_object_id_limit)
    @property
    def is_encrypted(self):
        '\n        Read-only boolean property showing whether this PDF file is encrypted.\n\n        Note that this property, if true, will remain true even after the\n        :meth:`decrypt()<pypdf.PdfReader.decrypt>` method is called.\n        '
        return _writermod.writermod_pdfwriter_get_is_encrypted(self)
    @property
    def root_object(self):
        '\n        Provide direct access to PDF Structure.\n\n        Note:\n            Recommended only for read access.\n\n        '
        return _writermod.writermod_pdfwriter_get_root_object(self)
    @property
    def _info(self):
        '\n        Provide access to "/Info". Standardized with PdfReader.\n\n        Returns:\n            /Info Dictionary; None if the entry does not exist\n\n        '
        return _writermod.writermod_pdfwriter_get_private_info(self)
    @_info.setter
    def _info(self, value: IndirectObject | DictionaryObject | None):
        return _writermod.writermod_pdfwriter_property_set_private_info(self, value)
    @property
    def xmp_metadata(self):
        'XMP (Extensible Metadata Platform) data.'
        return _writermod.writermod_pdfwriter_get_xmp_metadata(self)
    @xmp_metadata.setter
    def xmp_metadata(self, value: XmpInformation | bytes | None):
        'XMP (Extensible Metadata Platform) data.'
        return _writermod.writermod_pdfwriter_property_set_xmp_metadata(self, value)
    def __enter__(self):
        "Store how writer is initialized by 'with'."
        return _writermod.writermod_pdfwriter_enter(self)
    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        traceback: TracebackType | None,
    ):
        'Write data to the fileobj.'
        return _writermod.writermod_pdfwriter_exit(self, exc_type, exc, traceback)
    @property
    def pdf_header(self):
        "\n        Read/Write property of the PDF header that is written.\n\n        This should be something like ``'%PDF-1.5'``. It is recommended to set\n        the lowest version that supports all features which are used within the\n        PDF file.\n\n        Note: `pdf_header` returns a string but accepts bytes or str for writing\n        "
        return _writermod.writermod_pdfwriter_get_pdf_header(self)
    @pdf_header.setter
    def pdf_header(self, new_header: str | bytes):
        return _writermod.writermod_pdfwriter_property_set_pdf_header(self, new_header)
    def _add_object(self, obj: PdfObject):
        return _writermod.writermod_pdfwriter_private_add_object(self, obj)
    def get_object(
        self,
        indirect_reference: int | IndirectObject,
    ):
        return _writermod.writermod_pdfwriter_get_object(self, indirect_reference)
    def _replace_object(
        self,
        indirect_reference: int | IndirectObject,
        obj: PdfObject,
    ):
        return _writermod.writermod_pdfwriter_private_replace_object(self, indirect_reference, obj)
    def _add_page(
        self,
        page: PageObject,
        index: int,
        excluded_keys: Iterable[str] = (),
    ):
        return _writermod.writermod_pdfwriter_private_add_page(self, page, index, excluded_keys)
    def set_need_appearances_writer(self, state: bool = True):
        '\n        Sets the "NeedAppearances" flag in the PDF writer.\n\n        The "NeedAppearances" flag indicates whether the appearance dictionary\n        for form fields should be automatically generated by the PDF viewer or\n        if the embedded appearance should be used.\n\n        Args:\n            state: The actual value of the NeedAppearances flag.\n\n        Returns:\n            None\n\n        '
        return _writermod.writermod_pdfwriter_set_need_appearances_writer(self, state)
    def create_viewer_preferences(self):
        return _writermod.writermod_pdfwriter_create_viewer_preferences(self)
    def add_page(
        self,
        page: PageObject,
        excluded_keys: Iterable[str] = (),
    ):
        '\n        Add a page to this PDF file.\n\n        Recommended for advanced usage including the adequate excluded_keys.\n\n        The page is usually acquired from a :class:`PdfReader<pypdf.PdfReader>`\n        instance.\n\n        Args:\n            page: The page to add to the document. Should be\n                an instance of :class:`PageObject<pypdf._page.PageObject>`\n            excluded_keys:\n\n        Returns:\n            The added PageObject.\n\n        '
        return _writermod.writermod_pdfwriter_add_page(self, page, excluded_keys)
    def insert_page(
        self,
        page: PageObject,
        index: int = 0,
        excluded_keys: Iterable[str] = (),
    ):
        '\n        Insert a page in this PDF file. The page is usually acquired from a\n        :class:`PdfReader<pypdf.PdfReader>` instance.\n\n        Args:\n            page: The page to add to the document.\n            index: Position at which the page will be inserted.\n            excluded_keys:\n\n        Returns:\n            The added PageObject.\n\n        '
        return _writermod.writermod_pdfwriter_insert_page(self, page, index, excluded_keys)
    def _get_page_number_by_indirect(
        self, indirect_reference: int | NullObject | IndirectObject | None
    ):
        '\n        Generate _page_id2num.\n\n        Args:\n            indirect_reference:\n\n        Returns:\n            The page number or None\n\n        '
        return _writermod.writermod_pdfwriter_private_get_page_number_by_indirect(self, indirect_reference)
    def add_blank_page(
        self, width: float | None = None, height: float | None = None
    ):
        '\n        Append a blank page to this PDF file and return it.\n\n        If no page size is specified, use the size of the last page.\n\n        Args:\n            width: The width of the new page expressed in default user\n                space units.\n            height: The height of the new page expressed in default\n                user space units.\n\n        Returns:\n            The newly appended page.\n\n        Raises:\n            PageSizeNotDefinedError: if width and height are not defined\n                and previous page does not exist.\n\n        '
        return _writermod.writermod_pdfwriter_add_blank_page(self, width, height)
    def insert_blank_page(
        self,
        width: float | decimal.Decimal | None = None,
        height: float | decimal.Decimal | None = None,
        index: int = 0,
    ):
        '\n        Insert a blank page to this PDF file and return it.\n\n        If no page size is specified for a dimension, use the size of the last page.\n\n        Args:\n            width: The width of the new page in default user space units.\n            height: The height of the new page in default user space units.\n            index: Position to add the page.\n\n        Returns:\n            The newly inserted page.\n\n        Raises:\n            PageSizeNotDefinedError: if width and height are not defined\n                and previous page does not exist.\n            IndexError: Index is outside of [-self.get_num_pages(), self.get_num_pages()]\n        '
        return _writermod.writermod_pdfwriter_insert_blank_page(self, width, height, index)
    @property
    def open_destination(
        self,
    ):
        return _writermod.writermod_pdfwriter_get_open_destination(self)
    @open_destination.setter
    def open_destination(self, dest: str | Destination | PageObject | None):
        return _writermod.writermod_pdfwriter_property_set_open_destination(self, dest)
    def add_js(self, javascript: str):
        '\n        Add JavaScript which will launch upon opening this PDF.\n\n        Args:\n            javascript: Your JavaScript.\n        '
        return _writermod.writermod_pdfwriter_add_js(self, javascript)
    def add_open_action(self, action: Action):
        '\n        Add an action to the document-level JavaScript name tree.\n\n        Args:\n            action: The action to add.\n\n        Example:\n            This will launch the print window when the PDF is opened.\n\n            >>> from pypdf import PdfWriter\n            >>> from pypdf.actions import JavaScript\n            >>> output = PdfWriter()\n            >>> output.add_open_action(JavaScript("this.print({bUI:true,bSilent:false,bShrinkToFit:true});"))\n        '
        return _writermod.writermod_pdfwriter_add_open_action(self, action)
    def add_attachment(self, filename: str, data: str | bytes):
        '\n        Embed a file inside the PDF.\n\n        Reference:\n        https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf\n        Section 7.11.3\n\n        Args:\n            filename: The filename to display.\n            data: The data in the file.\n\n        Returns:\n            EmbeddedFile instance for the newly created embedded file.\n\n        '
        return _writermod.writermod_pdfwriter_add_attachment(self, filename, data)
    def append_pages_from_reader(
        self,
        reader: PdfReader,
        after_page_append: Callable[[PageObject], None] | None = None,
    ):
        "\n        Copy pages from reader to writer. Includes an optional callback\n        parameter which is invoked after pages are appended to the writer.\n\n        ``append`` should be preferred.\n\n        Args:\n            reader: a PdfReader object from which to copy page\n                annotations to this writer object. The writer's annots\n                will then be updated.\n            after_page_append:\n                Callback function that is invoked after each page is appended to\n                the writer. Signature includes a reference to the appended page\n                (delegates to append_pages_from_reader). The single parameter of\n                the callback is a reference to the page just appended to the\n                document.\n\n        "
        return _writermod.writermod_pdfwriter_append_pages_from_reader(self, reader, after_page_append)
    def _merge_content_stream_to_page(
        self,
        page: PageObject,
        new_content_data: bytes,
    ):
        '\n        Combines existing content stream(s) with new content (as bytes).\n\n        Args:\n            page: The page to which the new content data will be added.\n            new_content_data: A binary-encoded new content stream, for\n                instance the commands to draw an XObject.\n        '
        return _writermod.writermod_pdfwriter_private_merge_content_stream_to_page(self, page, new_content_data)
    def _add_apstream_object(
            self,
            page: PageObject,
            appearance_stream_obj: StreamObject,
            object_name: str,
            x_offset: float,
            y_offset: float,
        ):
        '\n        Adds an appearance stream to the page content in the form of\n        an XObject.\n\n        Args:\n            page: The page to which to add the appearance stream.\n            appearance_stream_obj: The appearance stream.\n            object_name: The name of the appearance stream.\n            x_offset: The horizontal offset for the appearance stream.\n            y_offset: The vertical offset for the appearance stream.\n        '
        return _writermod.writermod_pdfwriter_private_add_apstream_object(self, page, appearance_stream_obj, object_name, x_offset, y_offset)
    FFBITS_NUL = FieldDictionaryAttributes.FfBits(0)
    def update_page_form_field_values(
        self,
        page: PageObject | list[PageObject] | None,
        fields: Mapping[str, str | list[str] | tuple[str, str, float]],
        flags: FieldDictionaryAttributes.FfBits = FFBITS_NUL,
        auto_regenerate: bool | None = True,
        flatten: bool = False,
    ):
        "\n        Update the form field values for a given page from a fields dictionary.\n\n        Copy field texts and values from fields to page.\n        If the field links to a parent object, add the information to the parent.\n\n        Args:\n            page: `PageObject` - references **PDF writer's page** where the\n                annotations and field data will be updated.\n                `List[Pageobject]` - provides list of pages to be processed.\n                `None` - all pages.\n            fields: a Python dictionary of:\n\n                * field names (/T) as keys and text values (/V) as value\n                * field names (/T) as keys and list of text values (/V) for multiple choice list\n                * field names (/T) as keys and tuple of:\n                    * text values (/V)\n                    * font id (e.g. /F1, the font id must exist)\n                    * font size (0 for autosize)\n\n            flags: A set of flags from :class:`~pypdf.constants.FieldDictionaryAttributes.FfBits`.\n\n            auto_regenerate: Set/unset the need_appearances flag;\n                the flag is unchanged if auto_regenerate is None.\n\n            flatten: Whether or not to flatten the annotation. If True, this adds the annotation's\n                appearance stream to the page contents. Note that this option does not remove the\n                annotation itself.\n\n        "
        return _writermod.writermod_pdfwriter_update_page_form_field_values(self, page, fields, flags, auto_regenerate, flatten)
    def reattach_fields(
        self, page: PageObject | None = None
    ):
        '\n        Parse annotations within the page looking for orphan fields and\n        reattach then into the Fields Structure.\n\n        Args:\n            page: page to analyze.\n                  If none is provided, all pages will be analyzed.\n\n        Returns:\n            list of reattached fields.\n\n        '
        return _writermod.writermod_pdfwriter_reattach_fields(self, page)
    def _collect_incremental_clone_object_ids(self, reader: PdfReader):
        return _writermod.writermod_pdfwriter_private_collect_incremental_clone_object_ids(self, reader)
    def clone_reader_document_root(self, reader: PdfReader):
        '\n        Copy the reader document root to the writer and all sub-elements,\n        including pages, threads, outlines,... For partial insertion, ``append``\n        should be considered.\n\n        Args:\n            reader: PdfReader from which the document root should be copied.\n\n        '
        return _writermod.writermod_pdfwriter_clone_reader_document_root(self, reader)
    def clone_document_from_reader(
        self,
        reader: PdfReader,
        after_page_append: Callable[[PageObject], None] | None = None,
    ):
        "\n        Create a copy (clone) of a document from a PDF file reader cloning\n        section '/Root' and '/Info' and '/ID' of the pdf.\n\n        Args:\n            reader: PDF file reader instance from which the clone\n                should be created.\n            after_page_append:\n                Callback function that is invoked after each page is appended to\n                the writer. Signature includes a reference to the appended page\n                (delegates to append_pages_from_reader). The single parameter of\n                the callback is a reference to the page just appended to the\n                document.\n\n        "
        return _writermod.writermod_pdfwriter_clone_document_from_reader(self, reader, after_page_append)
    def generate_file_identifiers(self):
        '\n        Generate an identifier for the PDF that will be written.\n\n        The only point of this is ensuring uniqueness. Reproducibility is not\n        required.\n        When a file is first written, both identifiers shall be set to the same value.\n        If both identifiers match when a file reference is resolved, it is very\n        likely that the correct and unchanged file has been found. If only the first\n        identifier matches, a different version of the correct file has been found.\n        see §14.4 "File Identifiers".\n        '
        return _writermod.writermod_pdfwriter_generate_file_identifiers(self)
    def encrypt(
        self,
        user_password: str,
        owner_password: str | None = None,
        use_128bit: bool = True,
        permissions_flag: UserAccessPermissions = ALL_DOCUMENT_PERMISSIONS,
        *,
        algorithm: str | None = None,
    ):
        '\n        Encrypt this PDF file with the PDF Standard encryption handler.\n\n        Args:\n            user_password: The password which allows for opening\n                and reading the PDF file with the restrictions provided.\n            owner_password: The password which allows for\n                opening the PDF files without any restrictions. By default,\n                the owner password is the same as the user password.\n            use_128bit: flag as to whether to use 128bit\n                encryption. When false, 40bit encryption will be used.\n                By default, this flag is on.\n            permissions_flag: permissions as described in\n                Table 3.20 of the PDF 1.7 specification. A bit value of 1 means\n                the permission is granted.\n                Hence an integer value of -1 will set all flags.\n                Bit position 3 is for printing, 4 is for modifying content,\n                5 and 6 control annotations, 9 for form fields,\n                10 for extraction of text and graphics.\n            algorithm: encrypt algorithm. Values may be one of "RC4-40", "RC4-128",\n                "AES-128", "AES-256-R5", "AES-256". If it is valid,\n                `use_128bit` will be ignored.\n\n        '
        return _writermod.writermod_pdfwriter_encrypt(self, user_password, owner_password, use_128bit, permissions_flag, algorithm)
    def _resolve_links(self):
        'Patch up links that were added to the document earlier, to\n        make sure they still point to the same pages.\n        '
        return _writermod.writermod_pdfwriter_private_resolve_links(self)
    def write_stream(self, stream: StreamType):
        return _writermod.writermod_pdfwriter_write_stream(self, stream)
    def write(self, stream: Path | StrByteType):
        '\n        Write the collection of pages added to this object out as a PDF file.\n\n        Args:\n            stream: An object to write the file to. The object can support\n                the write method and the tell method, similar to a file object, or\n                be a file path, just like the fileobj, just named it stream to keep\n                existing workflow.\n\n        Returns:\n            A tuple (bool, IO).\n\n        '
        return _writermod.writermod_pdfwriter_write(self, stream)
    def list_objects_in_increment(self):
        '\n        For analysis or debugging.\n        Provides the list of new or modified objects that will be written\n        in the increment.\n        Deleted objects will not be freed but will become orphans.\n\n        Returns:\n            List of new or modified IndirectObjects\n\n        '
        return _writermod.writermod_pdfwriter_list_objects_in_increment(self)
    def _write_increment(self, stream: StreamType):
        return _writermod.writermod_pdfwriter_private_write_increment(self, stream)
    def _write_pdf_structure(self, stream: StreamType):
        return _writermod.writermod_pdfwriter_private_write_pdf_structure(self, stream)
    def _write_xref_table(
        self, stream: StreamType, object_positions: list[int], free_objects: list[int]
    ):
        return _writermod.writermod_pdfwriter_private_write_xref_table(self, stream, object_positions, free_objects)
    def _write_trailer(self, stream: StreamType, xref_location: int):
        '\n        Write the PDF trailer to the stream.\n\n        To quote the PDF specification:\n            [The] trailer [gives] the location of the cross-reference table and\n            of certain special objects within the body of the file.\n        '
        return _writermod.writermod_pdfwriter_private_write_trailer(self, stream, xref_location)
    @property
    def metadata(self):
        "\n        Retrieve/set the PDF file's document information dictionary, if it exists.\n\n        Args:\n            value: dict with the entries to be set. if None : remove the /Info entry from the pdf.\n\n        Note that some PDF files use (XMP) metadata streams instead of document\n        information dictionaries, and these metadata streams will not be\n        accessed by this function, but by :meth:`~xmp_metadata`.\n\n        "
        return _writermod.writermod_pdfwriter_get_metadata(self)
    @metadata.setter
    def metadata(
        self,
        value: DocumentInformation | DictionaryObject | dict[Any, Any] | None,
    ):
        return _writermod.writermod_pdfwriter_property_set_metadata(self, value)
    def add_metadata(self, infos: dict[str, Any]):
        '\n        Add custom metadata to the output.\n\n        Args:\n            infos: a Python dictionary where each key is a field\n                and each value is your new metadata.\n\n        '
        return _writermod.writermod_pdfwriter_add_metadata(self, infos)
    _UNSET = object()
    def compress_identical_objects(
        self,
        remove_identicals: Any = _UNSET,
        remove_orphans: Any = _UNSET,
        *,
        remove_duplicates: bool = True,
        remove_unreferenced: bool = True,
    ):
        '\n        Parse the PDF file and merge objects that have the same hash.\n        This will make objects common to multiple pages.\n        Recommended to be used just before writing output.\n\n        Args:\n            remove_identicals: Deprecated.\n            remove_orphans: Deprecated.\n            remove_duplicates: Remove duplicate objects.\n            remove_unreferenced: Remove unreferenced objects.\n\n        '
        return _writermod.writermod_pdfwriter_compress_identical_objects(self, remove_identicals, remove_orphans, remove_duplicates, remove_unreferenced)
    def get_reference(self, obj: PdfObject):
        return _writermod.writermod_pdfwriter_get_reference(self, obj)
    def get_outline_root(self):
        return _writermod.writermod_pdfwriter_get_outline_root(self)
    def get_threads_root(self):
        '\n        The list of threads.\n\n        See §12.4.3 of the PDF 1.7 or PDF 2.0 specification.\n\n        Returns:\n            An array (possibly empty) of Dictionaries with an ``/F`` key,\n            and optionally information about the thread in ``/I`` or ``/Metadata`` keys.\n\n        '
        return _writermod.writermod_pdfwriter_get_threads_root(self)
    @property
    def threads(self):
        '\n        Read-only property for the list of threads.\n\n        See §12.4.3 of the PDF 1.7 or PDF 2.0 specification.\n\n        Each element is a dictionary with an ``/F`` key, and optionally\n        information about the thread in ``/I`` or ``/Metadata`` keys.\n        '
        return _writermod.writermod_pdfwriter_get_threads(self)
    def add_outline_item_destination(
        self,
        page_destination: IndirectObject | PageObject | TreeObject,
        parent: TreeObject | IndirectObject | None = None,
        before: TreeObject | IndirectObject | None = None,
        is_open: bool = True,
    ):
        return _writermod.writermod_pdfwriter_add_outline_item_destination(self, page_destination, parent, before, is_open)
    def add_outline_item_dict(
        self,
        outline_item: OutlineItemType,
        parent: TreeObject | IndirectObject | None = None,
        before: TreeObject | IndirectObject | None = None,
        is_open: bool = True,
    ):
        return _writermod.writermod_pdfwriter_add_outline_item_dict(self, outline_item, parent, before, is_open)
    @deprecate_positional_arguments("8.0.0")
    def add_outline_item(
        self,
        *,
        title: str,
        page_number: PageObject | IndirectObject | int | None,
        parent: TreeObject | IndirectObject | None = None,
        before: TreeObject | IndirectObject | None = None,
        color: tuple[float, float, float] | str | None = None,
        bold: bool = False,
        italic: bool = False,
        fit: Fit = PAGE_FIT,
        is_open: bool = True,
    ):
        '\n        Add an outline item (commonly referred to as a "Bookmark") to the PDF file.\n\n        Args:\n            title: Title to use for this outline item.\n            page_number: Page number this outline item will point to.\n            parent: A reference to a parent outline item to create nested\n                outline items.\n            before: The outline item in front of which the new outline item\n                is inserted. If None, the new outline item is appended.\n            color: Color of the outline item\'s font as a red, green, blue tuple\n                from 0.0 to 1.0 or as a Hex String (#RRGGBB)\n            bold: Outline item font is bold\n            italic: Outline item font is italic\n            fit: The fit of the destination page.\n            is_open: Whether the children of the outline item are shown.\n\n        Returns:\n            The added outline item as an indirect object.\n\n        .. deprecated:: 7.1.0\n            Passing positional arguments is deprecated and will be removed\n            in pypdf 8.0.0. Use keyword arguments instead.\n\n        '
        return _writermod.writermod_pdfwriter_add_outline_item(self, title, page_number, parent, before, color, bold, italic, fit, is_open)
    def add_outline(self):
        return _writermod.writermod_pdfwriter_add_outline(self)
    def add_named_destination_array(
        self, title: TextStringObject, destination: IndirectObject | ArrayObject
    ):
        return _writermod.writermod_pdfwriter_add_named_destination_array(self, title, destination)
    def add_named_destination_object(
        self,
        page_destination: PdfObject,
    ):
        return _writermod.writermod_pdfwriter_add_named_destination_object(self, page_destination)
    def _get_page_reference(self, page_number: int):
        'Look up a page by number, reporting a bad index rather than an IndexError from the kids array.'
        return _writermod.writermod_pdfwriter_private_get_page_reference(self, page_number)
    def add_named_destination(
        self,
        title: str,
        page_number: int,
        fit: Fit | None = None,
    ):
        "\n        Add a named destination to a page in this PDF.\n\n        Args:\n            title: Name of the destination.\n            page_number: Index of the destination page, starting at zero.\n            fit: How the destination is displayed.\n                Defaults to fitting the page horizontally with its\n                top edge at the top of the window.\n\n        Returns:\n            A reference to the destination's GoTo action.\n\n        "
        return _writermod.writermod_pdfwriter_add_named_destination(self, title, page_number, fit)
    def remove_links(self):
        'Remove links and annotations from this output.'
        return _writermod.writermod_pdfwriter_remove_links(self)
    def remove_annotations(
        self, subtypes: AnnotationSubtype | Iterable[AnnotationSubtype] | None
    ):
        '\n        Remove annotations by annotation subtype.\n\n        Args:\n            subtypes: subtype or list of subtypes to be removed.\n                Examples are: "/Link", "/FileAttachment", "/Sound",\n                "/Movie", "/Screen", ...\n                If you want to remove all annotations, use subtypes=None.\n\n        '
        return _writermod.writermod_pdfwriter_remove_annotations(self, subtypes)
    def _remove_annots_from_page(
        self,
        page: IndirectObject | PageObject | DictionaryObject,
        subtypes: Iterable[str] | None,
    ):
        return _writermod.writermod_pdfwriter_private_remove_annots_from_page(self, page, subtypes)
    def remove_objects_from_page(
        self,
        page: PageObject | DictionaryObject,
        to_delete: ObjectDeletionFlag | Iterable[ObjectDeletionFlag],
        text_filters: dict[str, Any] | None = None
    ):
        '\n        Remove objects specified by ``to_delete`` from the given page.\n\n        Args:\n            page: Page object to clean up.\n            to_delete: Objects to be deleted; can be a ``ObjectDeletionFlag``\n                or a list of ObjectDeletionFlag\n            text_filters: Properties of text to be deleted, if applicable. Optional.\n                This is a Python dictionary with the following properties:\n\n                * font_ids: List of font resource IDs (such as /F1 or /T1_0) to be deleted.\n\n        '
        return _writermod.writermod_pdfwriter_remove_objects_from_page(self, page, to_delete, text_filters)
    def _remove_objects_from_page__clean(
            self,
            content: ContentStream,
            images: list[str],
            forms: list[str],
            jump_operators: list[bytes],
            to_delete: ObjectDeletionFlag,
            text_filters: dict[str, Any] | None = None,
    ):
        return _writermod.writermod_pdfwriter_private_remove_objects_from_page_clean(self, content, images, forms, jump_operators, to_delete, text_filters)
    def _remove_objects_from_page__clean_forms(
            self,
            elt: DictionaryObject,
            stack: list[DictionaryObject],
            jump_operators: list[bytes],
            to_delete: ObjectDeletionFlag,
            text_filters: dict[str, Any] | None = None,
    ):
        return _writermod.writermod_pdfwriter_private_remove_objects_from_page_clean_forms(self, elt, stack, jump_operators, to_delete, text_filters)
    def remove_images(
        self,
        to_delete: ImageType = ImageType.ALL,
    ):
        '\n        Remove images from this output.\n\n        Args:\n            to_delete: The type of images to be deleted\n                (default = all images types)\n\n        .. deprecated:: 7.1.0\n            Passing a boolean as ``to_delete`` is deprecated and will be\n            removed in pypdf 8.0.0. Use ``ImageType.ALL`` instead.\n\n        '
        return _writermod.writermod_pdfwriter_remove_images(self, to_delete)
    def remove_text(self, font_names: list[str] | None = None):
        '\n        Remove text from the PDF.\n\n        Args:\n            font_names: List of font names to remove, such as "Helvetica-Bold".\n                Optional. If not specified, all text will be removed.\n        '
        return _writermod.writermod_pdfwriter_remove_text(self, font_names)
    def add_uri(
        self,
        page_number: int,
        uri: str,
        rect: RectangleObject,
        border: Sequence[Any] | None = None,
    ):
        '\n        Add an URI from a rectangular area to the specified page.\n\n        Args:\n            page_number: index of the page on which to place the URI action.\n            uri: URI of resource to link to.\n            rect: :class:`RectangleObject<pypdf.generic.RectangleObject>` or\n                array of four integers specifying the clickable rectangular area\n                ``[xLL, yLL, xUR, yUR]``, or string in the form\n                ``"[ xLL yLL xUR yUR ]"``.\n            border: if provided, an array describing border-drawing\n                properties. See the PDF spec for details. No border will be\n                drawn if this argument is omitted.\n\n        '
        return _writermod.writermod_pdfwriter_add_uri(self, page_number, uri, rect, border)
    _valid_layouts = (
        "/NoLayout",
        "/SinglePage",
        "/OneColumn",
        "/TwoColumnLeft",
        "/TwoColumnRight",
        "/TwoPageLeft",
        "/TwoPageRight",
    )
    def _get_page_layout(self):
        return _writermod.writermod_pdfwriter_private_get_page_layout(self)
    def _set_page_layout(self, layout: NameObject | LayoutType):
        '\n        Set the page layout.\n\n        Args:\n            layout: The page layout to be used.\n\n        .. list-table:: Valid ``layout`` arguments\n           :widths: 50 200\n\n           * - /NoLayout\n             - Layout explicitly not specified\n           * - /SinglePage\n             - Show one page at a time\n           * - /OneColumn\n             - Show one column at a time\n           * - /TwoColumnLeft\n             - Show pages in two columns, odd-numbered pages on the left\n           * - /TwoColumnRight\n             - Show pages in two columns, odd-numbered pages on the right\n           * - /TwoPageLeft\n             - Show two pages at a time, odd-numbered pages on the left\n           * - /TwoPageRight\n             - Show two pages at a time, odd-numbered pages on the right\n\n        '
        return _writermod.writermod_pdfwriter_private_set_page_layout(self, layout)
    def set_page_layout(self, layout: LayoutType):
        '\n        Set the page layout.\n\n        Args:\n            layout: The page layout to be used\n\n        .. list-table:: Valid ``layout`` arguments\n           :widths: 50 200\n\n           * - /NoLayout\n             - Layout explicitly not specified\n           * - /SinglePage\n             - Show one page at a time\n           * - /OneColumn\n             - Show one column at a time\n           * - /TwoColumnLeft\n             - Show pages in two columns, odd-numbered pages on the left\n           * - /TwoColumnRight\n             - Show pages in two columns, odd-numbered pages on the right\n           * - /TwoPageLeft\n             - Show two pages at a time, odd-numbered pages on the left\n           * - /TwoPageRight\n             - Show two pages at a time, odd-numbered pages on the right\n\n        '
        return _writermod.writermod_pdfwriter_set_page_layout(self, layout)
    @property
    def page_layout(self):
        '\n        Page layout property.\n\n        .. list-table:: Valid ``layout`` values\n           :widths: 50 200\n\n           * - /NoLayout\n             - Layout explicitly not specified\n           * - /SinglePage\n             - Show one page at a time\n           * - /OneColumn\n             - Show one column at a time\n           * - /TwoColumnLeft\n             - Show pages in two columns, odd-numbered pages on the left\n           * - /TwoColumnRight\n             - Show pages in two columns, odd-numbered pages on the right\n           * - /TwoPageLeft\n             - Show two pages at a time, odd-numbered pages on the left\n           * - /TwoPageRight\n             - Show two pages at a time, odd-numbered pages on the right\n        '
        return _writermod.writermod_pdfwriter_get_page_layout(self)
    @page_layout.setter
    def page_layout(self, layout: LayoutType):
        return _writermod.writermod_pdfwriter_property_set_page_layout(self, layout)
    _valid_modes = (
        "/UseNone",
        "/UseOutlines",
        "/UseThumbs",
        "/FullScreen",
        "/UseOC",
        "/UseAttachments",
    )
    def _get_page_mode(self):
        return _writermod.writermod_pdfwriter_private_get_page_mode(self)
    @property
    def page_mode(self):
        '\n        Page mode property.\n\n        .. list-table:: Valid ``mode`` values\n           :widths: 50 200\n\n           * - /UseNone\n             - Do not show outline or thumbnails panels\n           * - /UseOutlines\n             - Show outline (aka bookmarks) panel\n           * - /UseThumbs\n             - Show page thumbnails panel\n           * - /FullScreen\n             - Fullscreen view\n           * - /UseOC\n             - Show Optional Content Group (OCG) panel\n           * - /UseAttachments\n             - Show attachments panel\n        '
        return _writermod.writermod_pdfwriter_get_page_mode(self)
    @page_mode.setter
    def page_mode(self, mode: PagemodeType):
        return _writermod.writermod_pdfwriter_property_set_page_mode(self, mode)
    def add_annotation(
        self,
        page_number: int | PageObject,
        annotation: dict[str, Any],
    ):
        '\n        Add a single annotation to the page.\n        The added annotation must be a new annotation.\n        It cannot be recycled.\n\n        Args:\n            page_number: PageObject or page index.\n            annotation: Annotation to be added (created with annotation).\n\n        Returns:\n            The inserted object.\n            This can be used for popup creation, for example.\n\n        '
        return _writermod.writermod_pdfwriter_add_annotation(self, page_number, annotation)
    def clean_page(self, page: PageObject | IndirectObject):
        '\n        Perform some clean up in the page.\n        Currently: convert NameObject named destination to TextStringObject\n        (required for names/dests list)\n\n        Args:\n            page:\n\n        Returns:\n            The cleaned PageObject\n\n        '
        return _writermod.writermod_pdfwriter_clean_page(self, page)
    def _create_stream(self, fileobj: Path | StrByteType):
        return _writermod.writermod_pdfwriter_private_create_stream(self, fileobj)
    def append(
        self,
        fileobj: StrByteType | PdfReader | Path,
        outline_item: str | PageRange | tuple[int, int] | tuple[int, int, int] | list[int] | None = None,
        pages: PageRange | tuple[int, int] | tuple[int, int, int] | list[int] | list[PageObject] | None = None,
        import_outline: bool = True,
        excluded_fields: list[str] | tuple[str, ...] | None = None,
    ):
        "\n        Identical to the :meth:`merge()<merge>` method, but assumes you want to\n        concatenate all pages onto the end of the file instead of specifying a\n        position.\n\n        Args:\n            fileobj: A File Object or an object that supports the standard\n                read and seek methods similar to a File Object. Could also be a\n                string representing a path to a PDF file.\n            outline_item: Optionally, you may specify a string to build an\n                outline (aka 'bookmark') to identify the beginning of the\n                included file.\n            pages: Can be a :class:`PageRange<pypdf.pagerange.PageRange>`\n                or a ``(start, stop[, step])`` tuple\n                or a list of pages to be processed\n                to merge only the specified range of pages from the source\n                document into the output document.\n            import_outline: You may prevent the source document's\n                outline (collection of outline items, previously referred to as\n                'bookmarks') from being imported by specifying this as ``False``.\n            excluded_fields: Provide the list of fields/keys to be ignored\n                if ``/Annots`` is part of the list, the annotation will be ignored\n                if ``/B`` is part of the list, the articles will be ignored\n\n        "
        return _writermod.writermod_pdfwriter_append(self, fileobj, outline_item, pages, import_outline, excluded_fields)
    def merge(
        self,
        position: int | None,
        fileobj: Path | StrByteType | PdfReader,
        outline_item: str | None = None,
        pages: PageRangeSpec | list[PageObject] | None = None,
        import_outline: bool = True,
        excluded_fields: list[str] | tuple[str, ...] | None = (),
    ):
        "\n        Merge the pages from the given file into the output file at the\n        specified page number.\n\n        Args:\n            position: The *page number* to insert this file. File will\n                be inserted after the given number.\n            fileobj: A File Object or an object that supports the standard\n                read and seek methods similar to a File Object. Could also be a\n                string representing a path to a PDF file.\n            outline_item: Optionally, you may specify a string to build an outline\n                (aka 'bookmark') to identify the\n                beginning of the included file.\n            pages: can be a :class:`PageRange<pypdf.pagerange.PageRange>`\n                or a ``(start, stop[, step])`` tuple\n                or a list of pages to be processed\n                to merge only the specified range of pages from the source\n                document into the output document.\n            import_outline: You may prevent the source document's\n                outline (collection of outline items, previously referred to as\n                'bookmarks') from being imported by specifying this as ``False``.\n            excluded_fields: provide the list of fields/keys to be ignored\n                if ``/Annots`` is part of the list, the annotation will be ignored\n                if ``/B`` is part of the list, the articles will be ignored\n\n        Raises:\n            TypeError: The pages attribute is not configured properly\n\n        "
        return _writermod.writermod_pdfwriter_merge(self, position, fileobj, outline_item, pages, import_outline, excluded_fields)
    def _merge__process_named_dests(
        self,
        dest: Any,
        reader: PdfDocCommon,
        srcpages: dict[int, PageObject],
        named_destinations: _NamedDestinationIndex,
    ):
        return _writermod.writermod_pdfwriter_private_merge_process_named_dests(self, dest, reader, srcpages, named_destinations)
    def _add_named_destination_array_indexed(
        self, title: Any, destination: Any, named_destinations: _NamedDestinationIndex
    ):
        return _writermod.writermod_pdfwriter_private_add_named_destination_array_indexed(self, title, destination, named_destinations)
    def _add_articles_thread(
        self,
        thread: DictionaryObject,
        pages: dict[int, PageObject],
        reader: PdfReader,
    ):
        "\n        Clone the thread with only the applicable articles.\n\n        Args:\n            thread: Thread entry from the reader's array of threads\n            pages: Mapping of object numbers to page objects.\n            reader: The corresponding reader.\n\n        Returns:\n            The added thread as an indirect reference\n\n        "
        return _writermod.writermod_pdfwriter_private_add_articles_thread(self, thread, pages, reader)
    def add_filtered_articles(
        self,
        fltr: (
            Pattern[Any] | str
        ),  # thread entry from the reader's array of threads
        pages: dict[int, PageObject],
        reader: PdfReader,
    ):
        '\n        Add articles matching the defined criteria.\n\n        Args:\n            fltr:\n            pages:\n            reader:\n\n        '
        return _writermod.writermod_pdfwriter_add_filtered_articles(self, fltr, pages, reader)
    def _get_cloned_page(
        self,
        page: IndirectObject | PageObject | NullObject | int | None,
        pages: dict[int, PageObject],
        reader: PdfReader,
    ):
        return _writermod.writermod_pdfwriter_private_get_cloned_page(self, page, pages, reader)
    def _insert_filtered_annotations(
        self,
        *,
        annots: IndirectObject | list[PdfObject] | None,
        page: PageObject,
        pages: dict[int, PageObject],
        reader: PdfReader,
        parent_fields: dict[int, DictionaryObject] | None = None,
    ):
        return _writermod.writermod_pdfwriter_private_insert_filtered_annotations(self, annots, page, pages, reader, parent_fields)
    def _clone_widget_annotation(
        self, annotation: DictionaryObject, parent_fields: dict[int, DictionaryObject]
    ):
        '\n        Clone a widget annotation and link it to its parent fields.\n\n        Cloning ``/Parent`` directly would also clone the ``/Kids`` of the\n        parent fields, and with them the widgets and pages of sibling fields\n        which are not part of the merge. Instead, the parent fields are cloned\n        without ``/Kids`` and collected in ``parent_fields``, so that their kids\n        can be set by :meth:`_set_cloned_kids` once all annotations are cloned.\n        '
        return _writermod.writermod_pdfwriter_private_clone_widget_annotation(self, annotation, parent_fields)
    def _set_cloned_kids(self, parent_fields: dict[int, DictionaryObject], reader: PdfReader):
        'Set the ``/Kids`` of the cloned parent fields to the cloned kids, in their original order.'
        return _writermod.writermod_pdfwriter_private_set_cloned_kids(self, parent_fields, reader)
    def _get_filtered_outline(
        self,
        *,
        node: Any,
        pages: dict[int, PageObject],
        reader: PdfReader,
        visited: set[int] | None = None,
    ):
        '\n        Extract outline item entries that are part of the specified page set.\n\n        Returns:\n            A list of destination objects.\n\n        '
        return _writermod.writermod_pdfwriter_private_get_filtered_outline(self, node, pages, reader, visited)
    def _clone_outline(self, dest: Destination):
        return _writermod.writermod_pdfwriter_private_clone_outline(self, dest)
    def _insert_filtered_outline(
        self,
        outlines: list[Destination],
        parent: TreeObject | IndirectObject,
        before: TreeObject | IndirectObject | None = None,
    ):
        return _writermod.writermod_pdfwriter_private_insert_filtered_outline(self, outlines, parent, before)
    def close(self):
        'Implemented for API harmonization.'
        return _writermod.writermod_pdfwriter_close(self)
    def find_outline_item(
        self,
        outline_item: IndirectObject | str,
        root: OutlineType | None = None,
        *,
        visited: set[int] | None = None,
    ):
        return _writermod.writermod_pdfwriter_find_outline_item(self, outline_item, root, visited)
    def reset_translation(
        self, reader: PdfReader | IndirectObject | None = None
    ):
        '\n        Reset the translation table between reader and the writer object.\n\n        Late cloning will create new independent objects.\n\n        Args:\n            reader: PdfReader or IndirectObject referencing a PdfReader object.\n                if set to None or omitted, all tables will be reset.\n\n        '
        return _writermod.writermod_pdfwriter_reset_translation(self, reader)
    def set_page_label(
        self,
        page_index_from: int,
        page_index_to: int,
        style: PageLabelStyle | None = None,
        prefix: str | None = None,
        start: int | None = 0,
    ):
        '\n        Set a page label to a range of pages.\n\n        Page indexes must be given starting from 0.\n        Labels must have a style, a prefix or both.\n        If a range is not assigned any page label, a decimal label starting from 1 is applied.\n\n        Args:\n            page_index_from: page index of the beginning of the range starting from 0\n            page_index_to: page index of the beginning of the range starting from 0\n            style: The numbering style to be used for the numeric portion of each page label:\n\n                       * ``/D`` Decimal Arabic numerals\n                       * ``/R`` Uppercase Roman numerals\n                       * ``/r`` Lowercase Roman numerals\n                       * ``/A`` Uppercase letters (A to Z for the first 26 pages,\n                         AA to ZZ for the next 26, and so on)\n                       * ``/a`` Lowercase letters (a to z for the first 26 pages,\n                         aa to zz for the next 26, and so on)\n\n            prefix: The label prefix for page labels in this range.\n            start:  The value of the numeric portion for the first page label\n                    in the range.\n                    Subsequent pages are numbered sequentially from this value,\n                    which must be greater than or equal to 1.\n                    Default value: 1.\n\n        '
        return _writermod.writermod_pdfwriter_set_page_label(self, page_index_from, page_index_to, style, prefix, start)
    def _set_page_label(
        self,
        page_index_from: int,
        page_index_to: int,
        style: PageLabelStyle | None = None,
        prefix: str | None = None,
        start: int | None = 0,
    ):
        '\n        Set a page label to a range of pages.\n\n        Page indexes must be given starting from 0.\n        Labels must have a style, a prefix or both.\n        If a range is not assigned any page label a decimal label starting from 1 is applied.\n\n        Args:\n            page_index_from: page index of the beginning of the range starting from 0\n            page_index_to: page index of the beginning of the range starting from 0\n            style:  The numbering style to be used for the numeric portion of each page label:\n                        /D Decimal Arabic numerals\n                        /R Uppercase Roman numerals\n                        /r Lowercase Roman numerals\n                        /A Uppercase letters (A to Z for the first 26 pages,\n                           AA to ZZ for the next 26, and so on)\n                        /a Lowercase letters (a to z for the first 26 pages,\n                           aa to zz for the next 26, and so on)\n            prefix: The label prefix for page labels in this range.\n            start:  The value of the numeric portion for the first page label\n                    in the range.\n                    Subsequent pages are numbered sequentially from this value,\n                    which must be greater than or equal to 1. Default value: 1.\n\n        '
        return _writermod.writermod_pdfwriter_private_set_page_label(self, page_index_from, page_index_to, style, prefix, start)
    def _repr_mimebundle_(
        self,
        include: Iterable[str] | None = None,
        exclude: Iterable[str] | None = None,
    ):
        '\n        Integration into Jupyter Notebooks.\n\n        This method returns a dictionary that maps a mime-type to its\n        representation.\n\n        .. seealso::\n\n           https://ipython.readthedocs.io/en/stable/config/integrating.html\n        '
        return _writermod.writermod_pdfwriter_private_repr_mimebundle_(self, include, exclude)
def _pdf_objectify(obj: dict[str, Any] | str | float | list[Any]):
    return _writermod.writermod_private_pdf_objectify(obj)
def _create_outline_item(
    action_ref: IndirectObject | None,
    title: str,
    color: tuple[float, float, float] | str | None,
    italic: bool,
    bold: bool,
):
    return _writermod.writermod_private_create_outline_item(action_ref, title, color, italic, bold)

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
