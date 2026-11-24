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

"""Rust port of ``pypdf/_text_extraction/_text_extractor.py`` (upstream pypdf 7.1.1).

``TextExtraction`` stays a Python class because it is instantiated by
``pypdf._page`` and its state is plain Python attributes that upstream code (and
the ``visitor_*`` hooks) observe; every method body delegates to a
``#[pyfunction]`` in ``crates/pypdf-py/src/modules/text_extraction_mod``.
"""

from collections.abc import Callable
from typing import Any

from pypdf._pypdf import (
    text_extractor_compute_str_widths as _compute_str_widths,
    text_extractor_flush_text as _flush_text,
    text_extractor_get_number_operand as _get_number_operand_impl,
    text_extractor_handle_bt as _handle_bt,
    text_extractor_handle_cm as _handle_cm,
    text_extractor_handle_et as _handle_et,
    text_extractor_handle_restore_graphics_state as _handle_restore_graphics_state,
    text_extractor_handle_save_graphics_state as _handle_save_graphics_state,
    text_extractor_handle_t_star as _handle_t_star,
    text_extractor_handle_tc as _handle_tc,
    text_extractor_handle_td as _handle_td,
    text_extractor_handle_tf as _handle_tf,
    text_extractor_handle_tj as _handle_tj,
    text_extractor_handle_tj_operation as _handle_tj_operation,
    text_extractor_handle_tl as _handle_tl,
    text_extractor_handle_tm as _handle_tm,
    text_extractor_handle_tw as _handle_tw,
    text_extractor_handle_tz as _handle_tz,
    text_extractor_init as _init,
    text_extractor_initialize_extraction as _initialize_extraction,
    text_extractor_post_process_text_operation as _post_process_text_operation,
    text_extractor_process_operation as _process_operation,
    text_extractor_process_tj_offset as _process_tj_offset,
)

from .._codecs import encoding_dict_from_named_encoding
from ..generic import DictionaryObject, TextStringObject
from ..generic._font import Font, FontDescriptor
from . import OrientationNotFoundError, crlf_space_check, get_display_str, get_text_operands, mult


def _get_number_operand(operands: list[Any], index: int, default: float) -> float:
    """Return the operand at index as a float, or default if it is missing or not a number."""
    return _get_number_operand_impl(operands, index, default)


class TextExtraction:
    """
    A class to handle PDF text extraction operations.

    This class encapsulates all the state and operations needed for extracting
    text from PDF content streams, replacing the nested functions and nonlocal
    variables in the original implementation.
    """

    def __init__(self) -> None:
        # Text extraction state variables
        _init(self)

    def initialize_extraction(
        self,
        orientations: tuple[int, ...] = (0, 90, 180, 270),
        visitor_text: Callable[[Any, Any, Any, Any, Any], None] | None = None,
        font_resources: dict[str, DictionaryObject] | None = None,
        fonts: dict[str, Font] | None = None
    ) -> None:
        """Initialize the extractor with extraction parameters."""
        return _initialize_extraction(self, orientations, visitor_text, font_resources, fonts)

    def compute_str_widths(self, str_widths: float) -> float:
        return _compute_str_widths(self, str_widths)

    def process_operation(self, operator: bytes, operands: list[Any]) -> None:
        return _process_operation(self, operator, operands)

    def process_tj_offset(self, offset: float, space_width: float) -> None:
        """Add a space for a word gap in a TJ array and count the offset in the text width."""
        return _process_tj_offset(self, offset, space_width)

    def _post_process_text_operation(self, str_widths: float) -> None:
        """Handle common post-processing for text positioning operations."""
        return _post_process_text_operation(self, str_widths)

    def _handle_tj(
        self,
        text: str,
        operands: list[str | TextStringObject],
        cm_matrix: list[float],
        tm_matrix: list[float],
        font_resource: DictionaryObject | None,
        font: Font,
        orientations: tuple[int, ...],
        font_size: float,
        rtl_dir: bool,
        visitor_text: Callable[[Any, Any, Any, Any, Any], None] | None,
        actual_str_size: dict[str, float],
    ) -> tuple[str, bool, dict[str, float]]:
        return _handle_tj(
            self,
            text,
            operands,
            cm_matrix,
            tm_matrix,
            font_resource,
            font,
            orientations,
            font_size,
            rtl_dir,
            visitor_text,
            actual_str_size,
        )

    def _flush_text(self) -> None:
        """Flush accumulated text to output and call visitor if present."""
        return _flush_text(self)

    # Operation handlers

    def _handle_bt(self, operands: list[Any]) -> None:
        """Handle BT (Begin Text) operation - Table 5.4 page 405."""
        return _handle_bt(self, operands)

    def _handle_et(self, operands: list[Any]) -> None:
        """Handle ET (End Text) operation - Table 5.4 page 405."""
        return _handle_et(self, operands)

    def _handle_save_graphics_state(self, operands: list[Any]) -> None:
        """Handle q (Save graphics state) operation - Table 4.7 page 219."""
        return _handle_save_graphics_state(self, operands)

    def _handle_restore_graphics_state(self, operands: list[Any]) -> None:
        """Handle Q (Restore graphics state) operation - Table 4.7 page 219."""
        return _handle_restore_graphics_state(self, operands)

    def _handle_cm(self, operands: list[Any]) -> None:
        """Handle cm (Modify current matrix) operation - Table 4.7 page 219."""
        return _handle_cm(self, operands)

    def _handle_tz(self, operands: list[Any]) -> None:
        """Handle Tz (Set horizontal text scaling) operation - Table 5.2 page 398."""
        return _handle_tz(self, operands)

    def _handle_tc(self, operands: list[Any]) -> None:
        """Handle Tc (Set character spacing) operation - Table 5.2 page 398."""
        return _handle_tc(self, operands)

    def _handle_tw(self, operands: list[Any]) -> None:
        """Handle Tw (Set word spacing) operation - Table 5.2 page 398."""
        return _handle_tw(self, operands)

    def _handle_tl(self, operands: list[Any]) -> None:
        """Handle TL (Set Text Leading) operation - Table 5.2 page 398."""
        return _handle_tl(self, operands)

    def _handle_tf(self, operands: list[Any]) -> None:
        """Handle Tf (Set font size) operation - Table 5.2 page 398."""
        return _handle_tf(self, operands)

    def _handle_td(self, operands: list[Any]) -> float:
        """Handle Td (Move text position) operation - Table 5.5 page 406."""
        return _handle_td(self, operands)

    def _handle_tm(self, operands: list[Any]) -> float:
        """Handle Tm (Set text matrix) operation - Table 5.5 page 406."""
        return _handle_tm(self, operands)

    def _handle_t_star(self, operands: list[Any]) -> float:
        """Handle T* (Move to next line) operation - Table 5.5 page 406."""
        return _handle_t_star(self, operands)

    def _handle_tj_operation(self, operands: list[Any]) -> float:
        """Handle Tj (Show text) operation - Table 5.5 page 406."""
        return _handle_tj_operation(self, operands)
