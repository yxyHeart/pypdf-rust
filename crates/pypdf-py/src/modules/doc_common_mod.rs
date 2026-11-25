//! Rust port of `pypdf/_doc_common.py` (upstream pypdf 7.1.1).
//!
//! # Design: Python class model, Rust logic
//!
//! `PdfDocCommon` is the abstract base class shared by `PdfReader` and
//! `PdfWriter`, and `DocumentInformation` is a `DictionaryObject` subclass whose
//! MRO (`isinstance(x, DictionaryObject)`, `dict` behaviour, subclassing and
//! monkeypatching) is observable by the official test suite.  PyO3 cannot
//! inherit from an arbitrary Python class, so the whole class structure --
//! ABC-ness, `@property`/`@abstractmethod` decorators, class attributes such as
//! `strict`/`flattened_pages` and the `NamedTuple` containers -- lives verbatim
//! in the hand-written shim `python/pypdf/_doc_common.py`, and every method body
//! is a one-line delegation to a `#[pyfunction]` implemented here.  All
//! algorithm/parsing logic therefore lives in Rust while the Python call graph
//! (and hence `sys.settrace` counts and `mock.patch` targets) stays identical to
//! upstream.
//!
//! Cross-module collaborators (`pypdf.generic`, `pypdf._utils`,
//! `pypdf._configuration`, `pypdf._page`, ...) are imported lazily at call time
//! exactly like upstream's module-level imports are resolved at import time, so
//! this module can be loaded while those modules are still being ported.

#[path = "doc_common_mod/dests.rs"]
mod dests;
#[path = "doc_common_mod/docmeta.rs"]
mod docmeta;
#[path = "doc_common_mod/fields.rs"]
mod fields;
#[path = "doc_common_mod/info.rs"]
mod info;
#[path = "doc_common_mod/outline.rs"]
mod outline;
#[path = "doc_common_mod/pages.rs"]
mod pages;

use pyo3::exceptions::{PyAssertionError, PyAttributeError};
use pyo3::prelude::*;
use pyo3::types::{
    PyAny, PyBool, PyBytes, PyDict, PyInt, PyIterator, PyList, PyModule, PySet, PyString, PyTuple,
};

use crate::errors;
use crate::pyutil;

/// Upstream module name; used as `logger_warning(..., source=__name__)`.
const SOURCE: &str = "pypdf._doc_common";

// ---------------------------------------------------------------------------
// `PdfDocCommon` itself lives in the shim with upstream's exact class
// structure: the class attributes below and the `@property`/`@abstractmethod`
// members that `PdfReader`/`PdfWriter` implement (or that are re-exported for
// the type annotations).  Annotated here for the coverage gate.
// ---------------------------------------------------------------------------
// @pypdf: pypdf._doc_common.PdfDocCommon
// @pypdf: pypdf._doc_common.PdfDocCommon.strict
// @pypdf: pypdf._doc_common.PdfDocCommon.flattened_pages
// @pypdf: pypdf._doc_common.PdfDocCommon._encryption
// @pypdf: pypdf._doc_common.PdfDocCommon._readonly
// @pypdf: pypdf._doc_common.PdfDocCommon.root_object
// @pypdf: pypdf._doc_common.PdfDocCommon.pdf_header
// @pypdf: pypdf._doc_common.PdfDocCommon.get_object
// @pypdf: pypdf._doc_common.PdfDocCommon._replace_object
// @pypdf: pypdf._doc_common.PdfDocCommon._info
// @pypdf: pypdf._doc_common.PdfDocCommon.xmp_metadata
// @pypdf: pypdf._doc_common.PdfDocCommon._get_page_number_by_indirect
// @pypdf: pypdf._doc_common.PdfDocCommon.is_encrypted
// @pypdf: pypdf._doc_common.PdfDocCommon._repr_mimebundle_

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    crate::pyutil::cached_module(py, "builtins")
}

fn shim<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("pypdf._doc_common")
}

/// Fetch a global (class, sentinel, ...) from this module's shim: upstream code
/// references these by global name.
fn shim_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim(py)?.getattr(name)
}

fn shim_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, name)
}

/// Fetch a class/function re-exported by `pypdf.generic` (upstream's
/// `from .generic import ...`).
fn generic_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf.generic")?.getattr(name)
}

