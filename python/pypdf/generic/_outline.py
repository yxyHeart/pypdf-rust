"""Outline items (Rust port of ``pypdf/generic/_outline.py``, pypdf 7.1.1).

``OutlineItem`` subclasses the *Python* ``Destination`` class from
``pypdf.generic._data_structures`` exactly like upstream; ``write_to_stream``
delegates to ``crates/pypdf-py/src/modules/genbasic_mod.rs``.
"""

from pypdf._pypdf import (
    genbasic_outlineitem_write_to_stream as _outlineitem_write_to_stream,
)

from .._utils import StreamType
from ._base import NameObject
from ._data_structures import Destination


class OutlineItem(Destination):
    def write_to_stream(self, stream: StreamType) -> None:
        return _outlineitem_write_to_stream(self, stream)
