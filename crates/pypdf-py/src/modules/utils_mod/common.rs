//! Constants and small helpers shared by the `pypdf._utils` port.

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyInt, PyString, PyTuple};

/// Logger source used by upstream (`source=__name__` inside `pypdf._utils`).
pub const SOURCE: &str = "pypdf._utils";

/// `pypdf.errors.STREAM_TRUNCATED_PREMATURELY` (a fixed upstream constant).
pub const STREAM_TRUNCATED_PREMATURELY: &str = "Stream has ended unexpectedly";

/// Upstream `WHITESPACES`, in upstream order.
pub const WHITESPACES_BYTES: [u8; 6] = [0x00, b'\t', b'\n', 0x0c, b'\r', b' '];

/// `WHITESPACES` as a tuple of one-byte `bytes` objects.
pub fn whitespaces(py: Python<'_>) -> PyResult<Bound<'_, PyTuple>> {
    PyTuple::new(
        py,
        [
            PyBytes::new(py, &[WHITESPACES_BYTES[0]]),
            PyBytes::new(py, &[WHITESPACES_BYTES[1]]),
            PyBytes::new(py, &[WHITESPACES_BYTES[2]]),
            PyBytes::new(py, &[WHITESPACES_BYTES[3]]),
            PyBytes::new(py, &[WHITESPACES_BYTES[4]]),
            PyBytes::new(py, &[WHITESPACES_BYTES[5]]),
        ],
    )
}

/// Python `int(value)`.
pub fn py_int(py: Python<'_>, value: i64) -> Bound<'_, PyInt> {
    PyInt::new(py, value)
}

/// `len >= limit`, where `limit` is an arbitrary Python object (upstream
/// compares plain Python integers here).
pub fn len_ge_limit(py: Python<'_>, len: usize, limit: &Bound<'_, PyAny>) -> PyResult<bool> {
    py_int(py, len as i64).ge(limit)
}

/// `int >= limit`.
pub fn int_ge_limit(py: Python<'_>, value: i64, limit: &Bound<'_, PyAny>) -> PyResult<bool> {
    py_int(py, value).ge(limit)
}

/// `stream.read(1)`.
pub fn read_one<'py>(stream: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    stream.call_method1("read", (1,))
}

/// `b in WHITESPACES` for one byte.
#[must_use]
pub fn is_whitespace_byte(byte: u8) -> bool {
    matches!(byte, 0x00 | b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

/// Python `bytes.isspace()` for one ASCII byte.
///
/// Note: `u8::is_ascii_whitespace` deliberately excludes vertical tab, while
/// Python's `bytes.isspace()` includes it, hence the explicit table.
#[must_use]
pub fn is_ascii_space_byte(byte: u8) -> bool {
    matches!(byte, 0x09 | 0x0a | 0x0b | 0x0c | 0x0d | 0x20)
}

/// `tok.isspace() or tok in WHITESPACES` (empty tokens are handled by callers).
pub fn is_whitespace_token(py: Python<'_>, tok: &Bound<'_, PyAny>) -> PyResult<bool> {
    if let Ok(bytes) = tok.cast::<PyBytes>() {
        let raw = bytes.as_bytes();
        return match raw.len() {
            0 => Ok(false),
            1 => Ok(is_ascii_space_byte(raw[0]) || is_whitespace_byte(raw[0])),
            _ => {
                if bytes.call_method0("isspace")?.is_truthy()? {
                    Ok(true)
                } else {
                    whitespaces(py)?.contains(tok)
                }
            }
        };
    }
    if tok.call_method0("isspace")?.is_truthy()? {
        return Ok(true);
    }
    whitespaces(py)?.contains(tok)
}

/// `tok in WHITESPACES` (bytes fast path).
pub fn in_whitespaces(py: Python<'_>, tok: &Bound<'_, PyAny>) -> PyResult<bool> {
    if let Ok(bytes) = tok.cast::<PyBytes>() {
        let raw = bytes.as_bytes();
        return Ok(raw.len() == 1 && is_whitespace_byte(raw[0]));
    }
    whitespaces(py)?.contains(tok)
}

/// `"D:" + text` (Python `str.__add__`, so non-strings fail exactly like upstream).
pub fn str_prefix<'py>(
    py: Python<'py>,
    prefix: &str,
    text: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    PyString::new(py, prefix).call_method1("__add__", (text,))
}

/// `text + "0000"`.
pub fn str_suffix<'py>(text: &Bound<'py, PyAny>, suffix: &str) -> PyResult<Bound<'py, PyAny>> {
    text.call_method1("__add__", (PyString::new(text.py(), suffix),))
}
