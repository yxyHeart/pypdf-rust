"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._page``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import math
from collections.abc import Callable, Iterable, Iterator, Sequence
from copy import deepcopy
from dataclasses import asdict, dataclass
from decimal import Decimal
from io import BytesIO
from pathlib import Path
from typing import Any, Literal, NoReturn, cast, overload
from ._configuration import get_configuration
from ._protocols import PdfCommonDocProtocol
from ._text_extraction import _layout_mode
from ._text_extraction._text_extractor import TextExtraction
from ._utils import CompressedTransformationMatrix, TransformationMatrixType, _human_readable_bytes, _TraversalState, deprecation, deprecation_no_replacement, deprecation_with_replacement, logger_warning, matrix_multiply
from .actions import Action, PageTrigger
from .constants import _INLINE_IMAGE_KEY_MAPPING, _INLINE_IMAGE_VALUE_MAPPING, AnnotationDictionaryAttributes, ImageAttributes, PageAttributes, Resources
from .errors import PageSizeNotDefinedError, PdfReadError
from .generic import ArrayObject, ContentStream, DictionaryObject, EncodedStreamObject, FloatObject, IndirectObject, NameObject, NullObject, NumberObject, PdfObject, RectangleObject, StreamObject, is_null_or_none
from .generic._font import Font
try:
    from PIL.Image import Image
    pil_not_imported = False
except ImportError:
    Image = None
    pil_not_imported = True
MERGE_CROP_BOX = 'cropbox'
def _get_page_resources(*args, **kwargs):
    return None  # inert stub: pypdf._page._get_page_resources
def _get_font_resources(*args, **kwargs):
    return None  # inert stub: pypdf._page._get_font_resources
def _get_rectangle(*args, **kwargs):
    return None  # inert stub: pypdf._page._get_rectangle
def _set_rectangle(*args, **kwargs):
    return None  # inert stub: pypdf._page._set_rectangle
def _delete_rectangle(*args, **kwargs):
    return None  # inert stub: pypdf._page._delete_rectangle
def _create_rectangle_accessor(*args, **kwargs):
    return None  # inert stub: pypdf._page._create_rectangle_accessor
class Transformation:
    """Stub for pypdf._page.Transformation."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    @property
    def matrix(self):
        return None  # inert stub
    @staticmethod
    def compress(*args, **kwargs):
        return None  # inert stub
    def _to_cm(self, *args, **kwargs):
        return None  # inert stub
    def transform(self, *args, **kwargs):
        return None  # inert stub
    def translate(self, *args, **kwargs):
        return None  # inert stub
    def scale(self, *args, **kwargs):
        return None  # inert stub
    def rotate(self, *args, **kwargs):
        return None  # inert stub
    def __repr__(self, *args, **kwargs):
        return ""  # inert stub
    def apply_on(self, *args, **kwargs):
        return None  # inert stub
    def apply_on(self, *args, **kwargs):
        return None  # inert stub
    def apply_on(self, *args, **kwargs):
        return None  # inert stub
class ImageFile:
    """Stub for pypdf._page.ImageFile."""
    name = None
    data = None
    image = None
    indirect_reference = None
    is_inline = None
    is_displayed = None
    def replace(self, *args, **kwargs):
        return None  # inert stub
    def __str__(self, *args, **kwargs):
        return ""  # inert stub
    def __repr__(self, *args, **kwargs):
        return ""  # inert stub
class VirtualListImages(Sequence):
    """Stub for pypdf._page.VirtualListImages."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def __len__(self, *args, **kwargs):
        return None  # inert stub
    def keys(self, *args, **kwargs):
        return None  # inert stub
    def items(self, *args, **kwargs):
        return None  # inert stub
    def __getitem__(self, *args, **kwargs):
        return None  # inert stub
    def __getitem__(self, *args, **kwargs):
        return None  # inert stub
    def __getitem__(self, *args, **kwargs):
        return None  # inert stub
    def __iter__(self, *args, **kwargs):
        return None  # inert stub
    def __str__(self, *args, **kwargs):
        return ""  # inert stub
