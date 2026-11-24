"""A dataclass that captures the CTM and Text State for a tj operation

Rust port of ``pypdf/_text_extraction/_layout_mode/_text_state_params.py``
(upstream pypdf 7.1.1): the ``@dataclass`` definition (fields, defaults,
``init=False`` markers) is kept verbatim because it is observable Python API
(``dataclasses.fields``/``asdict``, keyword construction, attribute access);
every method body delegates to ``crates/pypdf-py/src/modules/text_extraction_mod``.
"""

import math
from dataclasses import dataclass, field
from typing import Any

from pypdf._pypdf import (
    text_state_params_displaced_transform as _displaced_transform,
    text_state_params_displacement_matrix as _displacement_matrix,
    text_state_params_font_size_matrix as _font_size_matrix,
    text_state_params_post_init as _post_init,
    text_state_params_render_transform as _render_transform,
    text_state_params_to_dict as _to_dict,
    text_state_params_word_tx as _word_tx,
)

from ...generic._font import Font
from .. import mult, orient


@dataclass
class TextStateParams:
    """
    Text state parameters and operator values for a single text value in a
    TJ or Tj PDF operation.

    Attributes:
        value (bytes | str): the raw text to be rendered.
        font (Font): font object
        font_size (int | float): font size
        Tc (float): character spacing. Defaults to 0.0.
        Tw (float): word spacing. Defaults to 0.0.
        Tz (float): horizontal scaling. Defaults to 100.0.
        TL (float): leading, vertical displacement between text lines. Defaults to 0.0.
        Ts (float): text rise. Used for super/subscripts. Defaults to 0.0.
        transform (List[float]): effective transformation matrix.
        tx (float): x cood of rendered text, i.e. self.transform[4]
        ty (float): y cood of rendered text. May differ from self.transform[5] per self.Ts.
        displaced_tx (float): x coord immediately following rendered text
        space_tx (float): tx for a space character
        font_height (float): effective font height accounting for CTM
        flip_vertical (bool): True if y axis has been inverted (i.e. if self.transform[3] < 0.)
        rotated (bool): True if the text orientation is rotated with respect to the page.

    """

    value: bytes | str
    font: Font
    font_size: int | float
    Tc: float = 0.0
    Tw: float = 0.0
    Tz: float = 100.0
    TL: float = 0.0
    Ts: float = 0.0
    transform: list[float] = field(
        default_factory=lambda: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
    )
    tx: float = field(default=0.0, init=False)
    ty: float = field(default=0.0, init=False)
    displaced_tx: float = field(default=0.0, init=False)
    space_tx: float = field(default=0.0, init=False)
    font_height: float = field(default=0.0, init=False)
    flip_vertical: bool = field(default=False, init=False)
    rotated: bool = field(default=False, init=False)
    text: str = ""
    _raw_chars: str = ""

    def __post_init__(self) -> None:
        return _post_init(self)

    def font_size_matrix(self) -> list[float]:
        """Font size matrix"""
        return _font_size_matrix(self)

    def displaced_transform(self) -> list[float]:
        """Effective transform matrix after text has been rendered."""
        return _displaced_transform(self)

    def render_transform(self) -> list[float]:
        """Effective transform matrix accounting for font size, Tz, and Ts."""
        return _render_transform(self)

    def displacement_matrix(
        self, word: bytes | str | None = None, td_offset: float = 0.0
    ) -> list[float]:
        """
        Text displacement matrix

        Args:
            word (bytes | str, optional): Defaults to None in which case self.text displacement is
                returned.
            td_offset (float, optional): translation applied by TD operator. Defaults to 0.0.

        """
        return _displacement_matrix(self, word, td_offset)

    def word_tx(self, word: bytes | str, td_offset: float = 0.0) -> float:
        """Horizontal text displacement for any word according this text state"""
        return _word_tx(self, word, td_offset)

    @staticmethod
    def to_dict(inst: "TextStateParams") -> dict[str, Any]:
        """Dataclass to dict for json.dumps serialization"""
        return _to_dict(inst)

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
