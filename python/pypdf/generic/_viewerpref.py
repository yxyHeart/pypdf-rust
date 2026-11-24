"""Viewer preferences (Rust port of ``pypdf/generic/_viewerpref.py``, pypdf 7.1.1).

``ViewerPreferences`` stays a ``DictionaryObject`` subclass with exactly
upstream's structure: the per-instance ``property`` definitions created in
``__new__`` (including their ``__doc__`` text), the ``PRINT_SCALING`` property
and the module constants are kept verbatim because they are observable Python
API.  Every getter/setter body delegates to
``crates/pypdf-py/src/modules/genbasic_mod.rs``, which holds all the logic
(validation, error messages, item conversion).
"""

from typing import Any, cast

from pypdf._pypdf import (
    genbasic_viewerpreferences_get_arr as _viewerpreferences_get_arr,
    genbasic_viewerpreferences_get_bool as _viewerpreferences_get_bool,
    genbasic_viewerpreferences_get_int as _viewerpreferences_get_int,
    genbasic_viewerpreferences_get_name as _viewerpreferences_get_name,
    genbasic_viewerpreferences_new as _viewerpreferences_new,
    genbasic_viewerpreferences_print_scaling as _viewerpreferences_print_scaling,
    genbasic_viewerpreferences_set_arr as _viewerpreferences_set_arr,
    genbasic_viewerpreferences_set_bool as _viewerpreferences_set_bool,
    genbasic_viewerpreferences_set_int as _viewerpreferences_set_int,
    genbasic_viewerpreferences_set_name as _viewerpreferences_set_name,
)

from ..constants import PageAttributes
from ._base import BooleanObject, NameObject, NumberObject, is_null_or_none  # noqa: F401
from ._data_structures import ArrayObject, DictionaryObject

f_obj = BooleanObject(False)

#: The page boundaries a viewer preference may name, per Table 147 in the 2.0
#: reference.
BOX_NAMES = [
    PageAttributes.MEDIABOX,
    PageAttributes.CROPBOX,
    PageAttributes.BLEEDBOX,
    PageAttributes.TRIMBOX,
    PageAttributes.ARTBOX,
]


class ViewerPreferences(DictionaryObject):
    def __init__(self, obj: DictionaryObject | None = None) -> None:
        return _viewerpreferences_new(self, obj)

    def _get_bool(self, key: str, default: BooleanObject | None) -> BooleanObject | None:
        return _viewerpreferences_get_bool(self, key, default)

    def _set_bool(self, key: str, v: bool) -> None:
        return _viewerpreferences_set_bool(self, key, v)

    def _get_name(self, key: str, default: NameObject | None) -> NameObject | None:
        return _viewerpreferences_get_name(self, key, default)

    def _set_name(self, key: str, lst: list[str], v: NameObject) -> None:
        return _viewerpreferences_set_name(self, key, lst, v)

    def _get_arr(self, key: str, default: list[Any] | None) -> ArrayObject | None:
        return _viewerpreferences_get_arr(self, key, default)

    def _set_arr(self, key: str, v: ArrayObject | None) -> None:
        return _viewerpreferences_set_arr(self, key, v)

    def _get_int(self, key: str, default: NumberObject | None) -> NumberObject | None:
        return _viewerpreferences_get_int(self, key, default)

    def _set_int(self, key: str, v: int) -> None:
        return _viewerpreferences_set_int(self, key, v)

    @property
    def PRINT_SCALING(self) -> NameObject:
        return _viewerpreferences_print_scaling(self)

    def __new__(cls: Any, value: Any = None) -> "ViewerPreferences":  # noqa: PYI034
        def _add_prop_bool(key: str, default: BooleanObject | None) -> property:
            return property(
                lambda self: self._get_bool(key, default),
                lambda self, v: self._set_bool(key, v),
                None,
                f"""
            Returns/Modify the status of {key}, Returns {default} if not defined
            """,
            )

        def _add_prop_name(
            key: str, lst: list[str], default: NameObject | None
        ) -> property:
            return property(
                lambda self: self._get_name(key, default),
                lambda self, v: self._set_name(key, lst, NameObject(v)),
                None,
                f"""
            Returns/Modify the status of {key}, Returns {default} if not defined.
            Acceptable values: {lst}
            """,
            )

        def _add_prop_arr(key: str, default: ArrayObject | None) -> property:
            return property(
                lambda self: self._get_arr(key, default),
                lambda self, v: self._set_arr(key, v),
                None,
                f"""
            Returns/Modify the status of {key}, Returns {default} if not defined
            """,
            )

        def _add_prop_int(key: str, default: int | None) -> property:
            return property(
                lambda self: self._get_int(key, default),
                lambda self, v: self._set_int(key, v),
                None,
                f"""
            Returns/Modify the status of {key}, Returns {default} if not defined
            """,
            )

        cls.hide_toolbar = _add_prop_bool("/HideToolbar", f_obj)
        cls.hide_menubar = _add_prop_bool("/HideMenubar", f_obj)
        cls.hide_windowui = _add_prop_bool("/HideWindowUI", f_obj)
        cls.fit_window = _add_prop_bool("/FitWindow", f_obj)
        cls.center_window = _add_prop_bool("/CenterWindow", f_obj)
        cls.display_doctitle = _add_prop_bool("/DisplayDocTitle", f_obj)

        cls.non_fullscreen_pagemode = _add_prop_name(
            "/NonFullScreenPageMode",
            ["/UseNone", "/UseOutlines", "/UseThumbs", "/UseOC"],
            NameObject("/UseNone"),
        )
        cls.direction = _add_prop_name(
            "/Direction", ["/L2R", "/R2L"], NameObject("/L2R")
        )
        cls.view_area = _add_prop_name("/ViewArea", BOX_NAMES, None)
        cls.view_clip = _add_prop_name("/ViewClip", BOX_NAMES, None)
        cls.print_area = _add_prop_name("/PrintArea", BOX_NAMES, None)
        cls.print_clip = _add_prop_name("/PrintClip", BOX_NAMES, None)
        cls.print_scaling = _add_prop_name(
            "/PrintScaling", ["/None", "/AppDefault"], None
        )
        cls.duplex = _add_prop_name(
            "/Duplex", ["/Simplex", "/DuplexFlipShortEdge", "/DuplexFlipLongEdge"], None
        )
        cls.pick_tray_by_pdfsize = _add_prop_bool("/PickTrayByPDFSize", None)
        cls.print_pagerange = _add_prop_arr("/PrintPageRange", None)
        cls.num_copies = _add_prop_int("/NumCopies", None)

        cls.enforce = _add_prop_arr("/Enforce", ArrayObject())

        return cast("ViewerPreferences", DictionaryObject.__new__(cls))
