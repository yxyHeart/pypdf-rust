//! Rust port of `pypdf/generic/_data_structures.py` (upstream pypdf 7.1.1).
//!
//! # Design
//!
//! The container classes (`ArrayObject`, `DictionaryObject`, `TreeObject`,
//! `StreamObject`, `ContentStream`, `Field`, `Destination`, ...) are *real Python
//! classes* defined in the hand-written shim
//! `python/pypdf/generic/_data_structures.py`, exactly like upstream: they
//! subclass `list`/`dict` and `PdfObject`, keep upstream's class attributes,
//! properties and decorators.  Every method body delegates to a `#[pyfunction]`
//! implemented here, so all logic (parsing, cloning, tree bookkeeping) lives in
//! Rust while the Python object model stays byte-for-byte compatible with pypdf.
//!
//! # Fidelity notes
//!
//! * Cross-object work is done by *calling the Python methods*, never by
//!   re-implementing them here (`src.raw_get(...)`, `src["/Type"]`,
//!   `cur_obj._new_for_clone()`, `child.get_object()`, ...).  Upstream's
//!   behaviour for subclasses and monkeypatching is defined by those dynamic
//!   lookups, and the official tests count Python frames, so an "optimised"
//!   direct call would be observable.
//! * Clone bookkeeping is *not* reimplemented: `_reference_clone` (implemented by
//!   `crate::base_mod::reference_clone`) is reached through the inherited Python
//!   method, which keeps the exact upstream call graph.
//! * Byte-level parsing uses the same stream protocol (`read`/`seek`/`tell`) and
//!   the same helper functions from `pypdf._utils` / `pypdf.generic._utils`.
//! * No `HashMap`/`HashSet` iteration can affect output: the only Rust sets
//!   store object ids for cycle detection and are used for membership only.
//!   Sets that upstream hands to recursive calls (`DictionaryObject._clone`'s
//!   `visited`) stay Python sets so the shared state is preserved across the
//!   Python-level recursion.

use std::collections::HashSet;

use pyo3::exceptions::{
    PyAssertionError, PyAttributeError, PyKeyError, PyMemoryError, PyRecursionError, PyTypeError,
    PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyInt, PyList, PyModule, PySet, PyString, PyTuple};

use crate::base_mod;
use crate::errors;
use crate::pyutil;

/// Upstream module name; used as `logger_warning(..., source=__name__)`.
const SOURCE: &str = "pypdf.generic._data_structures";

/// `CONTENT_STREAM_ARRAY_MAX_LENGTH` (upstream module constant).
const CONTENT_STREAM_ARRAY_MAX_LENGTH: usize = 10_000;

/// `_MAX_LEADING_COMMENTS` (upstream module constant).
const MAX_LEADING_COMMENTS: usize = 1000;

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("builtins")
}

fn shim<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("pypdf.generic._data_structures")
}

/// Fetch a name from this module's shim (upstream references these by global name).
fn shim_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim(py)?.getattr(name)
}

/// Fetch a class from the shim module.
fn shim_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, name)
}

fn base_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf.generic._base")?.getattr(name)
}

fn utils_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("pypdf._utils")
}

fn generic_utils_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("pypdf.generic._utils")
}

fn constants_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf.constants")?.getattr(name)
}

/// `from .._configuration import get_configuration` (the public `pypdf.get_configuration`).
fn configuration<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf")?.call_method0("get_configuration")
}

fn config_item<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    configuration(py)?.getattr(name)
}

fn py_bytes<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, data).into_any()
}

fn py_int<'py>(py: Python<'py>, value: i64) -> Bound<'py, PyAny> {
    PyInt::new(py, value).into_any()
}

fn py_str<'py>(py: Python<'py>, value: &str) -> Bound<'py, PyAny> {
    PyString::new(py, value).into_any()
}

fn py_list<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyList::new(py, items)?.into_any())
}

fn py_tuple<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyTuple::new(py, items)?.into_any())
}

/// `id(obj)` as a Python int (used as a dict key, exactly like upstream).
fn py_id<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("id")?.call1((obj,))
}

/// Rust-side identity of a Python object (membership-only use).
fn object_id(obj: &Bound<'_, PyAny>) -> usize {
    obj.as_ptr() as usize
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

fn assertion(py: Python<'_>, message: Option<&str>) -> PyErr {
    match message {
        Some(message) => PyAssertionError::new_err(message.to_string()),
        None => empty_exception(py, "AssertionError"),
    }
}

/// `isinstance(err, <module>.<name>)`.
fn err_is(py: Python<'_>, err: &PyErr, module: &str, name: &str) -> bool {
    match py.import(module).and_then(|module| module.getattr(name)) {
        Ok(class) => err
            .value(py)
            .is_instance(&class)
            .unwrap_or(false),
        Err(_) => false,
    }
}

fn err_is_limit_reached(py: Python<'_>, err: &PyErr) -> bool {
    err_is(py, err, "pypdf.errors", "LimitReachedError")
}

fn err_is_pdf_read_error(py: Python<'_>, err: &PyErr) -> bool {
    err_is(py, err, "pypdf.errors", "PdfReadError")
}

/// `raise <Exception>(...) from exc`: sets `__cause__` (and therefore
/// `__suppress_context__`) plus `__context__`, exactly like CPython's
/// `raise ... from ...` inside an `except` block.
fn raise_from(new_error: PyErr, py: Python<'_>, cause: PyErr) -> PyErr {
    new_error.set_cause(py, Some(cause.clone_ref(py)));
    new_error.set_context(py, Some(cause));
    new_error
}

/// `logger_warning(message, source=__name__, **values)`.
fn log_warning(
    py: Python<'_>,
    message: &str,
    values: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    pyutil::logger_warning(py, SOURCE, message, values)
}

/// `obj.get(key, default)` by dynamic dispatch (DictionaryObject subclasses may override `get`).
fn dict_get<'py>(
    obj: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    default: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    obj.call_method1("get", (key, default))
}

/// `stream.read(n)`.
fn read_n<'py>(stream: &Bound<'py, PyAny>, n: usize) -> PyResult<Bound<'py, PyAny>> {
    stream.call_method1("read", (n,))
}

fn seek(stream: &Bound<'_, PyAny>, offset: i64, whence: i64) -> PyResult<()> {
    stream.call_method1("seek", (offset, whence))?;
    Ok(())
}

fn seek_to(stream: &Bound<'_, PyAny>, position: &Bound<'_, PyAny>) -> PyResult<()> {
    stream.call_method1("seek", (position,))?;
    Ok(())
}

fn tell<'py>(stream: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    stream.call_method0("tell")
}

/// `len(obj)`.
fn length(obj: &Bound<'_, PyAny>) -> PyResult<usize> {
    obj.len()
}

/// `obj == <bytes literal>`.
fn eq_bytes(obj: &Bound<'_, PyAny>, py: Python<'_>, data: &[u8]) -> PyResult<bool> {
    obj.eq(py_bytes(py, data))
}

/// `value in _SCALAR_TYPES` where `value` is `type(instance)`.
fn is_scalar_type(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    let scalar_types = shim_attr(py, "_SCALAR_TYPES")?;
    scalar_types.contains(value.get_type())
}

/// `type(value) in _SCALAR_TYPES`.
fn value_is_scalar(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    is_scalar_type(py, &value.get_type())
}

/// `type(value).__name__`.
fn type_name(value: &Bound<'_, PyAny>) -> PyResult<String> {
    pyutil::py_str(&value.get_type().getattr("__name__")?)
}

/// `repr(value)[:500]` (upstream truncates at 500 *characters*).
fn repr_truncated(value: &Bound<'_, PyAny>, limit: usize) -> PyResult<String> {
    let rendered = pyutil::py_repr(value)?;
    Ok(rendered.chars().take(limit).collect())
}

/// `hex(value)` as a string.
fn py_hex(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<String> {
    pyutil::py_str(&builtins(py)?.getattr("hex")?.call1((value,))?)
}

/// `stream.seek(-len(rest), 1)` style helper for a Python bytes object.
fn seek_back(stream: &Bound<'_, PyAny>, back: usize) -> PyResult<()> {
    seek(stream, -(back as i64), 1)
}

/// `str(obj)` (for exception messages).
fn str_of(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    pyutil::py_str(obj)
}

/// `"%s" % values`-style formatting (upstream uses `msg % values` with a dict).
fn percent_format(py: Python<'_>, template: &str, values: &Bound<'_, PyDict>) -> PyResult<String> {
    let formatted = py_str(py, template).call_method1("__mod__", (values,))?;
    pyutil::py_str(&formatted)
}

/// Build `set()` or a shared Python set.
fn new_set<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PySet::empty(py)?.into_any())
}

/// `tuple((idnum, generation))`.
fn id_generation_key<'py>(
    py: Python<'py>,
    idnum: &Bound<'py, PyAny>,
    generation: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    py_tuple(py, vec![idnum.clone(), generation.clone()])
}

/// `min(a, b)` through Python (preserves the exact int type/behaviour).
fn py_min<'py>(
    py: Python<'py>,
    a: &Bound<'py, PyAny>,
    b: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("min")?.call1((a, b))
}

// ---------------------------------------------------------------------------
// ArrayObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._data_structures.ArrayObject.replicate
#[pyfunction]
fn datastructures_arrayobject_replicate<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let array_class = shim_class(py, "ArrayObject")?;
    let empty = array_class.call0()?;
    let arr = slf.call_method1("_reference_clone", (empty, pdf_dest, false))?;
    for data in slf.try_iter()? {
        let data = data?;
        if data.hasattr("replicate")? {
            let replicated = data.call_method1("replicate", (pdf_dest,))?;
            arr.call_method1("append", (replicated,))?;
        } else {
            arr.call_method1("append", (data,))?;
        }
    }
    Ok(arr.unbind())
}

/// Upstream `try: if self.indirect_reference.pdf == pdf_dest and not force_duplicate: return self`.
///
/// Returns `Ok(true)` when the shortcut applies.  `MemoryError` and every other
/// exception are propagated to the caller, which mirrors upstream's
/// `except MemoryError: raise` / `except Exception: pass`.
fn indirect_reference_matches(
    slf: &Bound<'_, PyAny>,
    pdf_dest: &Bound<'_, PyAny>,
    force_duplicate: bool,
) -> PyResult<bool> {
    let reference = slf.getattr("indirect_reference")?;
    let reference_pdf = reference.getattr("pdf")?;
    let matches = reference_pdf.eq(pdf_dest)?;
    Ok(matches && !force_duplicate)
}

/// Upstream's `try/except` wrapper around `indirect_reference_matches`.
///
/// Returns `Ok(true)` for "return self", `Ok(false)` to continue cloning and
/// propagates `MemoryError` (upstream re-raises it) while swallowing the other
/// exceptions (upstream `except Exception: pass`).
fn clone_shortcut(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    pdf_dest: &Bound<'_, PyAny>,
    force_duplicate: bool,
) -> PyResult<bool> {
    match indirect_reference_matches(slf, pdf_dest, force_duplicate) {
        Ok(applies) => Ok(applies),
        Err(error) if error.is_instance_of::<PyMemoryError>(py) => Err(error),
        Err(_) => Ok(false),
    }
}

// @pypdf: pypdf.generic._data_structures.ArrayObject.clone
#[pyfunction]
fn datastructures_arrayobject_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
    ignore_fields: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if clone_shortcut(py, slf, pdf_dest, force_duplicate)? {
        return Ok(slf.clone().unbind());
    }
    let array_class = shim_class(py, "ArrayObject")?;
    let arr = slf.call_method1(
        "_reference_clone",
        (array_class.call0()?, pdf_dest, true),
    )?;
    let stream_class = shim_class(py, "StreamObject")?;
    let indirect_class = base_class(py, "IndirectObject")?;
    for data in slf.try_iter()? {
        let data = data?;
        if value_is_scalar(py, &data)? {
            let cloned = data.call_method1(
                "clone",
                (pdf_dest, force_duplicate, ignore_fields),
            )?;
            arr.call_method1("append", (cloned,))?;
        } else if data.is_instance(&stream_class)? {
            let cloned = data.call_method1(
                "clone",
                (pdf_dest, force_duplicate, ignore_fields),
            )?;
            let dup = data.call_method1(
                "_reference_clone",
                (cloned, pdf_dest, force_duplicate),
            )?;
            let reference = dup.getattr("indirect_reference")?;
            arr.call_method1("append", (reference,))?;
        } else if data.is_instance(&indirect_class)? {
            let resolved = data.call_method0("get_object")?;
            if resolved.is_instance(&stream_class)? {
                let kwargs = PyDict::new(py);
                kwargs.set_item("force_duplicate", true)?;
                kwargs.set_item("ignore_fields", ignore_fields)?;
                let cloned = resolved.call_method("clone", (pdf_dest,), Some(&kwargs))?;
                let dup = data.call_method1(
                    "_reference_clone",
                    (cloned, pdf_dest, force_duplicate),
                )?;
                let reference = dup.getattr("indirect_reference")?;
                arr.call_method1("append", (reference,))?;
            } else if data.hasattr("clone")? {
                let cloned = data.call_method1(
                    "clone",
                    (pdf_dest, force_duplicate, ignore_fields),
                )?;
                arr.call_method1("append", (cloned,))?;
            } else {
                arr.call_method1("append", (data,))?;
            }
        } else if data.hasattr("clone")? {
            let cloned = data.call_method1(
                "clone",
                (pdf_dest, force_duplicate, ignore_fields),
            )?;
            arr.call_method1("append", (cloned,))?;
        } else {
            arr.call_method1("append", (data,))?;
        }
    }
    Ok(arr.unbind())
}

// @pypdf: pypdf.generic._data_structures.ArrayObject.hash_bin
#[pyfunction]
fn datastructures_arrayobject_hash_bin<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<isize> {
    let mut item_hashes: Vec<Bound<'py, PyAny>> = Vec::new();
    for item in slf.try_iter()? {
        let item = item?;
        item_hashes.push(item.call_method0("hash_bin")?);
    }
    let tuple = py_tuple(py, item_hashes)?;
    py_tuple(py, vec![slf.get_type().into_any(), tuple])?.hash()
}

// @pypdf: pypdf.generic._data_structures.ArrayObject.items
#[pyfunction]
fn datastructures_arrayobject_items<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(builtins(py)?
        .getattr("enumerate")?
        .call1((slf,))?
        .unbind())
}

// @pypdf: pypdf.generic._data_structures.ArrayObject._to_lst
#[pyfunction]
fn datastructures_arrayobject_to_lst<'py>(
    py: Python<'py>,
    _slf: &Bound<'py, PyAny>,
    lst: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let builtins_module = builtins(py)?;
    let list_type = builtins_module.getattr("list")?;
    let tuple_type = builtins_module.getattr("tuple")?;
    let set_type = builtins_module.getattr("set")?;
    let str_type = builtins_module.getattr("str")?;
    let bytes_type = builtins_module.getattr("bytes")?;
    if lst.is_instance(&list_type)?
        || lst.is_instance(&tuple_type)?
        || lst.is_instance(&set_type)?
    {
        return Ok(list_type.call1((lst,))?.unbind());
    }
    // Upstream `isinstance(lst, PdfObject)`; the port accepts the documented
    // PDF-object protocol surface as well (see PORTING_GUIDE §1.4).
    if base_mod::is_pdf_object_like(py, lst)? {
        return Ok(py_list(py, vec![lst.clone()])?.unbind());
    }
    if lst.is_instance(&str_type)? {
        let first = lst.get_item(0)?;
        let item = if first.eq(py_str(py, "/"))? {
            base_class(py, "NameObject")?.call1((lst,))?
        } else {
            base_class(py, "TextStringObject")?.call1((lst,))?
        };
        return Ok(py_list(py, vec![item])?.unbind());
    }
    if lst.is_instance(&bytes_type)? {
        let item = base_class(py, "ByteStringObject")?.call1((lst,))?;
        return Ok(py_list(py, vec![item])?.unbind());
    }
    Ok(py_list(py, vec![lst.clone()])?.unbind())
}