fn utils_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf._utils")?.getattr(name)
}

fn constants_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf.constants")?.getattr(name)
}

/// `from ._configuration import get_configuration` then calling it.
fn configuration<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf._configuration")?
        .getattr("get_configuration")?
        .call0()
}

/// `get_configuration().<name>`.
fn config_item<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    configuration(py)?.getattr(name)
}

fn py_none<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    py.None().into_bound(py)
}

fn py_int<'py>(py: Python<'py>, value: i64) -> Bound<'py, PyAny> {
    PyInt::new(py, value).into_any()
}

fn py_str<'py>(py: Python<'py>, value: &str) -> Bound<'py, PyAny> {
    PyString::new(py, value).into_any()
}

fn py_bool<'py>(py: Python<'py>, value: bool) -> Bound<'py, PyAny> {
    PyBool::new(py, value).to_owned().into_any()
}

fn py_bytes<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, data).into_any()
}

fn py_list<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyList::new(py, items)?.into_any())
}

fn py_tuple<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyTuple::new(py, items)?.into_any())
}

/// `id(obj)` as a Python int (upstream stores `id()` values, so the values that
/// end up in `visited` sets must be the very same Python ints).
fn py_id<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("id")?.call1((obj,))
}

/// `a is b`.
fn is_same(a: &Bound<'_, PyAny>, b: &Bound<'_, PyAny>) -> bool {
    a.as_ptr() == b.as_ptr()
}

/// `raise <builtin>(message)` built from a Python exception class.
fn message_exception(py: Python<'_>, class_name: &str, message: &str) -> PyErr {
    match builtins(py)
        .and_then(|builtins| builtins.getattr(class_name))
        .and_then(|class| class.call1((message,)))
    {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    }
}

/// `raise <builtin>()` for exception classes that take no arguments.
fn empty_exception(py: Python<'_>, class_name: &str) -> PyErr {
    match builtins(py)
        .and_then(|builtins| builtins.getattr(class_name))
        .and_then(|class| class.call0())
    {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    }
}

/// `raise ValueError(message) from cause`.
fn value_error_from(py: Python<'_>, message: &str, cause: PyErr) -> PyErr {
    let error = message_exception(py, "ValueError", message);
    error.set_cause(py, Some(cause.clone_ref(py)));
    error.set_context(py, Some(cause));
    error
}

/// `raise Exception(message) from cause`.
fn exception_from(py: Python<'_>, message: &str, cause: PyErr) -> PyErr {
    let error = message_exception(py, "Exception", message);
    error.set_cause(py, Some(cause.clone_ref(py)));
    error.set_context(py, Some(cause));
    error
}

/// `logger_warning(message, source=__name__, **values)`.
fn log_warning(py: Python<'_>, message: &str, values: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    pyutil::logger_warning(py, SOURCE, message, values)
}

/// `logger_warning(message, source=__name__, **values)` with inline values.
fn log_warning_with<'py>(
    py: Python<'py>,
    message: &str,
    values: &[(&str, &Bound<'py, PyAny>)],
) -> PyResult<()> {
    let kwargs = PyDict::new(py);
    for (name, value) in values {
        kwargs.set_item(*name, value)?;
    }
    log_warning(py, message, Some(&kwargs))
}

/// `obj.get(key, default)` by dynamic dispatch: `DictionaryObject` subclasses may
/// override `get`, and upstream always goes through the Python method.
fn dict_get<'py>(
    obj: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    default: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    obj.call_method1("get", (key, default))
}

/// `obj.get(key)` (one argument, exactly like upstream).
fn dict_get_one<'py>(
    obj: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    obj.call_method1("get", (key,))
}

/// `getattr(obj, name, object())` -- a fresh `object()` per call, so two missing
/// attributes never compare equal (upstream relies on that).
fn object_attr<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    name: &str,
) -> PyResult<Bound<'py, PyAny>> {
    match obj.getattr(name) {
        Ok(value) => Ok(value),
        Err(error) if error.is_instance_of::<PyAttributeError>(py) => {
            builtins(py)?.getattr("object")?.call0()
        }
        Err(error) => Err(error),
    }
}

