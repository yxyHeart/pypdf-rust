"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._doc_common``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

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
def convert_to_int(*args, **kwargs):
    return None  # inert stub: pypdf._doc_common.convert_to_int
_INHERITABLE_PAGE_ATTRIBUTES = ()
class _PageTreeItem:
    """Stub for pypdf._doc_common._PageTreeItem."""
    node = None
    inherit = None
    indirect_reference = None
    depth = None
_PageTreeFrame = None
_UNSET = None
class DocumentInformation(DictionaryObject):
    """Stub for pypdf._doc_common.DocumentInformation."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def _get_text(self, *args, **kwargs):
        return None  # inert stub
    @property
    def title(self):
        return None  # inert stub
    @property
    def title_raw(self):
        return None  # inert stub
    @property
    def author(self):
        return None  # inert stub
    @property
    def author_raw(self):
        return None  # inert stub
    @property
    def subject(self):
        return None  # inert stub
    @property
    def subject_raw(self):
        return None  # inert stub
    @property
    def creator(self):
        return None  # inert stub
    @property
    def creator_raw(self):
        return None  # inert stub
    @property
    def producer(self):
        return None  # inert stub
    @property
    def producer_raw(self):
        return None  # inert stub
    @property
    def creation_date(self):
        return None  # inert stub
    @property
    def creation_date_raw(self):
        return None  # inert stub
    @property
    def modification_date(self):
        return None  # inert stub
    @property
    def modification_date_raw(self):
        return None  # inert stub
    @property
    def keywords(self):
        return None  # inert stub
    @property
    def keywords_raw(self):
        return None  # inert stub
class PdfDocCommon:
    """Stub for pypdf._doc_common.PdfDocCommon."""
    strict = None
    flattened_pages = None
    _encryption = None
    _readonly = None
    @property
    def root_object(self):
        return None  # inert stub
    @property
    def pdf_header(self):
        return None  # inert stub
    def get_object(self, *args, **kwargs):
        return None  # inert stub
    def _replace_object(self, *args, **kwargs):
        return None  # inert stub
    @property
    def _info(self):
        return None  # inert stub
    @property
    def metadata(self):
        return None  # inert stub
    @property
    def xmp_metadata(self):
        return None  # inert stub
    @property
    def viewer_preferences(self):
        return None  # inert stub
    def get_num_pages(self, *args, **kwargs):
        return None  # inert stub
    def get_page(self, *args, **kwargs):
        return None  # inert stub
    def _get_page_in_node(self, *args, **kwargs):
        return None  # inert stub
    @property
    def named_destinations(self):
        return None  # inert stub
    def get_named_dest_root(self, *args, **kwargs):
        return None  # inert stub
    def _get_named_destinations(self, *args, **kwargs):
        return None  # inert stub
    def get_fields(self, *args, **kwargs):
        return None  # inert stub
    def _get_qualified_field_name(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _normal_appearance(*args, **kwargs):
        return None  # inert stub
    def _build_field(self, *args, **kwargs):
        return None  # inert stub
    def _check_kids(self, *args, **kwargs):
        return None  # inert stub
    def _write_field(self, *args, **kwargs):
        return None  # inert stub
    def get_form_text_fields(self, *args, **kwargs):
        return None  # inert stub
    def get_pages_showing_field(self, *args, **kwargs):
        return None  # inert stub
    @property
    def open_destination(self):
        return None  # inert stub
    def open_destination(self, *args, **kwargs):
        return None  # inert stub
    @property
    def outline(self):
        return None  # inert stub
    def _get_outline(self, *args, **kwargs):
        return None  # inert stub
    @property
    def threads(self):
        return None  # inert stub
    def _get_page_number_by_indirect(self, *args, **kwargs):
        return None  # inert stub
    def get_page_number(self, *args, **kwargs):
        return None  # inert stub
    def get_destination_page_number(self, *args, **kwargs):
        return None  # inert stub
    def _build_destination(self, *args, **kwargs):
        return None  # inert stub
    def _build_outline_item(self, *args, **kwargs):
        return None  # inert stub
    @property
    def pages(self):
        return None  # inert stub
    @property
    def page_labels(self):
        return None  # inert stub
    @property
    def page_layout(self):
        return None  # inert stub
    @property
    def page_mode(self):
        return None  # inert stub
    def _flatten(self, *args, **kwargs):
        return None  # inert stub
    def _enter_page_tree_node(self, *args, **kwargs):
        return None  # inert stub
    def _pop_next_page_tree_kid(self, *args, **kwargs):
        return None  # inert stub
    def _flatten_leaf_page(self, *args, **kwargs):
        return None  # inert stub
    def remove_page(self, *args, **kwargs):
        return None  # inert stub
    def _get_indirect_object(self, *args, **kwargs):
        return None  # inert stub
    @property
    def user_access_permissions(self):
        return None  # inert stub
    @property
    def are_permissions_valid(self):
        return None  # inert stub
    @property
    def is_encrypted(self):
        return None  # inert stub
    @property
    def xfa(self):
        return None  # inert stub
    @property
    def attachments(self):
        return None  # inert stub
    @classmethod
    def _get_attachment_contents(cls, *args, **kwargs):
        return None  # inert stub
    @property
    def attachment_list(self):
        return None  # inert stub
    def _repr_mimebundle_(self, *args, **kwargs):
        return None  # inert stub
class LazyDict(Mapping):
    """Stub for pypdf._doc_common.LazyDict."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def __getitem__(self, *args, **kwargs):
        return None  # inert stub
    def __iter__(self, *args, **kwargs):
        return None  # inert stub
    def __len__(self, *args, **kwargs):
        return None  # inert stub
    def __str__(self, *args, **kwargs):
        return ""  # inert stub
