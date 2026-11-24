"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.generic._viewerpref``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

from typing import Any, cast
from ..constants import PageAttributes
from ._base import BooleanObject, NameObject, NumberObject, is_null_or_none
from ._data_structures import ArrayObject, DictionaryObject
f_obj = None
BOX_NAMES = []
class ViewerPreferences(DictionaryObject):
    """Stub for pypdf.generic._viewerpref.ViewerPreferences."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def _get_bool(self, *args, **kwargs):
        return None  # inert stub
    def _set_bool(self, *args, **kwargs):
        return None  # inert stub
    def _get_name(self, *args, **kwargs):
        return None  # inert stub
    def _set_name(self, *args, **kwargs):
        return None  # inert stub
    def _get_arr(self, *args, **kwargs):
        return None  # inert stub
    def _set_arr(self, *args, **kwargs):
        return None  # inert stub
    def _get_int(self, *args, **kwargs):
        return None  # inert stub
    def _set_int(self, *args, **kwargs):
        return None  # inert stub
    @property
    def PRINT_SCALING(self):
        return None  # inert stub
    def __new__(cls, *args, **kwargs):
        return super().__new__(cls)  # inert stub