// @pypdf: pypdf.generic._data_structures.ArrayObject.__add__
#[pyfunction]
fn datastructures_arrayobject_add<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    lst: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let array_class = shim_class(py, "ArrayObject")?;
    let temp = array_class.call1((slf,))?;
    let converted = slf.call_method1("_to_lst", (lst,))?;
    temp.call_method1("extend", (converted,))?;
    Ok(temp.unbind())
}

// @pypdf: pypdf.generic._data_structures.ArrayObject.__iadd__
#[pyfunction]
fn datastructures_arrayobject_iadd<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    lst: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let converted = slf.call_method1("_to_lst", (lst,))?;
    slf.call_method1("extend", (converted,))?;
    Ok(slf.clone().unbind())
}

// @pypdf: pypdf.generic._data_structures.ArrayObject.__isub__
#[pyfunction]
fn datastructures_arrayobject_isub<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    lst: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let converted = slf.call_method1("_to_lst", (lst,))?;
    for item in converted.try_iter()? {
        let item = item?;
        match slf.call_method1("index", (&item,)) {
            Ok(index) => {
                slf.del_item(&index)?;
            }
            Err(error) if error.is_instance_of::<PyValueError>(py) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(slf.clone().unbind())
}

// @pypdf: pypdf.generic._data_structures.ArrayObject.write_to_stream
#[pyfunction]
fn datastructures_arrayobject_write_to_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    stream.call_method1("write", (py_bytes(py, b"["),))?;
    for data in slf.try_iter()? {
        let data = data?;
        stream.call_method1("write", (py_bytes(py, b" "),))?;
        data.call_method1("write_to_stream", (stream,))?;
    }
    stream.call_method1("write", (py_bytes(py, b" ]"),))?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.ArrayObject._check_limits
#[pyfunction]
#[pyo3(signature = (remaining_elements, parsing_budget=None))]
fn datastructures_arrayobject_check_limits<'py>(
    py: Python<'py>,
    remaining_elements: &Bound<'py, PyAny>,
    parsing_budget: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let remaining = remaining_elements.call_method1("__sub__", (py_int(py, 1),))?;
    if remaining.lt(py_int(py, 0))? {
        return Err(errors::limit_reached_error(
            py,
            "Reached maximum array element count.",
        ));
    }
    if let Some(budget) = parsing_budget {
        let current_budget = budget.getattr("remaining_budget")?;
        let next_budget = current_budget.call_method1("__sub__", (py_int(py, 1),))?;
        budget.setattr("remaining_budget", &next_budget)?;
        if next_budget.lt(py_int(py, 0))? {
            return Err(errors::limit_reached_error(
                py,
                "Reached maximum number of tokens.",
            ));
        }
    }
    Ok(remaining.unbind())
}

// @pypdf: pypdf.generic._data_structures.ArrayObject.read_from_stream
#[pyfunction]
#[pyo3(signature = (stream, pdf, forced_encoding=None, parsing_budget=None))]
fn datastructures_arrayobject_read_from_stream<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
    forced_encoding: Option<&Bound<'py, PyAny>>,
    parsing_budget: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let array_class = shim_class(py, "ArrayObject")?;
    let arr = array_class.call0()?;
    let tmp = read_n(stream, 1)?;
    if !eq_bytes(&tmp, py, b"[")? {
        return Err(errors::pdf_read_error(py, "Could not read array"));
    }
    let mut remaining = config_item(py, "array_maximum_element_count")?;
    let utils = utils_module(py)?;
    let whitespaces = utils.getattr("WHITESPACES")?;
    loop {
        let mut tok = read_n(stream, 1)?;
        loop {
            let is_space = tok.call_method0("isspace")?.is_truthy()?;
            if !is_space && !whitespaces.contains(&tok)? {
                break;
            }
            tok = read_n(stream, 1)?;
        }
        if eq_bytes(&tok, py, b"")? {
            break;
        }
        if eq_bytes(&tok, py, b"%")? {
            seek(stream, -1, 1)?;
            utils.call_method1("skip_over_comment", (stream,))?;
            continue;
        }
        if eq_bytes(&tok, py, b"]")? {
            break;
        }
        let kwargs = PyDict::new(py);
        kwargs.set_item("remaining_elements", &remaining)?;
        match parsing_budget {
            Some(budget) => kwargs.set_item("parsing_budget", budget)?,
            None => kwargs.set_item("parsing_budget", py.None().into_bound(py))?,
        }
        let next_remaining = array_class
            .getattr("_check_limits")?
            .call((), Some(&kwargs))?;
        remaining = next_remaining;
        if eq_bytes(&tok, py, b"n")? {
            // Shortcut for null, as some arrays consist mostly of nulls.
            let rest = read_n(stream, 3)?;
            if eq_bytes(&rest, py, b"ull")? {
                let null = base_class(py, "NullObject")?.call0()?;
                arr.call_method1("append", (null,))?;
                continue;
            }
            let rest_len = length(&rest)?;
            seek_back(stream, rest_len)?;
        }
        seek(stream, -1, 1)?;
        let encoding = match forced_encoding {
            Some(encoding) => encoding.clone(),
            None => py.None().into_bound(py),
        };
        let kwargs = PyDict::new(py);
        if let Some(budget) = parsing_budget {
            kwargs.set_item("parsing_budget", budget)?;
        }
        let value = shim_attr(py, "read_object")?.call((stream, pdf, encoding), Some(&kwargs))?;
        arr.call_method1("append", (value,))?;
    }
    Ok(arr.unbind())
}

// ---------------------------------------------------------------------------
// _requires_constructor_arguments
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._data_structures._requires_constructor_arguments
#[pyfunction]
fn datastructures_requires_constructor_arguments<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
) -> PyResult<bool> {
    let inspect = py.import("inspect")?;
    let init = cls.getattr("__init__")?;
    let signature = inspect.call_method1("signature", (init,))?;
    let parameters = signature.getattr("parameters")?;
    let parameter_values = parameters.call_method0("values")?;
    let values = py
        .import("builtins")?
        .getattr("list")?
        .call1((parameter_values,))?;
    let parameter_class = inspect.getattr("Parameter")?;
    let empty = parameter_class.getattr("empty")?;
    let positional_only = parameter_class.getattr("POSITIONAL_ONLY")?;
    let positional_or_keyword = parameter_class.getattr("POSITIONAL_OR_KEYWORD")?;
    let keyword_only = parameter_class.getattr("KEYWORD_ONLY")?;
    let mut index = 0usize;
    for parameter in values.try_iter()? {
        let parameter = parameter?;
        index += 1;
        if index == 1 {
            // `list(parameters.values())[1:]` skips `self`.
            continue;
        }
        let default = parameter.getattr("default")?;
        if !is_same(&default, &empty) {
            continue;
        }
        let kind = parameter.getattr("kind")?;
        if kind.eq(&positional_only)?
            || kind.eq(&positional_or_keyword)?
            || kind.eq(&keyword_only)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `builtins.list(obj)`.
fn builtins_list<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("list")?.call1((obj,))
}

/// `getattr(obj, name, None)`.
fn getattr_or_none<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    name: &str,
) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("getattr")?.call1((obj, name, py.None()))
}

/// `pdf is not None and pdf.strict`.
fn pdf_strict(_py: Python<'_>, pdf: &Bound<'_, PyAny>) -> PyResult<bool> {
    if pdf.is_none() {
        return Ok(false);
    }
    pdf.getattr("strict")?.is_truthy()
}

/// `PdfStreamError(STREAM_TRUNCATED_PREMATURELY)`.
fn stream_truncated_error(py: Python<'_>) -> PyErr {
    let constant = match py
        .import("pypdf.errors")
        .and_then(|module| module.getattr("STREAM_TRUNCATED_PREMATURELY"))
    {
        Ok(value) => value,
        Err(error) => return error,
    };
    match PyTuple::new(py, [constant]) {
        Ok(args) => errors::raise(py, "PdfStreamError", args),
        Err(error) => error,
    }
}

/// `parsing_budget.remaining_budget -= amount` then raise when it goes negative.
fn decrement_budget(
    py: Python<'_>,
    parsing_budget: Option<&Bound<'_, PyAny>>,
    amount: i64,
    message: &str,
) -> PyResult<()> {
    let Some(budget) = parsing_budget else {
        return Ok(());
    };
    let current = budget.getattr("remaining_budget")?;
    let next = current.call_method1("__sub__", (py_int(py, amount),))?;
    budget.setattr("remaining_budget", &next)?;
    if next.lt(py_int(py, 0))? {
        return Err(errors::limit_reached_error(py, message));
    }
    Ok(())
}

/// `obj[start:stop]` for bytes-like objects (Python slicing semantics).
fn slice_object<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    start: Option<i64>,
    stop: Option<i64>,
) -> PyResult<Bound<'py, PyAny>> {
    let start = start.map(|value| py_int(py, value));
    let stop = stop.map(|value| py_int(py, value));
    let slice = pyutil::py_slice(py, start.as_ref(), stop.as_ref(), None)?;
    obj.get_item(&slice)
}

/// True when `obj` is still the shim's own `def` (i.e. not monkeypatched).
fn is_default_shim_function(
    obj: &Bound<'_, PyAny>,
    module: &str,
    qualname: &str,
) -> PyResult<bool> {
    let obj_module = match obj.getattr("__module__") {
        Ok(value) => value.extract::<String>().unwrap_or_default(),
        Err(_) => return Ok(false),
    };
    if obj_module != module {
        return Ok(false);
    }
    let obj_qualname = match obj.getattr("__qualname__") {
        Ok(value) => value.extract::<String>().unwrap_or_default(),
        Err(_) => return Ok(false),
    };
    Ok(obj_qualname == qualname || obj_qualname.ends_with(&format!(".{qualname}")))
}

/// `read_object(stream, pdf[, forced_encoding], parsing_budget=...)` via the
/// module-level function, so recursive calls go through the same Python entry
/// point as upstream.
fn call_read_object<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
    forced_encoding: Option<&Bound<'py, PyAny>>,
    parsing_budget: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let read_object = shim_attr(py, "read_object")?;
    // Fast path: for content streams this is called once per token (hundreds of
    // thousands of times per page), so call the Rust implementation directly
    // instead of bouncing through the Python shim wrapper.  Monkeypatching the
    // module-level `read_object` still works: the identity check below falls
    // back to the Python entry point.
    if is_default_shim_function(&read_object, "pypdf.generic._data_structures", "read_object")? {
        let result = datastructures_read_object(py, stream, pdf, forced_encoding, parsing_budget)?;
        return Ok(result.into_bound(py));
    }
    let kwargs = PyDict::new(py);
    if let Some(budget) = parsing_budget {
        kwargs.set_item("parsing_budget", budget)?;
    }
    match forced_encoding {
        Some(encoding) => read_object.call((stream, pdf, encoding), Some(&kwargs)),
        None => read_object.call((stream, pdf), Some(&kwargs)),
    }
}

