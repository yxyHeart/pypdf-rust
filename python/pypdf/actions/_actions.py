"""Action types

Rust port of ``pypdf/actions/_actions.py`` (upstream pypdf 7.1.1).  The
``PageTrigger`` enum members and the ``Action``/``JavaScript`` class hierarchy
(``DictionaryObject`` subclasses) are kept verbatim; the logic lives in
``crates/pypdf-py/src/modules/actions_mod.rs``.
"""

import sys
import uuid
from abc import ABC
from enum import Enum, unique
from typing import (
    TYPE_CHECKING,
    cast,
)

import pypdf._pypdf as _rust

from .._utils import logger_warning
from ..constants import CatalogAttributes
from ..errors import ParseError
from ..generic import (
    ArrayObject,
    DictionaryObject,
    NameObject,
    NullObject,
    TextStringObject,
    create_string_object,
    is_null_or_none,
)

if sys.version_info >= (3, 11):
    from enum import StrEnum
else:

    class StrEnum(str, Enum):
        def __str__(self) -> str:
            return str(self.value)


if TYPE_CHECKING:
    from .._page import PageObject
    from .._writer import PdfWriter


@unique
class PageTrigger(StrEnum):
    """Trigger event entries in a page object's additional-actions dictionary."""

    OPEN = "open"
    """Trigger an action when the page is opened."""
    CLOSE = "close"
    """Trigger an action when the page is closed."""

    @property
    def _name_object(self) -> NameObject:
        """Returns the corresponding NameObject for the trigger."""
        return _rust.actionsmod_page_trigger_name_object(self)


class Action(DictionaryObject, ABC):
    """An action dictionary defines the characteristics and behaviour of an action."""

    def __init__(self) -> None:
        _rust.actionsmod_action_init(self)

    @classmethod
    def _create_open_action(cls, writer: "PdfWriter", action: "Action") -> None:
        """
        Create an action that shall be performed when the document is opened.

        Args:
            writer: The writer to add the action to.
            action: The action to add.
        """
        _rust.actionsmod_action_create_open_action(writer, action)

    @classmethod
    def _create_new(cls, page: "PageObject", trigger: PageTrigger, action: "Action") -> None:
        """
        Create a new action and add it to the page.

        Args:
            page: The page to add the action to.
            trigger: An open or close trigger.
            action: The action to be done.
        """
        _rust.actionsmod_action_create_new(page, trigger, action)

    @classmethod
    def _delete(cls, page: "PageObject", trigger: PageTrigger) -> None:
        """
        Delete an action from the page.

        Args:
            page: The page to delete the action.
            trigger: An open or close trigger.
        """
        _rust.actionsmod_action_delete(page, trigger)


class JavaScript(Action):
    """
    Upon invocation of an ECMAScript action, a PDF processor shall execute a
    script that is written in the ECMAScript programming language. ECMAScript
    extensions described in ISO/DIS 21757-1 shall also be allowed.

    Args:
        java_script: A text string containing the ECMAScript script to be executed.
    """

    def __init__(self, java_script: str) -> None:
        """Initialize JavaScript with a string."""
        _rust.actionsmod_javascript_init(self, java_script)
