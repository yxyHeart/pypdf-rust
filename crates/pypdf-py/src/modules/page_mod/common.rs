//! Shared helpers for the `pypdf._page` port.
//!
//! Upstream `_page.py` is Python code operating on Python objects; the port keeps
//! exactly that: every helper here wraps a Python semantic (operator, builtin,
//! module-global lookup) instead of re-implementing it, so that monkeypatching,
//! subclassing, `isinstance` and error messages stay identical.

use pyo3::exceptions::{PyAssertionError, PyAttributeError, PyIndexError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyFloat, PyInt, PyList, PyModule, PyString, PyTuple};

use crate::pyutil::logger_warning as util_logger_warning;

/// Upstream module name; used as `logger_warning(..., source=__name__)`.
pub const SOURCE: &str = "pypdf._page";

/// The hand-written shim module (`python/pypdf/_page.py`).
pub fn shim(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    py.import("pypdf._page")
}

/// Fetch a module-global from the shim.  Upstream resolves these names through
/// the module globals at call time, so the port does the same.
pub fn globals<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim(py)?.getattr(name)
}

pub fn generic<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("pypdf.generic")
}

/// Fetch a class from `pypdf.generic` (the same objects the shim imports).
pub fn generic_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    generic(py)?.getattr(name)
}

pub fn constants<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("pypdf.constants")
}

/// `pypdf.constants.<class>.<name>`.
pub fn constants_attr<'py>(
    py: Python<'py>,
    class_name: &str,
    name: &str,
) -> PyResult<Bound<'py, PyAny>> {
    constants(py)?.getattr(class_name)?.getattr(name)
}

/// `pypdf.constants.<class>.<name>` as a `String`.
pub fn constants_str(py: Python<'_>, class_name: &str, name: &str) -> PyResult<String> {
    constants_attr(py, class_name, name)?.extract()
}

pub fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("builtins")
}

/// `operator.<name>` (binary operator without short-circuit surprises).
pub fn operator<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("operator")?.getattr(name)
}

pub fn op_add<'py>(
    py: Python<'py>,
    a: &Bound<'py, PyAny>,
    b: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    operator(py, "add")?.call1((a, b))
}

pub fn op_sub<'py>(
    py: Python<'py>,
    a: &Bound<'py, PyAny>,
    b: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    operator(py, "sub")?.call1((a, b))
}

pub fn op_mul<'py>(
    py: Python<'py>,
    a: &Bound<'py, PyAny>,
    b: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    operator(py, "mul")?.call1((a, b))
}

pub fn op_truediv<'py>(
    py: Python<'py>,
    a: &Bound<'py, PyAny>,
    b: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    operator(py, "truediv")?.call1((a, b))
}

pub fn op_neg<'py>(py: Python<'py>, a: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    operator(py, "neg")?.call1((a,))
}

/// `a == b` through Python equality.
pub fn py_len(obj: &Bound<'_, PyAny>) -> PyResult<usize> {
    obj.len()
}

pub fn py_str(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    obj.str()?.extract()
}

/// `float(obj)`.
/// `float(obj)` through the builtin (accepts `__float__`-only objects), then f64.
pub fn py_float_of(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<f64> {
    builtins(py)?.getattr("float")?.call1((obj,))?.extract()
}

/// `str(obj)`.
/// `isinstance(obj, cls)`.
/// `hasattr(obj, name)`.
/// `getattr(obj, name)` mapped to `Option` when the attribute is missing
/// (other exceptions propagate, exactly like upstream's `except AttributeError`).
pub fn getattr_opt<'py>(
    obj: &Bound<'py, PyAny>,
    name: &str,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    match obj.getattr(name) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.is_instance_of::<PyAttributeError>(obj.py()) => Ok(None),
        Err(error) => Err(error),
    }
}

/// `is_null_or_none(obj)` (from `pypdf.generic`).
pub fn is_null_or_none(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    generic(obj.py())?
        .getattr("is_null_or_none")?
        .call1((obj,))?
        .extract()
}

pub fn null_object(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    generic_class(py, "NullObject")?.call0()
}

pub fn dict_object(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    generic_class(py, "DictionaryObject")?.call0()
}

pub fn array_object(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    generic_class(py, "ArrayObject")?.call0()
}

pub fn name_object<'py>(py: Python<'py>, value: &str) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "NameObject")?.call1((value,))
}

pub fn number_object<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "NumberObject")?.call1((value,))
}

pub fn float_object<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "FloatObject")?.call1((value,))
}

pub fn rectangle_object<'py>(
    py: Python<'py>,
    value: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    globals(py, "RectangleObject")?.call1((value,))
}

pub fn py_float<'py>(py: Python<'py>, value: f64) -> Bound<'py, PyAny> {
    PyFloat::new(py, value).into_any()
}

pub fn py_int<'py>(py: Python<'py>, value: i64) -> Bound<'py, PyAny> {
    PyInt::new(py, value).into_any()
}

pub fn py_bytes<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, data).into_any()
}

pub fn py_str_type<'py>(py: Python<'py>, value: &str) -> Bound<'py, PyAny> {
    PyString::new(py, value).into_any()
}

pub fn py_tuple<'py>(
    py: Python<'py>,
    values: Vec<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyTuple::new(py, values)?.into_any())
}

pub fn py_list<'py>(
    py: Python<'py>,
    values: Vec<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyList::new(py, values)?.into_any())
}

pub fn slice_object<'py>(
    py: Python<'py>,
    start: Option<Bound<'py, PyAny>>,
    stop: Option<Bound<'py, PyAny>>,
    step: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let slice_type = builtins(py)?.getattr("slice")?;
    let args: Vec<Bound<'py, PyAny>> = vec![
        start.unwrap_or_else(|| py.None().into_bound(py)),
        stop.unwrap_or_else(|| py.None().into_bound(py)),
        step.unwrap_or_else(|| py.None().into_bound(py)),
    ];
    slice_type.call1(PyTuple::new(py, args)?)
}

/// Emulate ``logger_warning(message, source="pypdf._page", **values)``.
pub fn logger_warning(
    py: Python<'_>,
    message: &str,
    values: &[(&str, Bound<'_, PyAny>)],
) -> PyResult<()> {
    if values.is_empty() {
        return util_logger_warning(py, SOURCE, message, None);
    }
    let kwargs = PyDict::new(py);
    for (key, value) in values {
        kwargs.set_item(*key, value)?;
    }
    util_logger_warning(py, SOURCE, message, Some(&kwargs))
}

pub fn type_error(_py: Python<'_>, message: &str) -> PyErr {
    PyTypeError::new_err(message.to_owned())
}

pub fn index_error(_py: Python<'_>, message: &str) -> PyErr {
    PyIndexError::new_err(message.to_owned())
}

pub fn assertion_error(_py: Python<'_>, message: &str) -> PyErr {
    PyAssertionError::new_err(message.to_owned())
}

/// The `range` builtin as a Python object: `range(*indices)`.
pub fn range_from_tuple<'py>(
    py: Python<'py>,
    indices: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let range_type = builtins(py)?.getattr("range")?;
    range_type.call1(PyTuple::new(
        py,
        indices.try_iter()?.collect::<PyResult<Vec<_>>>()?,
    )?)
}

/// `list(iterable)`.
pub fn list_of<'py>(py: Python<'py>, iterable: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("list")?.call1((iterable,))
}

/// `hash(obj)`.
pub fn py_hash(obj: &Bound<'_, PyAny>) -> PyResult<isize> {
    obj.hash()
}
