//! Small helpers for working with pypdf's Python-native object model.
//!
//! The port deliberately manipulates Python objects (see PORTING_GUIDE §1), so
//! most helpers here are thin, exact wrappers around Python semantics rather
//! than Rust reinterpretations of them.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

/// Fetch a class object from a pypdf module, e.g. `("pypdf.generic", "NameObject")`.
pub fn pypdf_class<'py>(
    py: Python<'py>,
    module: &str,
    name: &str,
) -> PyResult<Bound<'py, PyAny>> {
    py.import(module)?.getattr(name)
}

/// `str(obj)` through Python's `__str__`.
pub fn py_str(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    obj.str()?.extract()
}

/// `repr(obj)` through Python's `__repr__`.
pub fn py_repr(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    obj.repr()?.extract()
}

/// `a + b` using Python's operator protocol.
pub fn py_add<'py>(a: &Bound<'py, PyAny>, b: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    a.call_method1("__add__", (b,))
}

/// `a == b` using Python equality semantics.
pub fn py_eq(a: &Bound<'_, PyAny>, b: &Bound<'_, PyAny>) -> PyResult<bool> {
    a.eq(b)
}

/// `hash(obj)`.
pub fn py_hash(obj: &Bound<'_, PyAny>) -> PyResult<isize> {
    obj.hash()
}

/// `slice(start, stop, step)`.
pub fn py_slice<'py>(
    py: Python<'py>,
    start: Option<&Bound<'py, PyAny>>,
    stop: Option<&Bound<'py, PyAny>>,
    step: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let slice_type = py.import("builtins")?.getattr("slice")?;
    let mut args: Vec<Bound<'py, PyAny>> = Vec::with_capacity(3);
    for value in [start, stop, step] {
        match value {
            Some(value) => args.push(value.clone()),
            None => args.push(py.None().into_bound(py)),
        }
    }
    // ``slice(None, None, None)`` is valid and equals ``slice()``; always pass
    // all three positions so the argument count matches upstream calls.
    slice_type.call1(PyTuple::new(py, args)?)
}

/// `isinstance(obj, cls)`.
pub fn is_instance(obj: &Bound<'_, PyAny>, class: &Bound<'_, PyAny>) -> PyResult<bool> {
    obj.is_instance(class)
}

/// `len(obj)`.
pub fn py_len(obj: &Bound<'_, PyAny>) -> PyResult<usize> {
    obj.len()
}

/// Emulate ``logger_warning(message, source=..., **values)`` from pypdf._utils.
///
/// Upstream logs through `logging.getLogger(source).warning(message, values)`
/// when values are present and `.warning(message)` otherwise; reproducing both
/// shapes keeps `caplog` output identical.
pub fn logger_warning(
    py: Python<'_>,
    source: &str,
    message: &str,
    values: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let logger = py.import("logging")?.call_method1("getLogger", (source,))?;
    match values {
        Some(values) if !values.is_empty() => {
            logger.call_method1("warning", (message, values))?;
        }
        _ => {
            logger.call_method1("warning", (message,))?;
        }
    }
    Ok(())
}

/// `warnings.warn(message, category, stacklevel=...)`.
pub fn warn(
    py: Python<'_>,
    message: &str,
    category: &str,
    stacklevel: usize,
) -> PyResult<()> {
    let warnings = py.import("warnings")?;
    let category = py.import("builtins")?.getattr(category)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("stacklevel", stacklevel)?;
    warnings.call_method("warn", (message, category), Some(&kwargs))?;
    Ok(())
}
