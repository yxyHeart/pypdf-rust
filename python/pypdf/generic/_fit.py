"""Page fit objects (Rust port of ``pypdf/generic/_fit.py``, pypdf 7.1.1).

``Fit`` stays a plain Python class (no inheritance, no class attributes beyond
the defaults of ``__init__``) so that ``Fit`` instances keep upstream's identity
and attribute surface.  Every method body delegates to
``crates/pypdf-py/src/modules/genbasic_mod.rs``.
"""

from collections.abc import Sequence
from typing import Any

from pypdf._pypdf import (
    genbasic_fit_fit as _fit_fit,
    genbasic_fit_fit_box as _fit_fit_box,
    genbasic_fit_fit_box_horizontally as _fit_fit_box_horizontally,
    genbasic_fit_fit_box_vertically as _fit_fit_box_vertically,
    genbasic_fit_fit_horizontally as _fit_fit_horizontally,
    genbasic_fit_fit_rectangle as _fit_fit_rectangle,
    genbasic_fit_fit_vertically as _fit_fit_vertically,
    genbasic_fit_new as _fit_new,
    genbasic_fit_str as _fit_str,
    genbasic_fit_xyz as _fit_xyz,
)

from ._base import is_null_or_none  # noqa: F401  (resolved by the Rust implementation)


class Fit:
    def __init__(
        self, fit_type: str, fit_args: Sequence[float | Any | None] = ()
    ) -> None:
        return _fit_new(self, fit_type, fit_args)

    @classmethod
    def xyz(
        cls,
        left: float | None = None,
        top: float | None = None,
        zoom: float | None = None,
    ) -> "Fit":
        """
        Display the page designated by page, with the coordinates (left, top)
        positioned at the upper-left corner of the window and the contents
        of the page magnified by the factor zoom.

        A null value for any of the parameters left, top, or zoom specifies
        that the current value of that parameter is to be retained unchanged.

        A zoom value of 0 has the same meaning as a null value.

        Args:
            left:
            top:
            zoom:

        Returns:
            The created fit object.

        """
        return _fit_xyz(left, top, zoom)

    @classmethod
    def fit(cls) -> "Fit":
        """
        Display the page designated by page, with its contents magnified just
        enough to fit the entire page within the window both horizontally and
        vertically.

        If the required horizontal and vertical magnification factors are
        different, use the smaller of the two, centering the page within the
        window in the other dimension.
        """
        return _fit_fit()

    @classmethod
    def fit_horizontally(cls, top: float | None = None) -> "Fit":
        """
        Display the page designated by page, with the vertical coordinate top
        positioned at the top edge of the window and the contents of the page
        magnified just enough to fit the entire width of the page within the
        window.

        A null value for ``top`` specifies that the current value of that
        parameter is to be retained unchanged.

        Args:
            top:

        Returns:
            The created fit object.

        """
        return _fit_fit_horizontally(top)

    @classmethod
    def fit_vertically(cls, left: float | None = None) -> "Fit":
        return _fit_fit_vertically(left)

    @classmethod
    def fit_rectangle(
        cls,
        left: float | None = None,
        bottom: float | None = None,
        right: float | None = None,
        top: float | None = None,
    ) -> "Fit":
        """
        Display the page designated by page, with its contents magnified
        just enough to fit the rectangle specified by the coordinates
        left, bottom, right, and top entirely within the window
        both horizontally and vertically.

        If the required horizontal and vertical magnification factors are
        different, use the smaller of the two, centering the rectangle within
        the window in the other dimension.

        A null value for any of the parameters may result in unpredictable
        behavior.

        Args:
            left:
            bottom:
            right:
            top:

        Returns:
            The created fit object.

        """
        return _fit_fit_rectangle(left, bottom, right, top)

    @classmethod
    def fit_box(cls) -> "Fit":
        """
        Display the page designated by page, with its contents magnified just
        enough to fit its bounding box entirely within the window both
        horizontally and vertically.

        If the required horizontal and vertical magnification factors are
        different, use the smaller of the two, centering the bounding box
        within the window in the other dimension.
        """
        return _fit_fit_box()

    @classmethod
    def fit_box_horizontally(cls, top: float | None = None) -> "Fit":
        """
        Display the page designated by page, with the vertical coordinate top
        positioned at the top edge of the window and the contents of the page
        magnified just enough to fit the entire width of its bounding box
        within the window.

        A null value for top specifies that the current value of that parameter
        is to be retained unchanged.

        Args:
            top:

        Returns:
            The created fit object.

        """
        return _fit_fit_box_horizontally(top)

    @classmethod
    def fit_box_vertically(cls, left: float | None = None) -> "Fit":
        """
        Display the page designated by page, with the horizontal coordinate
        left positioned at the left edge of the window and the contents of the
        page magnified just enough to fit the entire height of its bounding box
        within the window.

        A null value for left specifies that the current value of that
        parameter is to be retained unchanged.

        Args:
            left:

        Returns:
            The created fit object.

        """
        return _fit_fit_box_vertically(left)

    def __str__(self) -> str:
        return _fit_str(self)


DEFAULT_FIT = Fit.fit()
