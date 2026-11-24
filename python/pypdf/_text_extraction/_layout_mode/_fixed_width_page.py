"""Extract PDF text preserving the layout of the source PDF

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._text_extraction._layout_mode._fixed_width_page``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

from collections.abc import Iterator
from itertools import groupby
from math import ceil
from pathlib import Path
from typing import Any, Literal, TypedDict
from ..._utils import logger_warning
from ...generic import ArrayObject
from ...generic._font import Font
from .. import LAYOUT_NEW_BT_GROUP_SPACE_WIDTHS
from ._text_state_manager import TextStateManager
from ._text_state_params import TextStateParams
WHITESPACE_LIMIT = 10000
NEWLINE_LIMIT = 1000
class BTGroup:
    """Stub for pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup."""
    tx = None
    ty = None
    font_size = None
    font_height = None
    text = None
    displaced_tx = None
    flip_sort = None
def set_font_operation(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction._layout_mode._fixed_width_page.set_font_operation
def resolve_font(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction._layout_mode._fixed_width_page.resolve_font
def bt_group(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction._layout_mode._fixed_width_page.bt_group
def _show_text_operand(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction._layout_mode._fixed_width_page._show_text_operand
def recurse_to_target_op(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction._layout_mode._fixed_width_page.recurse_to_target_op
def y_coordinate_groups(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction._layout_mode._fixed_width_page.y_coordinate_groups
def text_show_operations(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction._layout_mode._fixed_width_page.text_show_operations
def fixed_char_width(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction._layout_mode._fixed_width_page.fixed_char_width
def fixed_width_page(*args, **kwargs):
    return None  # inert stub: pypdf._text_extraction._layout_mode._fixed_width_page.fixed_width_page
