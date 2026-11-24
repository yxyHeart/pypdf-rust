"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._writer``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

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
from typing import IO, Any, Literal, NoReturn, cast
if sys.version_info >= (3, 11):
    from typing import Self
else:
    from typing_extensions import Self
from ._doc_common import DocumentInformation, PdfDocCommon
from ._encryption import EncryptAlgorithm, Encryption
from ._page import PageObject, Transformation
from ._page_labels import nums_clear_range, nums_insert, nums_next
from ._reader import PdfReader
from ._utils import StrByteType, StreamType, _get_max_pdf_version_header, deprecate_positional_arguments, deprecate_with_replacement, deprecation_with_replacement, logger_warning
from .actions import Action
from .constants import AnnotationDictionaryAttributes, CatalogAttributes, Core, FieldDictionaryAttributes, GoToActionArguments, ImageType, InteractiveFormDictEntries, OutlineFontFlag, PageAttributes, PageLabelStyle, PagesAttributes, TrailerKeys, UserAccessPermissions
from .errors import LimitReachedError, PdfReadError, PyPdfError
from .generic import PAGE_FIT, ArrayObject, BooleanObject, ByteStringObject, ContentStream, Destination, DictionaryObject, EmbeddedFile, Fit, FloatObject, IndirectObject, NamedReferenceLink, NameObject, NullObject, NumberObject, PdfObject, RectangleObject, ReferenceLink, StreamObject, TextStringObject, TreeObject, ViewerPreferences, create_string_object, extract_links, hex_to_rgb, is_null_or_none
from .generic._appearance_stream import TextStreamAppearance
from .pagerange import PageRange, PageRangeSpec
from .types import AnnotationSubtype, BorderArrayType, LayoutType, OutlineItemType, OutlineType, PagemodeType
from .xmp import XmpInformation
ALL_DOCUMENT_PERMISSIONS = None
class ObjectDeletionFlag:
    """Stub for pypdf._writer.ObjectDeletionFlag."""
    NONE = 0
    TEXT = None
    LINKS = None
    ATTACHMENTS = None
    OBJECTS_3D = None
    ALL_ANNOTATIONS = None
    XOBJECT_IMAGES = None
    INLINE_IMAGES = None
    DRAWING_IMAGES = None
    IMAGES = None
