# Copyright (c) 2006, Mathieu Fenniak
# Copyright (c) 2007, Ashish Kulkarni <kulkarni.ashish@gmail.com>
# Copyright (c) 2024, Pubpub-ZZ
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

"""Rust-backed port of ``pypdf/_doc_common.py`` (pypdf 7.1.1).

The class structure -- the :class:`PdfDocCommon` ABC with its
``@property``/``@abstractmethod`` members, the ``DocumentInformation``
dictionary subclass, the ``_PageTreeItem`` NamedTuple and the ``LazyDict``
mapping -- is kept verbatim from upstream so that MRO, ``isinstance``,
subclassing, ``mock.patch`` targets and pickling behaviour are unchanged.  Every
method body is a one-line delegation to the Rust implementation in
``crates/pypdf-py/src/modules/doc_common_mod.rs`` (reached through
``pypdf._pypdf``), which contains all of the logic.
"""

import struct
from abc import ABC, abstractmethod
from collections.abc import Iterable, Iterator, Mapping, Sequence
from datetime import datetime
from typing import Any, NamedTuple, cast
from ._configuration import get_configuration
from ._encryption import Encryption
from ._page import PageObject, _VirtualList
from ._page_labels import index2labels
from ._utils import _TraversalState, deprecation_no_replacement, logger_warning, parse_iso8824_date
from .constants import CatalogAttributes, CheckboxRadioButtonAttributes, Core, DocumentInformationAttributes, FieldDictionaryAttributes, GoToActionArguments, PageAttributes, PagesAttributes, UserAccessPermissions
from .errors import LimitReachedError, PdfReadError, PyPdfError
from .generic import ArrayObject, BooleanObject, ByteStringObject, Destination, DictionaryObject, Field, Fit, FloatObject, IndirectObject, NameObject, NullObject, PdfObject, StreamObject, TextStringObject, TreeObject, ViewerPreferences, create_string_object, is_null_or_none
from .generic._files import EmbeddedFile
from .types import OutlineType, PagemodeType
from .xmp import XmpInformation

from pypdf._pypdf import (
    doccommon_attachment_list as _attachment_list,
    doccommon_attachments as _attachments,
    doccommon_are_permissions_valid as _are_permissions_valid,
    doccommon_build_destination as _build_destination,
    doccommon_build_field as _build_field,
    doccommon_build_outline_item as _build_outline_item,
    doccommon_check_kids as _check_kids,
    doccommon_convert_to_int as _convert_to_int,
    doccommon_documentinformation_author as _documentinformation_author,
    doccommon_documentinformation_author_raw as _documentinformation_author_raw,
    doccommon_documentinformation_creation_date as _documentinformation_creation_date,
    doccommon_documentinformation_creation_date_raw as _documentinformation_creation_date_raw,
    doccommon_documentinformation_creator as _documentinformation_creator,
    doccommon_documentinformation_creator_raw as _documentinformation_creator_raw,
    doccommon_documentinformation_get_text as _documentinformation_get_text,
    doccommon_documentinformation_keywords as _documentinformation_keywords,
    doccommon_documentinformation_keywords_raw as _documentinformation_keywords_raw,
    doccommon_documentinformation_modification_date as _documentinformation_modification_date,
    doccommon_documentinformation_modification_date_raw as _documentinformation_modification_date_raw,
    doccommon_documentinformation_producer as _documentinformation_producer,
    doccommon_documentinformation_producer_raw as _documentinformation_producer_raw,
    doccommon_documentinformation_subject as _documentinformation_subject,
    doccommon_documentinformation_subject_raw as _documentinformation_subject_raw,
    doccommon_documentinformation_title as _documentinformation_title,
    doccommon_documentinformation_title_raw as _documentinformation_title_raw,
    doccommon_enter_page_tree_node as _enter_page_tree_node,
    doccommon_flatten as _flatten,
    doccommon_flatten_leaf_page as _flatten_leaf_page,
    doccommon_get_attachment_contents as _get_attachment_contents,
    doccommon_get_destination_page_number as _get_destination_page_number,
    doccommon_get_fields as _get_fields,
    doccommon_get_form_text_fields as _get_form_text_fields,
    doccommon_get_indirect_object as _get_indirect_object,
    doccommon_get_named_dest_root as _get_named_dest_root,
    doccommon_get_named_destinations as _get_named_destinations,
    doccommon_get_num_pages as _get_num_pages,
    doccommon_get_outline as _get_outline,
    doccommon_get_page as _get_page,
    doccommon_get_page_in_node as _get_page_in_node,
    doccommon_get_page_number as _get_page_number,
    doccommon_get_pages_showing_field as _get_pages_showing_field,
    doccommon_get_qualified_field_name as _get_qualified_field_name,
    doccommon_inheritable_page_attributes as _inheritable_page_attributes,
    doccommon_lazydict_getitem as _lazydict_getitem,
    doccommon_lazydict_init as _lazydict_init,
    doccommon_lazydict_iter as _lazydict_iter,
    doccommon_lazydict_len as _lazydict_len,
    doccommon_lazydict_str as _lazydict_str,
    doccommon_metadata as _metadata,
    doccommon_named_destinations as _named_destinations,
    doccommon_normal_appearance as _normal_appearance,
    doccommon_open_destination as _open_destination,
    doccommon_outline as _outline,
    doccommon_page_labels as _page_labels,
    doccommon_page_layout as _page_layout,
    doccommon_page_mode as _page_mode,
    doccommon_pages as _pages,
    doccommon_pop_next_page_tree_kid as _pop_next_page_tree_kid,
    doccommon_remove_page as _remove_page,
    doccommon_set_open_destination as _set_open_destination,
    doccommon_threads as _threads,
    doccommon_user_access_permissions as _user_access_permissions,
    doccommon_viewer_preferences as _viewer_preferences,
    doccommon_write_field as _write_field,
    doccommon_xfa as _xfa,
)


