//! Rust port of `pypdf/_utils.py` (upstream pypdf 7.1.1).
//!
//! Implementation boundary: all computational logic lives in Rust.  The only
//! Python-only piece kept in the shim is `deprecate_positional_arguments`,
//! whose decorator factory has to introspect arbitrary Python signatures with
//! `inspect.signature` and re-wrap with `functools.wraps` in a way PyO3 cannot
//! reproduce byte-for-byte (see `python/pypdf/_utils.py`).
//!
//! Submodules mirror upstream concerns: stream readers, date formatting,
//! deprecation/logging helpers, character classification and the value classes.

#[path = "utils_mod/classes.rs"]
mod classes;
#[path = "utils_mod/common.rs"]
mod common;
#[path = "utils_mod/dates.rs"]
mod dates;
#[path = "utils_mod/deprecation.rs"]
mod deprecation;
#[path = "utils_mod/misc.rs"]
mod misc;
#[path = "utils_mod/streams.rs"]
mod streams;
#[path = "utils_mod/text.rs"]
mod text;

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyTuple};

/// `tuple[...]` generic alias, exactly like the upstream annotations.
fn tuple_alias<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    crate::pyutil::cached_module(py, "builtins")?
        .getattr("tuple")?
        .get_item(PyTuple::new(py, items)?)
}

// @pypdf: pypdf._utils.__author__
const AUTHOR: &str = "Mathieu Fenniak";
// @pypdf: pypdf._utils.__author_email__
const AUTHOR_EMAIL: &str = "biziqe@mathieu.fenniak.net";

// @pypdf: pypdf._utils.TransformationMatrixType
fn transformation_matrix_type(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let float_type = crate::pyutil::cached_module(py, "builtins")?.getattr("float")?;
    let triple = tuple_alias(
        py,
        vec![float_type.clone(), float_type.clone(), float_type.clone()],
    )?;
    Ok(tuple_alias(py, vec![triple.clone(), triple.clone(), triple])?.unbind())
}

// @pypdf: pypdf._utils.CompressedTransformationMatrix
fn compressed_transformation_matrix(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let float_type = crate::pyutil::cached_module(py, "builtins")?.getattr("float")?;
    let items: Vec<Bound<'_, PyAny>> = (0..6).map(|_| float_type.clone()).collect();
    Ok(tuple_alias(py, items)?.unbind())
}

// @pypdf: pypdf._utils.StreamType
fn stream_type(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let typing = py.import("typing")?;
    typing.getattr("IO")?.get_item(typing.getattr("Any")?)
}

// @pypdf: pypdf._utils.BinaryStreamType
fn binary_stream_type(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let typing = py.import("typing")?;
    let bytes_type = crate::pyutil::cached_module(py, "builtins")?.getattr("bytes")?;
    typing.getattr("IO")?.get_item(bytes_type)
}

// @pypdf: pypdf._utils.StrByteType
fn str_byte_type(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let str_type = crate::pyutil::cached_module(py, "builtins")?.getattr("str")?;
    let stream = stream_type(py)?;
    // `str | StreamType` via the `|` operator (falls back to `StreamType.__ror__`).
    Ok(py
        .import("operator")?
        .getattr("or_")?
        .call1((str_type, stream))?
        .unbind())
}

// @pypdf: pypdf._utils._P
fn param_spec(py: Python<'_>) -> PyResult<Py<PyAny>> {
    Ok(py
        .import("typing")?
        .getattr("ParamSpec")?
        .call1(("_P",))?
        .unbind())
}

// @pypdf: pypdf._utils._R
fn type_var(py: Python<'_>) -> PyResult<Py<PyAny>> {
    Ok(py
        .import("typing")?
        .getattr("TypeVar")?
        .call1(("_R",))?
        .unbind())
}