_NAMED_DESTINATION_INDEX_MINIMUM = 32
class _NamedDestinationIndex:
    """Stub for pypdf._writer._NamedDestinationIndex."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def _update(self, *args, **kwargs):
        return None  # inert stub
    def contains(self, *args, **kwargs):
        return None  # inert stub
    def insert(self, *args, **kwargs):
        return None  # inert stub
    def record_insertion(self, *args, **kwargs):
        return None  # inert stub
    def flush(self, *args, **kwargs):
        return None  # inert stub
class PdfWriter(PdfDocCommon):
    """Stub for pypdf._writer.PdfWriter."""
    incremental = None
    _objects = None
    _id_translated = None
    _reader = None
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    @property
    def is_encrypted(self):
        return None  # inert stub
    @property
    def root_object(self):
        return None  # inert stub
    @property
    def _info(self):
        return None  # inert stub
    def _info(self, *args, **kwargs):
        return None  # inert stub
    @property
    def xmp_metadata(self):
        return None  # inert stub
    def xmp_metadata(self, *args, **kwargs):
        return None  # inert stub
    def __enter__(self, *args, **kwargs):
        return None  # inert stub
    def __exit__(self, *args, **kwargs):
        return None  # inert stub
    @property
    def pdf_header(self):
        return None  # inert stub
    def pdf_header(self, *args, **kwargs):
        return None  # inert stub
    def _add_object(self, *args, **kwargs):
        return None  # inert stub
    def get_object(self, *args, **kwargs):
        return None  # inert stub
    def _replace_object(self, *args, **kwargs):
        return None  # inert stub
    def _add_page(self, *args, **kwargs):
        return None  # inert stub
    def set_need_appearances_writer(self, *args, **kwargs):
        return None  # inert stub
    def create_viewer_preferences(self, *args, **kwargs):
        return None  # inert stub
    def add_page(self, *args, **kwargs):
        return None  # inert stub
    def insert_page(self, *args, **kwargs):
        return None  # inert stub
    def _get_page_number_by_indirect(self, *args, **kwargs):
        return None  # inert stub
    def add_blank_page(self, *args, **kwargs):
        return None  # inert stub
    def insert_blank_page(self, *args, **kwargs):
        return None  # inert stub
    @property
    def open_destination(self):
        return None  # inert stub
    def open_destination(self, *args, **kwargs):
        return None  # inert stub
    def add_js(self, *args, **kwargs):
        return None  # inert stub
    def add_open_action(self, *args, **kwargs):
        return None  # inert stub
    def add_attachment(self, *args, **kwargs):
        return None  # inert stub
    def append_pages_from_reader(self, *args, **kwargs):
        return None  # inert stub
    def _merge_content_stream_to_page(self, *args, **kwargs):
        return None  # inert stub
    def _add_apstream_object(self, *args, **kwargs):
        return None  # inert stub
    FFBITS_NUL = None
    def update_page_form_field_values(self, *args, **kwargs):
        return None  # inert stub
    def reattach_fields(self, *args, **kwargs):
        return None  # inert stub
    def _collect_incremental_clone_object_ids(self, *args, **kwargs):
        return None  # inert stub
    def clone_reader_document_root(self, *args, **kwargs):
        return None  # inert stub
    def clone_document_from_reader(self, *args, **kwargs):
        return None  # inert stub
    def generate_file_identifiers(self, *args, **kwargs):
        return None  # inert stub
    def encrypt(self, *args, **kwargs):
        return None  # inert stub
    def _resolve_links(self, *args, **kwargs):
        return None  # inert stub
    def write_stream(self, *args, **kwargs):
        return None  # inert stub
    def write(self, *args, **kwargs):
        return None  # inert stub
    def list_objects_in_increment(self, *args, **kwargs):
        return None  # inert stub
    def _write_increment(self, *args, **kwargs):
        return None  # inert stub
    def _write_pdf_structure(self, *args, **kwargs):
        return None  # inert stub
    def _write_xref_table(self, *args, **kwargs):
        return None  # inert stub
    def _write_trailer(self, *args, **kwargs):
        return None  # inert stub
    @property
    def metadata(self):
        return None  # inert stub
    def metadata(self, *args, **kwargs):
        return None  # inert stub
    def add_metadata(self, *args, **kwargs):
        return None  # inert stub
    _UNSET = None
    def compress_identical_objects(self, *args, **kwargs):
        return None  # inert stub
    def get_reference(self, *args, **kwargs):
        return None  # inert stub
    def get_outline_root(self, *args, **kwargs):
        return None  # inert stub
    def get_threads_root(self, *args, **kwargs):
        return None  # inert stub
    @property
    def threads(self):
        return None  # inert stub
    def add_outline_item_destination(self, *args, **kwargs):
        return None  # inert stub
    def add_outline_item_dict(self, *args, **kwargs):
        return None  # inert stub
    def add_outline_item(self, *args, **kwargs):
        return None  # inert stub
    def add_outline(self, *args, **kwargs):
        return None  # inert stub
    def add_named_destination_array(self, *args, **kwargs):
        return None  # inert stub
    def add_named_destination_object(self, *args, **kwargs):
        return None  # inert stub
    def _get_page_reference(self, *args, **kwargs):
        return None  # inert stub
    def add_named_destination(self, *args, **kwargs):
        return None  # inert stub
    def remove_links(self, *args, **kwargs):
        return None  # inert stub
    def remove_annotations(self, *args, **kwargs):
        return None  # inert stub
    def _remove_annots_from_page(self, *args, **kwargs):
        return None  # inert stub
    def remove_objects_from_page(self, *args, **kwargs):
        return None  # inert stub
    def _remove_objects_from_page__clean(self, *args, **kwargs):
        return None  # inert stub
    def _remove_objects_from_page__clean_forms(self, *args, **kwargs):
        return None  # inert stub
    def remove_images(self, *args, **kwargs):
        return None  # inert stub
    def remove_text(self, *args, **kwargs):
        return None  # inert stub
    def add_uri(self, *args, **kwargs):
        return None  # inert stub
    _valid_layouts = ()
    def _get_page_layout(self, *args, **kwargs):
        return None  # inert stub
    def _set_page_layout(self, *args, **kwargs):
        return None  # inert stub
    def set_page_layout(self, *args, **kwargs):
        return None  # inert stub
    @property
    def page_layout(self):
        return None  # inert stub
    def page_layout(self, *args, **kwargs):
        return None  # inert stub
    _valid_modes = ()
    def _get_page_mode(self, *args, **kwargs):
        return None  # inert stub
    @property
    def page_mode(self):
        return None  # inert stub
    def page_mode(self, *args, **kwargs):
        return None  # inert stub
    def add_annotation(self, *args, **kwargs):
        return None  # inert stub
    def clean_page(self, *args, **kwargs):
        return None  # inert stub
    def _create_stream(self, *args, **kwargs):
        return None  # inert stub
    def append(self, *args, **kwargs):
        return None  # inert stub
    def merge(self, *args, **kwargs):
        return None  # inert stub
    def _merge__process_named_dests(self, *args, **kwargs):
        return None  # inert stub
    def _add_named_destination_array_indexed(self, *args, **kwargs):
        return None  # inert stub
    def _add_articles_thread(self, *args, **kwargs):
        return None  # inert stub
    def add_filtered_articles(self, *args, **kwargs):
        return None  # inert stub
    def _get_cloned_page(self, *args, **kwargs):
        return None  # inert stub
    def _insert_filtered_annotations(self, *args, **kwargs):
        return None  # inert stub
    def _clone_widget_annotation(self, *args, **kwargs):
        return None  # inert stub
    def _set_cloned_kids(self, *args, **kwargs):
        return None  # inert stub
    def _get_filtered_outline(self, *args, **kwargs):
        return None  # inert stub
    def _clone_outline(self, *args, **kwargs):
        return None  # inert stub
    def _insert_filtered_outline(self, *args, **kwargs):
        return None  # inert stub
    def close(self, *args, **kwargs):
        return None  # inert stub
    def find_outline_item(self, *args, **kwargs):
        return None  # inert stub
    def reset_translation(self, *args, **kwargs):
        return None  # inert stub
    def set_page_label(self, *args, **kwargs):
        return None  # inert stub
    def _set_page_label(self, *args, **kwargs):
        return None  # inert stub
    def _repr_mimebundle_(self, *args, **kwargs):
        return None  # inert stub
def _pdf_objectify(*args, **kwargs):
    return None  # inert stub: pypdf._writer._pdf_objectify
def _create_outline_item(*args, **kwargs):
    return None  # inert stub: pypdf._writer._create_outline_item
