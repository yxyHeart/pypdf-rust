"""manage the PDF transform stack during "layout" mode text extraction

Rust port of ``pypdf/_text_extraction/_layout_mode/_text_state_manager.py``
(upstream pypdf 7.1.1): ``TextStateManager`` stays a Python class (its state —
including the ``ChainMap``/``Counter`` objects — is observed by tests and by the
layout-mode functions), every method body delegates to
``crates/pypdf-py/src/modules/text_extraction_mod``.
"""

from collections import ChainMap, Counter
from collections import ChainMap as ChainMapType
from collections import Counter as CounterType
from collections.abc import MutableMapping
from typing import Any, Literal

from pypdf._pypdf import (
    text_state_manager_add_cm as _add_cm,
    text_state_manager_add_q as _add_q,
    text_state_manager_add_tm as _add_tm,
    text_state_manager_add_trm as _add_trm,
    text_state_manager_as_float as _as_float_impl,
    text_state_manager_complete_matrix as _complete_matrix,
    text_state_manager_effective_transform as _effective_transform,
    text_state_manager_init as _init,
    text_state_manager_new_transform as _new_transform_impl,
    text_state_manager_raw_transform as _raw_transform_impl,
    text_state_manager_remove_q as _remove_q,
    text_state_manager_reset_tm as _reset_tm,
    text_state_manager_reset_trm as _reset_trm,
    text_state_manager_set_font as _set_font,
    text_state_manager_set_state_param as _set_state_param,
    text_state_manager_text_state_params as _text_state_params,
)

from ...errors import PdfReadError
from ...generic._font import Font
from .. import mult
from ._text_state_params import TextStateParams

TextStateManagerKeyType = int | Literal["is_text", "is_render"]
TextStateManagerChainMapType = ChainMapType[TextStateManagerKeyType, float | bool]
TextStateManagerDictType = MutableMapping[TextStateManagerKeyType, float | bool]


class TextStateManager:
    """
    Tracks the current text state including cm/tm/trm transformation matrices.

    Attributes:
        transform_stack (ChainMap): ChainMap of cm/tm transformation matrices
        q_queue (Counter[int]): Counter of q operators
        q_depth (List[int]): list of q operator nesting levels
        Tc (float): character spacing
        Tw (float): word spacing
        Tz (int): horizontal scaling
        TL (float): leading
        Ts (float): text rise
        font (Font): font object
        font_size (int | float): font size

    """

    def __init__(self) -> None:
        return _init(self)

    def set_state_param(self, op: bytes, value: float | list[Any]) -> None:
        """
        Set a text state parameter. Supports Tc, Tz, Tw, TL, and Ts operators.

        Args:
            op: operator read from PDF stream as bytes. No action is taken
                for unsupported operators (see supported operators above).
            value (float | List[Any]): new parameter value. If a list,
                value[0] is used.

        """
        return _set_state_param(self, op, value)

    def set_font(self, font: Font, size: float) -> None:
        """
        Set the current font and font_size.

        Args:
            font (Font): a layout mode Font
            size (float): font size

        """
        return _set_font(self, font, size)

    def text_state_params(self, value: bytes | str = "") -> TextStateParams:
        """
        Create a TextStateParams instance to display a text string. Type[bytes] values
        will be decoded implicitly.

        Args:
            value (str | bytes): text to associate with the captured state.

        Raises:
            PdfReadError: if font not set (no Tf operator in incoming pdf content stream)

        Returns:
            TextStateParams: current text state parameters

        """
        return _text_state_params(self, value)

    @staticmethod
    def _as_float(value: Any, default: float = 0.0) -> float:
        """An operand as a float, or default if it is not a number
        (e.g. a name from a malformed content stream).
        """
        return _as_float_impl(value, default)

    @staticmethod
    def raw_transform(
        _a: float = 1.0,
        _b: float = 0.0,
        _c: float = 0.0,
        _d: float = 1.0,
        _e: float = 0.0,
        _f: float = 0.0,
    ) -> TextStateManagerDictType:
        """Only a/b/c/d/e/f matrix params"""
        return _raw_transform_impl(_a, _b, _c, _d, _e, _f)

    @staticmethod
    def new_transform(
        _a: float = 1.0,
        _b: float = 0.0,
        _c: float = 0.0,
        _d: float = 1.0,
        _e: float = 0.0,
        _f: float = 0.0,
        *,
        is_text: bool = False,
        is_render: bool = False,
    ) -> TextStateManagerDictType:
        """Standard a/b/c/d/e/f matrix params + 'is_text' and 'is_render' keys"""
        return _new_transform_impl(_a, _b, _c, _d, _e, _f, is_text, is_render)

    def reset_tm(self) -> TextStateManagerChainMapType:
        """Clear all transforms from chainmap having is_text==True or is_render==True"""
        return _reset_tm(self)

    def reset_trm(self) -> TextStateManagerChainMapType:
        """Clear all transforms from chainmap having is_render==True"""
        return _reset_trm(self)

    def remove_q(self) -> TextStateManagerChainMapType:
        """Rewind to stack prior state after closing a 'q' with internal 'cm' ops"""
        return _remove_q(self)

    def add_q(self) -> None:
        """Add another level to q_queue"""
        return _add_q(self)

    def add_cm(self, *args: Any) -> TextStateManagerChainMapType:
        """Concatenate an additional transform matrix"""
        return _add_cm(self, *args)

    def _complete_matrix(self, operands: list[float]) -> list[float]:
        """Adds a, b, c, and d to an "e/f only" operand set (e.g Td)"""
        return _complete_matrix(self, operands)

    def add_tm(self, operands: list[float]) -> TextStateManagerChainMapType:
        """Append a text transform matrix"""
        return _add_tm(self, operands)

    def add_trm(self, operands: list[float]) -> TextStateManagerChainMapType:
        """Append a text rendering transform matrix"""
        return _add_trm(self, operands)

    @property
    def effective_transform(self) -> list[float]:
        """Current effective transform accounting for cm, tm, and trm transforms"""
        return _effective_transform(self)

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
