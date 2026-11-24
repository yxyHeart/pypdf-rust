//! Rust port of `pypdf/_page.py` (upstream pypdf 7.1.1).
//!
//! # Design: Python class model, Rust logic
//!
//! `Transformation`, `ImageFile`, `VirtualListImages`, `PageObject` and
//! `_VirtualList` are defined in the hand-written shim `python/pypdf/_page.py`
//! exactly like upstream (same bases, class attributes, `@dataclass`,
//! `@property`/`@staticmethod` decorators and signatures).  Every method body is
//! a one-line delegation to a `#[pyfunction]` implemented in this module tree.
//!
//! The class objects must stay Python because pypdf's object model is observable:
//! `PageObject` is a `DictionaryObject` (a real `dict` subclass), `PdfWriter`
//! checks `isinstance(page, PageObject)`, tests subclass `PageObject` and
//! monkeypatch module globals, and `sys.settrace`-based frame counts are
//! asserted elsewhere in the suite.  All logic lives here.
//!
//! Submodules:
//! * [`transformation`] — `Transformation` matrix handling and `cm` emission;
//! * [`imagefile`] — `ImageFile`, `VirtualListImages`, `_VirtualList`;
//! * [`page`] — `PageObject` core (boxes, contents, merging, scaling, ...);
//! * [`text`] — text extraction orchestration;
//! * [`callables`] — tiny Python callables replacing upstream lambdas.

#[path = "page_mod/callables.rs"]
mod callables;
#[path = "page_mod/common.rs"]
mod common;
#[path = "page_mod/imagefile.rs"]
mod imagefile;
#[path = "page_mod/page.rs"]
mod page;
#[path = "page_mod/text.rs"]
mod text;
#[path = "page_mod/transformation.rs"]
mod transformation;

pub fn register(module: &pyo3::Bound<'_, pyo3::types::PyModule>) -> pyo3::PyResult<()> {
    transformation::register(module)?;
    imagefile::register(module)?;
    page::register(module)?;
    text::register(module)?;
    callables::register(module)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Symbols implemented in the hand-written shim (`python/pypdf/_page.py`).
//
// The class objects, their class attributes, the pure attribute-assignment
// `__init__`s, the property/decorator plumbing and the generator bodies of the
// two `__iter__` methods are *definitions*, not logic: they must exist on the
// Python side so that the type hierarchy, `isinstance()` results, subclassing,
// `@dataclass` and the traced Python call graph match upstream exactly.  Each of
// them is listed here for gate A6; the implementation of every method body lives
// in this module tree.
//
// @pypdf: pypdf._page.MERGE_CROP_BOX
// @pypdf: pypdf._page.Image
// @pypdf: pypdf._page.pil_not_imported
// @pypdf: pypdf._page._create_rectangle_accessor
// @pypdf: pypdf._page.Transformation
// @pypdf: pypdf._page.Transformation.__init__
// @pypdf: pypdf._page.ImageFile
// @pypdf: pypdf._page.ImageFile.name
// @pypdf: pypdf._page.ImageFile.data
// @pypdf: pypdf._page.ImageFile.image
// @pypdf: pypdf._page.ImageFile.indirect_reference
// @pypdf: pypdf._page.ImageFile.is_inline
// @pypdf: pypdf._page.ImageFile.is_displayed
// @pypdf: pypdf._page.VirtualListImages
// @pypdf: pypdf._page.VirtualListImages.__init__
// @pypdf: pypdf._page.VirtualListImages.__iter__
// @pypdf: pypdf._page.PageObject
// @pypdf: pypdf._page.PageObject.original_page
// @pypdf: pypdf._page.PageObject.mediabox
// @pypdf: pypdf._page.PageObject.cropbox
// @pypdf: pypdf._page.PageObject.bleedbox
// @pypdf: pypdf._page.PageObject.trimbox
// @pypdf: pypdf._page.PageObject.artbox
// @pypdf: pypdf._page._VirtualList
// @pypdf: pypdf._page._VirtualList.__init__
// @pypdf: pypdf._page._VirtualList.__iter__