// ---------------------------------------------------------------------------
// DictionaryObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._data_structures.DictionaryObject._new_for_clone
#[pyfunction]
fn datastructures_dictionaryobject_new_for_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let cls = slf.get_type();
    let requires = shim_attr(py, "_requires_constructor_arguments")?
        .call1((&cls,))?
        .is_truthy()?;
    if requires {
        // `self.__class__.__new__(self.__class__)` -- skips `__init__`.
        return Ok(cls.getattr("__new__")?.call1((&cls,))?.unbind());
    }
    Ok(cls.call0()?.unbind())
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject.replicate
#[pyfunction]
fn datastructures_dictionaryobject_replicate<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let fresh = slf.call_method0("_new_for_clone")?;
    let destination = slf.call_method1("_reference_clone", (fresh, pdf_dest, false))?;
    let items = slf.call_method0("items")?;
    for item in items.try_iter()? {
        let item = item?;
        let key = item.get_item(0)?;
        let value = item.get_item(1)?;
        let new_key = key.call_method1("replicate", (pdf_dest,))?;
        let new_value = if value.hasattr("replicate")? {
            value.call_method1("replicate", (pdf_dest,))?
        } else {
            value
        };
        destination.set_item(new_key, new_value)?;
    }
    Ok(destination.unbind())
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject.clone
#[pyfunction]
fn datastructures_dictionaryobject_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
    ignore_fields: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if clone_shortcut(py, slf, pdf_dest, force_duplicate)? {
        return Ok(slf.clone().unbind());
    }
    let visited = new_set(py)?;
    let fresh = slf.call_method0("_new_for_clone")?;
    let destination =
        slf.call_method1("_reference_clone", (fresh, pdf_dest, force_duplicate))?;
    let ignore = if ignore_fields.is_none() {
        builtins_list(py, &py_tuple(py, vec![])?)?
    } else {
        ignore_fields.clone()
    };
    if length(&destination)? == 0 {
        destination.call_method1(
            "_clone",
            (slf, pdf_dest, force_duplicate, ignore, visited),
        )?;
    }
    Ok(destination.unbind())
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject._clone
#[pyfunction]
fn datastructures_dictionaryobject_clone_inner<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    src: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
    ignore_fields: &Bound<'py, PyAny>,
    visited: &Bound<'py, PyAny>,
) -> PyResult<()> {
    if ignore_fields.is_none() {
        return Err(assertion(py, None));
    }
    let dictionary_class = shim_class(py, "DictionaryObject")?;
    let name_class = base_class(py, "NameObject")?;
    let indirect_class = base_class(py, "IndirectObject")?;
    let stream_class = shim_class(py, "StreamObject")?;
    let int_type = builtins(py)?.getattr("int")?;

    // First remove the ignore_fields that are only for a limited number of levels.
    let ignore = builtins_list(py, ignore_fields)?;
    let mut index = 0usize;
    while index < ignore.len()? {
        let item = ignore.get_item(index)?;
        if item.is_instance(&int_type)? {
            if item.le(py_int(py, 0))? {
                ignore.del_item(index)?;
                ignore.del_item(index)?;
                continue;
            }
            let decremented = item.call_method1("__sub__", (py_int(py, 1),))?;
            ignore.set_item(index, decremented)?;
        }
        index += 1;
    }

    // Check whether this is a linked list of dictionaries: iterate it to avoid
    // recursion (upstream loops over `/Next`, `/Prev`, `/N`, `/V`).
    let mut is_chain = false;
    for field in ["/Next", "/Prev", "/N", "/V"] {
        let field_key = py_str(py, field);
        if ignore.contains(&field_key)? {
            continue;
        }
        if !src.contains(&field_key)? {
            continue;
        }
        let raw = src.call_method1("raw_get", (&field_key,))?;
        if !raw.is_instance(&indirect_class)? {
            continue;
        }
        let resolved = src.get_item(&field_key)?;
        if !resolved.is_instance(&dictionary_class)? {
            continue;
        }
        let src_type = dict_get(src, &py_str(py, "/Type"), py.None().into_bound(py))?;
        if src_type.is_none() {
            is_chain = true;
            break;
        }
        let resolved_type = dict_get(&resolved, &py_str(py, "/Type"), py.None().into_bound(py))?;
        if resolved_type.is_none() || src_type.eq(&resolved_type)? {
            is_chain = true;
            break;
        }
    }

    if is_chain {
        let ignore = builtins_list(py, &ignore)?;
        for list_index in 0..2 {
            let keys: [&str; 2] = if list_index == 0 {
                ["/Next", "/Prev"]
            } else {
                ["/N", "/V"]
            };
            for key in keys {
                let field_key = py_str(py, key);
                let condition = (|| -> PyResult<bool> {
                    if !src.contains(&field_key)? {
                        return Ok(false);
                    }
                    if slf.contains(&field_key)? {
                        return Ok(false);
                    }
                    let raw = src.call_method1("raw_get", (&field_key,))?;
                    if !raw.is_instance(&indirect_class)? {
                        return Ok(false);
                    }
                    let resolved = src.get_item(&field_key)?;
                    if !resolved.is_instance(&dictionary_class)? {
                        return Ok(false);
                    }
                    let src_type = dict_get(src, &py_str(py, "/Type"), py.None().into_bound(py))?;
                    if src_type.is_none() {
                        return Ok(true);
                    }
                    let resolved_type =
                        dict_get(&resolved, &py_str(py, "/Type"), py.None().into_bound(py))?;
                    Ok(resolved_type.is_none() || src_type.eq(&resolved_type)?)
                })()?;
                if !condition {
                    continue;
                }
                let mut current: Option<Bound<'py, PyAny>> = Some(src.get_item(&field_key)?);
                let mut previous: Bound<'py, PyAny> = slf.clone();
                let mut objects: Vec<(Bound<'py, PyAny>, Bound<'py, PyAny>)> = Vec::new();
                while let Some(cur_obj) = current.take() {
                    let source_reference = getattr_or_none(py, &cur_obj, "indirect_reference")?;
                    let fresh = cur_obj.call_method0("_new_for_clone")?;
                    let clon = cur_obj.call_method1(
                        "_reference_clone",
                        (fresh, pdf_dest, force_duplicate),
                    )?;
                    // Check whether the item has been processed before.
                    if !source_reference.is_none() {
                        let idnum = source_reference.getattr("idnum")?;
                        let generation = source_reference.getattr("generation")?;
                        let visit_key = id_generation_key(py, &idnum, &generation)?;
                        if visited.contains(&visit_key)? {
                            break;
                        }
                        visited.call_method1("add", (&visit_key,))?;
                    }
                    objects.push((cur_obj.clone(), clon.clone()));
                    let clone_reference = getattr_or_none(py, &clon, "indirect_reference")?;
                    let assigned = if clone_reference.is_none() {
                        clon.clone()
                    } else {
                        clone_reference
                    };
                    let name_key = name_class.call1((&field_key,))?;
                    previous.set_item(name_key, assigned)?;
                    previous = clon.clone();
                    let outcome: PyResult<Option<Bound<'py, PyAny>>> = (|| {
                        if cur_obj.eq(src)? {
                            return Ok(None);
                        }
                        let next_obj = cur_obj.get_item(&field_key)?;
                        if next_obj.is_instance(&dictionary_class)? {
                            Ok(Some(next_obj))
                        } else {
                            Ok(None)
                        }
                    })();
                    match outcome {
                        Ok(Some(next)) => current = Some(next),
                        Ok(None) => current = None,
                        Err(error) if error.is_instance_of::<PyMemoryError>(py) => {
                            return Err(error);
                        }
                        Err(_) => current = None,
                    }
                }
                for (source, clone) in objects {
                    clone.call_method1(
                        "_clone",
                        (&source, pdf_dest, force_duplicate, &ignore, visited),
                    )?;
                }
            }
        }
    }

    let items = src.call_method0("items")?;
    for item in items.try_iter()? {
        let item = item?;
        let key = item.get_item(0)?;
        let value = item.get_item(1)?;
        if ignore.contains(&key)? {
            continue;
        }
        let is_scalar = value_is_scalar(py, &value)?;
        if !is_scalar && value.is_instance(&stream_class)? {
            if !value.hasattr("indirect_reference")? {
                value.setattr("indirect_reference", py.None())?;
            }
            let cloned = value.call_method1(
                "clone",
                (pdf_dest, force_duplicate, &ignore),
            )?;
            let reference = cloned.getattr("indirect_reference")?;
            if reference.is_none() {
                return Err(assertion(py, Some("mypy")));
            }
            let cloned_key = key.call_method1("clone", (pdf_dest,))?;
            slf.set_item(cloned_key, reference)?;
        } else if !slf.contains(&key)? {
            let new_value = if value.hasattr("clone")? {
                value.call_method1("clone", (pdf_dest, force_duplicate, &ignore))?
            } else {
                value
            };
            let name_key = name_class.call1((&key,))?;
            slf.set_item(name_key, new_value)?;
        }
    }
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject.hash_bin
#[pyfunction]
fn datastructures_dictionaryobject_hash_bin<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<isize> {
    let items = slf.call_method0("items")?;
    let mut pairs: Vec<Bound<'py, PyAny>> = Vec::new();
    for item in items.try_iter()? {
        let item = item?;
        let key = item.get_item(0)?;
        let value = item.get_item(1)?;
        let value_hash = value.call_method0("hash_bin")?;
        pairs.push(py_tuple(py, vec![key, value_hash])?);
    }
    let tuple = py_tuple(py, pairs)?;
    py_tuple(py, vec![slf.get_type().into_any(), tuple])?.hash()
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject.raw_get
#[pyfunction]
fn datastructures_dictionaryobject_raw_get<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    // `dict.__getitem__(self, key)` -- bypasses the overridden `__getitem__`.
    let dict_type = builtins(py)?.getattr("dict")?;
    Ok(dict_type
        .getattr("__getitem__")?
        .call1((slf, key))?
        .unbind())
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject.get_inherited
#[pyfunction]
#[pyo3(signature = (slf, key, default=None))]
fn datastructures_dictionaryobject_get_inherited<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    default: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let dictionary_class = shim_class(py, "DictionaryObject")?;
    let default = match default {
        Some(value) => value.clone(),
        None => py.None().into_bound(py),
    };
    let mut current = slf.clone();
    let mut visited: HashSet<usize> = HashSet::new();
    let parent_key = py_str(py, "/Parent");
    loop {
        let obj_id = object_id(&current);
        if !visited.insert(obj_id) {
            let message = format!(
                "Detected cycle in /Parent hierarchy when retrieving value for key {}.",
                pyutil::py_repr(key)?
            );
            return Err(errors::limit_reached_error(py, &message));
        }
        if current.contains(key)? {
            return Ok(current.get_item(key)?.unbind());
        }
        if !current.contains(&parent_key)? {
            return Ok(default.unbind());
        }
        let parent_reference = current.get_item(&parent_key)?;
        let parent = parent_reference.call_method0("get_object")?;
        if !parent.is_instance(&dictionary_class)? {
            return Ok(default.unbind());
        }
        current = parent;
    }
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject.__setitem__
#[pyfunction]
fn datastructures_dictionaryobject_setitem<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if !base_mod::is_pdf_object_like(py, key)? {
        return Err(message_exception(py, "ValueError", "Key must be a PdfObject"));
    }
    if !base_mod::is_pdf_object_like(py, value)? {
        return Err(message_exception(py, "ValueError", "Value must be a PdfObject"));
    }
    let dict_type = builtins(py)?.getattr("dict")?;
    Ok(dict_type
        .getattr("__setitem__")?
        .call1((slf, key, value))?
        .unbind())
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject.setdefault
#[pyfunction]
#[pyo3(signature = (slf, key, value=None))]
fn datastructures_dictionaryobject_setdefault<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    value: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let value = match value {
        Some(value) => value.clone(),
        None => py.None().into_bound(py),
    };
    if !base_mod::is_pdf_object_like(py, key)? {
        return Err(message_exception(py, "ValueError", "Key must be a PdfObject"));
    }
    if !base_mod::is_pdf_object_like(py, &value)? {
        return Err(message_exception(py, "ValueError", "Value must be a PdfObject"));
    }
    let dict_type = builtins(py)?.getattr("dict")?;
    Ok(dict_type
        .getattr("setdefault")?
        .call1((slf, key, value))?
        .unbind())
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject.__getitem__
#[pyfunction]
fn datastructures_dictionaryobject_getitem<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let dict_type = builtins(py)?.getattr("dict")?;
    let value = dict_type.getattr("__getitem__")?.call1((slf, key))?;
    Ok(value.call_method0("get_object")?.unbind())
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject.xmp_metadata
#[pyfunction]
fn datastructures_dictionaryobject_xmp_metadata<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let xmp_class = py.import("pypdf.xmp")?.getattr("XmpInformation")?;
    let metadata = dict_get(slf, &py_str(py, "/Metadata"), py.None().into_bound(py))?;
    let is_null = base_class(py, "is_null_or_none")?
        .call1((&metadata,))?
        .is_truthy()?;
    if is_null {
        return Ok(py.None());
    }
    if metadata.is_none() {
        return Err(assertion(py, Some("mypy")));
    }
    let metadata = metadata.call_method0("get_object")?;
    Ok(xmp_class.call1((metadata,))?.unbind())
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject.write_to_stream
#[pyfunction]
fn datastructures_dictionaryobject_write_to_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    stream.call_method1("write", (py_bytes(py, b"<<\n"),))?;
    let items = slf.call_method0("items")?;
    let percent = py_str(py, "%");
    for item in items.try_iter()? {
        let item = item?;
        let key = item.get_item(0)?;
        let value = item.get_item(1)?;
        if length(&key)? > 2
            && key.get_item(1)?.eq(&percent)?
            && key.get_item(-1)?.eq(&percent)?
        {
            continue;
        }
        key.call_method1("write_to_stream", (stream,))?;
        stream.call_method1("write", (py_bytes(py, b" "),))?;
        value.call_method1("write_to_stream", (stream,))?;
        stream.call_method1("write", (py_bytes(py, b"\n"),))?;
    }
    stream.call_method1("write", (py_bytes(py, b">>"),))?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject._get_next_object_position
#[pyfunction]
fn datastructures_dictionaryobject_get_next_object_position<'py>(
    py: Python<'py>,
    _cls: &Bound<'py, PyAny>,
    position_before: &Bound<'py, PyAny>,
    position_end: &Bound<'py, PyAny>,
    generations: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let mut out = position_end.clone();
    let xref = pdf.getattr("xref")?;
    for generation in generations.try_iter()? {
        let generation = generation?;
        let entries = xref.get_item(&generation)?;
        let values = entries.call_method0("values")?;
        for position in values.try_iter()? {
            let position = position?;
            if position_before.lt(&position)? && position.le(position_end)? {
                out = py_min(py, &out, &position)?;
            }
        }
    }
    Ok(out.unbind())
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject._read_unsized_from_stream
#[pyfunction]
#[pyo3(signature = (cls, *, stream, pdf, length))]
fn datastructures_dictionaryobject_read_unsized_from_stream<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
    length: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let current_position = tell(stream)?;
    // try: seek to the end and read the size; finally: restore the position.
    let stream_length = {
        stream.call_method1("seek", (0, 2))?;
        let outcome = tell(stream);
        stream.call_method1("seek", (&current_position,))?;
        outcome?
    };
    let generations = builtins_list(py, &pdf.getattr("xref")?)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("position_before", &current_position)?;
    kwargs.set_item("position_end", &stream_length)?;
    kwargs.set_item("generations", generations)?;
    kwargs.set_item("pdf", pdf)?;
    let object_position = cls
        .getattr("_get_next_object_position")?
        .call((), Some(&kwargs))?;
    let bytes_to_read = object_position.call_method1("__sub__", (&current_position,))?;
    if bytes_to_read.ge(length)? {
        let message = format!(
            "Requested length of {} exceeds maximum allowed length.",
            str_of(&bytes_to_read)?
        );
        return Err(errors::limit_reached_error(py, &message));
    }
    let read_value = stream.call_method1("read", (&bytes_to_read,))?;
    let endstream_position = read_value.call_method1("find", (py_bytes(py, b"endstream"),))?;
    if endstream_position.lt(py_int(py, 0))? {
        let message = format!(
            "Unable to find 'endstream' marker for obj starting at {}.",
            str_of(&current_position)?
        );
        return Err(errors::pdf_read_error(py, &message));
    }
    let target = current_position
        .call_method1("__add__", (&endstream_position,))?
        .call_method1("__add__", (py_int(py, 9),))?;
    seek_to(stream, &target)?;
    let stop = endstream_position.call_method1("__sub__", (py_int(py, 1),))?;
    let slice = pyutil::py_slice(py, None, Some(&stop), None)?;
    Ok(read_value.get_item(&slice)?.unbind())
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject.read_from_stream
#[pyfunction]
#[pyo3(signature = (stream, pdf, forced_encoding=None, parsing_budget=None))]
fn datastructures_dictionaryobject_read_from_stream<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
    forced_encoding: Option<&Bound<'py, PyAny>>,
    parsing_budget: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let dictionary_class = shim_class(py, "DictionaryObject")?;
    let null_class = base_class(py, "NullObject")?;
    let name_class = base_class(py, "NameObject")?;
    let stream_class = shim_class(py, "StreamObject")?;
    let utils = utils_module(py)?;
    let tmp = read_n(stream, 2)?;
    if !eq_bytes(&tmp, py, b"<<")? {
        let message = format!(
            "Dictionary read error at byte {}: stream must begin with '<<'",
            py_hex(py, &tell(stream)?)?
        );
        return Err(errors::pdf_read_error(py, &message));
    }
    let data = PyDict::new(py);
    let mut remaining = config_item(py, "dictionary_maximum_element_count")?;
    loop {
        let tok = utils.call_method1("read_non_whitespace", (stream,))?;
        if eq_bytes(&tok, py, b"%")? {
            seek(stream, -1, 1)?;
            utils.call_method1("skip_over_comment", (stream,))?;
            continue;
        }
        if !tok.is_truthy()? {
            return Err(stream_truncated_error(py));
        }
        if eq_bytes(&tok, py, b">")? {
            read_n(stream, 1)?;
            break;
        }
        seek(stream, -1, 1)?;

        remaining = remaining.call_method1("__sub__", (py_int(py, 1),))?;
        if remaining.lt(py_int(py, 0))? {
            return Err(errors::limit_reached_error(
                py,
                "Reached maximum number of dictionary entries.",
            ));
        }
        decrement_budget(
            py,
            parsing_budget,
            2,
            "Reached maximum number of tokens.",
        )?;

        // Inner `try:` block: read the key.
        //
        // Upstream nests this in the outer `try`, so the bare `raise` of the
        // inner `except PdfReadError` handler does *not* leave `read_from_stream`:
        // it is caught again by the outer `except Exception as exc:` arm, which
        // (in strict mode) wraps it as `PdfReadError("Error reading PDF file.")`
        // with the original error as `__cause__`.  Routing the strict re-raise
        // through `handle_read_value_error` reproduces exactly that.
        let key = match call_read_object(py, stream, pdf, None, parsing_budget) {
            Ok(key) => {
                if key.is_instance(&null_class)? {
                    break;
                }
                if !key.is_instance(&name_class)? {
                    let message = format!(
                        "Expecting a NameObject for key but found {}",
                        pyutil::py_repr(&key)?
                    );
                    let error = errors::pdf_read_error(py, &message);
                    if pdf_strict(py, pdf)? {
                        // Upstream: inner `raise` -> outer `except Exception`.
                        return handle_read_value_error(py, pdf, &data, error);
                    }
                    let values = PyDict::new(py);
                    values.set_item("exception", error.value(py).clone().into_any())?;
                    log_warning(py, "%(exception)r", Some(&values))?;
                    continue;
                }
                key
            }
            Err(error) => {
                if err_is_pdf_read_error(py, &error) {
                    if pdf_strict(py, pdf)? {
                        // Upstream: inner `raise` -> outer `except Exception`
                        // -> `raise PdfReadError("Error reading PDF file.") from exc`.
                        return handle_read_value_error(py, pdf, &data, error);
                    }
                    let values = PyDict::new(py);
                    values.set_item("exception", error.value(py).clone().into_any())?;
                    log_warning(py, "%(exception)r", Some(&values))?;
                    continue;
                }
                // Not a PdfReadError: the outer handlers apply.
                return handle_read_value_error(py, pdf, &data, error);
            }
        };

        let _token = utils.call_method1("read_non_whitespace", (stream,))?;
        seek(stream, -1, 1)?;
        let value =
            match call_read_object(py, stream, pdf, forced_encoding, parsing_budget) {
                Ok(value) => value,
                Err(error) => return handle_read_value_error(py, pdf, &data, error),
            };
        if !data.contains(&key)? {
            data.set_item(&key, value)?;
        } else {
            // Multiple definitions of a key are not permitted.
            let message =
                "Multiple definitions in dictionary at byte %(position)s for key %(key)s";
            let values = PyDict::new(py);
            values.set_item("position", py_hex(py, &tell(stream)?)?)?;
            values.set_item("key", &key)?;
            if pdf_strict(py, pdf)? {
                return Err(errors::pdf_read_error(
                    py,
                    &percent_format(py, message, &values)?,
                ));
            }
            log_warning(py, message, Some(&values))?;
        }
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("stream", stream)?;
    kwargs.set_item("pdf", pdf)?;
    kwargs.set_item("data", &data)?;
    dictionary_class
        .getattr("_read_stream_data")?
        .call((), Some(&kwargs))?;
    if data.contains("__streamdata__")? {
        return Ok(stream_class
            .getattr("initialize_from_dictionary")?
            .call1((&data,))?
            .unbind());
    }
    let retval = dictionary_class.call0()?;
    retval.call_method1("update", (&data,))?;
    Ok(retval.unbind())
}

/// Exception handling for reading a dictionary *value*.
///
/// Mirrors upstream's
/// `except LimitReachedError: raise` /
/// `except (MemoryError, RecursionError) as exc: raise PdfReadError(...) from exc` /
/// `except Exception as exc: ...` (strict re-raises, non-strict returns partial data).
fn handle_read_value_error<'py>(
    py: Python<'py>,
    pdf: &Bound<'py, PyAny>,
    data: &Bound<'py, PyDict>,
    error: PyErr,
) -> PyResult<Py<PyAny>> {
    if err_is_limit_reached(py, &error) {
        return Err(error);
    }
    let is_memory = error.is_instance_of::<PyMemoryError>(py);
    let is_recursion = error.is_instance_of::<PyRecursionError>(py);
    if is_memory || is_recursion {
        let wrapped = errors::pdf_read_error(py, "Error reading PDF file.");
        return Err(raise_from(wrapped, py, error));
    }
    if pdf_strict(py, pdf)? {
        let wrapped = errors::pdf_read_error(py, "Error reading PDF file.");
        return Err(raise_from(wrapped, py, error));
    }
    let values = PyDict::new(py);
    values.set_item("exception", repr_truncated(&error.value(py).clone().into_any(), 500)?)?;
    log_warning(py, "%(exception)s", Some(&values))?;
    let retval = shim_class(py, "DictionaryObject")?.call0()?;
    retval.call_method1("update", (data,))?;
    Ok(retval.unbind())
}

