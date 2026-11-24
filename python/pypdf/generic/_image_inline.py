"""Reading of inline images (``BI`` ... ``ID`` ... ``EI``) from content streams.

Rust-backed port of ``pypdf/generic/_image_inline.py`` (upstream pypdf 7.1.1).
The module-level constant and the function definitions with their exact
signatures stay here; every function body delegates to
``crates/pypdf-py/src/modules/filesimages_mod.rs`` through the
``pypdf._pypdf`` extension module.

The functions are looked up dynamically inside the Rust implementation (via
this module's globals), so monkeypatching e.g. ``extract_inline_default`` keeps
working exactly like upstream.
"""

from __future__ import annotations

import logging
from io import BytesIO
from math import ceil
from typing import IO, Any

from pypdf._pypdf import (
    imageinline_check_end_image_marker as _check_end_image_marker_impl,
    imageinline_extract_ascii85_decode as _extract_ascii85_decode_impl,
    imageinline_extract_ascii_hex_decode as _extract_ascii_hex_decode_impl,
    imageinline_extract_dct_decode as _extract_dct_decode_impl,
    imageinline_extract_default as _extract_default_impl,
    imageinline_extract_not_set as _extract_not_set_impl,
    imageinline_extract_run_length_decode as _extract_run_length_decode_impl,
    imageinline_handle_filters as _handle_filters_impl,
    imageinline_is_followed_by_binary_data as _is_followed_by_binary_data_impl,
    imageinline_is_followed_by_closing_operator as _is_followed_by_closing_operator_impl,
    imageinline_is_preceded_by_whitespace as _is_preceded_by_whitespace_impl,
    imageinline_read_inline_image as _read_inline_image_impl,
    imageinline_read_settings_dictionary as _read_settings_dictionary_impl,
)

from .._configuration import get_configuration
from .._utils import (
    WHITESPACES,
    WHITESPACES_AS_BYTES,
    BinaryStreamType,
    StreamType,
    logger_warning,
    read_non_whitespace,
)
from ..errors import LimitReachedError, PdfReadError, PdfStreamError
from ._data_structures import ContentStream, DictionaryObject, read_object
from ._utils import StreamParsingBudget

logger = logging.getLogger(__name__)

# An inline image should be used only for small images (4096 bytes or less),
# but allow twice this for cases where this has been exceeded.
BUFFER_SIZE = 8192


def _check_end_image_marker(stream: StreamType) -> bool:
    return _check_end_image_marker_impl(stream)


def extract_inline__ascii_hex_decode(stream: StreamType) -> bytes:
    """
    Extract HexEncoded stream from inline image.
    The stream will be moved onto the EI.
    """
    return _extract_ascii_hex_decode_impl(stream)


def extract_inline__ascii85_decode(stream: StreamType) -> bytes:
    """
    Extract A85 stream from inline image.
    The stream will be moved onto the EI.
    """
    return _extract_ascii85_decode_impl(stream)


def extract_inline__run_length_decode(stream: StreamType) -> bytes:
    """
    Extract RL (RunLengthDecode) stream from inline image.
    The stream will be moved onto the EI.
    """
    return _extract_run_length_decode_impl(stream)


def extract_inline__dct_decode(stream: BinaryStreamType) -> bytes:
    """
    Extract DCT (JPEG) stream from inline image.
    The stream will be moved onto the EI.
    """
    return _extract_dct_decode_impl(stream)


def _is_preceded_by_whitespace(stream_out: BytesIO, position: int) -> bool:
    """
    Check whether the byte in front of `position` is a whitespace.

    Returns:
        Whether the preceding byte is whitespace. If no preceding byte exists,
        returns ``False`` because the ``EI`` marker is at the beginning of the
        image data and is therefore not delimited by anything.
    """
    return _is_preceded_by_whitespace_impl(stream_out, position)


def _is_followed_by_closing_operator(stream: StreamType, first_byte: bytes) -> bool:
    """
    Check whether `first_byte` starts an exact `Q` or `EMC` operator.

    `first_byte` has already been consumed from the stream, which is restored to its
    current position afterwards. Looking at this byte alone is not sufficient, as
    binary image data can easily be mistaken for one of these operators.
    """
    return _is_followed_by_closing_operator_impl(stream, first_byte)


def extract_inline_default(stream: StreamType) -> bytes:
    """Legacy method, used by default"""
    return _extract_default_impl(stream)


def is_followed_by_binary_data(stream: IO[bytes], length: int = 10) -> bool:
    """
    Check if the next bytes of the stream look like binary image data or regular page content.

    This is just some heuristics due to the PDF specification being too imprecise about
    inline images containing the `EI` marker which would end an image. Starting with PDF 2.0,
    we finally get a mandatory length field, but with (proper) PDF 2.0 support being very limited
    everywhere, we should not expect to be able to remove such hacks in the near future - especially
    considering legacy documents as well.

    The actual implementation draws some inspiration from
    https://github.com/itext/itext-java/blob/9.1.0/kernel/src/main/java/com/itextpdf/kernel/pdf/canvas/parser/util/InlineImageParsingUtils.java
    """
    return _is_followed_by_binary_data_impl(stream, length)


def _read_settings_dictionary(
        *,
        content_stream: ContentStream,
        stream: StreamType,
        parsing_budget: StreamParsingBudget,
        remaining_count: int | None
) -> DictionaryObject:
    return _read_settings_dictionary_impl(
        content_stream=content_stream,
        stream=stream,
        parsing_budget=parsing_budget,
        remaining_count=remaining_count,
    )


def extract_inline__not_set(
        *,
        content_stream: ContentStream,
        saved_position: int,
        settings: DictionaryObject,
        stream: StreamType
) -> bytes:
    return _extract_not_set_impl(
        content_stream=content_stream,
        saved_position=saved_position,
        settings=settings,
        stream=stream,
    )


def _handle_filters(
        *,
        filters: Any,
        stream: StreamType,
        settings: DictionaryObject,
        content_stream: ContentStream,
        saved_position: int
) -> bytes:
    return _handle_filters_impl(
        filters=filters,
        stream=stream,
        settings=settings,
        content_stream=content_stream,
        saved_position=saved_position,
    )


def read_inline_image(
        *,
        content_stream: ContentStream,
        stream: StreamType,
        parsing_budget: StreamParsingBudget
) -> dict[str, DictionaryObject | bytes]:
    """Begin reading the inline image just after the "BI" - begin image."""
    return _read_inline_image_impl(
        content_stream=content_stream,
        stream=stream,
        parsing_budget=parsing_budget,
    )