// Two items intentionally live in the Python shim instead of Rust; they are
// pure Python-level plumbing over arbitrary Python callables/descriptors and
// PyO3 mediation changes their observable behaviour:
//
// * `deprecate_positional_arguments` needs `inspect.signature` and
//   `functools.wraps` so the wrapper keeps the original signature and
//   `__wrapped__` metadata exactly.
// * `classproperty` is a descriptor; PyO3's `__get__` slot mediation rejects the
//   upstream one-argument form `descr.__get__(instance)` (it raises
//   `TypeError: __get__(None, None) is invalid` instead of calling
//   `fget(instance)`/`fget(cls)`), so the Python version is byte-for-byte
//   compatible where the Rust one is not.
//
// shim-only: see `python/pypdf/_utils.py` (the definitions are verbatim upstream).
// @pypdf: pypdf._utils.deprecate_positional_arguments
// @pypdf: pypdf._utils.classproperty
// @pypdf: pypdf._utils.classproperty.__init__
// @pypdf: pypdf._utils.classproperty.__get__
// @pypdf: pypdf._utils.classproperty.getter

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = module.py();

    // Constants and typing aliases.
    module.add("__author__", AUTHOR)?;
    module.add("__author_email__", AUTHOR_EMAIL)?;
    module.add("TransformationMatrixType", transformation_matrix_type(py)?)?;
    module.add(
        "CompressedTransformationMatrix",
        compressed_transformation_matrix(py)?,
    )?;
    module.add("StreamType", stream_type(py)?)?;
    module.add("BinaryStreamType", binary_stream_type(py)?)?;
    module.add("StrByteType", str_byte_type(py)?)?;
    module.add("_P", param_spec(py)?)?;
    module.add("_R", type_var(py)?)?;

    // @pypdf: pypdf._utils.WHITESPACES
    module.add("WHITESPACES", common::whitespaces(py)?)?;
    // @pypdf: pypdf._utils.WHITESPACES_AS_BYTES
    module.add(
        "WHITESPACES_AS_BYTES",
        PyBytes::new(py, &common::WHITESPACES_BYTES),
    )?;
    // @pypdf: pypdf._utils.WHITESPACES_AS_REGEXP
    let mut whitespaces_regexp = Vec::with_capacity(common::WHITESPACES_BYTES.len() + 2);
    whitespaces_regexp.push(b'[');
    whitespaces_regexp.extend_from_slice(&common::WHITESPACES_BYTES);
    whitespaces_regexp.push(b']');
    module.add(
        "WHITESPACES_AS_REGEXP",
        PyBytes::new(py, &whitespaces_regexp),
    )?;
    module.add(
        "NEUTRAL_CHARACTER_RANGES",
        text::neutral_character_ranges(py)?,
    )?;
    module.add("RTL_CHARACTER_RANGES", text::rtl_character_ranges(py)?)?;

    // Stream helpers.
    module.add_function(wrap_pyfunction!(streams::read_until_whitespace, module)?)?;
    module.add_function(wrap_pyfunction!(streams::read_non_whitespace, module)?)?;
    module.add_function(wrap_pyfunction!(streams::skip_over_whitespace, module)?)?;
    module.add_function(wrap_pyfunction!(streams::check_if_whitespace_only, module)?)?;
    module.add_function(wrap_pyfunction!(streams::skip_over_comment, module)?)?;
    module.add_function(wrap_pyfunction!(streams::skip_over_comments, module)?)?;
    module.add_function(wrap_pyfunction!(streams::read_until_regex, module)?)?;
    module.add_function(wrap_pyfunction!(streams::read_block_backwards, module)?)?;
    module.add_function(wrap_pyfunction!(streams::read_previous_line, module)?)?;
    module.add_function(wrap_pyfunction!(streams::get_stream_size, module)?)?;
    module.add_function(wrap_pyfunction!(streams::get_stream_data, module)?)?;
    module.add_function(wrap_pyfunction!(streams::mark_location, module)?)?;

    // Dates.
    module.add_function(wrap_pyfunction!(dates::parse_iso8824_date, module)?)?;
    module.add_function(wrap_pyfunction!(dates::format_iso8824_date, module)?)?;

    // Character classification.
    module.add_function(wrap_pyfunction!(text::is_char_neutral, module)?)?;
    module.add_function(wrap_pyfunction!(text::is_char_rtl, module)?)?;

    // Numeric / header helpers.
    module.add_function(wrap_pyfunction!(misc::matrix_multiply, module)?)?;
    module.add_function(wrap_pyfunction!(misc::_get_max_pdf_version_header, module)?)?;
    module.add_function(wrap_pyfunction!(misc::_human_readable_bytes, module)?)?;

    // Deprecation and logging helpers.
    module.add_function(wrap_pyfunction!(deprecation::deprecate, module)?)?;
    module.add_function(wrap_pyfunction!(deprecation::deprecation, module)?)?;
    module.add_function(wrap_pyfunction!(
        deprecation::deprecate_with_replacement,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        deprecation::deprecation_with_replacement,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        deprecation::deprecate_no_replacement,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        deprecation::deprecation_no_replacement,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(deprecation::logger_error, module)?)?;
    module.add_function(wrap_pyfunction!(deprecation::logger_warning, module)?)?;
    module.add_function(wrap_pyfunction!(deprecation::rename_kwargs, module)?)?;

    // Classes.
    module.add_class::<classes::File>()?;
    module.add_class::<classes::Version>()?;
    module.add_class::<classes::TraversalState>()?;

    Ok(())
}