// @pypdf: pypdf.generic._data_structures.DictionaryObject._read_stream_data
#[pyfunction]
#[pyo3(signature = (*, stream, pdf, data))]
fn datastructures_dictionaryobject_read_stream_data<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
    data: &Bound<'py, PyDict>,
) -> PyResult<()> {
    let dictionary_class = shim_class(py, "DictionaryObject")?;
    let name_class = base_class(py, "NameObject")?;
    let number_class = base_class(py, "NumberObject")?;
    let indirect_class = base_class(py, "IndirectObject")?;
    let utils = utils_module(py)?;

    let pos = tell(stream)?;
    let first = utils.call_method1("read_non_whitespace", (stream,))?;
    if eq_bytes(&first, py, b"s")? && eq_bytes(&read_n(stream, 5)?, py, b"tream")? {
        let mut eol = read_n(stream, 1)?;
        // Occasional PDF file output has spaces after 'stream' keyword but before EOL.
        while eq_bytes(&eol, py, b" ")? {
            eol = read_n(stream, 1)?;
        }
        if !eq_bytes(&eol, py, b"\n")? && !eq_bytes(&eol, py, b"\r")? {
            return Err(errors::pdf_stream_error(
                py,
                "Stream data must be followed by a newline",
            ));
        }
        if eq_bytes(&eol, py, b"\r")? && !eq_bytes(&read_n(stream, 1)?, py, b"\n")? {
            seek(stream, -1, 1)?;
        }
        // This is a stream object, not a dictionary.
        let length_key = py_str(py, "/Length");
        if !data.contains(&length_key)? {
            if pdf_strict(py, pdf)? {
                return Err(errors::pdf_stream_error(py, "Stream length not defined"));
            }
            let values = PyDict::new(py);
            values.set_item("position", tell(stream)?)?;
            log_warning(
                py,
                "Stream length not defined @pos=%(position)d",
                Some(&values),
            )?;
            let name_key = name_class.call1((&length_key,))?;
            data.set_item(name_key, number_class.call1((-1,))?)?;
        }
        let mut stream_length = data.as_any().get_item(&length_key)?;
        if stream_length.is_instance(&indirect_class)? {
            let position = tell(stream)?;
            if pdf.is_none() {
                return Err(assertion(py, Some("mypy")));
            }
            stream_length = pdf.call_method1("get_object", (&stream_length,))?;
            seek_to(stream, &position)?;
        }
        let stream_length = if stream_length.is_none() {
            py_int(py, -1)
        } else {
            stream_length
        };
        let pstart = tell(stream)?;

        let configuration = configuration(py)?;
        let maximum_declared = configuration.getattr("maximum_declared_stream_length")?;
        let data_key = py_str(py, "__streamdata__");
        if stream_length.ge(py_int(py, 0))? {
            if stream_length.gt(&maximum_declared)? {
                let message = format!(
                    "Declared stream length of {} exceeds maximum allowed length.",
                    str_of(&stream_length)?
                );
                return Err(errors::limit_reached_error(py, &message));
            }
            let value = stream.call_method1("read", (&stream_length,))?;
            data.set_item(&data_key, value)?;
        } else {
            let re = py.import("re")?;
            let regex = re.call_method1("compile", (py_bytes(py, b"endstream"),))?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("stream", stream)?;
            kwargs.set_item("regex", &regex)?;
            kwargs.set_item("length", &maximum_declared)?;
            let value = utils.call_method("read_until_regex", (), Some(&kwargs))?;
            data.set_item(&data_key, value)?;
        }
        let end = utils.call_method1("read_non_whitespace", (stream,))?;
        let ndstream = read_n(stream, 8)?;
        let combined = end.call_method1("__add__", (&ndstream,))?;
        if !eq_bytes(&combined, py, b"endstream")? {
            // The odd PDF file has a length that is too long, so we need to read
            // backwards to find the "endstream" ending.
            let mismatch_position = tell(stream)?;
            seek(stream, -10, 1)?;
            let trailing = read_n(stream, 9)?;
            if eq_bytes(&trailing, py, b"endstream")? {
                // We found it by looking back one character further.
                let value = data.as_any().get_item(&data_key)?;
                let trimmed = slice_object(py, &value, None, Some(-1))?;
                data.set_item(&data_key, trimmed)?;
            } else if !pdf.is_none() && !pdf_strict(py, pdf)? {
                seek_to(stream, &pstart)?;
                let kwargs = PyDict::new(py);
                kwargs.set_item("stream", stream)?;
                kwargs.set_item("pdf", pdf)?;
                kwargs.set_item("length", &maximum_declared)?;
                let value = dictionary_class
                    .getattr("_read_unsized_from_stream")?
                    .call((), Some(&kwargs))?;
                data.set_item(&data_key, value)?;
            } else {
                seek_to(stream, &mismatch_position)?;
                let message = format!(
                    "Unable to find 'endstream' marker after stream at byte {} (nd='{}', end='{}').",
                    py_hex(py, &tell(stream)?)?,
                    pyutil::py_repr(&ndstream)?,
                    pyutil::py_repr(&trailing)?
                );
                return Err(errors::pdf_read_error(py, &message));
            }
        }
    } else {
        seek_to(stream, &pos)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// TreeObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._data_structures.TreeObject.has_children
#[pyfunction]
fn datastructures_treeobject_has_children<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<bool> {
    slf.contains("/First")
}

// @pypdf: pypdf.generic._data_structures.TreeObject.__iter__
#[pyfunction]
fn datastructures_treeobject_iter<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(slf.call_method0("children")?.unbind())
}

/// Generator trampoline support: build the traversal state, or `None` when the
/// tree has no children.
///
/// Upstream `children()` is a Python generator; a `#[pyfunction]` cannot be a
/// generator, so the shim contains a two-line generator that yields the items
/// produced by [`datastructures_treeobject_children_next`].  The traversal logic
/// itself lives here.
// @pypdf: pypdf.generic._data_structures.TreeObject.children
#[pyfunction]
fn datastructures_treeobject_children_start<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if !slf.call_method0("has_children")?.is_truthy()? {
        return Ok(py.None());
    }
    let name_class = base_class(py, "NameObject")?;
    let first_key = name_class.call1(("/First",))?;
    let last_key = name_class.call1(("/Last",))?;
    let child_ref = slf.get_item(&first_key)?;
    let last = slf.get_item(&last_key)?;
    let child = child_ref.call_method0("get_object")?;
    let state = PyDict::new(py);
    state.set_item("child", child)?;
    state.set_item("last", last)?;
    state.set_item("visited", new_set(py)?)?;
    state.set_item("done", false)?;
    Ok(state.into_any().unbind())
}

/// One step of upstream `TreeObject.children()` (see
/// [`datastructures_treeobject_children_start`] for the generator trampoline).
#[pyfunction]
fn datastructures_treeobject_children_next<'py>(
    py: Python<'py>,
    state: &Bound<'py, PyDict>,
) -> PyResult<Py<PyAny>> {
    let stop = shim_attr(py, "_DATA_STRUCTURES_STOP")?;
    if state.as_any().get_item("done")?.is_truthy()? {
        return Ok(stop.unbind());
    }
    let child = state.as_any().get_item("child")?;
    let visited = state.as_any().get_item("visited")?;
    let child_id = py_id(py, &child)?;
    if visited.contains(&child_id)? {
        let values = PyDict::new(py);
        values.set_item("child", &child)?;
        log_warning(
            py,
            "Detected cycle in outline structure for %(child)s",
            Some(&values),
        )?;
        state.set_item("done", true)?;
        return Ok(stop.unbind());
    }
    visited.call_method1("add", (&child_id,))?;
    let last = state.as_any().get_item("last")?;
    if child.eq(&last)? {
        state.set_item("done", true)?;
        return Ok(child.unbind());
    }
    let name_class = base_class(py, "NameObject")?;
    let next_key = name_class.call1(("/Next",))?;
    let child_ref = child.call_method1("get", (&next_key,))?;
    let is_null = base_class(py, "is_null_or_none")?
        .call1((&child_ref,))?
        .is_truthy()?;
    if is_null {
        state.set_item("done", true)?;
    } else {
        let next_child = child_ref.call_method0("get_object")?;
        state.set_item("child", next_child)?;
    }
    Ok(child.unbind())
}

// @pypdf: pypdf.generic._data_structures.TreeObject.add_child
#[pyfunction]
fn datastructures_treeobject_add_child<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    child: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
) -> PyResult<()> {
    slf.call_method1("insert_child", (child, py.None(), pdf))?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.TreeObject.inc_parent_counter_default
#[pyfunction]
fn datastructures_treeobject_inc_parent_counter_default<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    parent: &Bound<'py, PyAny>,
    n: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let is_null = base_class(py, "is_null_or_none")?
        .call1((parent,))?
        .is_truthy()?;
    if is_null {
        return Ok(());
    }
    let parent = parent.call_method0("get_object")?;
    let count_key = py_str(py, "/Count");
    if parent.contains(&count_key)? {
        let parent_key = base_class(py, "NameObject")?.call1((&count_key,))?;
        let current = parent.get_item(&parent_key)?;
        let sum = current.call_method1("__add__", (n,))?;
        let zero = py_int(py, 0);
        let bounded = builtins(py)?.getattr("max")?.call1((zero, sum))?;
        let number = base_class(py, "NumberObject")?.call1((bounded,))?;
        parent.set_item(parent_key, number)?;
        let grandparent = dict_get(&parent, &py_str(py, "/Parent"), py.None().into_bound(py))?;
        slf.call_method1("inc_parent_counter_default", (grandparent, n))?;
    }
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.TreeObject.inc_parent_counter_outline
#[pyfunction]
fn datastructures_treeobject_inc_parent_counter_outline<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    parent: &Bound<'py, PyAny>,
    n: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let is_null = base_class(py, "is_null_or_none")?
        .call1((parent,))?
        .is_truthy()?;
    if is_null {
        return Ok(());
    }
    let parent = parent.call_method0("get_object")?;
    // BooleanObject requires comparison with == instead of `is`.
    let open_marker = dict_get(
        &parent,
        &py_str(py, "/%is_open%"),
        pyo3::types::PyBool::new(py, true).to_owned().into_any(),
    )?;
    let open = open_marker.eq(pyo3::types::PyBool::new(py, true))?;
    let count_key = py_str(py, "/Count");
    let mut count = dict_get(&parent, &count_key, py_int(py, 0))?;
    if count.lt(py_int(py, 0))? {
        count = builtins(py)?.getattr("abs")?.call1((&count,))?;
    }
    let total = count.call_method1("__add__", (n,))?;
    let factor = py_int(py, if open { 1 } else { -1 });
    let product = total.call_method1("__mul__", (factor,))?;
    let number = base_class(py, "NumberObject")?.call1((product,))?;
    let name_key = base_class(py, "NameObject")?.call1((&count_key,))?;
    parent.set_item(name_key, number)?;
    if !open {
        return Ok(());
    }
    let grandparent = dict_get(&parent, &py_str(py, "/Parent"), py.None().into_bound(py))?;
    slf.call_method1("inc_parent_counter_outline", (grandparent, n))?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.TreeObject.insert_child
#[pyfunction]
#[pyo3(signature = (slf, child, before, pdf, inc_parent_counter=None))]
fn datastructures_treeobject_insert_child<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    child: &Bound<'py, PyAny>,
    before: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
    inc_parent_counter: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let _ = pdf;
    let inc_parent_counter = match inc_parent_counter {
        Some(callable) if !callable.is_none() => callable.clone(),
        _ => slf.getattr("inc_parent_counter_default")?,
    };
    let dictionary_class = shim_class(py, "DictionaryObject")?;
    let name_class = base_class(py, "NameObject")?;
    let number_class = base_class(py, "NumberObject")?;
    let child_obj = child.call_method0("get_object")?;
    let child_reference = child.getattr("indirect_reference")?;
    if child_reference.is_none() {
        return Err(assertion(py, Some("mypy")));
    }
    let first_key = py_str(py, "/First");
    if !slf.contains(&first_key)? {
        // No child yet.
        let first_name = name_class.call1((&first_key,))?;
        slf.set_item(first_name, &child_reference)?;
        slf.set_item(
            name_class.call1(("/Count",))?,
            number_class.call1((0,))?,
        )?;
        slf.set_item(name_class.call1(("/Last",))?, &child_reference)?;
        child_obj.set_item(
            name_class.call1(("/Parent",))?,
            slf.getattr("indirect_reference")?,
        )?;
        let child_count = dict_get(&child_obj, &py_str(py, "/Count"), py_int(py, 1))?;
        inc_parent_counter.call1((slf, child_count))?;
        if child_obj.contains("/Next")? {
            child_obj.del_item("/Next")?;
        }
        if child_obj.contains("/Prev")? {
            child_obj.del_item("/Prev")?;
        }
        return Ok(child_reference.unbind());
    }
    let mut previous = slf.get_item("/Last")?;
    let mut visited: HashSet<usize> = HashSet::new();
    loop {
        let previous_reference = getattr_or_none(py, &previous, "indirect_reference")?;
        if previous_reference.eq(before)? {
            break;
        }
        let previous_id = object_id(&previous);
        if !visited.insert(previous_id) {
            return Err(errors::limit_reached_error(
                py,
                "Detected cycle in tree structure.",
            ));
        }
        if previous.contains("/Next")? {
            previous = previous.get_item("/Next")?;
            continue;
        }
        // Append at the end.
        previous.set_item(
            name_class.call1(("/Next",))?,
            &child_reference,
        )?;
        child_obj.set_item(
            name_class.call1(("/Prev",))?,
            getattr_or_none(py, &previous, "indirect_reference")?,
        )?;
        child_obj.set_item(
            name_class.call1(("/Parent",))?,
            slf.getattr("indirect_reference")?,
        )?;
        if child_obj.contains("/Next")? {
            child_obj.del_item("/Next")?;
        }
        slf.set_item(name_class.call1(("/Last",))?, &child_reference)?;
        let child_count = dict_get(&child_obj, &py_str(py, "/Count"), py_int(py, 1))?;
        inc_parent_counter.call1((slf, child_count))?;
        return Ok(child_reference.unbind());
    }
    // Insert as the first or in the middle.
    let outcome: PyResult<()> = (|| {
        let prev_link = previous.get_item("/Prev")?;
        if !prev_link.is_instance(&dictionary_class)? {
            return Err(assertion(py, None));
        }
        prev_link.set_item(name_class.call1(("/Next",))?, &child_reference)?;
        let prev_link = previous.get_item("/Prev")?;
        child_obj.set_item(name_class.call1(("/Prev",))?, prev_link)?;
        Ok(())
    })();
    match outcome {
        Ok(()) => {}
        Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
        Err(_) => {
            child_obj.call_method1("pop", ("/Next", py.None()))?;
        }
    }
    child_obj.set_item(
        name_class.call1(("/Next",))?,
        &previous,
    )?;
    previous.set_item(
        name_class.call1(("/Prev",))?,
        &child_reference,
    )?;
    child_obj.set_item(
        name_class.call1(("/Parent",))?,
        slf.getattr("indirect_reference")?,
    )?;
    let child_count = dict_get(&child_obj, &py_str(py, "/Count"), py_int(py, 1))?;
    inc_parent_counter.call1((slf, child_count))?;
    Ok(child_reference.unbind())
}