class PageObject(DictionaryObject):
    """Stub for pypdf._page.PageObject."""
    original_page = None
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def hash_bin(self, *args, **kwargs):
        return None  # inert stub
    def hash_value_data(self, *args, **kwargs):
        return None  # inert stub
    @property
    def user_unit(self):
        return None  # inert stub
    @staticmethod
    def create_blank_page(*args, **kwargs):
        return None  # inert stub
    def _get_ids_image(self, *args, **kwargs):
        return None  # inert stub
    def _get_image(self, *args, **kwargs):
        return None  # inert stub
    @property
    def images(self):
        return None  # inert stub
    @property
    def inline_images(self):
        return None  # inert stub
    def inline_images(self, *args, **kwargs):
        return None  # inert stub
    def _translate_value_inline_image(self, *args, **kwargs):
        return None  # inert stub
    def _parse_images_from_content_stream(self, *args, **kwargs):
        return None  # inert stub
    @property
    def rotation(self):
        return None  # inert stub
    def rotation(self, *args, **kwargs):
        return None  # inert stub
    def transfer_rotation_to_content(self, *args, **kwargs):
        return None  # inert stub
    def rotate(self, *args, **kwargs):
        return None  # inert stub
    def _merge_resources(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _content_stream_rename(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _add_transformation_matrix(*args, **kwargs):
        return None  # inert stub
    def _get_contents_as_bytes(self, *args, **kwargs):
        return None  # inert stub
    def get_contents(self, *args, **kwargs):
        return None  # inert stub
    def replace_contents(self, *args, **kwargs):
        return None  # inert stub
    def merge_page(self, *args, **kwargs):
        return None  # inert stub
    def _merge_page(self, *args, **kwargs):
        return None  # inert stub
    def _merge_page_writer(self, *args, **kwargs):
        return None  # inert stub
    def _expand_mediabox(self, *args, **kwargs):
        return None  # inert stub
    def merge_transformed_page(self, *args, **kwargs):
        return None  # inert stub
    def merge_scaled_page(self, *args, **kwargs):
        return None  # inert stub
    def merge_rotated_page(self, *args, **kwargs):
        return None  # inert stub
    def merge_translated_page(self, *args, **kwargs):
        return None  # inert stub
    def add_transformation(self, *args, **kwargs):
        return None  # inert stub
    def scale(self, *args, **kwargs):
        return None  # inert stub
    def scale_by(self, *args, **kwargs):
        return None  # inert stub
    def scale_to(self, *args, **kwargs):
        return None  # inert stub
    def compress_content_streams(self, *args, **kwargs):
        return None  # inert stub
    @property
    def page_number(self):
        return None  # inert stub
    def _debug_for_extract(self, *args, **kwargs):
        return None  # inert stub
    def _extract_text(self, *args, **kwargs):
        return None  # inert stub
    def _extract_text__xform(self, *args, **kwargs):
        return None  # inert stub
    def _layout_mode_fonts(self, *args, **kwargs):
        return None  # inert stub
    def _layout_mode_text(self, *args, **kwargs):
        return None  # inert stub
    def extract_text(self, *args, **kwargs):
        return None  # inert stub
    def extract_xform_text(self, *args, **kwargs):
        return None  # inert stub
    def _get_fonts(self, *args, **kwargs):
        return None  # inert stub
    mediabox = None
    cropbox = None
    bleedbox = None
    trimbox = None
    artbox = None
    @property
    def annotations(self):
        return None  # inert stub
    def annotations(self, *args, **kwargs):
        return None  # inert stub
    def add_action(self, *args, **kwargs):
        return None  # inert stub
    def delete_action(self, *args, **kwargs):
        return None  # inert stub
class _VirtualList(Sequence):
    """Stub for pypdf._page._VirtualList."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def __len__(self, *args, **kwargs):
        return None  # inert stub
    def __getitem__(self, *args, **kwargs):
        return None  # inert stub
    def __getitem__(self, *args, **kwargs):
        return None  # inert stub
    def __getitem__(self, *args, **kwargs):
        return None  # inert stub
    def __delitem__(self, *args, **kwargs):
        return None  # inert stub
    def __iter__(self, *args, **kwargs):
        return None  # inert stub
    def __str__(self, *args, **kwargs):
        return ""  # inert stub
def _get_fonts_walk(*args, **kwargs):
    return None  # inert stub: pypdf._page._get_fonts_walk
