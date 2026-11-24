//! Deprecation helpers and logger wrappers from `pypdf/_utils.py`.
//!
//! Stack levels: upstream's helpers are Python functions, so their own frames
//! are counted by `warnings.warn`.  The Rust functions are C functions and
//! therefore invisible to the frame walk (`sys._getframe` skips them), so each
//! exported helper subtracts the number of upstream Python frames it replaces.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyString, PyTuple};

use crate::errors;
use crate::pyutil;

/// Emit `warnings.warn(message, DeprecationWarning, stacklevel=stacklevel)`
/// where `stacklevel` is already expressed in *visible* Python frames.
fn warn_deprecation(py: Python<'_>, message: &Bound<'_, PyAny>, stacklevel: i64) -> PyResult<()> {
    let warnings = py.import("warnings")?;
    let category = py.import("builtins")?.getattr("DeprecationWarning")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("stacklevel", stacklevel.max(1))?;
    warnings.call_method("warn", (message, category), Some(&kwargs))?;
    Ok(())
}

fn raise_deprecation_error(py: Python<'_>, message: &Bound<'_, PyAny>) -> PyErr {
    match PyTuple::new(py, [message.clone()]) {
        Ok(args) => errors::raise(py, "DeprecationError", args),
        Err(error) => error,
    }
}

// @pypdf: pypdf._utils.deprecate
#[pyfunction]
#[pyo3(signature = (msg, stacklevel=3))]
pub(super) fn deprecate(py: Python<'_>, msg: &Bound<'_, PyAny>, stacklevel: i64) -> PyResult<()> {
    // Saturating to avoid integer overflow on hostile input; `warn_deprecation`
    // clamps the visible stack level to at least 1.
    warn_deprecation(py, msg, stacklevel.saturating_sub(1))
}

// @pypdf: pypdf._utils.deprecation
#[pyfunction]
pub(super) fn deprecation(py: Python<'_>, msg: &Bound<'_, PyAny>) -> PyResult<()> {
    Err(raise_deprecation_error(py, msg))
}

// @pypdf: pypdf._utils.deprecate_with_replacement
#[pyfunction]
pub(super) fn deprecate_with_replacement(
    py: Python<'_>,
    old_name: &Bound<'_, PyAny>,
    new_name: &Bound<'_, PyAny>,
    removed_in: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let message = format!(
        "{} is deprecated and will be removed in pypdf {}. Use {} instead.",
        pyutil::py_str(old_name)?,
        pyutil::py_str(removed_in)?,
        pyutil::py_str(new_name)?
    );
    // Upstream `deprecate_with_replacement` -> `deprecate` spans two Python
    // frames; both are C frames here.
    warn_deprecation(py, PyString::new(py, &message).as_any(), 2)
}

// @pypdf: pypdf._utils.deprecation_with_replacement
#[pyfunction]
pub(super) fn deprecation_with_replacement(
    py: Python<'_>,
    old_name: &Bound<'_, PyAny>,
    new_name: &Bound<'_, PyAny>,
    removed_in: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let message = format!(
        "{} is deprecated and was removed in pypdf {}. Use {} instead.",
        pyutil::py_str(old_name)?,
        pyutil::py_str(removed_in)?,
        pyutil::py_str(new_name)?
    );
    Err(raise_deprecation_error(
        py,
        PyString::new(py, &message).as_any(),
    ))
}

// @pypdf: pypdf._utils.deprecate_no_replacement
#[pyfunction]
pub(super) fn deprecate_no_replacement(
    py: Python<'_>,
    name: &Bound<'_, PyAny>,
    removed_in: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let message = format!(
        "{} is deprecated and will be removed in pypdf {}.",
        pyutil::py_str(name)?,
        pyutil::py_str(removed_in)?
    );
    warn_deprecation(py, PyString::new(py, &message).as_any(), 2)
}

// @pypdf: pypdf._utils.deprecation_no_replacement
#[pyfunction]
pub(super) fn deprecation_no_replacement(
    py: Python<'_>,
    name: &Bound<'_, PyAny>,
    removed_in: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let message = format!(
        "{} is deprecated and was removed in pypdf {}.",
        pyutil::py_str(name)?,
        pyutil::py_str(removed_in)?
    );
    Err(raise_deprecation_error(
        py,
        PyString::new(py, &message).as_any(),
    ))
}

// @pypdf: pypdf._utils.logger_error
#[pyfunction]
#[pyo3(signature = (message, *, source, **values))]
pub(super) fn logger_error(
    py: Python<'_>,
    message: &Bound<'_, PyAny>,
    source: &Bound<'_, PyAny>,
    values: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let logger = py
        .import("logging")?
        .call_method1("getLogger", (source.clone(),))?;
    match values {
        Some(values) if !values.is_empty() => {
            logger.call_method1("error", (message.clone(), values.clone()))?;
        }
        _ => {
            logger.call_method1("error", (message.clone(),))?;
        }
    }
    Ok(())
}

// @pypdf: pypdf._utils.logger_warning
#[pyfunction]
#[pyo3(signature = (message, *, source, **values))]
pub(super) fn logger_warning(
    py: Python<'_>,
    message: &Bound<'_, PyAny>,
    source: &Bound<'_, PyAny>,
    values: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let logger = py
        .import("logging")?
        .call_method1("getLogger", (source.clone(),))?;
    match values {
        Some(values) if !values.is_empty() => {
            logger.call_method1("warning", (message.clone(), values.clone()))?;
        }
        _ => {
            logger.call_method1("warning", (message.clone(),))?;
        }
    }
    Ok(())
}

// @pypdf: pypdf._utils.rename_kwargs
#[pyfunction]
#[pyo3(signature = (func_name, kwargs, aliases, fail=false))]
pub(super) fn rename_kwargs(
    py: Python<'_>,
    func_name: &Bound<'_, PyAny>,
    kwargs: &Bound<'_, PyDict>,
    aliases: &Bound<'_, PyAny>,
    fail: bool,
) -> PyResult<()> {
    let items = aliases.call_method0("items")?;
    for item in items.try_iter()? {
        let item = item?;
        let old_term = item.get_item(0)?;
        let new_term = item.get_item(1)?;
        if !kwargs.contains(&old_term)? {
            continue;
        }
        if fail {
            return Err(raise_deprecation_error(
                py,
                PyString::new(
                    py,
                    &format!(
                        "{} is deprecated as an argument. Use {} instead",
                        pyutil::py_str(&old_term)?,
                        pyutil::py_str(&new_term)?
                    ),
                )
                .as_any(),
            ));
        }
        if kwargs.contains(&new_term)? {
            return Err(PyTypeError::new_err(format!(
                "{} received both {} and {} as an argument. {} is deprecated. Use {} instead.",
                pyutil::py_str(func_name)?,
                pyutil::py_str(&old_term)?,
                pyutil::py_str(&new_term)?,
                pyutil::py_str(&old_term)?,
                pyutil::py_str(&new_term)?,
            )));
        }
        let value = kwargs.call_method1("pop", (&old_term,))?;
        kwargs.set_item(&new_term, value)?;
        let message = format!(
            "{} is deprecated as an argument. Use {} instead",
            pyutil::py_str(&old_term)?,
            pyutil::py_str(&new_term)?
        );
        // Upstream `rename_kwargs` is a single Python frame; the C frame is
        // invisible, hence 3 - 1.
        warn_deprecation(py, PyString::new(py, &message).as_any(), 2)?;
    }
    Ok(())
}