// @pypdf: pypdf.generic._data_structures.TreeObject._remove_node_from_tree
#[pyfunction]
#[pyo3(signature = (slf, *, previous, previous_reference, current, last))]
fn datastructures_treeobject_remove_node_from_tree<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    previous: &Bound<'py, PyAny>,
    previous_reference: &Bound<'py, PyAny>,
    current: &Bound<'py, PyAny>,
    last: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let name_class = base_class(py, "NameObject")?;
    let number_class = base_class(py, "NumberObject")?;
    let next_reference = dict_get(current, &py_str(py, "/Next"), py.None().into_bound(py))?;
    if previous.is_none() {
        if next_reference.is_truthy()? {
            // Removing first tree node.
            let next_object = next_reference.call_method0("get_object")?;
            next_object.del_item("/Prev")?;
            slf.set_item(name_class.call1(("/First",))?, &next_reference)?;
            let count_key = name_class.call1(("/Count",))?;
            let count = slf.get_item(&count_key)?;
            let decremented = count.call_method1("__sub__", (py_int(py, 1),))?;
            slf.set_item(count_key, number_class.call1((decremented,))?)?;
        } else {
            // Removing only tree node.
            slf.set_item(
                name_class.call1(("/Count",))?,
                number_class.call1((0,))?,
            )?;
            slf.del_item("/First")?;
            if slf.contains(&name_class.call1(("/Last",))?)? {
                slf.del_item(name_class.call1(("/Last",))?)?;
            }
        }
    } else {
        if next_reference.is_truthy()? {
            // Removing middle tree node.
            let next_object = next_reference.call_method0("get_object")?;
            next_object.set_item(name_class.call1(("/Prev",))?, previous_reference)?;
            previous.set_item(name_class.call1(("/Next",))?, &next_reference)?;
        } else {
            // Removing last tree node.
            if !current.eq(last)? {
                return Err(assertion(py, None));
            }
            previous.del_item("/Next")?;
            slf.set_item(
                name_class.call1(("/Last",))?,
                previous_reference,
            )?;
        }
        let count_key = name_class.call1(("/Count",))?;
        let count = slf.get_item(&count_key)?;
        let decremented = count.call_method1("__sub__", (py_int(py, 1),))?;
        slf.set_item(count_key, number_class.call1((decremented,))?)?;
    }
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.TreeObject.remove_child
#[pyfunction]
fn datastructures_treeobject_remove_child<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    child: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let child_object = child.call_method0("get_object")?;
    let parent_key = py_str(py, "/Parent");
    if !child_object.contains(&parent_key)? {
        return Err(message_exception(
            py,
            "ValueError",
            "Removed child does not appear to be a tree item",
        ));
    }
    let child_parent = child_object.get_item(&parent_key)?;
    if !child_parent.eq(slf)? {
        return Err(message_exception(
            py,
            "ValueError",
            "Removed child is not a member of this tree",
        ));
    }
    let mut found = false;
    let mut previous: Option<Bound<'py, PyAny>> = None;
    let mut previous_reference: Option<Bound<'py, PyAny>> = None;
    let first_reference = slf.get_item("/First")?;
    let mut current_reference: Option<Bound<'py, PyAny>> = Some(first_reference.clone());
    let mut current: Option<Bound<'py, PyAny>> =
        Some(first_reference.call_method0("get_object")?);
    let last_reference = slf.get_item("/Last")?;
    let last = last_reference.call_method0("get_object")?;
    let mut visited: HashSet<usize> = HashSet::new();
    while let Some(current_object) = current.take() {
        if current_object.eq(&child_object)? {
            let kwargs = PyDict::new(py);
            kwargs.set_item(
                "previous",
                previous.unwrap_or_else(|| py.None().into_bound(py)),
            )?;
            kwargs.set_item(
                "previous_reference",
                previous_reference.unwrap_or_else(|| py.None().into_bound(py)),
            )?;
            kwargs.set_item("current", &current_object)?;
            kwargs.set_item("last", &last)?;
            let tree_class = shim_class(py, "TreeObject")?;
            tree_class
                .getattr("_remove_node_from_tree")?
                .call((slf,), Some(&kwargs))?;
            found = true;
            break;
        }
        let current_id = object_id(&current_object);
        if !visited.insert(current_id) {
            return Err(errors::limit_reached_error(
                py,
                "Detected cyclic /Next hierarchy.",
            ));
        }
        previous_reference = current_reference.take();
        previous = Some(current_object.clone());
        if !current_object.contains("/Next")? {
            break;
        }
        let next_reference = current_object.get_item("/Next")?;
        current = Some(next_reference.call_method0("get_object")?);
        current_reference = Some(next_reference);
    }
    if !found {
        return Err(message_exception(
            py,
            "ValueError",
            "Removal couldn't find item in tree",
        ));
    }
    shim_attr(py, "_reset_node_tree_relationship")?.call1((&child_object,))?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.TreeObject.remove_from_tree
#[pyfunction]
fn datastructures_treeobject_remove_from_tree<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<()> {
    if !slf.contains("/Parent")? {
        return Err(message_exception(
            py,
            "ValueError",
            "Removed child does not appear to be a tree item",
        ));
    }
    let parent = slf.get_item("/Parent")?;
    parent.call_method1("remove_child", (slf,))?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.TreeObject.empty_tree
#[pyfunction]
fn datastructures_treeobject_empty_tree<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let name_class = base_class(py, "NameObject")?;
    let reset = shim_attr(py, "_reset_node_tree_relationship")?;
    for child in slf.try_iter()? {
        let child = child?;
        let child_obj = child.call_method0("get_object")?;
        reset.call1((&child_obj,))?;
    }
    let count_key = name_class.call1(("/Count",))?;
    if slf.contains(&count_key)? {
        slf.del_item(count_key)?;
    }
    let first_key = name_class.call1(("/First",))?;
    if slf.contains(&first_key)? {
        slf.del_item(first_key)?;
    }
    let last_key = name_class.call1(("/Last",))?;
    if slf.contains(&last_key)? {
        slf.del_item(last_key)?;
    }
    Ok(())
}

// @pypdf: pypdf.generic._data_structures._reset_node_tree_relationship
#[pyfunction]
fn datastructures_reset_node_tree_relationship<'py>(
    py: Python<'py>,
    child_obj: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let name_class = base_class(py, "NameObject")?;
    child_obj.del_item(name_class.call1(("/Parent",))?)?;
    for key in ["/Next", "/Prev"] {
        let name_key = name_class.call1((key,))?;
        if child_obj.contains(&name_key)? {
            child_obj.del_item(name_key)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// StreamObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._data_structures.StreamObject.replicate
#[pyfunction]
fn datastructures_streamobject_replicate<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let fresh = slf.get_type().call0()?;
    let destination = slf.call_method1("_reference_clone", (fresh, pdf_dest, false))?;
    destination.setattr("_data", slf.getattr("_data")?)?;
    let outcome: PyResult<()> = (|| {
        let decoded_self = slf.getattr("decoded_self")?;
        if decoded_self.is_none() {
            slf.setattr("decoded_self", py.None())?;
        } else {
            let replicated = decoded_self.call_method1("replicate", (pdf_dest,))?;
            slf.setattr("decoded_self", replicated)?;
        }
        Ok(())
    })();
    match outcome {
        Ok(()) => {}
        Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
        Err(_) => {}
    }
    let items = slf.call_method0("items")?;
    for item in items.try_iter()? {
        let item = item?;
        let key = item.get_item(0)?;
        let value = item.get_item(1)?;
        let new_key = key.call_method1("replicate", (pdf_dest,))?;
        let new_value = if value.hasattr("replicate")? {
            value.call_method1("replicate", (pdf_dest,))?
        } else {
            value
        };
        destination.set_item(new_key, new_value)?;
    }
    Ok(destination.unbind())
}

// @pypdf: pypdf.generic._data_structures.StreamObject._clone
#[pyfunction]
fn datastructures_streamobject_clone_inner<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    src: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
    ignore_fields: &Bound<'py, PyAny>,
    visited: &Bound<'py, PyAny>,
) -> PyResult<()> {
    slf.setattr("_data", src.getattr("_data")?)?;
    let outcome: PyResult<()> = (|| {
        let decoded_self = src.getattr("decoded_self")?;
        if decoded_self.is_none() {
            slf.setattr("decoded_self", py.None())?;
        } else {
            let cloned = decoded_self.call_method1(
                "clone",
                (pdf_dest, force_duplicate, ignore_fields),
            )?;
            slf.setattr("decoded_self", cloned)?;
        }
        Ok(())
    })();
    match outcome {
        Ok(()) => {}
        Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
        Err(_) => {}
    }
    shim_class(py, "DictionaryObject")?
        .getattr("_clone")?
        .call1((slf, src, pdf_dest, force_duplicate, ignore_fields, visited))?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.StreamObject.hash_bin
#[pyfunction]
fn datastructures_streamobject_hash_bin<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<isize> {
    let base_hash = shim_class(py, "DictionaryObject")?
        .getattr("hash_bin")?
        .call1((slf,))?;
    let data = slf.getattr("_data")?;
    py_tuple(py, vec![base_hash, data])?.hash()
}

// @pypdf: pypdf.generic._data_structures.StreamObject.get_data
#[pyfunction]
fn datastructures_streamobject_get_data<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(slf.getattr("_data")?.unbind())
}

// @pypdf: pypdf.generic._data_structures.StreamObject.set_data
#[pyfunction]
fn datastructures_streamobject_set_data<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    data: &Bound<'py, PyAny>,
) -> PyResult<()> {
    slf.setattr("_data", data)?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.StreamObject.hash_value_data
#[pyfunction]
fn datastructures_streamobject_hash_value_data<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let base = base_class(py, "PdfObject")?
        .getattr("hash_value_data")?
        .call1((slf,))?;
    let data = slf.call_method0("get_data")?;
    Ok(base.call_method1("__add__", (data,))?.unbind())
}

// @pypdf: pypdf.generic._data_structures.StreamObject.write_to_stream
#[pyfunction]
fn datastructures_streamobject_write_to_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let name_class = base_class(py, "NameObject")?;
    let number_class = base_class(py, "NumberObject")?;
    let data = slf.getattr("_data")?;
    let data_length = length(&data)?;
    slf.set_item(
        name_class.call1(("/Length",))?,
        number_class.call1((data_length as i64,))?,
    )?;
    shim_class(py, "DictionaryObject")?
        .getattr("write_to_stream")?
        .call1((slf, stream))?;
    slf.del_item("/Length")?;
    stream.call_method1("write", (py_bytes(py, b"\nstream\n"),))?;
    stream.call_method1("write", (data,))?;
    stream.call_method1("write", (py_bytes(py, b"\nendstream"),))?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.StreamObject.initialize_from_dictionary
#[pyfunction]
fn datastructures_streamobject_initialize_from_dictionary<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyDict>,
) -> PyResult<Py<PyAny>> {
    let filter_key = py_str(py, "/Filter");
    let retval = if data.contains(&filter_key)? {
        shim_class(py, "EncodedStreamObject")?.call0()?
    } else {
        shim_class(py, "DecodedStreamObject")?.call0()?
    };
    let stream_data_key = py_str(py, "__streamdata__");
    let stream_data = data.as_any().get_item(&stream_data_key)?;
    retval.setattr("_data", stream_data)?;
    data.del_item(&stream_data_key)?;
    let length_key = py_str(py, "/Length");
    if data.contains(&length_key)? {
        data.del_item(&length_key)?;
    }
    retval.call_method1("update", (data,))?;
    Ok(retval.unbind())
}

// @pypdf: pypdf.generic._data_structures.StreamObject.flate_encode
#[pyfunction]
#[pyo3(signature = (slf, level=-1))]
fn datastructures_streamobject_flate_encode<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    level: i32,
) -> PyResult<Py<PyAny>> {
    let name_class = base_class(py, "NameObject")?;
    let null_class = base_class(py, "NullObject")?;
    let array_class = shim_class(py, "ArrayObject")?;
    let constants = constants_class(py, "FilterTypes")?;
    let flate_decode = constants.getattr("FLATE_DECODE")?;
    let filter_key = py_str(py, "/Filter");
    let decode_parms_key = py_str(py, "/DecodeParms");
    let mut params: Option<Bound<'py, PyAny>> = None;
    let filter: Bound<'py, PyAny>;
    if slf.contains(&filter_key)? {
        let current = slf.get_item(&filter_key)?;
        if current.is_instance(&array_class)? {
            let name = name_class.call1((&flate_decode,))?;
            let mut items = vec![name];
            for item in current.try_iter()? {
                items.push(item?);
            }
            filter = array_class.call1((py_list(py, items)?,))?;
            let fallback = array_class.call0()?;
            let outcome: PyResult<Bound<'py, PyAny>> = (|| {
                let decode_parms =
                    dict_get(slf, &decode_parms_key, fallback.clone())?;
                let mut items = vec![null_class.call0()?];
                for item in decode_parms.try_iter()? {
                    items.push(item?);
                }
                array_class.call1((py_list(py, items)?,))
            })();
            params = Some(match outcome {
                Ok(value) => value,
                Err(error) if error.is_instance_of::<PyTypeError>(py) => {
                    let decode_parms =
                        dict_get(slf, &decode_parms_key, fallback)?;
                    array_class.call1((py_list(py, vec![null_class.call0()?, decode_parms])?,))?
                }
                Err(error) => return Err(error),
            });
        } else {
            let name = name_class.call1((&flate_decode,))?;
            filter = array_class.call1((py_list(py, vec![name, current])?,))?;
            let decode_parms = dict_get(slf, &decode_parms_key, null_class.call0()?)?;
            params = Some(array_class.call1((py_list(
                py,
                vec![null_class.call0()?, decode_parms],
            )?,))?);
        }
    } else {
        filter = name_class.call1((&flate_decode,))?;
    }
    let retval = shim_class(py, "EncodedStreamObject")?.call0()?;
    retval.call_method1("update", (slf,))?;
    retval.set_item(name_class.call1((&filter_key,))?, context_filter(py, &filter)?)?;
    if let Some(params) = params {
        retval.set_item(name_class.call1((&decode_parms_key,))?, params)?;
    }
    let flate = py.import("pypdf.filters")?.getattr("FlateDecode")?;
    let data = slf.getattr("_data")?;
    let encoded = flate.call_method1("encode", (data, level))?;
    retval.setattr("_data", encoded)?;
    Ok(retval.unbind())
}