def convert_to_int(d: bytes, size: int) -> int | tuple[Any, ...]:
    return _convert_to_int(d, size)


# Attributes a page inherits from its ancestors in the page tree.
_INHERITABLE_PAGE_ATTRIBUTES = _inheritable_page_attributes()


class _PageTreeItem(NamedTuple):
    """A page tree node waiting to be flattened by :meth:`PdfDocCommon._flatten`."""
    node: DictionaryObject
    inherit: dict[str, Any]
    indirect_reference: IndirectObject | None
    depth: int


_PageTreeFrame = tuple[_PageTreeItem, Iterator[PdfObject]]

_UNSET = object()


class DocumentInformation(DictionaryObject):
    """
    A class representing the basic document metadata provided in a PDF File.
    This class is accessible through
    :py:class:`PdfReader.metadata<pypdf.PdfReader.metadata>`.

    All text properties of the document metadata have
    *two* properties, e.g. author and author_raw. The non-raw property will
    always return a ``TextStringObject``, making it ideal for a case where the
    metadata is being displayed. The raw property can sometimes return a
    ``ByteStringObject``, if pypdf was unable to decode the string's text
    encoding; this requires additional safety in the caller and therefore is not
    as commonly accessed.
    """

    def __init__(self) -> None:
        DictionaryObject.__init__(self)

    def _get_text(self, key: str) -> str | None:
        return _documentinformation_get_text(self, key)

    @property
    def title(self) -> str | None:
        """
        Read-only property accessing the document's title.

        Returns a ``TextStringObject`` or ``None`` if the title is not
        specified.
        """
        return _documentinformation_title(self)

    @property
    def title_raw(self) -> str | None:
        """The "raw" version of title; can return a ``ByteStringObject``."""
        return _documentinformation_title_raw(self)

    @property
    def author(self) -> str | None:
        """
        Read-only property accessing the document's author.

        Returns a ``TextStringObject`` or ``None`` if the author is not
        specified.
        """
        return _documentinformation_author(self)

    @property
    def author_raw(self) -> str | None:
        """The "raw" version of author; can return a ``ByteStringObject``."""
        return _documentinformation_author_raw(self)

    @property
    def subject(self) -> str | None:
        """
        Read-only property accessing the document's subject.

        Returns a ``TextStringObject`` or ``None`` if the subject is not
        specified.
        """
        return _documentinformation_subject(self)

    @property
    def subject_raw(self) -> str | None:
        """The "raw" version of subject; can return a ``ByteStringObject``."""
        return _documentinformation_subject_raw(self)

    @property
    def creator(self) -> str | None:
        """
        Read-only property accessing the document's creator.

        If the document was converted to PDF from another format, this is the
        name of the application (e.g. OpenOffice) that created the original
        document from which it was converted. Returns a ``TextStringObject`` or
        ``None`` if the creator is not specified.
        """
        return _documentinformation_creator(self)

    @property
    def creator_raw(self) -> str | None:
        """The "raw" version of creator; can return a ``ByteStringObject``."""
        return _documentinformation_creator_raw(self)

    @property
    def producer(self) -> str | None:
        """
        Read-only property accessing the document's producer.

        If the document was converted to PDF from another format, this is the
        name of the application (for example, macOS Quartz) that converted it to
        PDF. Returns a ``TextStringObject`` or ``None`` if the producer is not
        specified.
        """
        return _documentinformation_producer(self)

    @property
    def producer_raw(self) -> str | None:
        """The "raw" version of producer; can return a ``ByteStringObject``."""
        return _documentinformation_producer_raw(self)

    @property
    def creation_date(self) -> datetime | None:
        """Read-only property accessing the document's creation date."""
        return _documentinformation_creation_date(self)

    @property
    def creation_date_raw(self) -> str | None:
        """
        The "raw" version of creation date; can return a ``ByteStringObject``.

        Typically in the format ``D:YYYYMMDDhhmmss[+Z-]hh'mm`` where the suffix
        is the offset from UTC.
        """
        return _documentinformation_creation_date_raw(self)

    @property
    def modification_date(self) -> datetime | None:
        """
        Read-only property accessing the document's modification date.

        The date and time the document was most recently modified.
        """
        return _documentinformation_modification_date(self)

    @property
    def modification_date_raw(self) -> str | None:
        """
        The "raw" version of modification date; can return a
        ``ByteStringObject``.

        Typically, in the format ``D:YYYYMMDDhhmmss[+Z-]hh'mm`` where the suffix
        is the offset from UTC.
        """
        return _documentinformation_modification_date_raw(self)

    @property
    def keywords(self) -> str | None:
        """
        Read-only property accessing the document's keywords.

        Returns a ``TextStringObject`` or ``None`` if keywords are not
        specified.
        """
        return _documentinformation_keywords(self)

    @property
    def keywords_raw(self) -> str | None:
        """The "raw" version of keywords; can return a ``ByteStringObject``."""
        return _documentinformation_keywords_raw(self)

