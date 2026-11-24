"""Extract PDF text preserving the layout of the source PDF

Rust port of ``pypdf/_text_extraction/_layout_mode/_fixed_width_page.py``
(upstream pypdf 7.1.1): the ``BTGroup`` TypedDict, the module constants and the
public signatures are kept verbatim; every function body delegates to
``crates/pypdf-py/src/modules/text_extraction_mod``.
"""

from collections.abc import Iterator
from itertools import groupby
from math import ceil
from pathlib import Path
from typing import Any, Literal, TypedDict

from pypdf._pypdf import (
    layout_bt_group as _bt_group,
    layout_fixed_char_width as _fixed_char_width,
    layout_fixed_width_page as _fixed_width_page,
    layout_recurse_to_target_op as _recurse_to_target_op,
    layout_resolve_font as _resolve_font,
    layout_set_font_operation as _set_font_operation,
    layout_show_text_operand as _show_text_operand_impl,
    layout_text_show_operations as _text_show_operations,
    layout_y_coordinate_groups as _y_coordinate_groups,
)

from ..._utils import logger_warning
from ...generic import ArrayObject
from ...generic._font import Font
from .. import LAYOUT_NEW_BT_GROUP_SPACE_WIDTHS
from ._text_state_manager import TextStateManager
from ._text_state_params import TextStateParams

WHITESPACE_LIMIT = 10_000
NEWLINE_LIMIT = 1_000


class BTGroup(TypedDict):
    """
    Dict describing a line of text rendered within a BT/ET operator pair.
    If multiple text show operations render text on the same line, the text
    will be combined into a single BTGroup dict.

    Keys:
        tx: x coordinate of first character in BTGroup
        ty: y coordinate of first character in BTGroup
        font_size: nominal font size
        font_height: effective font height
        text: rendered text
        displaced_tx: x coordinate of last character in BTGroup
        flip_sort: -1 if page is upside down, else 1
    """

    tx: float
    ty: float
    font_size: float
    font_height: float
    text: str
    displaced_tx: float
    flip_sort: Literal[-1, 1]


def set_font_operation(state_mgr: TextStateManager, fonts: dict[str, Font], operands: list[Any]) -> None:
    """
    Apply a Tf (Set font and size) operation to a text state manager.

    A missing font name resolves the same uninterpretable placeholder font as
    an unknown one (see resolve_font()). A missing or non-numeric size leaves
    the current size in place, matching the ROB handling of plain-mode Tf.

    Args:
        state_mgr: the text state manager to update
        fonts: font dictionary as returned by PageObject._layout_mode_fonts()
        operands: the Tf operator's operands: [name, size]

    """
    return _set_font_operation(state_mgr, fonts, operands)


def resolve_font(fonts: dict[str, Font], name: str) -> Font:
    """
    Resolve a Tf font name to a layout mode Font.

    A content stream may select a font name that is not declared in the page
    resources. Fall back to an uninterpretable font so extraction degrades to
    the existing incomplete-output path instead of raising KeyError.

    Args:
        fonts: font dictionary as returned by PageObject._layout_mode_fonts()
        name: font name supplied by a Tf operator

    Returns:
        Font: the matching font, or an uninterpretable placeholder font.

    """
    return _resolve_font(fonts, name)


def bt_group(tj_op: TextStateParams, rendered_text: str, displaced_tx: float) -> BTGroup:
    """
    BTGroup constructed from a TextStateParams instance, rendered text, and
    displaced tx value.

    Args:
        tj_op (TextStateParams): TextStateParams instance
        rendered_text (str): rendered text
        displaced_tx (float): x coordinate of last character in BTGroup

    """
    return _bt_group(tj_op, rendered_text, displaced_tx)


def _show_text_operand(
    operands: list[Any], index: int, text_state_mgr: TextStateManager, tj_ops: list[TextStateParams]
) -> None:
    """Show the operand at index like Tj if it is a string. A missing or non-string operand shows no text."""
    return _show_text_operand_impl(operands, index, text_state_mgr, tj_ops)


def recurse_to_target_op(
    ops: Iterator[tuple[list[Any], bytes]],
    text_state_mgr: TextStateManager,
    end_target: Literal[b"Q", b"ET"],
    fonts: dict[str, Font],
    strip_rotated: bool = True,
) -> tuple[list[BTGroup], list[TextStateParams]]:
    """
    Recurse operators between BT/ET and/or q/Q operators managing the transform
    stack and capturing text positioning and rendering data.

    Args:
        ops: iterator of operators in content stream
        text_state_mgr: a TextStateManager instance
        end_target: Either b"Q" (ends b"q" op) or b"ET" (ends b"BT" op)
        fonts: font dictionary as returned by PageObject._layout_mode_fonts()

    Returns:
        tuple: list of BTGroup dicts + list of TextStateParams dataclass instances.

    """
    return _recurse_to_target_op(ops, text_state_mgr, end_target, fonts, strip_rotated)


def y_coordinate_groups(
    bt_groups: list[BTGroup], debug_path: Path | None = None
) -> dict[int, list[BTGroup]]:
    """
    Group text operations by rendered y coordinate, i.e. the line number.

    Args:
        bt_groups: list of dicts as returned by text_show_operations()
        debug_path (Path, optional): Path to a directory for saving debug output.

    Returns:
        Dict[int, List[BTGroup]]: dict of lists of text rendered by each BT operator
            keyed by y coordinate

    """
    return _y_coordinate_groups(bt_groups, debug_path)


def text_show_operations(
    ops: Iterator[tuple[list[Any], bytes]],
    fonts: dict[str, Font],
    strip_rotated: bool = True,
    debug_path: Path | None = None,
) -> list[BTGroup]:
    """
    Extract text from BT/ET operator pairs.

    Args:
        ops (Iterator[Tuple[List, bytes]]): iterator of operators in content stream
        fonts (Dict[str, Font]): font dictionary
        strip_rotated: Removes text if rotated w.r.t. to the page. Defaults to True.
        debug_path (Path, optional): Path to a directory for saving debug output.

    Returns:
        List[BTGroup]: list of dicts of text rendered by each BT operator

    """
    return _text_show_operations(ops, fonts, strip_rotated, debug_path)


def fixed_char_width(bt_groups: list[BTGroup], scale_weight: float = 1.25) -> float:
    """
    Calculate average character width weighted by the length of the rendered
    text in each sample for conversion to fixed-width layout.

    Args:
        bt_groups (List[BTGroup]): List of dicts of text rendered by each
            BT operator

    Returns:
        float: fixed character width

    """
    return _fixed_char_width(bt_groups, scale_weight)


def fixed_width_page(
    ty_groups: dict[int, list[BTGroup]], char_width: float, space_vertically: bool, font_height_weight: float
) -> str:
    """
    Generate page text from text operations grouped by rendered y coordinate.

    Args:
        ty_groups: dict of text show ops as returned by y_coordinate_groups()
        char_width: fixed character width
        space_vertically: include blank lines inferred from y distance + font height.
        font_height_weight: multiplier for font height when calculating blank lines.

    Returns:
        str: page text in a fixed width format that closely adheres to the rendered
            layout in the source pdf.

    """
    return _fixed_width_page(ty_groups, char_width, space_vertically, font_height_weight)
