"""Functions to convert an image XObject to an image

Rust-backed port of ``pypdf/generic/_image_xobject.py`` (upstream pypdf 7.1.1).
The module constants/type aliases stay Python definitions and every function
body delegates to ``crates/pypdf-py/src/modules/filesimages_mod.rs`` through the
``pypdf._pypdf`` extension module.

All Pillow work (``Image.open``, ``frombytes``, ``convert``, ``resize``,
``merge``, ``point``, ``putpalette``, ``putalpha``, ``save``) is executed in
Rust through PyO3 against the installed Pillow, with the same arguments and
argument order as upstream, so image bytes match by construction (dependency
parity; see docs/FIDELITY_CONTRACT.md section 5.1).
"""

from __future__ import annotations

from collections.abc import Sequence
from io import BytesIO
from typing import IO, Any, Literal, TypeAlias, cast

from pypdf._pypdf import (
    imagexobject_apply_alpha as _apply_alpha_impl,
    imagexobject_apply_decode as _apply_decode_impl,
    imagexobject_bits2byte as _bits2byte_impl,
    imagexobject_convert_image as _convert_image_impl,
    imagexobject_create_image_from_buffer as _create_image_from_buffer_impl,
    imagexobject_create_image_from_bytes as _create_image_from_bytes_impl,
    imagexobject_expand_low_bit_samples as _expand_low_bit_samples_impl,
    imagexobject_get_allocated_bytes_per_pixel as _get_allocated_bytes_per_pixel_impl,
    imagexobject_get_image_mode as _get_image_mode_impl,
    imagexobject_get_mode_and_invert_color as _get_mode_and_invert_color_impl,
    imagexobject_get_process_colorant_channels as _get_process_colorant_channels_impl,
    imagexobject_handle_flate as _handle_flate_impl,
    imagexobject_handle_flate__indexed as _handle_flate_indexed_impl,
    imagexobject_handle_jpx as _handle_jpx_impl,
    imagexobject_merge_image as _merge_image_impl,
    imagexobject_resize_image as _resize_image_impl,
    imagexobject_validate_image_size as _validate_image_size_impl,
    imagexobject_xobj_to_image as _xobj_to_image_impl,
)

from .._configuration import get_configuration
from .._utils import check_if_whitespace_only, logger_warning
from ..constants import ColorSpaces, FilterTypes, ImageAttributes, StreamAttributes
from ..errors import EmptyImageDataError, LimitReachedError, PdfReadError
from ..generic import (
    ArrayObject,
    DecodedStreamObject,
    EncodedStreamObject,
    NullObject,
    TextStringObject,
    is_null_or_none,
)

try:
    from PIL import Image, UnidentifiedImageError
except ImportError as exception:
    raise ImportError(
        "pillow is required to do image extraction. "
        "It can be installed via 'pip install pypdf[image]'"
    ) from exception

ImageMode: TypeAlias = Literal[
    "", "1", "RGB", "2bits", "4bits", "P", "L", "RGBA", "CMYK"
]

MAX_IMAGE_MODE_NESTING_DEPTH: int = 10

_PROCESS_COLORANTS = ("/Cyan", "/Magenta", "/Yellow", "/Black")


# Bytes per pixel Pillow allocates for a single-band image, keyed by mode.
# Pillow keeps one sample of "1", "L" and "P" in one byte, the 16-bit family in
# two and "I"/"F" in four. Image.getmodetype() cannot tell them apart: it
# reports "L" for "1", "L", "P" and the whole 16-bit family.
_SINGLE_BAND_BYTES_PER_PIXEL: dict[str, int] = {
    "1": 1,
    "L": 1,
    "P": 1,
    "I;16": 2,
    "I;16B": 2,
    "I;16L": 2,
    "I;16N": 2,
    "I": 4,
    "F": 4,
}


def _get_process_colorant_channels(color_space: Any) -> list[int] | None:
    """
    Return the CMYK channel of each colorant of a /DeviceN space.

    Returns None if the space is something else or uses a colorant which is not a process colorant.
    """
    return _get_process_colorant_channels_impl(color_space)


def _get_image_mode(
    color_space: str | list[Any] | Any,
    color_components: int,
    prev_mode: ImageMode,
    depth: int = 0,
) -> tuple[ImageMode, bool]:
    """
    Returns:
        Image mode, not taking into account mask (transparency).
        ColorInversion is required (like for some DeviceCMYK).

    """
    return _get_image_mode_impl(color_space, color_components, prev_mode, depth)


def bits2byte(
    data: bytes,
    size: tuple[int, int],
    bits: int,
    colors: int = 1,
    scale: bool = False,
) -> bytes:
    return _bits2byte_impl(data, size, bits, colors, scale)