class PdfDocCommon(ABC):
    """
    Common functions from PdfWriter and PdfReader objects.

    This root class is strongly abstracted.
    """
    strict: bool = False
    flattened_pages: list[PageObject] | None = None
    _encryption: Encryption | None = None
    _readonly: bool = False

    @property
    @abstractmethod
    def root_object(self) -> DictionaryObject:
        ...

    @property
    @abstractmethod
    def pdf_header(self) -> str:
        ...

    @abstractmethod
    def get_object(self, indirect_reference: int | IndirectObject) -> PdfObject | None:
        ...

    @abstractmethod
    def _replace_object(self, indirect_reference: IndirectObject, obj: PdfObject) -> PdfObject:
        ...

    @property
    @abstractmethod
    def _info(self) -> DictionaryObject | None:
        ...

    @property
    def metadata(self) -> DocumentInformation | None:
        """
        Retrieve the PDF file's document information dictionary, if it exists.

        Note that some PDF files use metadata streams instead of document
        information dictionaries, and these metadata streams will not be
        accessed by this function.
        """
        return _metadata(self)

    @property
    @abstractmethod
    def xmp_metadata(self) -> XmpInformation | None:
        ...

    @property
    def viewer_preferences(self) -> ViewerPreferences | None:
        """Returns the existing ViewerPreferences as an overloaded dictionary."""
        return _viewer_preferences(self)

    def get_num_pages(self) -> int:
        """
        Calculate the number of pages in this PDF file.

        Returns:
            The number of pages of the parsed PDF file.

        Raises:
            PdfReadError: If restrictions prevent this action.

        """
        return _get_num_pages(self)

    def get_page(self, page_number: int) -> PageObject:
        """
        Retrieve a page by number from this PDF file.
        Most of the time ``.pages[page_number]`` is preferred.

        Args:
            page_number: The page number to retrieve
                (pages begin at zero)

        Returns:
            A :class:`PageObject<pypdf._page.PageObject>` instance.

        """
        return _get_page(self, page_number)

    def _get_page_in_node(self, page_number: int) -> tuple[DictionaryObject, int]:
        """
        Retrieve the node and position within the /Kids containing the page.
        If page_number is greater than the number of pages, it returns the top node, -1.
        """
        return _get_page_in_node(self, page_number)

    @property
    def named_destinations(self) -> dict[str, Destination]:
        """A read-only dictionary which maps names to destinations."""
        return _named_destinations(self)

    def get_named_dest_root(self) -> ArrayObject:
        return _get_named_dest_root(self)

    def _get_named_destinations(self, *, tree: DictionaryObject | None=None, retval: dict[str, Destination] | None=None, visited: set[int] | None=None, depth: int=0, traversal_state: _TraversalState | None=None) -> dict[str, Destination]:
        """
        Retrieve the named destinations present in the document.

        Args:
            tree: The current tree.
            retval: The previously retrieved destinations for nested calls.
            visited: Already known/visited objects.
            depth: Current name tree nesting depth.
            traversal_state: Shared visitor counters.

        Returns:
            A dictionary which maps names to destinations.

        """
        return _get_named_destinations(self, tree, retval, visited, depth, traversal_state)

    def get_fields(self, tree: DictionaryObject | None=None, retval: dict[Any, Any] | None=None, fileobj: Any | None=None, stack: Any=_UNSET, *, _visited: set[int] | None=None, _depth: int=0, _traversal_state: _TraversalState | None=None) -> dict[str, Any] | None:
        """
        Extract field data if this PDF contains interactive form fields.

        The *tree*, *retval*, *visited*, *depth* parameters are for internal use.

        Args:
            tree: Current object to parse.
            retval: In-progress list of fields.
            fileobj: A file object (usually a text file) to write
                a report to on all interactive form fields found.
            stack: The stack to use. Deprecated.
            _visited: Set of already visited IDs. Callers should usually omit this.
            _depth: The current field depth.
            _traversal_state: The current traversal state.

        Returns:
            A dictionary where each key is a field name, and each
            value is a :class:`Field<pypdf.generic.Field>` object. By
            default, the mapping name is used for keys.
            ``None`` if form data could not be located.

        """
        return _get_fields(self, tree, retval, fileobj, stack, _visited, _depth, _traversal_state)

    def _get_qualified_field_name(self, *, parent: DictionaryObject, visited: set[int] | None=None) -> str:
        return _get_qualified_field_name(self, parent, visited)

    @staticmethod
    def _normal_appearance(appearance: Any) -> DictionaryObject:
        """Return the /N normal-appearance sub-dictionary of an /AP entry."""
        return _normal_appearance(appearance)

    def _build_field(self, *, field: TreeObject | DictionaryObject, retval: dict[Any, Any], fileobj: Any, field_attributes: Any, visited: set[int], depth: int, traversal_state: _TraversalState) -> None:
        return _build_field(self, field, retval, fileobj, field_attributes, visited, depth, traversal_state)

    def _check_kids(self, *, tree: TreeObject | DictionaryObject, retval: Any, fileobj: Any, visited: set[int], depth: int, traversal_state: _TraversalState) -> None:
        return _check_kids(self, tree, retval, fileobj, visited, depth, traversal_state)

    def _write_field(self, fileobj: Any, field: Any, field_attributes: Any) -> None:
        return _write_field(fileobj, field, field_attributes)

    def get_form_text_fields(self, full_qualified_name: bool=False) -> dict[str, Any]:
        """
        Retrieve form fields from the document with textual data.

        Args:
            full_qualified_name: to get full name

        Returns:
            A dictionary. The key is the name of the form field,
            the value is the content of the field.

            If the document contains multiple form fields with the same name, the
            second and following will get the suffix .2, .3, ...

        """
        return _get_form_text_fields(self, full_qualified_name)

    def get_pages_showing_field(self, field: Field | PdfObject | IndirectObject) -> list[PageObject]:
        """
        Provides list of pages where the field is called.

        Args:
            field: Field Object, PdfObject or IndirectObject referencing a Field

        Returns:
            List of pages:
                - Empty list:
                    The field has no widgets attached
                    (either hidden field or ancestor field).
                - Single page list:
                    Page where the widget is present
                    (most common).
                - Multi-page list:
                    Field with multiple kids widgets
                    (example: radio buttons, field repeated on multiple pages).

        """
        return _get_pages_showing_field(self, field)

    @property
    def open_destination(self) -> Destination | TextStringObject | ByteStringObject | None:
        """
        Property to access the opening destination (``/OpenAction`` entry in
        the PDF catalog). It returns ``None`` if the entry does not exist
        or is not set.

        Raises:
            Exception: If a destination is invalid.

        """
        return _open_destination(self)

    @open_destination.setter
    def open_destination(self, dest: str | Destination | PageObject | None) -> None:
        return _set_open_destination(self, dest)

    @property
    def outline(self) -> OutlineType:
        """
        Read-only property for the outline present in the document
        (i.e., a collection of 'outline items' which are also known as
        'bookmarks').
        """
        return _outline(self)

    def _get_outline(self, *, node: DictionaryObject | None=None, outline: Any | None=None, visited: set[int] | None=None, depth: int=0, traversal_state: _TraversalState | None=None) -> OutlineType:
        return _get_outline(self, node, outline, visited, depth, traversal_state)

    @property
    def threads(self) -> ArrayObject | None:
        """
        Read-only property for the list of threads.

        See §12.4.3 from the PDF 1.7 or 2.0 specification.

        It is an array of dictionaries with "/F" (the first bead in the thread)
        and "/I" (a thread information dictionary containing information about
        the thread, such as its title, author, and creation date) properties or
        None if there are no articles.

        Since PDF 2.0 it can also contain an indirect reference to a metadata
        stream containing information about the thread, such as its title,
        author, and creation date.
        """
        return _threads(self)

    @abstractmethod
    def _get_page_number_by_indirect(self, indirect_reference: int | NullObject | IndirectObject | None) -> int | None:
        ...

    def get_page_number(self, page: PageObject) -> int | None:
        """
        Retrieve page number of a given PageObject.

        Args:
            page: The page to get page number. Should be
                an instance of :class:`PageObject<pypdf._page.PageObject>`

        Returns:
            The page number or None if page is not found

        """
        return _get_page_number(self, page)

    def get_destination_page_number(self, destination: Destination) -> int | None:
        """
        Retrieve page number of a given Destination object.

        Args:
            destination: The destination to get page number.

        Returns:
            The page number or None if page is not found

        """
        return _get_destination_page_number(self, destination)

    def _build_destination(self, title: str | bytes, array: ArrayObject | None) -> Destination:
        return _build_destination(self, title, array)

    def _build_outline_item(self, node: DictionaryObject) -> Destination | None:
        return _build_outline_item(self, node)

    @property
    def pages(self) -> Sequence[PageObject]:
        """
        Property that emulates a list of :class:`PageObject<pypdf._page.PageObject>`.
        This property allows to get a page or a range of pages.

        The returned object supports indexing, slicing, ``len()``, iteration and
        (for PdfWriter) ``del``, but it is not a :class:`list` - pages are looked
        up on demand rather than materialized up front, so list-only operations
        such as ``append()`` or concatenation with ``+`` are not available.

        Note:
            For PdfWriter only: Provides the capability to remove a page/range of
            page from the list (using the del operator). Remember: Only the page
            entry is removed, as the objects beneath can be used elsewhere. A
            solution to completely remove them - if they are not used anywhere - is
            to write to a buffer/temporary file and then load it into a new
            PdfWriter.

        """
        return _pages(self)

    @property
    def page_labels(self) -> list[str]:
        """
        A list of labels for the pages in this document.

        This property is read-only. The labels are in the order that the pages
        appear in the document.
        """
        return _page_labels(self)

    @property
    def page_layout(self) -> str | None:
        """
        Get the page layout currently being used.

        .. list-table:: Valid ``layout`` values
           :widths: 50 200

           * - /NoLayout
             - Layout explicitly not specified
           * - /SinglePage
             - Show one page at a time
           * - /OneColumn
             - Show one column at a time
           * - /TwoColumnLeft
             - Show pages in two columns, odd-numbered pages on the left
           * - /TwoColumnRight
             - Show pages in two columns, odd-numbered pages on the right
           * - /TwoPageLeft
             - Show two pages at a time, odd-numbered pages on the left
           * - /TwoPageRight
             - Show two pages at a time, odd-numbered pages on the right
        """
        return _page_layout(self)

    @property
    def page_mode(self) -> PagemodeType | None:
        """
        Get the page mode currently being used.

        .. list-table:: Valid ``mode`` values
           :widths: 50 200

           * - /UseNone
             - Do not show outline or thumbnails panels
           * - /UseOutlines
             - Show outline (aka bookmarks) panel
           * - /UseThumbs
             - Show page thumbnails panel
           * - /FullScreen
             - Fullscreen view
           * - /UseOC
             - Show Optional Content Group (OCG) panel
           * - /UseAttachments
             - Show attachments panel
        """
        return _page_mode(self)

    def _flatten(self, list_only: bool=False) -> None:
        """
        Process the document pages to ease searching.

        Attributes of a page may inherit from ancestor nodes
        in the page tree. Flattening means moving
        any inheritance data into descendant nodes,
        effectively removing the inheritance dependency.
        The result is stored in ``self.flattened_pages``.

        Note: It is distinct from another use of "flattening" applied to PDFs.
        Flattening a PDF also means combining all the contents into one single layer
        and making the file less editable.

        The page tree is walked with an explicit stack instead of recursion, so the
        traversal is bound by ``Configuration.page_tree_maximum_depth`` and
        ``Configuration.page_tree_maximum_entries`` only, not by the interpreter
        recursion limit.

        Args:
            list_only: If True, the page's own entries are not copied into the
                generated :class:`PageObject`. Attributes inherited from ancestor
                nodes are applied either way. Note that a page reached through an
                indirect reference is already populated by :class:`PageObject`
                itself, so this only has an effect for ``/Kids`` entries that are
                inline dictionaries.

        """
        return _flatten(self, list_only)

    def _enter_page_tree_node(self, item: _PageTreeItem) -> _PageTreeFrame:
        """
        Start walking an intermediate ``/Pages`` node: propagate its inheritable
        attributes and pair it with an iterator over its ``/Kids``.
        """
        return _enter_page_tree_node(self, item)

    def _pop_next_page_tree_kid(self, stack: list[_PageTreeFrame], ancestor_ids: set[int], traversal_state: _TraversalState) -> _PageTreeItem | None:
        """
        Return the next page tree node to flatten, leaving the ``/Pages`` nodes whose
        ``/Kids`` are exhausted, or ``None`` when the traversal is complete.

        Cyclic references and the configured page-tree entry limit are enforced here.
        """
        return _pop_next_page_tree_kid(self, stack, ancestor_ids, traversal_state)

    def _flatten_leaf_page(self, *, item: _PageTreeItem, list_only: bool, remaining_page_entries: _TraversalState) -> PageObject:
        """
        Build the :class:`PageObject` for a leaf ``/Page`` node.

        ``list_only`` suppresses copying the page's own entries; the inherited
        attributes are applied regardless. When the node has an indirect reference,
        :class:`PageObject` has already copied those entries in its constructor,
        so the flag only matters for inline ``/Kids`` dictionaries.
        """
        return _flatten_leaf_page(self, item, list_only, remaining_page_entries)

    def remove_page(self, page: int | PageObject | IndirectObject, clean: bool=False) -> None:
        """
        Remove page from pages list.

        Args:
            page:
                * :class:`int`: Page number to be removed.
                * :class:`~pypdf._page.PageObject`: page to be removed. If the page appears many times
                  only the first one will be removed.
                * :class:`~pypdf.generic.IndirectObject`: Reference to page to be removed.

            clean: replace PageObject with NullObject to prevent annotations
                or destinations to reference a detached page.

        """
        return _remove_page(self, page, clean)

    def _get_indirect_object(self, num: int, gen: int) -> PdfObject | None:
        """
        Used to ease development.

        This is equivalent to generic.IndirectObject(num,gen,self).get_object()

        Args:
            num: The object number of the indirect object.
            gen: The generation number of the indirect object.

        Returns:
            A PdfObject

        """
        return _get_indirect_object(self, num, gen)

    @property
    def user_access_permissions(self) -> UserAccessPermissions | None:
        """
        Get the user access permissions for encrypted documents.
        Returns None if not encrypted.

        .. warning::

            For AES-256 encrypted documents (R=5/R=6), the returned
            permissions are derived from the ``/P`` field, which is
            only trustworthy if the ``/Perms`` integrity check passed.
            Check :attr:`are_permissions_valid` to verify.
        """
        return _user_access_permissions(self)

    @property
    def are_permissions_valid(self) -> bool | None:
        """
        Whether the ``/Perms`` integrity check passed for this document.

        For AES-256 encrypted documents (R=5/R=6), the ``/Perms`` field
        is an encrypted copy of the permissions that can be verified
        independently. Returns ``False`` if this check fails (the ``/P``
        permissions may have been tampered with).

        Returns ``None`` if the document is not encrypted or has not yet
        been decrypted via :meth:`decrypt()<pypdf.PdfReader.decrypt>`.
        Returns ``True`` for non-AES-256 encryption (no ``/Perms`` to check).
        """
        return _are_permissions_valid(self)

    @property
    @abstractmethod
    def is_encrypted(self) -> bool:
        """
        Read-only boolean property showing whether this PDF file is encrypted.

        Note that this property, if true, will remain true even after the
        :meth:`decrypt()<pypdf.PdfReader.decrypt>` method is called.
        """
        ...

    @property
    def xfa(self) -> dict[str, bytes] | None:
        return _xfa(self)

    @property
    def attachments(self) -> Mapping[str, list[bytes]]:
        """Mapping of attachment filenames to their content."""
        return _attachments(self)

    @classmethod
    def _get_attachment_contents(cls, entries: list[EmbeddedFile]) -> list[bytes]:
        return _get_attachment_contents(entries)

    @property
    def attachment_list(self) -> Iterator[EmbeddedFile]:
        """Iterable of attachment objects."""
        return _attachment_list(self)

    @abstractmethod
    def _repr_mimebundle_(self, include: Iterable[str] | None=None, exclude: Iterable[str] | None=None) -> dict[str, Any]:
        """
        Integration into Jupyter Notebooks.

        This method returns a dictionary that maps a mime-type to its
        representation.

        .. seealso::

            https://ipython.readthedocs.io/en/stable/config/integrating.html
        """
        ...


class LazyDict(Mapping[Any, Any]):

    def __init__(self, *args: Any, **kwargs: Any) -> None:
        return _lazydict_init(self, args, kwargs)

    def __getitem__(self, key: str) -> Any:
        return _lazydict_getitem(self, key)

    def __iter__(self) -> Iterator[Any]:
        return _lazydict_iter(self)

    def __len__(self) -> int:
        return _lazydict_len(self)

    def __str__(self) -> str:
        return _lazydict_str(self)
