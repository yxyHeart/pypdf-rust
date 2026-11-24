//! PDF date string helpers ported from `pypdf/_utils.py`.

use pyo3::exceptions::{PyAssertionError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyString, PyTuple};

use crate::pyutil;

use super::common;

const DATE_FORMATS: [&str; 7] = [
    "D:%Y",
    "D:%Y%m",
    "D:%Y%m%d",
    "D:%Y%m%d%H",
    "D:%Y%m%d%H%M",
    "D:%Y%m%d%H%M%S",
    "D:%Y%m%d%H%M%S%z",
];

// @pypdf: pypdf._utils.parse_iso8824_date
#[pyfunction]
pub(super) fn parse_iso8824_date(py: Python<'_>, text: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let original = text.clone();
    if !text.is_truthy()? {
        return Ok(py.None());
    }
    let mut text = text.clone();

    let first = text.get_item(0)?;
    if first.call_method0("isdigit")?.is_truthy()? {
        text = common::str_prefix(py, "D:", &text)?;
    }
    let z_suffixes = PyTuple::new(py, [PyString::new(py, "Z"), PyString::new(py, "z")])?;
    if text.call_method1("endswith", (z_suffixes,))?.is_truthy()? {
        text = common::str_suffix(&text, "0000")?;
    }
    text = text.call_method1("replace", ("z", "+"))?;
    text = text.call_method1("replace", ("Z", "+"))?;
    text = text.call_method1("replace", ("'", ""))?;

    let plus = text.call_method1("find", ("+",))?.extract::<i64>()?;
    let minus = text.call_method1("find", ("-",))?.extract::<i64>()?;
    let index = plus.max(minus);
    if index > 0 && index != pyutil::py_len(&text)? as i64 - 5 {
        text = common::str_suffix(&text, "00")?;
    }

    let datetime_module = py.import("datetime")?;
    let datetime_class = datetime_module.getattr("datetime")?;
    for format in DATE_FORMATS {
        let parsed = match datetime_class.call_method1("strptime", (text.clone(), format)) {
            Ok(parsed) => parsed,
            Err(error) => {
                if error.is_instance_of::<PyValueError>(py) {
                    continue;
                }
                return Err(error);
            }
        };
        if text.call_method1("endswith", ("+0000",))?.is_truthy()? {
            let timezone = datetime_module.getattr("timezone")?;
            let utc = timezone.getattr("utc")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("tzinfo", utc)?;
            return Ok(parsed.call_method("replace", (), Some(&kwargs))?.unbind());
        }
        return Ok(parsed.unbind());
    }
    Err(PyValueError::new_err(format!(
        "Can not convert date: {}",
        pyutil::py_str(&original)?
    )))
}

// @pypdf: pypdf._utils.format_iso8824_date
#[pyfunction]
pub(super) fn format_iso8824_date(py: Python<'_>, dt: &Bound<'_, PyAny>) -> PyResult<String> {
    let mut date_str: String = dt
        .call_method1("strftime", ("D:%Y%m%d%H%M%S",))?
        .extract()?;
    let tzinfo = dt.getattr("tzinfo")?;
    if !tzinfo.is_none() {
        let offset = dt.call_method0("utcoffset")?;
        if offset.is_none() {
            return Err(PyAssertionError::new_err(()));
        }
        let builtins = py.import("builtins")?;
        let total_seconds = builtins
            .getattr("int")?
            .call1((offset.call_method0("total_seconds")?,))?;
        let absolute = total_seconds.call_method0("__abs__")?;
        let divmod = builtins.getattr("divmod")?.call1((absolute, 3600i64))?;
        let hours = divmod.get_item(0)?;
        let remainder = divmod.get_item(1)?;
        let minutes = remainder.call_method1("__floordiv__", (60i64,))?;
        let sign = if total_seconds.ge(0i64)? { "+" } else { "-" };
        let hours_text: String = builtins
            .getattr("format")?
            .call1((hours, "02d"))?
            .extract()?;
        let minutes_text: String = builtins
            .getattr("format")?
            .call1((minutes, "02d"))?
            .extract()?;
        date_str.push_str(&format!("{sign}{hours_text}'{minutes_text}'"));
    }
    Ok(date_str)
}
