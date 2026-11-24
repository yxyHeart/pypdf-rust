//! RTL/neutral character classification from `pypdf/_utils.py`.

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyString, PyTuple};

// @pypdf: pypdf._utils.NEUTRAL_CHARACTER_RANGES
pub const NEUTRAL_CHARACTER_RANGES: [(&str, &str); 6] = [
    ("\u{0000}", "\u{002F}"),
    ("\u{003A}", "\u{0040}"),
    ("\u{0660}", "\u{066D}"),
    ("\u{06F0}", "\u{06F9}"),
    ("\u{2000}", "\u{206F}"),
    ("\u{20A0}", "\u{21FF}"),
];

// @pypdf: pypdf._utils.RTL_CHARACTER_RANGES
pub const RTL_CHARACTER_RANGES: [(&str, &str); 3] = [
    ("\u{0590}", "\u{08FF}"),
    ("\u{FB1D}", "\u{FDFF}"),
    ("\u{FE70}", "\u{FEFF}"),
];

fn ranges_tuple<'py>(py: Python<'py>, ranges: &[(&str, &str)]) -> PyResult<Bound<'py, PyTuple>> {
    let mut items: Vec<Bound<'_, PyAny>> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        let pair = PyTuple::new(py, [PyString::new(py, start), PyString::new(py, end)])?;
        items.push(pair.into_any());
    }
    PyTuple::new(py, items)
}

/// `NEUTRAL_CHARACTER_RANGES` as a Python tuple of tuples of `str`.
pub(super) fn neutral_character_ranges<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
    ranges_tuple(py, &NEUTRAL_CHARACTER_RANGES)
}

/// `RTL_CHARACTER_RANGES` as a Python tuple of tuples of `str`.
pub(super) fn rtl_character_ranges<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
    ranges_tuple(py, &RTL_CHARACTER_RANGES)
}

/// Python string ordering of `str` values is code-point order; for valid UTF-8
/// that is exactly Rust's `str` ordering, so comparisons stay in Rust.
fn in_ranges(value: &str, ranges: &[(&str, &str)]) -> bool {
    ranges
        .iter()
        .any(|(start, end)| *start <= value && value <= *end)
}

fn python_in_ranges(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    ranges: &[(&str, &str)],
) -> PyResult<bool> {
    for (start, end) in ranges {
        let start = PyString::new(py, start);
        let end = PyString::new(py, end);
        if start.le(value)? && value.le(&end)? {
            return Ok(true);
        }
    }
    Ok(false)
}

// @pypdf: pypdf._utils.is_char_neutral
#[pyfunction]
#[pyo3(signature = (char, custom_special_characters=None))]
pub(super) fn is_char_neutral(
    py: Python<'_>,
    char: &Bound<'_, PyAny>,
    custom_special_characters: Option<&Bound<'_, PyAny>>,
) -> PyResult<bool> {
    if let Ok(value) = char.extract::<String>() {
        if in_ranges(&value, &NEUTRAL_CHARACTER_RANGES) {
            return Ok(true);
        }
        if let Some(custom) = custom_special_characters {
            if custom.is_truthy()? {
                if let Ok(custom_text) = custom.extract::<String>() {
                    // `char in str` is a substring test.
                    return Ok(custom_text.contains(value.as_str()));
                }
                return custom
                    .call_method1("__contains__", (char.clone(),))?
                    .is_truthy();
            }
        }
        return Ok(false);
    }
    if python_in_ranges(py, char, &NEUTRAL_CHARACTER_RANGES)? {
        return Ok(true);
    }
    if let Some(custom) = custom_special_characters {
        if custom.is_truthy()? {
            return custom
                .call_method1("__contains__", (char.clone(),))?
                .is_truthy();
        }
    }
    Ok(false)
}

// @pypdf: pypdf._utils.is_char_rtl
#[pyfunction]
#[pyo3(signature = (char, custom_rtl_min=None, custom_rtl_max=None))]
pub(super) fn is_char_rtl(
    py: Python<'_>,
    char: &Bound<'_, PyAny>,
    custom_rtl_min: Option<&Bound<'_, PyAny>>,
    custom_rtl_max: Option<&Bound<'_, PyAny>>,
) -> PyResult<bool> {
    if let Ok(value) = char.extract::<String>() {
        if in_ranges(&value, &RTL_CHARACTER_RANGES) {
            return Ok(true);
        }
        let min = custom_rtl_min.and_then(|bound| bound.extract::<String>().ok());
        let max = custom_rtl_max.and_then(|bound| bound.extract::<String>().ok());
        if let (Some(min), Some(max)) = (min, max) {
            return Ok(!min.is_empty()
                && !max.is_empty()
                && min.as_str() <= value.as_str()
                && value.as_str() <= max.as_str());
        }
    } else if python_in_ranges(py, char, &RTL_CHARACTER_RANGES)? {
        return Ok(true);
    }
    // Fall back to Python comparisons for non-`str` bounds (or surrogate
    // strings, which cannot be extracted as Rust `String`).
    let min_truthy = custom_rtl_min.map_or(Ok(false), |bound| bound.is_truthy())?;
    let max_truthy = custom_rtl_max.map_or(Ok(false), |bound| bound.is_truthy())?;
    if !(min_truthy && max_truthy) {
        return Ok(false);
    }
    let (Some(min), Some(max)) = (custom_rtl_min, custom_rtl_max) else {
        return Ok(false);
    };
    Ok(min.le(char)? && char.le(max)?)
}