fn str_of(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    pyutil::py_str(obj)
}

fn repr_of(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    pyutil::py_repr(obj)
}

/// `obj.__class__.__name__`.
fn class_name(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    str_of(&obj.get_type().getattr("__name__")?)
}

/// `a + b` through Python's operator protocol.
fn py_add<'py>(a: &Bound<'py, PyAny>, b: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    pyutil::py_add(a, b)
}

/// `iter_obj.__next__()`, with `StopIteration` folded into `None`.
fn next_item<'py>(iterator: &mut Bound<'py, PyIterator>) -> PyResult<Option<Bound<'py, PyAny>>> {
    match iterator.next() {
        Some(Ok(value)) => Ok(Some(value)),
        Some(Err(error)) => Err(error),
        None => Ok(None),
    }
}

/// `next(iterator, None)`.
fn next_or_none<'py>(
    py: Python<'py>,
    iterator: &mut Bound<'py, PyIterator>,
) -> PyResult<Bound<'py, PyAny>> {
    Ok(next_item(iterator)?.unwrap_or_else(|| py_none(py)))
}

/// `len(obj)` as a Python int.
fn py_len<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    Ok(py_int(py, obj.len()? as i64))
}

/// `isinstance(obj, pypdf.generic.<name>)`.
fn is_generic_instance(py: Python<'_>, obj: &Bound<'_, PyAny>, name: &str) -> PyResult<bool> {
    obj.is_instance(&generic_class(py, name)?)
}

/// `is_null_or_none(x)` from `pypdf.generic`.
fn is_null_or_none(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    generic_class(py, "is_null_or_none")?
        .call1((value,))?
        .extract()
}

// --- constructors ---------------------------------------------------------

fn array_object<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "ArrayObject")?.call0()
}

fn array_from<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "ArrayObject")?.call1((py_list(py, items)?,))
}

fn dictionary_object<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "DictionaryObject")?.call0()
}

fn name_object<'py>(py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "NameObject")?.call1((key,))
}

fn null_object<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "NullObject")?.call0()
}

fn boolean_object<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "BooleanObject")?.call1((value,))
}

/// `raise LimitReachedError(message)`.
fn limit_reached(py: Python<'_>, message: &str) -> PyErr {
    errors::limit_reached_error(py, message)
}

/// `raise PdfReadError(message)`.
fn pdf_read_error(py: Python<'_>, message: &str) -> PyErr {
    errors::pdf_read_error(py, message)
}

/// `a - b` through Python's operator protocol.
fn py_sub<'py>(a: &Bound<'py, PyAny>, b: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    a.call_method1("__sub__", (b,))
}

/// `a & b` through Python's operator protocol (`int & IntFlag` must stay
/// `IntFlag`-typed, so `__and__` cannot be called directly).
fn py_bitand<'py>(
    py: Python<'py>,
    a: &Bound<'py, PyAny>,
    b: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    py.import("operator")?.getattr("and_")?.call1((a, b))
}

/// Materialise any Python iterable into a `Vec` of its items.
fn list_items<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let mut items = Vec::new();
    let mut iterator = obj.try_iter()?;
    while let Some(item) = next_item(&mut iterator)? {
        items.push(item);
    }
    Ok(items)
}

/// `isinstance(err, <module>.<name>)`.
fn err_is(py: Python<'_>, err: &PyErr, module: &str, name: &str) -> bool {
    match py.import(module).and_then(|module| module.getattr(name)) {
        Ok(class) => err.value(py).is_instance(&class).unwrap_or(false),
        Err(_) => false,
    }
}

/// `assert <condition>, <message>` failure.
fn assertion(py: Python<'_>, message: Option<&str>) -> PyErr {
    match message {
        Some(message) => PyAssertionError::new_err(message.to_string()),
        None => empty_exception(py, "AssertionError"),
    }
}

/// `raise PyPdfError(message)`.
fn pypdf_error(py: Python<'_>, message: &str) -> PyErr {
    errors::raise_msg(py, "PyPdfError", message)
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    info::register(module)?;
    docmeta::register(module)?;
    fields::register(module)?;
    outline::register(module)?;
    dests::register(module)?;
    pages::register(module)?;
    Ok(())
}
