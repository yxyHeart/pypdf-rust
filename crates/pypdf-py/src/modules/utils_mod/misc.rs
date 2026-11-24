//! Remaining computational helpers from `pypdf/_utils.py`.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyFloat, PyList, PyString, PyTuple};

use pypdf_rs_core::pyfmt::py_float_fixed;

use crate::pyutil;

const PDF_VERSIONS: [&str; 6] = [
    "%PDF-1.3", "%PDF-1.4", "%PDF-1.5", "%PDF-1.6", "%PDF-1.7", "%PDF-2.0",
];

// @pypdf: pypdf._utils._get_max_pdf_version_header
#[pyfunction]
pub(super) fn _get_max_pdf_version_header(
    py: Python<'_>,
    header1: &Bound<'_, PyAny>,
    header2: &Bound<'_, PyAny>,
) -> PyResult<String> {
    let mut indices: Vec<usize> = Vec::new();
    for (index, version) in PDF_VERSIONS.iter().enumerate() {
        if header1.eq(PyString::new(py, version))? {
            indices.push(index);
        }
        if header2.eq(PyString::new(py, version))? {
            indices.push(index);
        }
    }
    if indices.is_empty() {
        return Err(PyValueError::new_err(format!(
            "Neither {} nor {} are proper headers",
            pyutil::py_repr(header1)?,
            pyutil::py_repr(header2)?
        )));
    }
    let maximum = indices.iter().copied().max().unwrap_or(0);
    Ok(PDF_VERSIONS[maximum].to_string())
}

fn format_divided(py: Python<'_>, value: &Bound<'_, PyAny>, divisor: i64) -> PyResult<String> {
    let divisor = py.import("builtins")?.getattr("int")?.call1((divisor,))?;
    let divided = value.call_method1("__truediv__", (divisor,))?;
    let number: f64 = divided.extract()?;
    Ok(py_float_fixed(number, 1))
}

// @pypdf: pypdf._utils._human_readable_bytes
#[pyfunction]
pub(super) fn _human_readable_bytes(py: Python<'_>, bytes: &Bound<'_, PyAny>) -> PyResult<String> {
    human_readable_bytes(py, bytes)
}

/// Shared implementation of `_human_readable_bytes` (also used by `File.__str__`).
pub(super) fn human_readable_bytes(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<String> {
    let builtins = py.import("builtins")?;
    let int_type = builtins.getattr("int")?;
    if value.lt(int_type.call1((1_000,))?)? {
        return Ok(format!("{} Byte", pyutil::py_str(value)?));
    }
    if value.lt(int_type.call1((1_000_000,))?)? {
        return Ok(format!("{} kB", format_divided(py, value, 1_000)?));
    }
    if value.lt(int_type.call1((1_000_000_000,))?)? {
        return Ok(format!("{} MB", format_divided(py, value, 1_000_000)?));
    }
    Ok(format!("{} GB", format_divided(py, value, 1_000_000_000)?))
}

// @pypdf: pypdf._utils.matrix_multiply
#[pyfunction]
pub(super) fn matrix_multiply(
    py: Python<'_>,
    a: &Bound<'_, PyAny>,
    b: &Bound<'_, PyAny>,
) -> PyResult<Py<PyTuple>> {
    let a_rows: Vec<Bound<'_, PyAny>> = a.try_iter()?.collect::<PyResult<_>>()?;
    let b_rows: Vec<Bound<'_, PyAny>> = b.try_iter()?.collect::<PyResult<_>>()?;
    let mut b_matrix: Vec<Vec<Bound<'_, PyAny>>> = Vec::with_capacity(b_rows.len());
    for row in &b_rows {
        b_matrix.push(row.try_iter()?.collect::<PyResult<_>>()?);
    }
    // `zip(*b)` stops at the shortest row of `b`.
    let column_count = b_matrix.iter().map(Vec::len).min().unwrap_or(0);

    let builtins = py.import("builtins")?;
    let float_type = builtins.getattr("float")?;
    let sum_function = builtins.getattr("sum")?;

    let mut result_rows: Vec<Bound<'_, PyAny>> = Vec::with_capacity(a_rows.len());
    for row in &a_rows {
        let mut cells: Vec<Bound<'_, PyAny>> = Vec::with_capacity(column_count);
        for column in 0..column_count {
            // Upstream re-iterates `row` for every column (`zip(row, col)`), which
            // matters for one-shot iterators.
            let row_items: Vec<Bound<'_, PyAny>> = row.try_iter()?.collect::<PyResult<_>>()?;
            let mut products: Vec<Bound<'_, PyAny>> = Vec::new();
            for (index, item) in row_items.iter().enumerate() {
                let Some(column_items) = b_matrix.get(index) else {
                    continue;
                };
                let Some(column_item) = column_items.get(column) else {
                    continue;
                };
                let left: f64 = float_type.call1((item.clone(),))?.extract()?;
                let right: f64 = float_type.call1((column_item.clone(),))?.extract()?;
                products.push(PyFloat::new(py, left * right).into_any());
            }
            // Python's builtin `sum` (not a naive Rust fold): CPython 3.12+
            // applies compensated summation for floats.
            cells.push(sum_function.call1((PyList::new(py, products)?,))?);
        }
        result_rows.push(PyTuple::new(py, cells)?.into_any());
    }
    Ok(PyTuple::new(py, result_rows)?.unbind())
}