/// `filter` used as a dictionary key: it is a `NameObject` already in every
/// upstream branch; kept as a helper for readability.
fn context_filter<'py>(
    _py: Python<'py>,
    filter: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    Ok(filter.clone())
}

// @pypdf: pypdf.generic._data_structures.StreamObject.decode_as_image
#[pyfunction]
#[pyo3(signature = (slf, pillow_parameters=None))]
fn datastructures_streamobject_decode_as_image<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pillow_parameters: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let fetch = py
        .import("pypdf.generic._image_xobject")?
        .getattr("_xobj_to_image")?;
    let subtype = dict_get(slf, &py_str(py, "/Subtype"), py_str(py, ""))?;
    if !subtype.eq("/Image")? {
        match getattr_or_none(py, slf, "indirect_reference") {
            Ok(reference) => {
                let values = PyDict::new(py);
                values.set_item("indirect_reference", reference)?;
                log_warning(
                    py,
                    "%(indirect_reference)s does not seem to be an Image",
                    Some(&values),
                )?;
            }
            Err(error) if error.is_instance_of::<PyAttributeError>(py) => {
                let values = PyDict::new(py);
                values.set_item("obj", slf)?;
                log_warning(
                    py,
                    "%(obj)r object does not seem to be an Image",
                    Some(&values),
                )?;
            }
            Err(error) => return Err(error),
        }
    }
    let pillow_parameters = match pillow_parameters {
        Some(value) => value.clone(),
        None => py.None().into_bound(py),
    };
    let result = fetch.call1((slf, pillow_parameters))?;
    let extension = result.get_item(0)?;
    let image = result.get_item(2)?;
    if extension.is_none() {
        return Ok(py.None());
    }
    Ok(image.unbind())
}

// ---------------------------------------------------------------------------
// EncodedStreamObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._data_structures.EncodedStreamObject.hash_value_data
#[pyfunction]
fn datastructures_encodedstreamobject_hash_value_data<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let base = shim_class(py, "DictionaryObject")?
        .getattr("hash_value_data")?
        .call1((slf,))?;
    let data = slf.getattr("_data")?;
    Ok(base.call_method1("__add__", (data,))?.unbind())
}

// @pypdf: pypdf.generic._data_structures.EncodedStreamObject.get_data
#[pyfunction]
fn datastructures_encodedstreamobject_get_data<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let decoded_self = slf.getattr("decoded_self")?;
    if !decoded_self.is_none() {
        // Cached version of the decoded object.
        return Ok(decoded_self.call_method0("get_data")?.unbind());
    }
    // Create the decoded object.
    let decoded = shim_class(py, "DecodedStreamObject")?.call0()?;
    let filters = py.import("pypdf.filters")?;
    let stream_data = filters.call_method1("decode_stream_data", (slf,))?;
    decoded.call_method1("set_data", (stream_data,))?;
    let items = slf.call_method0("items")?;
    for item in items.try_iter()? {
        let item = item?;
        let key = item.get_item(0)?;
        let value = item.get_item(1)?;
        let mut skip = false;
        for special in ["/Length", "/Filter", "/DecodeParms"] {
            if key.eq(special)? {
                skip = true;
                break;
            }
        }
        if !skip {
            decoded.set_item(key, value)?;
        }
    }
    slf.setattr("decoded_self", &decoded)?;
    Ok(decoded.call_method0("get_data")?.unbind())
}

// @pypdf: pypdf.generic._data_structures.EncodedStreamObject.set_data
#[pyfunction]
fn datastructures_encodedstreamobject_set_data<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    data: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let filters = py.import("pypdf.filters")?;
    let flate = filters.getattr("FlateDecode")?;
    let filter_value = dict_get(slf, &py_str(py, "/Filter"), py_str(py, ""))?;
    let flate_name = filters.getattr("FilterTypes")?.getattr("FLATE_DECODE")?;
    let flate_list = py_list(py, vec![flate_name.clone()])?;
    let supported = filter_value.eq(&flate_name)? || filter_value.eq(&flate_list)?;
    if !supported {
        return Err(errors::pdf_read_error(
            py,
            "Streams encoded with a filter different from FlateDecode are not supported",
        ));
    }
    if !data.is_instance_of::<PyBytes>() {
        return Err(PyTypeError::new_err("Data must be bytes"));
    }
    if slf.getattr("decoded_self")?.is_none() {
        // To create self.decoded_self.
        slf.call_method0("get_data")?;
    }
    let decoded_self = slf.getattr("decoded_self")?;
    if decoded_self.is_none() {
        return Err(assertion(py, Some("mypy")));
    }
    decoded_self.call_method1("set_data", (data,))?;
    let encoded = flate.call_method1("encode", (data,))?;
    shim_class(py, "StreamObject")?
        .getattr("set_data")?
        .call1((slf, encoded))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// ContentStream
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._data_structures.ContentStream.__init__
#[pyfunction]
#[pyo3(signature = (slf, stream, pdf, forced_encoding=None))]
fn datastructures_contentstream_new<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
    forced_encoding: Option<&Bound<'py, PyAny>>,
) -> PyResult<()> {
    let stream_object_class = shim_class(py, "StreamObject")?;
    let array_class = shim_class(py, "ArrayObject")?;
    let null_class = base_class(py, "NullObject")?;
    slf.setattr("pdf", pdf)?;
    slf.setattr("_operations", py_list(py, vec![])?)?;

    // `stream` may be a StreamObject or an ArrayObject of StreamObjects to concatenate.
    if stream.is_none() {
        stream_object_class
            .getattr("set_data")?
            .call1((slf, py_bytes(py, b"")))?;
    } else {
        let stream = stream.call_method0("get_object")?;
        if stream.is_instance(&array_class)? {
            let configuration = configuration(py)?;
            let stream_length = length(&stream)?;
            if stream_length > CONTENT_STREAM_ARRAY_MAX_LENGTH {
                let message = format!(
                    "Array-based stream has {} > {} elements.",
                    stream_length, CONTENT_STREAM_ARRAY_MAX_LENGTH
                );
                return Err(errors::limit_reached_error(py, &message));
            }
            let maximum: i64 = configuration
                .getattr("array_based_stream_maximum_output_length")?
                .extract()?;
            let mut data: Vec<u8> = Vec::new();
            let mut total: i64 = 0;
            for item in stream.try_iter()? {
                let item = item?;
                let resolved = item.call_method0("get_object")?;
                if resolved.is_instance(&null_class)? {
                    continue;
                }
                if !resolved.is_instance(&stream_object_class)? {
                    // No exception here for now - the PDF structure seems to
                    // already be broken beforehand in these cases.
                    let values = PyDict::new(py);
                    values.set_item("type_name", type_name(&resolved)?)?;
                    log_warning(
                        py,
                        "Expected StreamObject, got %(type_name)s instead. Data might be wrong.",
                        Some(&values),
                    )?;
                } else {
                    let new_data = resolved.call_method0("get_data")?;
                    let new_bytes: Vec<u8> = new_data.extract()?;
                    total += new_bytes.len() as i64;
                    if total > maximum {
                        let message = format!(
                            "Array-based stream has at least {} > {} output bytes.",
                            total, maximum
                        );
                        return Err(errors::limit_reached_error(py, &message));
                    }
                    data.extend_from_slice(&new_bytes);
                }
                if data.is_empty() || data.last() != Some(&b'\n') {
                    total += 1;
                    data.push(b'\n');
                }
            }
            stream_object_class
                .getattr("set_data")?
                .call1((slf, py_bytes(py, &data)))?;
        } else {
            let stream_data = stream.call_method0("get_data")?;
            if stream_data.is_none() {
                return Err(assertion(py, None));
            }
            stream_object_class
                .getattr("set_data")?
                .call1((slf, stream_data))?;
        }
    }
    slf.setattr(
        "forced_encoding",
        match forced_encoding {
            Some(encoding) => encoding.clone(),
            None => py.None().into_bound(py),
        },
    )?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.ContentStream.replicate
#[pyfunction]
fn datastructures_contentstream_replicate<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let cls = slf.get_type();
    let fresh = cls.call1((py.None(), py.None()))?;
    let destination = slf.call_method1("_reference_clone", (fresh, pdf_dest, false))?;
    destination.setattr("_data", slf.getattr("_data")?)?;
    let outcome: PyResult<()> = (|| {
        let decoded_self = slf.getattr("decoded_self")?;
        if decoded_self.is_none() {
            slf.setattr("decoded_self", py.None())?;
        } else {
            let replicated = decoded_self.call_method1("replicate", (pdf_dest,))?;
            slf.setattr("decoded_self", replicated)?;
        }
        Ok(())
    })();
    match outcome {
        Ok(()) => {}
        Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
        Err(_) => {}
    }
    let items = slf.call_method0("items")?;
    for item in items.try_iter()? {
        let item = item?;
        let key = item.get_item(0)?;
        let value = item.get_item(1)?;
        let new_key = key.call_method1("replicate", (pdf_dest,))?;
        let new_value = if value.hasattr("replicate")? {
            value.call_method1("replicate", (pdf_dest,))?
        } else {
            value
        };
        destination.set_item(new_key, new_value)?;
    }
    destination.setattr("pdf", pdf_dest)?;
    let operations = builtins_list(py, &slf.getattr("_operations")?)?;
    destination.setattr("_operations", operations)?;
    destination.setattr("forced_encoding", slf.getattr("forced_encoding")?)?;
    Ok(destination.unbind())
}

// @pypdf: pypdf.generic._data_structures.ContentStream.clone
#[pyfunction]
fn datastructures_contentstream_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
    ignore_fields: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if clone_shortcut(py, slf, pdf_dest, force_duplicate)? {
        return Ok(slf.clone().unbind());
    }
    let visited = new_set(py)?;
    let cls = slf.get_type();
    let fresh = cls.call1((py.None(), py.None()))?;
    let destination =
        slf.call_method1("_reference_clone", (fresh, pdf_dest, force_duplicate))?;
    let ignore = if ignore_fields.is_none() {
        py_list(py, vec![])?
    } else {
        ignore_fields.clone()
    };
    destination.call_method1(
        "_clone",
        (slf, pdf_dest, force_duplicate, ignore, visited),
    )?;
    Ok(destination.unbind())
}

// @pypdf: pypdf.generic._data_structures.ContentStream._clone
#[pyfunction]
fn datastructures_contentstream_clone_inner<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    src: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    _force_duplicate: bool,
    _ignore_fields: &Bound<'py, PyAny>,
    _visited: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let src_data = src.getattr("_data")?;
    shim_class(py, "StreamObject")?
        .getattr("set_data")?
        .call1((slf, src_data))?;
    slf.setattr("pdf", pdf_dest)?;
    let operations = builtins_list(py, &src.getattr("_operations")?)?;
    slf.setattr("_operations", operations)?;
    slf.setattr("forced_encoding", src.getattr("forced_encoding")?)?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.ContentStream._check_limits
#[pyfunction]
#[pyo3(signature = (*, remaining_elements=None, parsing_budget, token_decrement))]
fn datastructures_contentstream_check_limits<'py>(
    py: Python<'py>,
    remaining_elements: Option<&Bound<'py, PyAny>>,
    parsing_budget: &Bound<'py, PyAny>,
    token_decrement: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let mut remaining: Option<Bound<'py, PyAny>> = None;
    if let Some(elements) = remaining_elements {
        let decremented = elements.call_method1("__sub__", (py_int(py, 1),))?;
        if decremented.lt(py_int(py, 0))? {
            return Err(errors::limit_reached_error(
                py,
                "Reached maximum number of tokens.",
            ));
        }
        remaining = Some(decremented);
    }
    let current = parsing_budget.getattr("remaining_budget")?;
    let next = current.call_method1("__sub__", (token_decrement,))?;
    parsing_budget.setattr("remaining_budget", &next)?;
    if next.lt(py_int(py, 0))? {
        return Err(errors::limit_reached_error(
            py,
            "Reached maximum number of tokens per stream.",
        ));
    }
    Ok(match remaining {
        Some(value) => value.unbind(),
        None => py.None(),
    })
}

/// Fast path for `ContentStream._check_limits(parsing_budget=..., token_decrement=...)`.
/// Falls back to the Python entry point when the class attribute is monkeypatched.
fn content_stream_check_limits(
    py: Python<'_>,
    content_stream_class: &Bound<'_, PyAny>,
    budget: &Bound<'_, PyAny>,
    token_decrement: i64,
) -> PyResult<()> {
    let check_limits = content_stream_class.getattr("_check_limits")?;
    if !is_default_shim_function(&check_limits, "pypdf.generic._data_structures", "_check_limits")? {
        let kwargs = PyDict::new(py);
        kwargs.set_item("parsing_budget", budget)?;
        kwargs.set_item("token_decrement", py_int(py, token_decrement))?;
        check_limits.call((), Some(&kwargs))?;
        return Ok(());
    }
    datastructures_contentstream_check_limits(
        py,
        None,
        budget,
        &py_int(py, token_decrement),
    )?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.ContentStream._parse_content_stream
#[pyfunction]
fn datastructures_contentstream_parse_content_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let utils = utils_module(py)?;
    let generic_utils = generic_utils_module(py)?;
    let name_class = base_class(py, "NameObject")?;
    let content_stream_class = shim_class(py, "ContentStream")?;
    let configuration = configuration(py)?;
    let maximum = configuration.getattr("stream_parsing_work_maximum_count")?;
    let budget_kwargs = PyDict::new(py);
    budget_kwargs.set_item("remaining_budget", maximum)?;
    let budget = generic_utils
        .getattr("StreamParsingBudget")?
        .call((), Some(&budget_kwargs))?;

    seek(stream, 0, 0)?;
    let mut operands = py_list(py, vec![])?;
    loop {
        let peek = utils.call_method1("read_non_whitespace", (stream,))?;
        if eq_bytes(&peek, py, b"")? || peek.eq(py_int(py, 0))? {
            break;
        }
        seek(stream, -1, 1)?;
        let is_alpha = peek.call_method0("isalpha")?.is_truthy()?;
        if is_alpha || eq_bytes(&peek, py, b"'")? || eq_bytes(&peek, py, b"\"")? {
            let kwargs = PyDict::new(py);
            kwargs.set_item("stream", stream)?;
            kwargs.set_item("regex", name_class.getattr("delimiter_pattern")?)?;
            kwargs.set_item("length", content_stream_class.getattr("_OPERATOR_LENGTH_LIMIT")?)?;
            let operator = utils.call_method("read_until_regex", (), Some(&kwargs))?;
            if eq_bytes(&operator, py, b"BI")? {
                // Begin inline image - a completely different parsing mechanism is required.
                if !operands.eq(&py_list(py, vec![])?)? {
                    return Err(assertion(py, None));
                }
                let kwargs = PyDict::new(py);
                kwargs.set_item("parsing_budget", &budget)?;
                kwargs.set_item("token_decrement", py_int(py, 2))?;
                content_stream_check_limits(py, &content_stream_class, &budget, 2)?;
                let read_inline_image = py
                    .import("pypdf.generic._image_inline")?
                    .getattr("read_inline_image")?;
                let kwargs = PyDict::new(py);
                kwargs.set_item("content_stream", slf)?;
                kwargs.set_item("stream", stream)?;
                kwargs.set_item("parsing_budget", &budget)?;
                let image = read_inline_image.call((), Some(&kwargs))?;
                let pair = py_tuple(py, vec![image, py_bytes(py, b"INLINE IMAGE")])?;
                slf.getattr("_operations")?
                    .call_method1("append", (pair,))?;
            } else {
                let kwargs = PyDict::new(py);
                kwargs.set_item("parsing_budget", &budget)?;
                kwargs.set_item("token_decrement", py_int(py, 1))?;
                content_stream_check_limits(py, &content_stream_class, &budget, 1)?;
                let pair = py_tuple(py, vec![operands.clone(), operator])?;
                slf.getattr("_operations")?
                    .call_method1("append", (pair,))?;
                operands = py_list(py, vec![])?;
            }
        } else if eq_bytes(&peek, py, b"%")? {
            // Comments have to be handled here: read_object assumes that the
            // object follows the comment, but here an operator may follow.
            let mut peek = peek;
            loop {
                if eq_bytes(&peek, py, b"\r")?
                    || eq_bytes(&peek, py, b"\n")?
                    || eq_bytes(&peek, py, b"")?
                {
                    break;
                }
                peek = read_n(stream, 1)?;
            }
        } else {
            let kwargs = PyDict::new(py);
            kwargs.set_item("parsing_budget", &budget)?;
            kwargs.set_item("token_decrement", py_int(py, 1))?;
            content_stream_check_limits(py, &content_stream_class, &budget, 1)?;
            let encoding = slf.getattr("forced_encoding")?;
            let none = py.None().into_bound(py);
            let value = call_read_object(py, stream, &none, Some(&encoding), Some(&budget))?;
            operands.call_method1("append", (value,))?;
        }
    }
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.ContentStream.get_data
#[pyfunction]
fn datastructures_contentstream_get_data<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let data = slf.getattr("_data")?;
    if data.is_truthy()? {
        return Ok(data.unbind());
    }
    let bytes_io = py.import("io")?.getattr("BytesIO")?;
    let new_data = bytes_io.call0()?;
    let operations = slf.getattr("_operations")?;
    for item in operations.try_iter()? {
        let item = item?;
        let operands = item.get_item(0)?;
        let operator = item.get_item(1)?;
        if eq_bytes(&operator, py, b"INLINE IMAGE")? {
            new_data.call_method1("write", (py_bytes(py, b"BI"),))?;
            let dict_text = bytes_io.call0()?;
            let settings = operands.get_item("settings")?;
            settings.call_method1("write_to_stream", (&dict_text,))?;
            let dict_bytes = dict_text.call_method0("getvalue")?;
            let trimmed = slice_object(py, &dict_bytes, Some(2), Some(-2))?;
            new_data.call_method1("write", (trimmed,))?;
            new_data.call_method1("write", (py_bytes(py, b"ID "),))?;
            new_data.call_method1("write", (operands.get_item("data")?,))?;
            new_data.call_method1("write", (py_bytes(py, b"EI"),))?;
        } else {
            for operand in operands.try_iter()? {
                let operand = operand?;
                operand.call_method1("write_to_stream", (&new_data,))?;
                new_data.call_method1("write", (py_bytes(py, b" "),))?;
            }
            new_data.call_method1("write", (operator,))?;
        }
        new_data.call_method1("write", (py_bytes(py, b"\n"),))?;
    }
    let result = new_data.call_method0("getvalue")?;
    slf.setattr("_data", &result)?;
    Ok(result.unbind())
}

