"""Action types

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.actions._actions``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import sys
import uuid
from abc import ABC
from enum import Enum, unique
from typing import TYPE_CHECKING, cast
from .._utils import logger_warning
from ..constants import CatalogAttributes
from ..errors import ParseError
from ..generic import ArrayObject, DictionaryObject, NameObject, NullObject, TextStringObject, create_string_object, is_null_or_none
if sys.version_info >= (3, 11):
    from enum import StrEnum
else:
    class StrEnum:
        """Stub for pypdf.actions._actions.StrEnum."""
        def __str__(self, *args, **kwargs):
            return ""  # inert stub
if TYPE_CHECKING:
    from .._page import PageObject
    from .._writer import PdfWriter
class PageTrigger:
    """Stub for pypdf.actions._actions.PageTrigger."""
    OPEN = 'open'
    CLOSE = 'close'
    @property
    def _name_object(self):
        return None  # inert stub
class Action:
    """Stub for pypdf.actions._actions.Action."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    @classmethod
    def _create_open_action(cls, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def _create_new(cls, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def _delete(cls, *args, **kwargs):
        return None  # inert stub
class JavaScript(Action):
    """Stub for pypdf.actions._actions.JavaScript."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
