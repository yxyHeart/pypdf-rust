"""Functions to convert an image XObject to an image

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.generic._image_xobject``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

from collections.abc import Sequence
from io import BytesIO
from typing import IO, Any, Literal, TypeAlias, cast
from .._configuration import get_configuration
from .._utils import check_if_whitespace_only, logger_warning
from ..constants import ColorSpaces, FilterTypes, ImageAttributes, StreamAttributes
from ..errors import EmptyImageDataError, LimitReachedError, PdfReadError
from ..generic import ArrayObject, DecodedStreamObject, EncodedStreamObject, NullObject, TextStringObject, is_null_or_none
try:
    from PIL import Image, UnidentifiedImageError
except ImportError:
    pass
ImageMode = None
MAX_IMAGE_MODE_NESTING_DEPTH = None
_PROCESS_COLORANTS = ()
def _get_process_colorant_channels(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._get_process_colorant_channels
def _get_image_mode(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._get_image_mode
def bits2byte(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject.bits2byte
def _expand_low_bit_samples(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._expand_low_bit_samples
_SINGLE_BAND_BYTES_PER_PIXEL = None
def _get_allocated_bytes_per_pixel(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._get_allocated_bytes_per_pixel
def _validate_image_size(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._validate_image_size
def _create_image_from_bytes(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._create_image_from_bytes
def _create_image_from_buffer(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._create_image_from_buffer
def _convert_image(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._convert_image
def _resize_image(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._resize_image
def _merge_image(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._merge_image
def __handle_flate__indexed(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject.__handle_flate__indexed
def _handle_flate(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._handle_flate
def _handle_jpx(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._handle_jpx
def _apply_decode(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._apply_decode
def _get_mode_and_invert_color(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._get_mode_and_invert_color
def _apply_alpha(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._apply_alpha
def _xobj_to_image(*args, **kwargs):
    return None  # inert stub: pypdf.generic._image_xobject._xobj_to_image
