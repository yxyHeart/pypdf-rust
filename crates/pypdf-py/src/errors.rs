//! Helpers to raise pypdf's Python exception classes from Rust.
//!
//! `pypdf/errors.py` is kept verbatim (type-only module), so every Rust error
//! path constructs the *same* Python exception objects with the *same* message
//! text as upstream.

use pyo3::prelude::*;
use pyo3::types::PyTuple;

/// Raise `pypdf.errors.<class_name>(*args)`.
pub fn raise(py: Python<'_>, class_name: &str, args: Bound<'_, PyTuple>) -> PyErr {
    match class(py, class_name).and_then(|class| class.call1(args)) {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    }
}

/// Raise `pypdf.errors.<class_name>("<message>")`.
pub fn raise_msg(py: Python<'_>, class_name: &str, message: &str) -> PyErr {
    let args = PyTuple::new(py, [message]);
    match args {
        Ok(args) => raise(py, class_name, args),
        Err(error) => error,
    }
}

pub fn parse_error(py: Python<'_>, arg: &Bound<'_, PyAny>) -> PyErr {
    match single_arg(py, "ParseError", arg) {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    }
}

pub fn pdf_read_error(py: Python<'_>, message: &str) -> PyErr {
    raise_msg(py, "PdfReadError", message)
}

pub fn pdf_stream_error(py: Python<'_>, message: &str) -> PyErr {
    raise_msg(py, "PdfStreamError", message)
}

pub fn file_not_decrypted_error(py: Python<'_>) -> PyErr {
    raise_msg(py, "FileNotDecryptedError", "File has not been decrypted")
}

pub fn wrong_password_error(py: Python<'_>) -> PyErr {
    raise_msg(py, "WrongPasswordError", "Wrong password")
}

pub fn dependency_error(py: Python<'_>, message: &str) -> PyErr {
    raise_msg(py, "DependencyError", message)
}

pub fn limit_reached_error(py: Python<'_>, message: &str) -> PyErr {
    raise_msg(py, "LimitReachedError", message)
}

pub fn empty_file_error(py: Python<'_>, message: &str) -> PyErr {
    raise_msg(py, "EmptyFileError", message)
}

fn single_arg<'py>(
    py: Python<'py>,
    class_name: &str,
    arg: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let class = class(py, class_name)?;
    class.call1((arg,))
}

fn class<'py>(py: Python<'py>, class_name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf.errors")?.getattr(class_name)
}
