"""
PDF includes a wide variety of standard action types, whose characteristics and
behaviour are defined by an action dictionary. These are defined in this
submodule.

Trigger events are the other component of actions, and are specific to their
associated object.
"""


from ._actions import Action, JavaScript, PageTrigger

__all__ = [
    "Action",
    "JavaScript",
    "PageTrigger",
]

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