// @pypdf: pypdf.generic._data_structures.ContentStream.set_data
#[pyfunction]
fn datastructures_contentstream_set_data<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    data: &Bound<'py, PyAny>,
) -> PyResult<()> {
    shim_class(py, "StreamObject")?
        .getattr("set_data")?
        .call1((slf, data))?;
    slf.setattr("_operations", py_list(py, vec![])?)?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.ContentStream.operations
#[pyfunction]
fn datastructures_contentstream_operations<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let operations = slf.getattr("_operations")?;
    let data = slf.getattr("_data")?;
    if !operations.is_truthy()? && data.is_truthy()? {
        let bytes_io = py.import("io")?.getattr("BytesIO")?;
        let stream = bytes_io.call1((data,))?;
        slf.call_method1("_parse_content_stream", (stream,))?;
        slf.setattr("_data", py_bytes(py, b""))?;
    }
    Ok(slf.getattr("_operations")?.unbind())
}

// @pypdf: pypdf.generic._data_structures.ContentStream.operations.setter
#[pyfunction]
fn datastructures_contentstream_set_operations<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    operations: &Bound<'py, PyAny>,
) -> PyResult<()> {
    slf.setattr("_operations", operations)?;
    slf.setattr("_data", py_bytes(py, b""))?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.ContentStream.isolate_graphics_state
#[pyfunction]
fn datastructures_contentstream_isolate_graphics_state<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let operations = slf.getattr("_operations")?;
    if operations.is_truthy()? {
        let first = py_tuple(py, vec![py_list(py, vec![])?, py_bytes(py, b"q")])?;
        operations.call_method1("insert", (0, first))?;
        let last = py_tuple(py, vec![py_list(py, vec![])?, py_bytes(py, b"Q")])?;
        operations.call_method1("append", (last,))?;
    } else {
        let data = slf.getattr("_data")?;
        if data.is_truthy()? {
            let prefix = py_bytes(py, b"q\n");
            let suffix = py_bytes(py, b"\nQ\n");
            let combined = prefix
                .call_method1("__add__", (&data,))?
                .call_method1("__add__", (suffix,))?;
            slf.setattr("_data", combined)?;
        }
    }
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.ContentStream.write_to_stream
#[pyfunction]
#[pyo3(signature = (slf, stream, encryption_key=None))]
fn datastructures_contentstream_write_to_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
    encryption_key: Option<&Bound<'py, PyAny>>,
) -> PyResult<()> {
    let _ = encryption_key;
    let data = slf.getattr("_data")?;
    let operations = slf.getattr("_operations")?;
    if !data.is_truthy()? && operations.is_truthy()? {
        // This ensures ._data is rebuilt.
        slf.call_method0("get_data")?;
    }
    shim_class(py, "StreamObject")?
        .getattr("write_to_stream")?
        .call1((slf, stream))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// read_object
// ---------------------------------------------------------------------------

/// `bytes` membership like `tok in b"..."` (empty bytes is a substring of
/// everything, exactly like CPython).
fn bytes_contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

// @pypdf: pypdf.generic._data_structures.read_object
#[pyfunction]
#[pyo3(signature = (stream, pdf, forced_encoding=None, parsing_budget=None))]
fn datastructures_read_object<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
    forced_encoding: Option<&Bound<'py, PyAny>>,
    parsing_budget: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let utils = utils_module(py)?;
    let generic_utils = generic_utils_module(py)?;
    let mut tok = read_n(stream, 1)?;
    seek(stream, -1, 1)?;
    if eq_bytes(&tok, py, b"%")? {
        // Skip consecutive comments in a loop instead of recursing per comment.
        let kwargs = PyDict::new(py);
        kwargs.set_item("limit", MAX_LEADING_COMMENTS)?;
        utils.call_method("skip_over_comments", (stream,), Some(&kwargs))?;
        tok = read_n(stream, 1)?;
        seek(stream, -1, 1)?;
    }
    if eq_bytes(&tok, py, b"/")? {
        return Ok(base_class(py, "NameObject")?
            .getattr("read_from_stream")?
            .call1((stream, pdf))?
            .unbind());
    }
    if eq_bytes(&tok, py, b"<")? {
        // Hexadecimal string or dictionary.
        let peek = read_n(stream, 2)?;
        seek(stream, -2, 1)?;
        if eq_bytes(&peek, py, b"<<")? {
            let dictionary_class = shim_class(py, "DictionaryObject")?;
            let kwargs = PyDict::new(py);
            if let Some(budget) = parsing_budget {
                kwargs.set_item("parsing_budget", budget)?;
            }
            let encoding = match forced_encoding {
                Some(encoding) => encoding.clone(),
                None => py.None().into_bound(py),
            };
            return Ok(dictionary_class
                .getattr("read_from_stream")?
                .call((stream, pdf, encoding), Some(&kwargs))?
                .unbind());
        }
        let read_hex = generic_utils.getattr("read_hex_string_from_stream")?;
        let encoding = match forced_encoding {
            Some(encoding) => encoding.clone(),
            None => py.None().into_bound(py),
        };
        return Ok(read_hex.call1((stream, encoding))?.unbind());
    }
    if eq_bytes(&tok, py, b"[")? {
        let array_class = shim_class(py, "ArrayObject")?;
        let kwargs = PyDict::new(py);
        if let Some(budget) = parsing_budget {
            kwargs.set_item("parsing_budget", budget)?;
        }
        let encoding = match forced_encoding {
            Some(encoding) => encoding.clone(),
            None => py.None().into_bound(py),
        };
        return Ok(array_class
            .getattr("read_from_stream")?
            .call((stream, pdf, encoding), Some(&kwargs))?
            .unbind());
    }
    if eq_bytes(&tok, py, b"t")? || eq_bytes(&tok, py, b"f")? {
        return Ok(base_class(py, "BooleanObject")?
            .getattr("read_from_stream")?
            .call1((stream,))?
            .unbind());
    }
    if eq_bytes(&tok, py, b"(")? {
        let read_string = generic_utils.getattr("read_string_from_stream")?;
        let encoding = match forced_encoding {
            Some(encoding) => encoding.clone(),
            None => py.None().into_bound(py),
        };
        return Ok(read_string.call1((stream, encoding))?.unbind());
    }
    if eq_bytes(&tok, py, b"e")? && eq_bytes(&read_n(stream, 6)?, py, b"endobj")? {
        return Ok(base_class(py, "NullObject")?.call0()?.unbind());
    }
    if eq_bytes(&tok, py, b"n")? {
        return Ok(base_class(py, "NullObject")?
            .getattr("read_from_stream")?
            .call1((stream,))?
            .unbind());
    }
    let tok_bytes = tok.extract::<Vec<u8>>()?;
    if bytes_contains(b"0123456789+-.", &tok_bytes) {
        // Number object or indirect reference.
        let peek = read_n(stream, 20)?;
        let indirect_pattern = shim_attr(py, "IndirectPattern")?;
        let matched = indirect_pattern.call_method1("match", (&peek,))?;
        if !matched.is_none() {
            if pdf.is_none() {
                return Err(assertion(py, Some("mypy")));
            }
            // Continue after the "R"; the match includes the following character.
            let end = matched.call_method0("end")?;
            let peek_len = length(&peek)?;
            let offset = end
                .call_method1("__sub__", (py_int(py, 1),))?
                .call_method1("__sub__", (peek_len as i64,))?;
            seek(stream, offset.extract()?, 1)?;
            let idnum_end = matched.call_method1("end", (1,))?;
            let idnum_stop: i64 = idnum_end.extract()?;
            let idnum_bytes = slice_object(py, &peek, None, Some(idnum_stop))?;
            // The object number includes an optional sign.
            let idnum = builtins(py)?.getattr("int")?.call1((idnum_bytes,))?;
            let generation_part = matched.get_item(2)?;
            let generation = builtins(py)?.getattr("int")?.call1((generation_part,))?;
            let indirect = base_class(py, "IndirectObject")?.call1((idnum, generation, pdf))?;
            return Ok(indirect.unbind());
        }
        let number_class = base_class(py, "NumberObject")?;
        let number_pattern = number_class.getattr("NumberPattern")?;
        let number_end = number_pattern.call_method1("search", (&peek,))?;
        if !number_end.is_none() {
            // The whole number is in the peek, no need to read it again.
            let number_start = number_end.call_method0("start")?;
            let number_stop: i64 = number_start.extract()?;
            let num = slice_object(py, &peek, None, Some(number_stop))?;
            let num_length = length(&num)?;
            let peek_length = length(&peek)?;
            seek(stream, num_length as i64 - peek_length as i64, 1)?;
            if num.contains(py_bytes(py, b"."))? {
                return Ok(base_class(py, "FloatObject")?.call1((num,))?.unbind());
            }
            return Ok(number_class.call1((num,))?.unbind());
        }
        seek(stream, -(peek_length_i64(&peek)?), 1)?;
        return Ok(number_class
            .getattr("read_from_stream")?
            .call1((stream,))?
            .unbind());
    }
    let pos = tell(stream)?;
    seek(stream, -20, 1)?;
    let stream_extract = read_n(stream, 80)?;
    seek_to(stream, &pos)?;
    utils.call_method1("read_until_whitespace", (stream,))?;
    let message = format!(
        "Invalid Elementary Object starting with {} @{}: {}",
        pyutil::py_repr(&tok)?,
        str_of(&pos)?,
        pyutil::py_repr(&stream_extract)?
    );
    Err(errors::pdf_read_error(py, &message))
}

fn peek_length_i64(obj: &Bound<'_, PyAny>) -> PyResult<i64> {
    Ok(obj.len()? as i64)
}

// ---------------------------------------------------------------------------
// Field
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._data_structures.Field.__init__
#[pyfunction]
fn datastructures_field_new<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    data: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let name_class = base_class(py, "NameObject")?;
    let field_attributes_class = constants_class(py, "FieldDictionaryAttributes")?;
    let checkbox_attributes_class = constants_class(py, "CheckboxRadioButtonAttributes")?;
    let dictionary_class = shim_class(py, "DictionaryObject")?;
    dictionary_class.getattr("__init__")?.call1((slf,))?;
    let field_attributes = field_attributes_class
        .call_method0("attributes")?
        .call_method1("__add__", (checkbox_attributes_class.call_method0("attributes")?,))?;
    slf.setattr("indirect_reference", data.getattr("indirect_reference")?)?;
    for attr in field_attributes.try_iter()? {
        let attr = attr?;
        let name_key = name_class.call1((&attr,))?;
        match data.get_item(&attr) {
            Ok(value) => slf.set_item(name_key, value)?,
            Err(error) if error.is_instance_of::<PyKeyError>(py) => {}
            Err(error) => return Err(error),
        }
    }
    let value_key = py_str(py, "/V");
    let value = dict_get(slf, &value_key, py.None().into_bound(py))?;
    if value.is_instance(&shim_class(py, "EncodedStreamObject")?)? {
        let name_key = name_class.call1((&value_key,))?;
        let resolved = slf.get_item(&name_key)?;
        let data = resolved.call_method0("get_data")?;
        let text = if data.is_instance_of::<PyBytes>() {
            data.call_method0("decode")?
        } else if data.is_none() {
            py_str(py, "")
        } else {
            return Err(message_exception(py, "Exception", "Should never happen"));
        };
        let text_string = base_class(py, "TextStringObject")?.call1((text,))?;
        slf.set_item(name_key, text_string)?;
    }
    Ok(())
}

fn field_attribute<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    name: &str,
) -> PyResult<Py<PyAny>> {
    let key = constants_class(py, "FieldDictionaryAttributes")?.getattr(name)?;
    Ok(dict_get(slf, &key, py.None().into_bound(py))?.unbind())
}

// @pypdf: pypdf.generic._data_structures.Field.field_type
#[pyfunction]
fn datastructures_field_field_type<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    field_attribute(py, slf, "FT")
}

// @pypdf: pypdf.generic._data_structures.Field.parent
#[pyfunction]
fn datastructures_field_parent<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    field_attribute(py, slf, "Parent")
}

// @pypdf: pypdf.generic._data_structures.Field.kids
#[pyfunction]
fn datastructures_field_kids<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    field_attribute(py, slf, "Kids")
}

// @pypdf: pypdf.generic._data_structures.Field.name
#[pyfunction]
fn datastructures_field_name<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    field_attribute(py, slf, "T")
}

// @pypdf: pypdf.generic._data_structures.Field.alternate_name
#[pyfunction]
fn datastructures_field_alternate_name<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    field_attribute(py, slf, "TU")
}

// @pypdf: pypdf.generic._data_structures.Field.mapping_name
#[pyfunction]
fn datastructures_field_mapping_name<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    field_attribute(py, slf, "TM")
}

