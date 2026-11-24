"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.annotations._base``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

from abc import ABC
from ..constants import AnnotationFlag
from ..generic import NameObject, NumberObject
from ..generic._data_structures import DictionaryObject
class AnnotationDictionary:
    """Stub for pypdf.annotations._base.AnnotationDictionary."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    @property
    def flags(self):
        return None  # inert stub
    def flags(self, *args, **kwargs):
        return None  # inert stub
NO_FLAGS = None