def _expand_low_bit_samples(
    mode: ImageMode,
    size: tuple[int, int],
    data: bytes,
    color_space: str | ArrayObject,
) -> tuple[ImageMode, bytes]:
    """
    Expand 2- or 4-bit-per-component samples to one byte per component.

    Pillow has no mode for sub-byte samples, so the packed data has to be
    unpacked before an image can be built from it. A single-component
    /DeviceGray image holds grayscale samples, scaled to the full 0-255 range
    as an "L" image. A multi-component space such as /DeviceRGB has interleaved
    color components, also scaled and combined into an "RGB" image. An indexed
    color space keeps its raw sample values as palette indices ("P").

    Returns the resulting mode and data, unchanged when the mode is not a
    low-bit one.
    """
    return _expand_low_bit_samples_impl(mode, size, data, color_space)


def _get_allocated_bytes_per_pixel(mode: str) -> int:
    """Bytes Pillow allocates per pixel for a decoded image in the given mode.

    This is the size of the internal pixel, not the size of the raw data: every
    mode with two or more bands is stored in a four-byte pixel, so "RGB",
    "YCbCr", "LAB", "HSV", "LA", "La" and "PA" all take four bytes although
    they have three or two bands. The length of the mode name says nothing
    about it either: "I" and "F" need four bytes per pixel and the 16-bit modes
    two.
    """
    return _get_allocated_bytes_per_pixel_impl(mode)


def _validate_image_size(*, mode: str, size: tuple[int, int]) -> tuple[int, int]:
    return _validate_image_size_impl(mode=mode, size=size)


def _create_image_from_bytes(
    mode: str, size: tuple[int, int], data: bytes
) -> Image.Image:
    return _create_image_from_bytes_impl(mode, size, data)


def _create_image_from_buffer(*, buf: IO[bytes], formats: tuple[str, ...] | None = None) -> Image.Image:
    return _create_image_from_buffer_impl(buf=buf, formats=formats)


def _convert_image(*, image: Image.Image, mode: str) -> Image.Image:
    return _convert_image_impl(image=image, mode=mode)


def _resize_image(*, image: Image.Image, size: tuple[int, int]) -> Image.Image:
    return _resize_image_impl(image=image, size=size)


def _merge_image(*, mode: str, bands: Sequence[Image.Image]) -> Image.Image:
    return _merge_image_impl(mode=mode, bands=bands)


def __handle_flate__indexed(color_space: ArrayObject) -> tuple[Any, Any, Any, Any]:
    return _handle_flate_indexed_impl(color_space)


def _handle_flate(
    size: tuple[int, int],
    data: bytes,
    mode: ImageMode,
    color_space: str | ArrayObject,
    colors: int,
    obj_as_text: str,
) -> tuple[Image.Image, str, str, bool]:
    """
    Process image encoded using the zlib/deflate compression method, corresponds to the FlateDecode filter
    Returns img, image_format, extension, color inversion
    """
    return _handle_flate_impl(size, data, mode, color_space, colors, obj_as_text)


def _handle_jpx(
    size: tuple[int, int],
    data: bytes,
    mode: ImageMode,
    color_space: str,
    colors: int,
) -> tuple[Image.Image, str, str, bool]:
    """
    Process image encoded as JPX/JPEG2000
    Returns img, image_format, extension, inversion
    """
    return _handle_jpx_impl(size, data, mode, color_space, colors)


def _apply_decode(
    img: Image.Image,
    x_object: dict[str, Any],
    lfilters: FilterTypes,
    color_space: str | list[Any] | Any,
    invert_color: bool,
) -> Image.Image:
    return _apply_decode_impl(img, x_object, lfilters, color_space, invert_color)


def _get_mode_and_invert_color(
    x_object: dict[str, Any], colors: int, color_space: str | list[Any] | Any
) -> tuple[ImageMode, bool]:
    return _get_mode_and_invert_color_impl(x_object, colors, color_space)


def _apply_alpha(
    *,
    img: Image.Image,
    x_object: dict[str, Any],
    obj_as_text: str,
    image_format: str,
    extension: str,
    visited: set[int],
) -> tuple[Image.Image, str, str]:
    return _apply_alpha_impl(
        img=img,
        x_object=x_object,
        obj_as_text=obj_as_text,
        image_format=image_format,
        extension=extension,
        visited=visited,
    )


def _xobj_to_image(
        x_object: dict[str, Any],
        pillow_parameters: dict[str, Any] | None = None,
        visited: set[int] | None = None,
) -> tuple[str | None, bytes, Any]:
    """
    Args:
        x_object:
        pillow_parameters: parameters provided to Pillow Image.save() method,
            cf. <https://pillow.readthedocs.io/en/stable/reference/Image.html#PIL.Image.Image.save>
        visited: Set of id() values of XObjects already being converted higher
            up the /SMask chain, used to detect cyclic soft masks.

    Returns:
        Tuple[file extension, bytes, PIL.Image.Image]

    """
    return _xobj_to_image_impl(x_object, pillow_parameters, visited)

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