// @pypdf: pypdf.generic._data_structures.Field.flags
#[pyfunction]
fn datastructures_field_flags<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    field_attribute(py, slf, "Ff")
}

// @pypdf: pypdf.generic._data_structures.Field.value
#[pyfunction]
fn datastructures_field_value<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    field_attribute(py, slf, "V")
}

// @pypdf: pypdf.generic._data_structures.Field.default_value
#[pyfunction]
fn datastructures_field_default_value<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    field_attribute(py, slf, "DV")
}

// @pypdf: pypdf.generic._data_structures.Field.additional_actions
#[pyfunction]
fn datastructures_field_additional_actions<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    field_attribute(py, slf, "AA")
}

// ---------------------------------------------------------------------------
// Destination
// ---------------------------------------------------------------------------

/// Collect the items of `args` (upstream tuple unpacking) into a Rust vector.
///
/// Errors other than `MemoryError` are reported as `Ok(None)` so callers can
/// reproduce upstream's `except Exception` fallbacks; `MemoryError` propagates
/// exactly like upstream's `except MemoryError: raise`.
fn collect_unpack<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyAny>,
) -> PyResult<Option<Vec<Bound<'py, PyAny>>>> {
    let mut items = Vec::new();
    match args.try_iter() {
        Ok(iterator) => {
            for item in iterator {
                match item {
                    Ok(item) => items.push(item),
                    Err(error) if error.is_instance_of::<PyMemoryError>(py) => {
                        return Err(error);
                    }
                    Err(_) => return Ok(None),
                }
            }
        }
        Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
        Err(_) => return Ok(None),
    }
    Ok(Some(items))
}

// @pypdf: pypdf.generic._data_structures.Destination.__init__
#[pyfunction]
fn datastructures_destination_new<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    title: &Bound<'py, PyAny>,
    page: &Bound<'py, PyAny>,
    fit: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let name_class = base_class(py, "NameObject")?;
    let null_class = base_class(py, "NullObject")?;
    let number_class = base_class(py, "NumberObject")?;
    let dictionary_class = shim_class(py, "DictionaryObject")?;
    let typ_arguments = constants_class(py, "TypArguments")?;
    let typ_fit_arguments = constants_class(py, "TypFitArguments")?;

    slf.setattr("_filtered_children", py_list(py, vec![])?)?;
    let typ = fit.getattr("fit_type")?;
    let args = fit.getattr("fit_args")?;

    dictionary_class.getattr("__init__")?.call1((slf,))?;
    let title_key = name_class.call1(("/Title",))?;
    let text = base_class(py, "TextStringObject")?.call1((title,))?;
    slf.set_item(title_key, text)?;
    slf.set_item(name_class.call1(("/Page",))?, page)?;
    slf.set_item(name_class.call1(("/Type",))?, &typ)?;

    // From table 8.2 of the PDF 1.7 reference.
    if typ.eq("/XYZ")? {
        while length(&args)? < 1 {
            // left is missing: should never occur
            args.call_method1("append", (number_class.call1((0.0,))?,))?;
        }
        while length(&args)? < 2 {
            // top is missing
            args.call_method1("append", (number_class.call1((0.0,))?,))?;
        }
        while length(&args)? < 3 {
            // zoom is missing
            args.call_method1("append", (number_class.call1((0.0,))?,))?;
        }
        // Surplus arguments are ignored rather than failing the unpacking.
        let mut targets = vec![
            typ_arguments.getattr("LEFT")?,
            typ_arguments.getattr("TOP")?,
            py_str(py, "/Zoom"),
        ];
        for (index, target) in targets.drain(..).enumerate() {
            let name_key = name_class.call1((target,))?;
            slf.set_item(name_key, args.get_item(index)?)?;
        }
    } else if length(&args)? == 0 {
        // pass
    } else if typ.eq(typ_fit_arguments.getattr("FIT_R")?)? {
        let targets = vec![
            typ_arguments.getattr("LEFT")?,
            typ_arguments.getattr("BOTTOM")?,
            typ_arguments.getattr("RIGHT")?,
            typ_arguments.getattr("TOP")?,
        ];
        if length(&args)? == 4 {
            for (index, target) in targets.into_iter().enumerate() {
                let name_key = name_class.call1((target,))?;
                slf.set_item(name_key, args.get_item(index)?)?;
            }
        } else {
            // A wrong number of arguments degrades to null.
            for target in targets {
                let name_key = name_class.call1((target,))?;
                slf.set_item(name_key, null_class.call0()?)?;
            }
        }
    } else if typ.eq(typ_fit_arguments.getattr("FIT_H")?)?
        || typ.eq(typ_fit_arguments.getattr("FIT_BH")?)?
    {
        // Prefer to be more robust, not only to null parameters.
        let value = match collect_unpack(py, &args)? {
            Some(mut items) if items.len() == 1 => match items.pop() {
                Some(item) => item,
                None => null_class.call0()?,
            },
            Some(_) | None => null_class.call0()?,
        };
        slf.set_item(name_class.call1((typ_arguments.getattr("TOP")?,))?, value)?;
    } else if typ.eq(typ_fit_arguments.getattr("FIT_V")?)?
        || typ.eq(typ_fit_arguments.getattr("FIT_BV")?)?
    {
        // Prefer to be more robust, not only to null parameters.
        let value = match collect_unpack(py, &args)? {
            Some(mut items) if items.len() == 1 => match items.pop() {
                Some(item) => item,
                None => null_class.call0()?,
            },
            Some(_) | None => null_class.call0()?,
        };
        slf.set_item(name_class.call1((typ_arguments.getattr("LEFT")?,))?, value)?;
    } else if typ.eq(typ_fit_arguments.getattr("FIT")?)?
        || typ.eq(typ_fit_arguments.getattr("FIT_B")?)?
    {
        // pass
    } else {
        let message = format!("Unknown Destination Type: {}", pyutil::py_repr(&typ)?);
        return Err(errors::pdf_read_error(py, &message));
    }
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.Destination.remove_from_tree
#[pyfunction]
fn datastructures_destination_remove_from_tree<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let node = slf.getattr("node")?;
    if node.is_none() {
        shim_class(py, "TreeObject")?
            .getattr("remove_from_tree")?
            .call1((slf,))?;
        return Ok(());
    }
    if !node.contains("/Parent")? {
        return Err(message_exception(
            py,
            "ValueError",
            "Removed child does not appear to be a tree item",
        ));
    }
    let parent = node.get_item("/Parent")?;
    shim_class(py, "TreeObject")?
        .getattr("remove_child")?
        .call1((parent, &node))?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.Destination.dest_array
#[pyfunction]
fn datastructures_destination_dest_array<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let mut items = vec![
        slf.call_method1("raw_get", ("/Page",))?,
        slf.get_item("/Type")?,
    ];
    for key in ["/Left", "/Bottom", "/Right", "/Top", "/Zoom"] {
        if slf.contains(key)? {
            items.push(slf.get_item(key)?);
        }
    }
    Ok(shim_class(py, "ArrayObject")?
        .call1((py_list(py, items)?,))?
        .unbind())
}

// @pypdf: pypdf.generic._data_structures.Destination.write_to_stream
#[pyfunction]
fn datastructures_destination_write_to_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let name_class = base_class(py, "NameObject")?;
    stream.call_method1("write", (py_bytes(py, b"<<\n"),))?;
    let key = name_class.call1(("/D",))?;
    key.call_method1("write_to_stream", (stream,))?;
    stream.call_method1("write", (py_bytes(py, b" "),))?;
    // `dest_array` is a property, so read it instead of calling it.
    let value = slf.getattr("dest_array")?;
    value.call_method1("write_to_stream", (stream,))?;

    let key = name_class.call1(("/S",))?;
    key.call_method1("write_to_stream", (stream,))?;
    stream.call_method1("write", (py_bytes(py, b" "),))?;
    let value_s = name_class.call1(("/GoTo",))?;
    value_s.call_method1("write_to_stream", (stream,))?;

    stream.call_method1("write", (py_bytes(py, b"\n"),))?;
    stream.call_method1("write", (py_bytes(py, b">>"),))?;
    Ok(())
}

// @pypdf: pypdf.generic._data_structures.Destination.title
#[pyfunction]
fn datastructures_destination_title<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(dict_get(slf, &py_str(py, "/Title"), py.None().into_bound(py))?.unbind())
}

// @pypdf: pypdf.generic._data_structures.Destination.page
#[pyfunction]
fn datastructures_destination_page<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(dict_get(slf, &py_str(py, "/Page"), py.None().into_bound(py))?.unbind())
}

// @pypdf: pypdf.generic._data_structures.Destination.typ
#[pyfunction]
fn datastructures_destination_typ<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(dict_get(slf, &py_str(py, "/Type"), py.None().into_bound(py))?.unbind())
}

// @pypdf: pypdf.generic._data_structures.Destination.zoom
#[pyfunction]
fn datastructures_destination_zoom<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    destination_item(py, slf, "/Zoom")
}

// @pypdf: pypdf.generic._data_structures.Destination.left
#[pyfunction]
fn datastructures_destination_left<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    destination_item(py, slf, "/Left")
}

// @pypdf: pypdf.generic._data_structures.Destination.right
#[pyfunction]
fn datastructures_destination_right<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    destination_item(py, slf, "/Right")
}

// @pypdf: pypdf.generic._data_structures.Destination.top
#[pyfunction]
fn datastructures_destination_top<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    destination_item(py, slf, "/Top")
}

// @pypdf: pypdf.generic._data_structures.Destination.bottom
#[pyfunction]
fn datastructures_destination_bottom<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    destination_item(py, slf, "/Bottom")
}

fn destination_item<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &str,
) -> PyResult<Py<PyAny>> {
    Ok(dict_get(slf, &py_str(py, key), py.None().into_bound(py))?.unbind())
}

// @pypdf: pypdf.generic._data_structures.Destination.color
#[pyfunction]
fn datastructures_destination_color<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let float_class = base_class(py, "FloatObject")?;
    let default = shim_class(py, "ArrayObject")?.call1((py_list(
        py,
        vec![
            float_class.call1((0,))?,
            float_class.call1((0,))?,
            float_class.call1((0,))?,
        ],
    )?,))?;
    Ok(dict_get(slf, &py_str(py, "/C"), default)?.unbind())
}

// @pypdf: pypdf.generic._data_structures.Destination.font_format
#[pyfunction]
fn datastructures_destination_font_format<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let outline_font_flag = constants_class(py, "OutlineFontFlag")?;
    let value = dict_get(slf, &py_str(py, "/F"), py_int(py, 0))?;
    Ok(outline_font_flag.call1((value,))?.unbind())
}

// @pypdf: pypdf.generic._data_structures.Destination.outline_count
#[pyfunction]
fn datastructures_destination_outline_count<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    destination_item(py, slf, "/Count")
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(datastructures_read_object, module)?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_requires_constructor_arguments,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_reset_node_tree_relationship,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(datastructures_arrayobject_replicate, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_arrayobject_clone, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_arrayobject_hash_bin, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_arrayobject_items, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_arrayobject_to_lst, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_arrayobject_add, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_arrayobject_iadd, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_arrayobject_isub, module)?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_arrayobject_write_to_stream,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_arrayobject_check_limits,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_arrayobject_read_from_stream,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_new_for_clone,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_replicate,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_clone,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_clone_inner,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_hash_bin,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_raw_get,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_get_inherited,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_setitem,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_setdefault,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_getitem,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_xmp_metadata,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_write_to_stream,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_get_next_object_position,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_read_unsized_from_stream,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_read_from_stream,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_dictionaryobject_read_stream_data,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(
        datastructures_treeobject_has_children,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(datastructures_treeobject_iter, module)?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_treeobject_children_start,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_treeobject_children_next,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_treeobject_add_child,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_treeobject_inc_parent_counter_default,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_treeobject_inc_parent_counter_outline,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_treeobject_insert_child,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_treeobject_remove_node_from_tree,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_treeobject_remove_child,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_treeobject_remove_from_tree,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_treeobject_empty_tree,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(datastructures_streamobject_replicate, module)?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_streamobject_clone_inner,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_streamobject_hash_bin,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(datastructures_streamobject_get_data, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_streamobject_set_data, module)?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_streamobject_hash_value_data,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_streamobject_write_to_stream,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_streamobject_initialize_from_dictionary,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_streamobject_flate_encode,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_streamobject_decode_as_image,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(
        datastructures_encodedstreamobject_hash_value_data,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_encodedstreamobject_get_data,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_encodedstreamobject_set_data,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(datastructures_contentstream_new, module)?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_contentstream_replicate,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(datastructures_contentstream_clone, module)?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_contentstream_clone_inner,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_contentstream_check_limits,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_contentstream_parse_content_stream,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_contentstream_get_data,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_contentstream_set_data,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_contentstream_operations,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_contentstream_set_operations,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_contentstream_isolate_graphics_state,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_contentstream_write_to_stream,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(datastructures_field_new, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_field_field_type, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_field_parent, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_field_kids, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_field_name, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_field_alternate_name, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_field_mapping_name, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_field_flags, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_field_value, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_field_default_value, module)?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_field_additional_actions,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(datastructures_destination_new, module)?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_destination_remove_from_tree,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_destination_dest_array,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_destination_write_to_stream,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(datastructures_destination_title, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_destination_page, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_destination_typ, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_destination_zoom, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_destination_left, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_destination_right, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_destination_top, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_destination_bottom, module)?)?;
    module.add_function(wrap_pyfunction!(datastructures_destination_color, module)?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_destination_font_format,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        datastructures_destination_outline_count,
        module
    )?)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Symbols implemented in the hand-written shim
// (`python/pypdf/generic/_data_structures.py`).
//
// Class objects, class attributes, module constants, the pure attribute
// assignment `__init__`s and the `typing.NamedTuple` data container stay Python
// definitions: they are part of the observable Python object model (MRO,
// `isinstance`, subclassing, `repr`).  Every method body lives in this file.
//
// @pypdf: pypdf.generic._data_structures.__author__
// @pypdf: pypdf.generic._data_structures.__author_email__
// @pypdf: pypdf.generic._data_structures.logger
// @pypdf: pypdf.generic._data_structures.IndirectPattern
// @pypdf: pypdf.generic._data_structures._SCALAR_TYPES
// @pypdf: pypdf.generic._data_structures._MAX_LEADING_COMMENTS
// @pypdf: pypdf.generic._data_structures.CONTENT_STREAM_ARRAY_MAX_LENGTH
// @pypdf: pypdf.generic._data_structures.ArrayObject
// @pypdf: pypdf.generic._data_structures.DictionaryObject
// @pypdf: pypdf.generic._data_structures.TreeObject
// @pypdf: pypdf.generic._data_structures.TreeObject.__init__
// @pypdf: pypdf.generic._data_structures._FontFileCharacterMap
// @pypdf: pypdf.generic._data_structures._FontFileCharacterMap.digest
// @pypdf: pypdf.generic._data_structures._FontFileCharacterMap.map_dict
// @pypdf: pypdf.generic._data_structures._FontFileCharacterMap.int_entry
// @pypdf: pypdf.generic._data_structures.StreamObject
// @pypdf: pypdf.generic._data_structures.StreamObject.__init__
// @pypdf: pypdf.generic._data_structures.StreamObject._font_file_character_map
// @pypdf: pypdf.generic._data_structures.DecodedStreamObject
// @pypdf: pypdf.generic._data_structures.EncodedStreamObject
// @pypdf: pypdf.generic._data_structures.EncodedStreamObject.__init__
// @pypdf: pypdf.generic._data_structures.ContentStream
// @pypdf: pypdf.generic._data_structures.ContentStream._OPERATOR_LENGTH_LIMIT
// @pypdf: pypdf.generic._data_structures.Field
// @pypdf: pypdf.generic._data_structures.Destination
// @pypdf: pypdf.generic._data_structures.Destination.node
