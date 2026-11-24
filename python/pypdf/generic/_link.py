"""Reference links (Rust port of ``pypdf/generic/_link.py``, pypdf 7.1.1).

This module contains code used by ``_writer.py`` to track links in pages being
added to the writer until the links can be resolved.

The classes stay plain Python classes (upstream has no base classes or class
attributes here) and the module functions stay Python functions so that
upstream's module-global dispatch (``_build_link``/``_create_link``/
``is_null_or_none``/``logger_warning``) keeps behaving identically.  Every body
delegates to ``crates/pypdf-py/src/modules/genbasic_mod.rs``.
"""

from typing import TYPE_CHECKING, cast

from pypdf._pypdf import (
    genbasic_build_link as _build_link_impl,
    genbasic_create_link as _create_link_impl,
    genbasic_direct_reference_link_find_referenced_page as _direct_reference_link_find_referenced_page,
    genbasic_direct_reference_link_new as _direct_reference_link_new,
    genbasic_direct_reference_link_patch_reference as _direct_reference_link_patch_reference,
    genbasic_extract_links as _extract_links_impl,
    genbasic_named_reference_link_find_referenced_page as _named_reference_link_find_referenced_page,
    genbasic_named_reference_link_new as _named_reference_link_new,
    genbasic_named_reference_link_patch_reference as _named_reference_link_patch_reference,
)

from .._utils import logger_warning  # noqa: F401  (resolved by the Rust implementation)
from . import ArrayObject, DictionaryObject, IndirectObject, PdfObject, TextStringObject, is_null_or_none

if TYPE_CHECKING:
    from .._page import PageObject
    from .._reader import PdfReader
    from .._writer import PdfWriter


class NamedReferenceLink:
    """Named reference link being preserved until we can resolve it correctly."""

    def __init__(self, reference: TextStringObject, source_pdf: "PdfReader") -> None:
        """reference: TextStringObject with named reference"""
        return _named_reference_link_new(self, reference, source_pdf)

    def find_referenced_page(self) -> IndirectObject | None:
        return _named_reference_link_find_referenced_page(self)

    def patch_reference(
        self,
        target_pdf: "PdfWriter",
        new_page: IndirectObject,
        *,
        known_names: set[str] | None = None,
    ) -> None:
        """
        Add the named destination of the link to the new page, unless it exists.

        Args:
            target_pdf: PdfWriter which the new link went into.
            new_page: IndirectObject representing the new page in the target PDF.
            known_names: Names of the named destinations of target_pdf, kept up to
                date by this method. Collecting them from target_pdf is expensive,
                thus callers patching many links should pass them.
        """
        return _named_reference_link_patch_reference(
            self, target_pdf, new_page, known_names=known_names
        )


class DirectReferenceLink:
    """Direct reference link being preserved until we can resolve it correctly."""

    def __init__(self, reference: ArrayObject) -> None:
        """reference: an ArrayObject whose first element is the Page indirect object"""
        return _direct_reference_link_new(self, reference)

    def find_referenced_page(self) -> IndirectObject | None:
        return _direct_reference_link_find_referenced_page(self)

    def patch_reference(self, target_pdf: "PdfWriter", new_page: IndirectObject) -> None:
        """target_pdf: PdfWriter which the new link went into"""
        return _direct_reference_link_patch_reference(self, target_pdf, new_page)


ReferenceLink = NamedReferenceLink | DirectReferenceLink


def extract_links(new_page: "PageObject", old_page: "PageObject") -> list[tuple[ReferenceLink, ReferenceLink]]:
    """Extracts links from two pages on the assumption that the two pages are
    the same. Produces one list of (new link, old link) tuples.

    Non-link annotations are ignored before pairing to avoid dropping valid
    links when one page includes additional non-link annotation entries.
    """
    return _extract_links_impl(new_page, old_page)


def _build_link(indirect_object: IndirectObject, page: "PageObject") -> ReferenceLink | None:
    return _build_link_impl(indirect_object, page)


def _create_link(reference: PdfObject, source_pdf: "PdfReader") -> ReferenceLink | None:
    return _create_link_impl(reference, source_pdf)

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
