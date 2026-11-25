//! Rust port of `pypdf/_reader.py` (upstream pypdf 7.1.1).
//!
//! # Design: Python class model, Rust logic
//!
//! `PdfReader` is a `PdfDocCommon` subclass whose MRO, `@property`
//! decorators, class attributes and module-level re-exports are observable by
//! the official test suite (and `mock.patch` targets such as
//! `pypdf._reader.read_object` / `pypdf._reader.get_stream_size`), so the class
//! itself lives verbatim in the hand-written shim `python/pypdf/_reader.py` and
//! every method body is a one-line delegation to a `#[pyfunction]` implemented
//! here.
//!
//! # Fidelity notes
//!
//! * Module-level names referenced by upstream code (`read_object`,
//!   `get_stream_size`, `read_non_whitespace`, `convert_to_int`, ...) are
//!   resolved through the `pypdf._reader` module object at call time, exactly
//!   like upstream's globals.  That keeps `mock.patch("pypdf._reader.<name>")`
//!   working and preserves the `caplog` texts.
//! * The byte stream protocol (`read`/`seek`/`tell`) is used as-is; the stream
//!   is never re-buffered in Rust.
//! * Iteration order that can influence output (warnings order, trailer merge
//!   order) follows Python `dict`/`list` order; the only Rust set is a
//!   membership-only set of visited xref offsets.

use std::collections::HashSet;

use pyo3::exceptions::{
    PyIndexError, PyMemoryError, PyRecursionError, PyUnicodeDecodeError, PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::{
    PyAny, PyBool, PyBytes, PyCFunction, PyDict, PyInt, PyList, PyModule, PySet, PyString, PyTuple,
};

use crate::errors;
use crate::pyutil;

/// Upstream module name; used as `logger_warning(..., source=__name__)`.
const SOURCE: &str = "pypdf._reader";

// @pypdf: pypdf._reader.PdfReader._MAX_STARTXREF_RECOVERY_LINES
/// `PdfReader._MAX_STARTXREF_RECOVERY_LINES` (upstream class attribute).
const MAX_STARTXREF_RECOVERY_LINES: i128 = 1000;

/// `pypdf._utils.WHITESPACES_AS_BYTES` (`b"".join(WHITESPACES)`), upstream order.
const WHITESPACES_AS_BYTES: [u8; 6] = [0x00, b'\t', b'\n', 0x0c, b'\r', b' '];

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    crate::pyutil::cached_module(py, "builtins")
}

fn shim<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    pyutil::cached_module(py, "pypdf._reader")
}

/// Fetch a module-level name from the `pypdf._reader` shim (upstream globals).
fn shim_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim(py)?.getattr(name)
}

fn generic_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    pyutil::cached_module(py, "pypdf.generic")?.getattr(name)
}

fn utils_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    pyutil::cached_module(py, "pypdf._utils")?.getattr(name)
}

fn int_class<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("int")
}

fn py_none<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    py.None().into_bound(py)
}

fn py_int<'py>(py: Python<'py>, value: i128) -> Bound<'py, PyAny> {
    PyInt::new(py, value).into_any()
}

fn py_bool<'py>(py: Python<'py>, value: bool) -> Bound<'py, PyAny> {
    PyBool::new(py, value).to_owned().into_any()
}

fn py_bytes<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, data).into_any()
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

fn list_empty<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    PyList::empty(py).into_any()
}

fn dictionary_object<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    generic_attr(py, "DictionaryObject")?.call0()
}

fn null_object<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    generic_attr(py, "NullObject")?.call0()
}

fn bytes_of(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    obj.extract::<Vec<u8>>()
}

fn int_of(obj: &Bound<'_, PyAny>) -> PyResult<i128> {
    obj.extract::<i128>()
}

fn str_of(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    pyutil::py_str(obj)
}

fn repr_of(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    pyutil::py_repr(obj)
}

fn is_instance(obj: &Bound<'_, PyAny>, class: &Bound<'_, PyAny>) -> PyResult<bool> {
    obj.is_instance(class)
}

fn is_instance_of_module(
    py: Python<'_>,
    obj: &Bound<'_, PyAny>,
    module: &str,
    name: &str,
) -> PyResult<bool> {
    obj.is_instance(&py.import(module)?.getattr(name)?)
}

fn warn(py: Python<'_>, message: &str) -> PyResult<()> {
    pyutil::logger_warning(py, SOURCE, message, None)
}

fn warn_values<'py>(
    py: Python<'py>,
    message: &str,
    values: Vec<(&str, Bound<'py, PyAny>)>,
) -> PyResult<()> {
    let dict = PyDict::new(py);
    for (key, value) in values {
        dict.set_item(key, value)?;
    }
    pyutil::logger_warning(py, SOURCE, message, Some(&dict))
}

/// `message % values` using Python's `%` formatting.
fn percent_format(py: Python<'_>, message: &str, values: &Bound<'_, PyAny>) -> PyResult<String> {
    py_str(py, message)
        .call_method1("__mod__", (values,))
        .and_then(|value| value.extract::<String>())
}

/// `assert <condition>` / `assert <condition>, <message>`.
fn assertion(py: Python<'_>, message: Option<&str>) -> PyErr {
    let class = match builtins(py).and_then(|module| module.getattr("AssertionError")) {
        Ok(class) => class,
        Err(error) => return error,
    };
    let result = match message {
        Some(message) => class.call1((message,)),
        None => class.call0(),
    };
    match result {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    }
}

fn assert_that(py: Python<'_>, condition: bool, message: Option<&str>) -> PyResult<()> {
    if condition {
        Ok(())
    } else {
        Err(assertion(py, message))
    }
}

fn is_python_exception(py: Python<'_>, error: &PyErr) -> bool {
    match builtins(py).and_then(|module| module.getattr("Exception")) {
        Ok(class) => error
            .value(py)
            .is_instance(&class)
            .unwrap_or(false),
        Err(_) => false,
    }
}

fn raise_with_cause(py: Python<'_>, error: PyErr, cause: PyErr) -> PyErr {
    error.set_cause(py, Some(cause));
    error
}

fn stream_tell(stream: &Bound<'_, PyAny>) -> PyResult<i128> {
    stream.call_method0("tell")?.extract::<i128>()
}

fn stream_seek(py: Python<'_>, stream: &Bound<'_, PyAny>, pos: i128, whence: i32) -> PyResult<()> {
    stream.call_method1("seek", (py_int(py, pos), whence))?;
    Ok(())
}

fn bytes_io<'py>(py: Python<'py>, data: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    py.import("io")?.getattr("BytesIO")?.call1((data,))
}

fn bytes_isdigit(data: &[u8]) -> bool {
    !data.is_empty() && data.iter().all(u8::is_ascii_digit)
}

fn is_whitespace(byte: u8) -> bool {
    WHITESPACES_AS_BYTES.contains(&byte)
}

fn find_subslice(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() || start > haystack.len() - needle.len() {
        return None;
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| position + start)
}

fn lstrip_bytes<'a>(data: &'a [u8], chars: &[u8]) -> &'a [u8] {
    let mut start = 0;
    while start < data.len() && chars.contains(&data[start]) {
        start += 1;
    }
    &data[start..]
}

/// Python `bytes.strip()` (ASCII whitespace, no arguments).  Python's bytes
/// whitespace set also includes `\x0b` (vertical tab), which Rust's
/// `is_ascii_whitespace` omits.
fn is_bytes_strip_whitespace(byte: u8) -> bool {
    byte.is_ascii_whitespace() || byte == 0x0b
}

fn strip_bytes(data: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = data.len();
    while start < end && is_bytes_strip_whitespace(data[start]) {
        start += 1;
    }
    while end > start && is_bytes_strip_whitespace(data[end - 1]) {
        end -= 1;
    }
    &data[start..end]
}

/// Call a module-level `pypdf._reader` function (upstream global name lookup).
fn call_shim<'py>(
    py: Python<'py>,
    name: &str,
    args: Vec<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let function = shim_attr(py, name)?;
    function.call1(PyTuple::new(py, args)?)
}

/// `a + b` where `b` is an owned value.
fn py_plus<'py>(
    a: &Bound<'py, PyAny>,
    b: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    pyutil::py_add(a, &b)
}

/// `a - b` using Python's operator protocol (`pyutil::py_add` covers `+`).
fn py_sub<'py>(a: &Bound<'py, PyAny>, b: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    a.call_method1("__sub__", (b,))
}

/// `bool(self.strict)` — Rust-side helpers take a real `bool` where upstream
/// passes the raw attribute through (truthiness is equivalent for the
/// supported values and keeps non-`bool` truthy values working).
fn strict_flag<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    Ok(py_bool(py, slf.getattr("strict")?.is_truthy()?))
}

fn py_slice_of<'py>(
    py: Python<'py>,
    start: Option<Bound<'py, PyAny>>,
    stop: Option<Bound<'py, PyAny>>,
    step: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    pyutil::py_slice(
        py,
        start.as_ref(),
        stop.as_ref(),
        step.as_ref(),
    )
}

// ---------------------------------------------------------------------------
// `PdfReader.__init__` and lifecycle
// ---------------------------------------------------------------------------

// @pypdf: pypdf._reader.PdfReader
// @pypdf: pypdf._reader.PdfReader.xref
// @pypdf: pypdf._reader.PdfReader.trailer
// @pypdf: pypdf._reader.PdfReader.__init__
#[pyfunction]
#[pyo3(signature = (slf, stream, strict, password=None, root_object_recovery_limit=None, xref_stream_maximum_entries=None))]
fn reader_init<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
    strict: &Bound<'py, PyAny>,
    password: Option<&Bound<'py, PyAny>>,
    root_object_recovery_limit: Option<&Bound<'py, PyAny>>,
    xref_stream_maximum_entries: Option<&Bound<'py, PyAny>>,
) -> PyResult<()> {
    slf.setattr("strict", strict.clone())?;
    slf.setattr("flattened_pages", py_none(py))?;

    //: Storage of parsed PDF objects.
    slf.setattr("resolved_objects", PyDict::new(py))?;

    slf.setattr("_startxref", py_int(py, 0))?;
    slf.setattr("xref_index", py_int(py, 0))?;
    slf.setattr("xref", PyDict::new(py))?;
    slf.setattr("xref_free_entry", PyDict::new(py))?;
    slf.setattr("xref_objStm", PyDict::new(py))?;
    slf.setattr("trailer", dictionary_object(py)?)?;

    // Security parameters.
    let limit = match root_object_recovery_limit {
        Some(value) if is_instance(value, &int_class(py)?)? => value.clone(),
        _ => py.import("sys")?.getattr("maxsize")?,
    };
    slf.setattr("_root_object_recovery_limit", limit)?;
    let maximum_entries = match xref_stream_maximum_entries {
        Some(value) => value.clone(),
        None => py_int(py, 2_000_000),
    };
    slf.setattr("_xref_stream_maximum_entries", maximum_entries)?;
    slf.setattr("_xref_stream_entry_count", py_int(py, 0))?;
    shim_attr(py, "apply_legacy_configuration")?.call0()?;

    // Map page indirect_reference number to page number
    slf.setattr("_page_id2num", py_none(py))?;
    slf.setattr("_validated_root", py_none(py))?;

    slf.call_method1("_initialize_stream", (stream,))?;
    slf.setattr("_known_objects", PySet::empty(py)?)?;

    slf.setattr("_override_encryption", false)?;
    slf.setattr("_encryption", py_none(py))?;
    if slf.getattr("is_encrypted")?.is_truthy()? {
        let password = match password {
            Some(password) => password.clone(),
            None => py_none(py),
        };
        slf.call_method1("_handle_encryption", (password,))?;
    } else if password.is_some() {
        return Err(errors::pdf_read_error(py, "Not an encrypted file"));
    }

    slf.setattr("_named_destinations_cache", py_none(py))?;
    slf.setattr("_object_stream_resolution_stack", list_empty(py))?;
    Ok(())
}

// @pypdf: pypdf._reader.PdfReader._initialize_stream
#[pyfunction]
fn reader_initialize_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    if stream.hasattr("mode")? {
        let mode = stream.getattr("mode")?;
        if !mode.contains("b")? {
            warn(
                py,
                "PdfReader stream/file object is not in binary mode. It may not be read correctly.",
            )?;
        }
    }
    slf.setattr("_stream_opened", false)?;
    let str_class = builtins(py)?.getattr("str")?;
    let path_class = py.import("pathlib")?.getattr("Path")?;
    let classes = PyTuple::new(py, [str_class, path_class])?;
    let stream = if stream.is_instance(classes.as_any())? {
        let file = builtins(py)?.call_method1("open", (stream, py_str(py, "rb")))?;
        let data = file.call_method0("read");
        let _ = file.call_method0("close");
        let data = data?;
        slf.setattr("_stream_opened", true)?;
        bytes_io(py, &data)?
    } else {
        stream.clone()
    };
    slf.call_method1("read", (&stream,))?;
    slf.setattr("stream", stream)?;
    Ok(())
}

// @pypdf: pypdf._reader.PdfReader._handle_encryption
#[pyfunction]
fn reader_handle_encryption<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    password: &Bound<'py, PyAny>,
) -> PyResult<()> {
    slf.setattr("_override_encryption", true)?;
    // Some documents may not have a /ID, use two empty byte strings instead.
    let trailer = slf.getattr("trailer")?;
    let id_entry = trailer
        .call_method1("get", (py_str(py, "/ID"), null_object(py)?))?
        .call_method0("get_object")?;
    let mut id1_entry = py_bytes(py, b"");
    let array_class = generic_attr(py, "ArrayObject")?;
    if is_instance(&id_entry, &array_class)? && id_entry.is_truthy()? {
        let first_id_entry = id_entry.get_item(0)?.call_method0("get_object")?;
        let byte_string_class = generic_attr(py, "ByteStringObject")?;
        let text_string_class = generic_attr(py, "TextStringObject")?;
        let classes = PyTuple::new(py, [byte_string_class, text_string_class])?;
        if first_id_entry.is_instance(classes.as_any())? {
            id1_entry = first_id_entry.getattr("original_bytes")?;
        } else {
            warn(
                py,
                "Ignoring non-string first /ID entry of the trailer",
            )?;
        }
    } else if !is_instance(&id_entry, &generic_attr(py, "NullObject")?)? {
        warn(py, "Ignoring malformed /ID entry of the trailer")?;
    }
    let encrypt_entry = trailer.get_item(py_str(py, "/Encrypt"))?;
    if !is_instance(&encrypt_entry, &generic_attr(py, "DictionaryObject")?)? {
        return Err(errors::pdf_read_error(
            py,
            "/Encrypt entry of the trailer is not a dictionary",
        ));
    }
    let encryption_class = py.import("pypdf._encryption")?.getattr("Encryption")?;
    let encryption = encryption_class
        .getattr("read")?
        .call1((encrypt_entry, id1_entry))?;
    slf.setattr("_encryption", encryption.clone())?;

    // try empty password if no password provided
    let pwd = if password.is_none() {
        py_bytes(py, b"")
    } else {
        password.clone()
    };
    let kwargs = PyDict::new(py);
    kwargs.set_item("strict", strict_flag(py, slf)?)?;
    let password_type = encryption.call_method("verify", (pwd,), Some(&kwargs))?;
    let password_type_class = py.import("pypdf._encryption")?.getattr("PasswordType")?;
    let not_decrypted = password_type_class.getattr("NOT_DECRYPTED")?;
    if password_type.eq(&not_decrypted)? && !password.is_none() {
        return Err(errors::wrong_password_error(py));
    }
    slf.setattr("_override_encryption", false)?;
    Ok(())
}

// @pypdf: pypdf._reader.PdfReader.__enter__
#[pyfunction]
fn reader_enter<'py>(slf: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    Ok(slf.clone())
}

// @pypdf: pypdf._reader.PdfReader.__exit__
#[pyfunction]
fn reader_exit(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    _exc_type: &Bound<'_, PyAny>,
    _exc_val: &Bound<'_, PyAny>,
    _exc_tb: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let _ = py;
    slf.call_method0("close")?;
    Ok(())
}

// @pypdf: pypdf._reader.PdfReader.close
#[pyfunction]
fn reader_close(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<()> {
    if slf.getattr("_stream_opened")?.is_truthy()? {
        slf.getattr("stream")?.call_method0("close")?;
    }
    slf.setattr("flattened_pages", list_empty(py))?;
    slf.setattr("resolved_objects", PyDict::new(py))?;
    slf.setattr("trailer", dictionary_object(py)?)?;
    slf.setattr("xref", PyDict::new(py))?;
    slf.setattr("xref_free_entry", PyDict::new(py))?;
    slf.setattr("xref_objStm", PyDict::new(py))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

// @pypdf: pypdf._reader.PdfReader.root_object
#[pyfunction]
fn reader_root_object<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let validated = slf.getattr("_validated_root")?;
    if validated.is_truthy()? {
        return Ok(validated);
    }
    let trailer = slf.getattr("trailer")?;
    let root = trailer
        .call_method1("get", (py_str(py, "/Root"), null_object(py)?))?
        .call_method0("get_object")?;
    let dictionary_class = generic_attr(py, "DictionaryObject")?;
    let root_dictionary = if is_instance(&root, &dictionary_class)? {
        root.clone()
    } else {
        // The catalog has to be a dictionary. Anything else cannot be used,
        // thus treat it like an empty one and attempt the recovery below.
        dictionary_class.call0()?
    };
    let is_null = shim_attr(py, "is_null_or_none")?.call1((&root,))?;
    if is_null.is_truthy()? {
        warn(py, r#"Cannot find "/Root" key in trailer"#)?;
    } else {
        let root_type = root_dictionary.call_method1("get", (py_str(py, "/Type"),))?;
        if root_type.eq(py_str(py, "/Catalog"))? {
            slf.setattr("_validated_root", root_dictionary.clone())?;
        } else {
            warn(py, "Invalid Root object in trailer")?;
        }
    }
    if slf.getattr("_validated_root")?.is_none() {
        warn(py, r#"Searching object with "/Catalog" key"#)?;
        let number_of_objects = trailer.call_method1("get", (py_str(py, "/Size"), py_int(py, 0)))?;
        let number_of_objects = if is_instance(&number_of_objects, &int_class(py)?)? {
            number_of_objects.extract::<i128>().unwrap_or(i128::MAX)
        } else {
            // the trailer may be corrupted and carry a non-integer /Size;
            // treat it like a missing one and fail with the regular
            // PdfReadError for an unrecoverable root
            0
        };
        let limit = slf.getattr("_root_object_recovery_limit")?;
        let mut index: i128 = 0;
        while index < number_of_objects {
            if py_int(py, index).ge(&limit)? {
                return Err(errors::limit_reached_error(
                    py,
                    "Maximum Root object recovery limit reached.",
                ));
            }
            let obj = match slf.call_method1("get_object", (py_int(py, index + 1),)) {
                Ok(value) => value,
                Err(error) => {
                    if error.is_instance_of::<PyMemoryError>(py) {
                        return Err(error);
                    }
                    if is_python_exception(py, &error) {
                        py_none(py)
                    } else {
                        return Err(error);
                    }
                }
            };
            index += 1;
            if !is_instance(&obj, &dictionary_class)? {
                continue;
            }
            let object_type = obj.call_method1("get", (py_str(py, "/Type"),))?;
            if !object_type.eq(py_str(py, "/Catalog"))? {
                continue;
            }
            let reference = obj.getattr("indirect_reference")?;
            slf.setattr("_validated_root", obj.clone())?;
            warn_values(
                py,
                "Root found at %(obj_reference)r",
                vec![("obj_reference", reference)],
            )?;
            break;
        }
    }
    let validated = slf.getattr("_validated_root")?;
    if validated.is_none() {
        if root_dictionary.contains(py_str(py, "/Pages"))? {
            let reference = root_dictionary.getattr("indirect_reference")?;
            warn_values(
                py,
                "Possible root found at %(root_ref)r, but missing /Catalog key",
                vec![("root_ref", reference)],
            )?;
            slf.setattr("_validated_root", root_dictionary.clone())?;
        } else {
            return Err(errors::pdf_read_error(py, "Cannot find Root object in pdf"));
        }
    }
    slf.getattr("_validated_root")
}

// @pypdf: pypdf._reader.PdfReader._info
#[pyfunction]
fn reader_info<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let info = slf
        .getattr("trailer")?
        .call_method1("get", (py_str(py, "/Info"), py_none(py)))?;
    let is_null = shim_attr(py, "is_null_or_none")?.call1((&info,))?;
    if is_null.is_truthy()? {
        return Ok(py_none(py));
    }
    let info = info.call_method0("get_object")?;
    if !is_instance(&info, &generic_attr(py, "DictionaryObject")?)? {
        return Err(errors::pdf_read_error(
            py,
            "Trailer not found or does not point to a document information dictionary",
        ));
    }
    Ok(info)
}

// @pypdf: pypdf._reader.PdfReader._ID
#[pyfunction]
fn reader_id<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let file_identifiers = slf
        .getattr("trailer")?
        .call_method1("get", (py_str(py, "/ID"), py_none(py)))?;
    let is_null = shim_attr(py, "is_null_or_none")?.call1((&file_identifiers,))?;
    if is_null.is_truthy()? {
        return Ok(py_none(py));
    }
    file_identifiers.call_method0("get_object")
}

// @pypdf: pypdf._reader.PdfReader.pdf_header
#[pyfunction]
fn reader_pdf_header(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<String> {
    let stream = slf.getattr("stream")?;
    let loc = stream.call_method0("tell")?;
    stream.call_method1("seek", (0i64, 0i64))?;
    let data = stream.call_method1("read", (8i64,))?;
    let pdf_file_version = data
        .call_method1("decode", (py_str(py, "utf-8"), py_str(py, "backslashreplace")))?
        .extract::<String>()?;
    stream.call_method1("seek", (loc, 0i64))?;
    Ok(pdf_file_version)
}

// @pypdf: pypdf._reader.PdfReader.xmp_metadata
#[pyfunction]
fn reader_xmp_metadata<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let encryption = slf.getattr("_encryption")?;
    let override_encryption = if encryption.is_none() {
        false
    } else {
        !encryption.getattr("EncryptMetadata")?.is_truthy()?
    };
    slf.setattr("_override_encryption", override_encryption)?;
    let result = slf
        .getattr("root_object")?
        .getattr("xmp_metadata");
    slf.setattr("_override_encryption", false)?;
    result
}

// ---------------------------------------------------------------------------
// Object resolution
// ---------------------------------------------------------------------------

// @pypdf: pypdf._reader.PdfReader._get_page_number_by_indirect
#[pyfunction]
fn reader_get_page_number_by_indirect<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    indirect_reference: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let page_id2num = slf.getattr("_page_id2num")?;
    if page_id2num.is_none() {
        let pages = slf.getattr("pages")?;
        let mapping = PyDict::new(py);
        for (index, page) in pages.try_iter()?.enumerate() {
            let page = page?;
            let idnum = page.getattr("indirect_reference")?.getattr("idnum")?;
            mapping.set_item(idnum, py_int(py, index as i128))?;
        }
        slf.setattr("_page_id2num", mapping)?;
    }

    let is_null = shim_attr(py, "is_null_or_none")?.call1((indirect_reference,))?;
    if is_null.is_truthy()? {
        return Ok(py_none(py));
    }
    let int_class = int_class(py)?;
    let indirect_class = generic_attr(py, "IndirectObject")?;
    let is_int = is_instance(indirect_reference, &int_class)?;
    let is_indirect = is_instance(indirect_reference, &indirect_class)?;
    assert_that(py, is_int || is_indirect, Some("mypy"))?;
    let idnum = if is_int {
        indirect_reference.clone()
    } else {
        indirect_reference.getattr("idnum")?
    };
    let page_id2num = slf.getattr("_page_id2num")?;
    assert_that(py, !page_id2num.is_none(), Some("hint for mypy"))?;
    page_id2num.call_method1("get", (idnum, py_none(py)))
}

/// Shared encryption hook: upstream decrypts in both `get_object` branches.
fn decrypt_retval<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    retval: Bound<'py, PyAny>,
    idnum: &Bound<'py, PyAny>,
    generation: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // override encryption is used for the /Encrypt dictionary
    if slf.getattr("_override_encryption")?.is_truthy()? {
        return Ok(retval);
    }
    let encryption = slf.getattr("_encryption")?;
    if encryption.is_none() {
        return Ok(retval);
    }
    // if we don't have the encryption key:
    if !encryption.call_method0("is_decrypted")?.is_truthy()? {
        return Err(errors::file_not_decrypted_error(py));
    }
    // otherwise, decrypt here...
    let kwargs = PyDict::new(py);
    kwargs.set_item("strict", strict_flag(py, slf)?)?;
    encryption.call_method("decrypt_object", (retval, idnum, generation), Some(&kwargs))
}

// @pypdf: pypdf._reader.PdfReader._get_object_from_stream
#[pyfunction]
fn reader_get_object_from_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    indirect_reference: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let obj_id = indirect_reference.getattr("idnum")?;
    let stack = slf.getattr("_object_stream_resolution_stack")?;
    if stack.contains(&obj_id)? {
        let start = stack.call_method1("index", (&obj_id,))?;
        let slice = py_slice_of(py, Some(start), None, None)?;
        let cycle = stack.call_method1("__getitem__", (slice,))?;
        let mut parts: Vec<String> = Vec::new();
        for item in cycle.try_iter()? {
            parts.push(str_of(&item?)?);
        }
        parts.push(str_of(&obj_id)?);
        return Err(errors::pdf_read_error(
            py,
            &format!(
                "Circular object-stream reference detected: {}",
                parts.join(" -> ")
            ),
        ));
    }
    stack.call_method1("append", (&obj_id,))?;

    let body = (|| -> PyResult<Bound<'py, PyAny>> {
        let xref_obj_stm = slf.getattr("xref_objStm")?;
        let stm_entry = xref_obj_stm.get_item(&obj_id)?;
        let stmnum = stm_entry.get_item(0)?;
        let indirect_class = generic_attr(py, "IndirectObject")?;
        let indirect = indirect_class.call1((&stmnum, py_int(py, 0), slf))?;
        let obj_stm = indirect.call_method0("get_object")?;
        // This is an xref to a stream, so its type better be a stream
        let object_type = obj_stm.get_item(py_str(py, "/Type"))?;
        assert_that(py, object_type.eq(py_str(py, "/ObjStm"))?, None)?;
        // Parse ALL objects in this stream in one pass and cache them.
        let stream_data = bytes_io(py, &obj_stm.call_method0("get_data")?)?;
        let n_value = obj_stm.call_method1("get", (py_str(py, "/N"),))?;
        let first_value = obj_stm.call_method1("get", (py_str(py, "/First"),))?;
        let number_class = generic_attr(py, "NumberObject")?;
        if !is_instance(&n_value, &number_class)? || !is_instance(&first_value, &number_class)? {
            return Err(errors::pdf_read_error(
                py,
                &format!(
                    "Invalid /ObjStm header for object {}: /N or /First missing or not a number",
                    str_of(&stmnum)?
                ),
            ));
        }
        let int = int_class(py)?;
        let n_int = int.call1((&n_value,))?;
        let first_int = int.call1((&first_value,))?;
        let stream_length = shim_attr(py, "get_stream_size")?.call1((&stream_data,))?;

        let n_negative = n_int.lt(0i64)?;
        let first_ok = first_int.ge(0i64)? && first_int.le(&stream_length)?;
        if n_negative || !first_ok {
            // A negative /N, or a /First outside the stream, cannot describe a
            // valid object stream; treating it as empty avoids leaking a
            // ValueError/OverflowError from a bad seek.
            if slf.getattr("strict")?.is_truthy()? {
                return Err(errors::pdf_read_error(
                    py,
                    &format!(
                        "Invalid /N ({}) or /First ({}) in object stream {}.",
                        str_of(&n_int)?,
                        str_of(&first_int)?,
                        str_of(&stmnum)?
                    ),
                ));
            }
            warn_values(
                py,
                "Invalid /N (%(n)d) or /First (%(first)d) in object stream \
                 %(stmnum)d; ignoring the stream.",
                vec![("n", n_int), ("first", first_int), ("stmnum", stmnum.clone())],
            )?;
            return null_object(py);
        }

        // ObjStm header format: "objnum offset objnum offset ..."
        let max_n = stream_length.call_method1("__floordiv__", (py_int(py, 3),))?;
        stream_data.call_method1("seek", (0i64, 0i64))?;
        let mut n_int = n_int;
        if n_int.gt(&max_n)? {
            if slf.getattr("strict")?.is_truthy()? {
                return Err(errors::limit_reached_error(
                    py,
                    &format!(
                        "Value /N {} for object {} exceeds maximum allowed value {}.",
                        str_of(&n_int)?,
                        str_of(&stmnum)?,
                        str_of(&max_n)?
                    ),
                ));
            }
            warn_values(
                py,
                "Value /N %(n)d for object %(stmnum)d exceeds maximum allowed value %(max_n)d. \
                 Limiting to %(max_n)d.",
                vec![
                    ("n", n_int.clone()),
                    ("stmnum", stmnum.clone()),
                    ("max_n", max_n.clone()),
                ],
            )?;
            n_int = max_n;
        }

        let n_count: i64 = n_int.extract().unwrap_or(0);
        let indirect_class = generic_attr(py, "IndirectObject")?;
        let max_part = indirect_class.getattr("_MAXIMUM_PART_LENGTH")?;

        let read_non_whitespace = utils_attr(py, "read_non_whitespace")?;
        let max_bytes_kwargs = PyDict::new(py);
        max_bytes_kwargs.set_item("max_bytes", &max_part)?;
        max_bytes_kwargs.set_item("strict", strict_flag(py, slf)?)?;

        // Phase 1: Read the index (objnum, offset) pairs from the header.
        let mut obj_index: Vec<(Bound<'py, PyAny>, i128)> = Vec::new();
        let mut last_offset: i128 = -1;
        for _i in 0..n_count {
            read_non_whitespace.call1((&stream_data,))?;
            stream_data.call_method1("seek", (-1i64, 1i64))?;
            let objnum = number_class
                .getattr("read_from_stream")?
                .call1((&stream_data,))?;
            read_non_whitespace.call1((&stream_data,))?;
            stream_data.call_method1("seek", (-1i64, 1i64))?;
            let offset = number_class
                .getattr("read_from_stream")?
                .call1((&stream_data,))?;
            read_non_whitespace.call1((&stream_data,))?;
            stream_data.call_method1("seek", (-1i64, 1i64))?;
            let invalid = objnum.lt(0i64)?
                || offset.lt(0i64)?
                || pyutil::py_add(&first_int, &offset)?.gt(&stream_length)?;
            if invalid {
                // An object number or target position outside the stream
                // cannot be valid; skipping the entry avoids leaking a
                // ValueError/OverflowError from a bad seek while keeping
                // the other entries.
                if slf.getattr("strict")?.is_truthy()? {
                    return Err(errors::pdf_read_error(
                        py,
                        &format!(
                            "Invalid object number ({}) or offset ({}) in object stream {}.",
                            str_of(&objnum)?,
                            str_of(&offset)?,
                            str_of(&stmnum)?
                        ),
                    ));
                }
                warn_values(
                    py,
                    "Invalid object number (%(objnum)d) or offset (%(offset)d) in object \
                     stream %(stmnum)d; skipping the entry.",
                    vec![
                        ("objnum", objnum),
                        ("offset", offset),
                        ("stmnum", stmnum.clone()),
                    ],
                )?;
                continue;
            }
            let int_offset = int_of(&offset)?;
            if int_offset <= last_offset {
                return Err(errors::pdf_read_error(
                    py,
                    &format!(
                        "Invalid offset {} in object stream {}.",
                        str_of(&offset)?,
                        str_of(&stmnum)?
                    ),
                ));
            }
            last_offset = int_offset;
            obj_index.push((objnum, int_offset));
        }

        // Phase 2: Parse each object and cache it.
        let mut target_obj = null_object(py)?;
        let mut found = false;
        for (i, (obj_num, obj_offset)) in obj_index.iter().enumerate() {
            // Skip objects already in the cache.
            let cached = slf.call_method1("cache_get_indirect_object", (py_int(py, 0), obj_num))?;
            if !cached.is_none() {
                if obj_num.eq(&obj_id)? {
                    target_obj = cached;
                    found = true;
                }
                continue;
            }
            let position = py_plus(&first_int, py_int(py, *obj_offset))?;
            stream_data.call_method1("seek", (position, 0i64))?;
            // To cope with case where the 'pointer' is on a white space
            read_non_whitespace.call1((&stream_data,))?;
            stream_data.call_method1("seek", (-1i64, 1i64))?;
            let read = call_shim(py, "read_object", vec![stream_data.clone(), slf.clone()]);
            let obj = match read {
                Ok(value) => value,
                Err(error) => {
                    if is_instance_of_module(py, error.value(py).as_any(), "pypdf.errors", "PdfStreamError")?
                    {
                        // Stream object cannot be read. Normally, a critical error, but
                        // Adobe Reader doesn't complain, so continue (in strict mode?)
                        warn_values(
                            py,
                            "Invalid stream (index %(index)d) within object %(obj_num)d 0: %(exc)s",
                            vec![
                                ("index", py_int(py, i as i128)),
                                ("obj_num", obj_num.clone()),
                                ("exc", error.value(py).clone().into_any()),
                            ],
                        )?;
                        if slf.getattr("strict")?.is_truthy()? {
                            let message = format!(
                                "Cannot read object stream: {}",
                                str_of(error.value(py).as_any())?
                            );
                            let new_error = errors::pdf_read_error(py, &message);
                            return Err(raise_with_cause(py, new_error, error));
                        }
                        null_object(py)?
                    } else {
                        return Err(error);
                    }
                }
            };
            // Only cache if this stream is the authoritative source for the object.
            let none_pair = py_tuple(py, vec![py_none(py), py_none(py)])?;
            let authoritative = xref_obj_stm.call_method1("get", (obj_num, none_pair))?;
            let authoritative_stm = authoritative.get_item(0)?;
            if authoritative_stm.eq(&stmnum)? {
                slf.call_method1(
                    "cache_indirect_object",
                    (py_int(py, 0), obj_num, &obj),
                )?;
            }
            if obj_num.eq(&obj_id)? {
                target_obj = obj;
                found = true;
            }
        }
        if !found && slf.getattr("strict")?.is_truthy()? {
            return Err(errors::pdf_read_error(
                py,
                "This is a fatal error in strict mode.",
            ));
        }
        Ok(target_obj)
    })();

    let _ = stack.call_method0("pop");
    body
}

// @pypdf: pypdf._reader.PdfReader.get_object
#[pyfunction]
fn reader_get_object<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    indirect_reference: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let int_class = int_class(py)?;
    let indirect_class = generic_attr(py, "IndirectObject")?;
    let indirect_reference = if is_instance(indirect_reference, &int_class)? {
        indirect_class.call1((indirect_reference, py_int(py, 0), slf))?
    } else {
        indirect_reference.clone()
    };

    let generation = indirect_reference.getattr("generation")?;
    let idnum = indirect_reference.getattr("idnum")?;

    let retval = slf.call_method1("cache_get_indirect_object", (&generation, &idnum))?;
    let key = py_tuple(py, vec![generation.clone(), idnum.clone()])?;
    if slf.getattr("resolved_objects")?.contains(&key)? {
        return Ok(retval);
    }

    let strict = slf.getattr("strict")?.is_truthy()?;
    let generation_is_zero = generation.eq(0i64)?;
    let xref_obj_stm = slf.getattr("xref_objStm")?;
    let in_object_stream = generation_is_zero && xref_obj_stm.contains(&idnum)?;

    let retval = if in_object_stream {
        slf.call_method1("_get_object_from_stream", (&indirect_reference,))?
    } else {
        let xref = slf.getattr("xref")?;
        let generation_in_xref = xref.contains(&generation)?;
        let idnum_in_generation = if generation_in_xref {
            xref.get_item(&generation)?.contains(&idnum)?
        } else {
            false
        };

        if generation_in_xref && idnum_in_generation {
            let free_entry = slf.getattr("xref_free_entry")?;
            let empty = PyDict::new(py);
            let generation_free_entry = free_entry.call_method1("get", (&generation, empty))?;
            let is_free = generation_free_entry.call_method1("get", (&idnum, false))?;
            if is_free.is_truthy()? {
                return null_object(py);
            }
            let start = xref.get_item(&generation)?.get_item(&idnum)?;
            let stream = slf.getattr("stream")?;
            stream.call_method1("seek", (&start, 0i64))?;

            let mut actual_idnum = idnum.clone();
            let mut actual_generation = generation.clone();
            let mut header_error: Option<PyErr> = None;
            match slf.call_method1("read_object_header", (&stream,)) {
                Ok(header) => {
                    let header_idnum = header.get_item(0)?;
                    let header_generation = header.get_item(1)?;
                    let mismatch = !header_idnum.eq(&idnum)? || !header_generation.eq(&generation)?;
                    actual_idnum = header_idnum;
                    actual_generation = header_generation;
                    if mismatch {
                        header_error = Some(errors::pdf_read_error(
                            py,
                            "Not matching, we parse the file for it",
                        ));
                    }
                }
                Err(error) => {
                    if error.is_instance_of::<PyMemoryError>(py) {
                        return Err(error);
                    }
                    header_error = Some(error);
                }
            }
            if header_error.is_some() {
                let buffer = shim_attr(py, "get_stream_data")?.call1((&stream,))?;
                let pattern = format!(
                    r"\s{}\s+{}\s+obj",
                    str_of(&idnum)?,
                    str_of(&generation)?
                );
                let matched = py
                    .import("re")?
                    .call_method1("search", (py_bytes(py, pattern.as_bytes()), buffer))?
                    .clone();
                if !matched.is_none() {
                    warn_values(
                        py,
                        "Object ID %(idnum)d,%(generation)d ref repaired",
                        vec![
                            ("idnum", idnum.clone()),
                            ("generation", generation.clone()),
                        ],
                    )?;
                    let position = py_plus(
                        &matched.call_method1("start", (0i64,))?,
                        py_int(py, 1),
                    )?;
                    xref
                        .get_item(&generation)?
                        .set_item(&idnum, &position)?;
                    stream.call_method1("seek", (&position, 0i64))?;
                    let header = slf.call_method1("read_object_header", (&stream,))?;
                    actual_idnum = header.get_item(0)?;
                    actual_generation = header.get_item(1)?;
                } else {
                    actual_idnum = py_int(py, -1);
                    actual_generation = py_int(py, -1); // exception will be raised below
                }
            }
            if !actual_idnum.eq(&idnum)? {
                let xref_index = slf.getattr("xref_index")?;
                if xref_index.is_truthy()? {
                    // xref table probably had bad indexes due to not being zero-indexed
                    if strict {
                        return Err(errors::pdf_read_error(
                            py,
                            &format!(
                                "Expected object ID ({} {}) does not match actual ({} {}); \
                                 xref table not zero-indexed.",
                                str_of(&idnum)?,
                                str_of(&generation)?,
                                str_of(&actual_idnum)?,
                                str_of(&actual_generation)?
                            ),
                        ));
                    }
                    // xref table is corrected in non-strict mode
                } else if strict {
                    // some other problem
                    return Err(errors::pdf_read_error(
                        py,
                        &format!(
                            "Expected object ID ({} {}) does not match actual ({} {}).",
                            str_of(&idnum)?,
                            str_of(&generation)?,
                            str_of(&actual_idnum)?,
                            str_of(&actual_generation)?
                        ),
                    ));
                }
            }
            if strict {
                assert_that(py, actual_generation.eq(&generation)?, None)?;
            }

            let current_object = py_tuple(py, vec![idnum.clone(), generation.clone()])?;
            let known_objects = slf.getattr("_known_objects")?;
            if known_objects.contains(&current_object)? {
                return Err(errors::limit_reached_error(
                    py,
                    &format!(
                        "Detected loop with self reference for {}.",
                        repr_of(&indirect_reference)?
                    ),
                ));
            }
            known_objects.call_method1("add", (&current_object,))?;
            let retval = call_shim(py, "read_object", vec![stream.clone(), slf.clone()])?;
            // Upstream removes the marker only after a successful read: when the
            // read raises, the object stays in `_known_objects`, which is
            // observable on later lookups ("Detected loop with self reference").
            known_objects.call_method1("remove", (&current_object,))?;
            decrypt_retval(py, slf, retval, &idnum, &generation)?
        } else {
            let stream = slf.getattr("stream")?;
            let buffer = shim_attr(py, "get_stream_data")?.call1((&stream,))?;
            let pattern = format!(r"\s{}\s+{}\s+obj", str_of(&idnum)?, str_of(&generation)?);
            let matched = py
                .import("re")?
                .call_method1("search", (py_bytes(py, pattern.as_bytes()), buffer))?
                .clone();
            if !matched.is_none() {
                warn_values(
                    py,
                    "Object %(idnum)d %(generation)d found",
                    vec![("idnum", idnum.clone()), ("generation", generation.clone())],
                )?;
                if !xref.contains(&generation)? {
                    xref.set_item(&generation, PyDict::new(py))?;
                }
                let position = py_plus(
                    &matched.call_method1("start", (0i64,))?,
                    py_int(py, 1),
                )?;
                xref.get_item(&generation)?.set_item(&idnum, &position)?;
                let end_position = py_plus(
                    &matched.call_method1("end", (0i64,))?,
                    py_int(py, 1),
                )?;
                stream.call_method1("seek", (end_position, 0i64))?;
                utils_attr(py, "skip_over_whitespace")?.call1((&stream,))?;
                stream.call_method1("seek", (-1i64, 1i64))?;
                let retval = call_shim(py, "read_object", vec![stream.clone(), slf.clone()])?;
                decrypt_retval(py, slf, retval, &idnum, &generation)?
            } else {
                warn_values(
                    py,
                    "Object %(idnum)d %(generation)d not defined.",
                    vec![("idnum", idnum.clone()), ("generation", generation.clone())],
                )?;
                if strict {
                    return Err(errors::pdf_read_error(py, "Could not find object."));
                }
                retval
            }
        }
    };

    // For ObjStm objects, _get_object_from_stream already cached the result
    // during batch parsing; skip the redundant cache write to avoid
    // "Overwriting cache" warnings.
    if !(generation_is_zero && xref_obj_stm.contains(&idnum)?) {
        slf.call_method1(
            "cache_indirect_object",
            (&generation, &idnum, &retval),
        )?;
    }
    Ok(retval)
}


#[inline]
fn header_is_whitespace(byte: u8) -> bool {
    // Python `bytes.isspace() or byte in WHITESPACES` (adds \x0b and \x00).
    matches!(byte, 0 | b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | b' ')
}

enum HeaderToken {
    Done(Vec<u8>, usize),
    Limit(Vec<u8>, usize),
    NeedMore,
}

fn header_read_token(
    data: &[u8],
    mut pos: usize,
    max_bytes: usize,
    eof: bool,
) -> HeaderToken {
    let mut out: Vec<u8> = Vec::new();
    loop {
        if pos >= data.len() {
            if eof {
                return HeaderToken::Done(out, pos);
            }
            return HeaderToken::NeedMore;
        }
        let byte = data[pos];
        pos += 1;
        if header_is_whitespace(byte) {
            return HeaderToken::Done(out, pos);
        }
        if out.len() >= max_bytes {
            return HeaderToken::Limit(out, pos);
        }
        out.push(byte);
    }
}

fn header_skip_whitespace(data: &[u8], mut pos: usize, eof: bool) -> Option<(bool, usize)> {
    if pos >= data.len() {
        return if eof { Some((false, pos)) } else { None };
    }
    let mut skipped = false;
    while pos < data.len() && header_is_whitespace(data[pos]) {
        skipped = true;
        pos += 1;
    }
    if pos >= data.len() {
        return if eof { Some((skipped, pos)) } else { None };
    }
    // `skip_over_whitespace` consumes the first non-whitespace byte too.
    Some((skipped, pos + 1))
}

fn header_read_non_whitespace(data: &[u8], mut pos: usize, eof: bool) -> Option<usize> {
    while pos < data.len() && header_is_whitespace(data[pos]) {
        pos += 1;
    }
    if pos >= data.len() {
        return if eof { Some(pos) } else { None };
    }
    Some(pos + 1)
}

/// `int(bytes)` with upstream's `ValueError` message and cause.
fn header_int_tuple<'py>(
    py: Python<'py>,
    idnum: &Bound<'py, PyAny>,
    generation: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let int = int_class(py)?;
    match (int.call1((idnum,)), int.call1((generation,))) {
        (Ok(idnum_value), Ok(generation_value)) => Ok(py_tuple(py, vec![idnum_value, generation_value])?),
        (idnum_result, generation_result) => {
            let error = idnum_result
                .err()
                .or_else(|| generation_result.err())
                .unwrap_or_else(|| PyValueError::new_err("invalid"));
            let message = format!(
                "Invalid indirect object reference ({} {} R): {}",
                raw_repr(py, idnum)?,
                raw_repr(py, generation)?,
                str_of(error.value(py).as_any())?
            );
            Err(raise_with_cause(py, PyValueError::new_err(message), error))
        }
    }
}

/// Buffered `read_object_header`: parse the header from one chunk instead of
/// ~18 Python-level stream operations.  `None` when the buffer cannot decide
/// (comment/token longer than the chunk), so the caller runs the legacy path.
fn read_object_header_buffered<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let start = stream.call_method0("tell")?.extract::<i64>()?;
    let strict = slf.getattr("strict")?.is_truthy()?;
    let indirect_class = generic_attr(py, "IndirectObject")?;
    let max_bytes: usize = indirect_class.getattr("_MAXIMUM_PART_LENGTH")?.extract()?;
    let mut chunk = 512usize;
    loop {
        let data: Vec<u8> = stream.call_method1("read", (chunk,))?.extract()?;
        let eof = data.len() < chunk;
        match parse_header_chunk(py, &data, eof, max_bytes, strict) {
            Ok(Some((consumed, idnum, generation, extra))) => {
                stream.call_method1("seek", (start + consumed as i64, 0i64))?;
                let idnum = PyBytes::new(py, &idnum);
                let generation = PyBytes::new(py, &generation);
                if extra && strict {
                    warn_values(
                        py,
                        "Superfluous whitespace found in object header %(idnum)r %(generation)r",
                        vec![("idnum", idnum.clone().into_any()), ("generation", generation.clone().into_any())],
                    )?;
                }
                return Ok(Some(header_int_tuple(py, &idnum.into_any(), &generation.into_any())?));
            }
            Ok(None) if !eof && chunk < 65536 => {
                chunk *= 8;
                stream.call_method1("seek", (start, 0i64))?;
                continue;
            }
            _ => {
                stream.call_method1("seek", (start, 0i64))?;
                return Ok(None);
            }
        }
    }
}

fn parse_header_chunk(
    py: Python<'_>,
    data: &[u8],
    eof: bool,
    max_bytes: usize,
    strict: bool,
) -> PyResult<Option<(usize, Vec<u8>, Vec<u8>, bool)>> {
    let len = data.len();
    let mut pos = 0usize;

    // skip_over_comment
    if pos < len && data[pos] == b'%' {
        pos += 1;
        loop {
            if pos >= len {
                if eof {
                    return Err(errors::pdf_stream_error(py, "File ended unexpectedly."));
                }
                return Ok(None);
            }
            let byte = data[pos];
            pos += 1;
            if byte == b'\n' || byte == b'\r' {
                break;
            }
        }
    }

    // extra = skip_over_whitespace(stream); stream.seek(-1, 1)
    let Some((mut extra, next)) = header_skip_whitespace(data, pos, eof) else {
        return Ok(None);
    };
    pos = next;
    if pos > 0 {
        pos -= 1;
    }

    // idnum = read_until_whitespace(...)
    let idnum = match header_read_token(data, pos, max_bytes, eof) {
        HeaderToken::Done(token, next) => {
            pos = next;
            token
        }
        HeaderToken::Limit(token, next) => {
            pos = next;
            if strict {
                return Err(errors::limit_reached_error(
                    py,
                    &format!("Token exceeds maximum length of {max_bytes} bytes."),
                ));
            }
            let values = PyDict::new(py);
            values.set_item("max_bytes", max_bytes as i64)?;
            pyutil::logger_warning(
                py,
                "pypdf._utils",
                "Token exceeds maximum length of %(max_bytes)d bytes.",
                Some(&values),
            )?;
            token
        }
        HeaderToken::NeedMore => return Ok(None),
    };

    // extra |= skip_over_whitespace(stream); stream.seek(-1, 1)
    let Some((skipped, next)) = header_skip_whitespace(data, pos, eof) else {
        return Ok(None);
    };
    extra |= skipped;
    pos = next;
    if pos > 0 {
        pos -= 1;
    }

    // generation
    let generation = match header_read_token(data, pos, max_bytes, eof) {
        HeaderToken::Done(token, next) => {
            pos = next;
            token
        }
        HeaderToken::Limit(token, next) => {
            pos = next;
            if strict {
                return Err(errors::limit_reached_error(
                    py,
                    &format!("Token exceeds maximum length of {max_bytes} bytes."),
                ));
            }
            let values = PyDict::new(py);
            values.set_item("max_bytes", max_bytes as i64)?;
            pyutil::logger_warning(
                py,
                "pypdf._utils",
                "Token exceeds maximum length of %(max_bytes)d bytes.",
                Some(&values),
            )?;
            token
        }
        HeaderToken::NeedMore => return Ok(None),
    };

    // extra |= skip_over_whitespace(stream); stream.seek(-1, 1)
    let Some((skipped, next)) = header_skip_whitespace(data, pos, eof) else {
        return Ok(None);
    };
    extra |= skipped;
    pos = next;
    if pos > 0 {
        pos -= 1;
    }

    // _obj = stream.read(3)
    let remaining = len.saturating_sub(pos);
    if remaining < 3 {
        if eof {
            pos = len;
        } else {
            return Ok(None);
        }
    } else {
        pos += 3;
    }

    // read_non_whitespace(stream); stream.seek(-1, 1)
    let Some(next) = header_read_non_whitespace(data, pos, eof) else {
        return Ok(None);
    };
    pos = next;
    if pos > 0 {
        pos -= 1;
    }

    Ok(Some((pos, idnum, generation, extra)))
}

// @pypdf: pypdf._reader.PdfReader.read_object_header
#[pyfunction]
fn reader_read_object_header<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    if std::env::var_os("PYPDF_RS_LEGACY_OBJECT_HEADER").is_none() {
        if let Some(header) = read_object_header_buffered(py, slf, stream)? {
            return Ok(header);
        }
    }
    // Should never be necessary to read out whitespace, since the
    // cross-reference table should put us in the right spot to read the
    // object header. In reality some files have stupid cross-reference
    // tables that are off by whitespace bytes.
    utils_attr(py, "skip_over_comment")?.call1((stream,))?;
    let mut extra = utils_attr(py, "skip_over_whitespace")?
        .call1((stream,))?
        .is_truthy()?;
    stream.call_method1("seek", (-1i64, 1i64))?;
    let indirect_class = generic_attr(py, "IndirectObject")?;
    let max_bytes = indirect_class.getattr("_MAXIMUM_PART_LENGTH")?;
    let strict = slf.getattr("strict")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("max_bytes", &max_bytes)?;
    kwargs.set_item("strict", py_bool(py, strict.is_truthy()?))?;
    let read_until_whitespace = utils_attr(py, "read_until_whitespace")?;
    let idnum = read_until_whitespace.call((stream,), Some(&kwargs))?;
    extra = utils_attr(py, "skip_over_whitespace")?
        .call1((stream,))?
        .is_truthy()?
        || extra;
    stream.call_method1("seek", (-1i64, 1i64))?;
    let generation = read_until_whitespace.call((stream,), Some(&kwargs))?;
    extra = utils_attr(py, "skip_over_whitespace")?
        .call1((stream,))?
        .is_truthy()?
        || extra;
    stream.call_method1("seek", (-1i64, 1i64))?;

    // although it's not used, it might still be necessary to read
    let _object = stream.call_method1("read", (3i64,))?;

    utils_attr(py, "read_non_whitespace")?.call1((stream,))?;
    stream.call_method1("seek", (-1i64, 1i64))?;
    if extra && strict.is_truthy()? {
        warn_values(
            py,
            "Superfluous whitespace found in object header %(idnum)r %(generation)r",
            vec![("idnum", idnum.clone()), ("generation", generation.clone())],
        )?;
    }

    let int = int_class(py)?;
    match (int.call1((&idnum,)), int.call1((&generation,))) {
        (Ok(idnum_value), Ok(generation_value)) => {
            Ok(py_tuple(py, vec![idnum_value, generation_value])?)
        }
        (idnum_result, generation_result) => {
            let error = idnum_result
                .err()
                .or_else(|| generation_result.err())
                .unwrap_or_else(|| PyValueError::new_err("invalid"));
            let message = format!(
                "Invalid indirect object reference ({} {} R): {}",
                raw_repr(py, &idnum)?,
                raw_repr(py, &generation)?,
                str_of(error.value(py).as_any())?
            );
            Err(raise_with_cause(py, PyValueError::new_err(message), error))
        }
    }
}

/// Python `repr(bytes)` without allocating a newly created object.
fn raw_repr(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<String> {
    let _ = py;
    pyutil::py_repr(obj)
}

// @pypdf: pypdf._reader.PdfReader.cache_get_indirect_object
#[pyfunction]
fn reader_cache_get_indirect_object<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    generation: &Bound<'py, PyAny>,
    idnum: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let key = py_tuple(py, vec![generation.clone(), idnum.clone()])?;
    match slf.getattr("resolved_objects")?.call_method1("get", (key,)) {
        Ok(value) => Ok(value),
        Err(error) => {
            if error.is_instance_of::<PyRecursionError>(py) {
                let new_error = errors::pdf_read_error(py, "Maximum recursion depth reached.");
                Err(raise_with_cause(py, new_error, error))
            } else {
                Err(error)
            }
        }
    }
}

// @pypdf: pypdf._reader.PdfReader.cache_indirect_object
#[pyfunction]
fn reader_cache_indirect_object<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    generation: &Bound<'py, PyAny>,
    idnum: &Bound<'py, PyAny>,
    obj: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let resolved = slf.getattr("resolved_objects")?;
    let key = py_tuple(py, vec![generation.clone(), idnum.clone()])?;
    if resolved.contains(&key)? {
        let values = PyDict::new(py);
        values.set_item("generation", generation)?;
        values.set_item("idnum", idnum)?;
        let message = "Overwriting cache for %(generation)d %(idnum)d";
        if slf.getattr("strict")?.is_truthy()? {
            return Err(errors::pdf_read_error(
                py,
                &percent_format(py, message, values.as_any())?,
            ));
        }
        pyutil::logger_warning(py, SOURCE, message, Some(&values))?;
    }
    resolved.set_item(key, obj)?;
    if !obj.is_none() {
        let indirect_class = generic_attr(py, "IndirectObject")?;
        let reference = indirect_class.call1((idnum, generation, slf))?;
        obj.setattr("indirect_reference", reference)?;
    }
    Ok(obj.clone())
}

// @pypdf: pypdf._reader.PdfReader._replace_object
#[pyfunction]
fn reader_replace_object<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    indirect_reference: &Bound<'py, PyAny>,
    obj: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    if indirect_reference.getattr("pdf")?.ne(slf)? {
        return Err(PyValueError::new_err(
            "Cannot update PdfReader with external object",
        ));
    }
    let generation = indirect_reference.getattr("generation")?;
    let idnum = indirect_reference.getattr("idnum")?;
    let resolved = slf.getattr("resolved_objects")?;
    let key = py_tuple(py, vec![generation, idnum])?;
    if !resolved.contains(&key)? {
        return Err(PyValueError::new_err("Cannot find referenced object"));
    }
    resolved.set_item(&key, obj)?;
    obj.setattr("indirect_reference", indirect_reference)?;
    Ok(obj.clone())
}

// ---------------------------------------------------------------------------
// Stream reading / validation
// ---------------------------------------------------------------------------

// @pypdf: pypdf._reader.PdfReader.read
#[pyfunction]
fn reader_read<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    slf.call_method1("_basic_validation", (stream,))?;
    slf.call_method1("_find_eof_marker", (stream,))?;
    let mut startxref: i128 = slf
        .call_method1("_find_startxref_pos", (stream,))?
        .extract()?;
    if startxref < 0 {
        // A negative offset cannot point into the file. Treat it like the
        // zero case (#3157) so the xref table is repaired instead of
        // leaking a ValueError from a negative seek.
        warn_values(
            py,
            "Negative startxref pointer (%(startxref)d), treating it as zero.",
            vec![("startxref", py_int(py, startxref))],
        )?;
        startxref = 0;
    }
    slf.setattr("_startxref", py_int(py, startxref))?;

    // check and eventually correct the startxref only if not strict
    let xref_issue_nr: i128 = slf
        .call_method1("_get_xref_issues", (stream, py_int(py, startxref)))?
        .extract()?;
    let strict = slf.getattr("strict")?.is_truthy()?;
    if xref_issue_nr != 0 {
        if strict && xref_issue_nr != 0 {
            return Err(errors::pdf_read_error(py, "Broken xref table"));
        }
        warn_values(
            py,
            "incorrect startxref pointer(%(xref_issue_nr)d)",
            vec![("xref_issue_nr", py_int(py, xref_issue_nr))],
        )?;
    }

    // read all cross-reference tables and their trailers
    slf.call_method1(
        "_read_xref_tables_and_trailers",
        (stream, py_int(py, startxref), py_int(py, xref_issue_nr)),
    )?;

    // if not zero-indexed, verify that the table is correct; change it if necessary
    if slf.getattr("xref_index")?.is_truthy()? && !strict {
        let loc = stream.call_method0("tell")?;
        let old_xref = slf.getattr("xref")?;
        for item in old_xref.call_method0("items")?.try_iter()? {
            let item = item?;
            let gen = item.get_item(0)?;
            let xref_entry = item.get_item(1)?;
            if gen.eq(65_535i64)? {
                continue;
            }
            // ensure ascending to prevent damage
            let xref_k = builtins(py)?
                .getattr("sorted")?
                .call1((xref_entry.call_method0("keys")?,))?;
            for xref_id in xref_k.try_iter()? {
                let xref_id = xref_id?;
                let offset = xref_entry.get_item(&xref_id)?;
                stream.call_method1("seek", (&offset, 0i64))?;
                let header = slf.call_method1("read_object_header", (stream,));
                let pid = match header {
                    Ok(header) => header.get_item(0)?,
                    Err(error) => {
                        if error.is_instance_of::<PyValueError>(py) {
                            slf.call_method1("_rebuild_xref_table", (stream,))?;
                            break;
                        }
                        return Err(error);
                    }
                };
                let pid_value: i128 = pid.extract()?;
                let xref_id_value: i128 = xref_id.extract()?;
                let xref_index_value: i128 = slf.getattr("xref_index")?.extract()?;
                if pid_value == xref_id_value - xref_index_value {
                    // fixing index item per item is required for revised PDF.
                    let current_xref = slf.getattr("xref")?;
                    let entry = current_xref.get_item(&gen)?;
                    let value = entry.get_item(&xref_id)?;
                    entry.set_item(&pid, value)?;
                    entry.del_item(&xref_id)?;
                }
            }
        }
        stream.call_method1("seek", (loc, 0i64))?;
    }

    // remove wrong objects (not pointing to correct structures) - cf #2326
    if !strict {
        let loc = stream.call_method0("tell")?;
        let xref = slf.getattr("xref")?;
        for item in xref.call_method0("items")?.try_iter()? {
            let item = item?;
            let gen = item.get_item(0)?;
            let xref_entry = item.get_item(1)?;
            if gen.eq(65_535i64)? {
                continue;
            }
            let ids = py_list(
                py,
                xref_entry
                    .call_method0("keys")?
                    .try_iter()?
                    .collect::<PyResult<Vec<_>>>()?,
            )?;
            for xref_id in ids.try_iter()? {
                let xref_id = xref_id?;
                let offset = xref_entry.get_item(&xref_id)?;
                stream.call_method1("seek", (&offset, 0i64))?;
                match slf.call_method1("read_object_header", (stream,)) {
                    Ok(_) => {}
                    Err(error) => {
                        if !error.is_instance_of::<PyValueError>(py) {
                            return Err(error);
                        }
                        warn_values(
                            py,
                            "Ignoring wrong pointing object %(id)d %(gen)d (offset %(offset)d)",
                            vec![
                                ("id", xref_id.clone()),
                                ("gen", gen.clone()),
                                ("offset", offset.clone()),
                            ],
                        )?;
                        xref_entry.del_item(&xref_id)?;
                    }
                }
            }
        }
        stream.call_method1("seek", (loc, 0i64))?;
    }
    Ok(())
}

// @pypdf: pypdf._reader.PdfReader._basic_validation
#[pyfunction]
fn reader_basic_validation(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    stream: &Bound<'_, PyAny>,
) -> PyResult<()> {
    stream.call_method1("seek", (0i64, 0i64))?;
    let header_byte = match stream.call_method1("read", (5i64,)) {
        Ok(value) => value,
        Err(error) => {
            if error.is_instance_of::<PyUnicodeDecodeError>(py) {
                let unsupported = py
                    .import("io")?
                    .getattr("UnsupportedOperation")?
                    .call1(("cannot read header",))?;
                return Err(raise_with_cause(
                    py,
                    PyErr::from_value(unsupported),
                    error,
                ));
            }
            return Err(error);
        }
    };
    if header_byte.eq(py_bytes(py, b""))? {
        return Err(errors::empty_file_error(py, "Cannot read an empty file"));
    }
    if !header_byte.eq(py_bytes(py, b"%PDF-"))? {
        if slf.getattr("strict")?.is_truthy()? {
            let decoded = header_byte.call_method1("decode", (py_str(py, "utf8"),))?;
            return Err(errors::pdf_read_error(
                py,
                &format!(
                    "PDF starts with '{}', but '%PDF-' expected",
                    str_of(&decoded)?
                ),
            ));
        }
        warn_values(
            py,
            "invalid pdf header: %(header_byte)r",
            vec![("header_byte", header_byte)],
        )?;
    }
    stream.call_method1("seek", (0i64, 2i64))?;
    Ok(())
}

// @pypdf: pypdf._reader.PdfReader._find_eof_marker
#[pyfunction]
fn reader_find_eof_marker(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    stream: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let header_size: i128 = 8;
    let mut line: Vec<u8> = Vec::new();
    let mut first = true;
    let mut end_position: i128 = stream_tell(stream)?;
    let strict = slf.getattr("strict")?.is_truthy()?;
    loop {
        let marker_index: i128 = match find_subslice(&line, b"%%EOF", 0) {
            Some(index) => index as i128,
            None => -1,
        };
        if marker_index == 0 {
            break;
        }
        if marker_index > 0 && first {
            // Only the trailing line is allowed to carry an inline marker.
            // Leading whitespace in front of the marker is ignored.
            if !lstrip_bytes(&line[..marker_index as usize], &WHITESPACES_AS_BYTES).is_empty() {
                warn(py, "EOF marker not at start of line")?;
                stream_seek(
                    py,
                    stream,
                    end_position - line.len() as i128 + marker_index,
                    0,
                )?;
            }
            break;
        }
        if !line.is_empty() && first {
            let stripped = strip_bytes(&line);
            if [b"%%EO".as_slice(), b"%%E".as_slice(), b"%%".as_slice(), b"%".as_slice()]
                .iter()
                .any(|suffix| stripped.ends_with(suffix))
            {
                // Consider the file as truncated while having enough
                // confidence to carry on.
                warn(py, "EOF marker seems truncated")?;
                break;
            }
            first = false;
        }
        if find_subslice(&line, b"startxref", 0).is_some() {
            warn(
                py,
                "CAUTION: startxref found while searching for %%EOF. The file might be \
                 truncated and some data might not be read.",
            )?;
        }
        if stream_tell(stream)? < header_size {
            if strict {
                return Err(errors::pdf_read_error(py, "EOF marker not found"));
            }
            warn(py, "EOF marker not found")?;
        }
        end_position = stream_tell(stream)?;
        line = bytes_of(&utils_attr(py, "read_previous_line")?.call1((stream,))?)?;
    }
    Ok(())
}

// @pypdf: pypdf._reader.PdfReader._find_startxref_pos
#[pyfunction]
fn reader_find_startxref_pos(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    stream: &Bound<'_, PyAny>,
) -> PyResult<i128> {
    let read_previous_line = utils_attr(py, "read_previous_line")?;
    let mut line = bytes_of(&read_previous_line.call1((stream,))?)?;
    // Some producers append further %%EOF markers below the one that closes
    // the last revision (#4008). _find_eof_marker() stops at the very last of
    // them, so the line above it is another marker rather than the offset.
    // Skip that trailing run to reach the real offset.
    let mut duplicate_markers = 0i128;
    while line.starts_with(b"%%EOF") && stream_tell(stream)? > 0 {
        if duplicate_markers == MAX_STARTXREF_RECOVERY_LINES {
            break;
        }
        line = bytes_of(&read_previous_line.call1((stream,))?)?;
        duplicate_markers += 1;
    }
    if duplicate_markers != 0 {
        warn(py, "Duplicate %%EOF marker(s) found, skipping them")?;
    }
    let int = int_class(py)?;
    let line_obj = py_bytes(py, &line);
    let startxref: i128 = match int.call1((&line_obj,)) {
        Ok(value) => value.extract()?,
        Err(error) => {
            if !error.is_instance_of::<PyValueError>(py) {
                return Err(error);
            }
            // 'startxref' may be on the same line as the location
            if !line.starts_with(b"startxref") {
                return Err(raise_with_cause(
                    py,
                    errors::pdf_read_error(py, "startxref not found"),
                    error,
                ));
            }
            let value = py_bytes(py, strip_bytes(&line[9..]));
            let valid = slf
                .getattr("_is_valid_startxref_value")?
                .call1((value.clone(),))?
                .is_truthy()?;
            if !valid {
                return Err(raise_with_cause(
                    py,
                    errors::pdf_read_error(py, "startxref not found"),
                    error,
                ));
            }
            let startxref: i128 = int.call1((value,))?.extract()?;
            warn(py, "startxref on same line as offset")?;
            return Ok(startxref);
        }
    };
    let line = bytes_of(&read_previous_line.call1((stream,))?)?;
    if !line.starts_with(b"startxref") {
        // The 'startxref' keyword expected just above the offset is missing or
        // corrupt (for example a truncated 'tartxref'). Recovering from this
        // violates the standard, so only attempt it in non-strict mode (#3238).
        if slf.getattr("strict")?.is_truthy()? {
            return Err(errors::pdf_read_error(py, "startxref not found"));
        }
        return slf
            .getattr("_find_previous_startxref_pos")?
            .call1((stream,))?
            .extract();
    }
    Ok(startxref)
}

// @pypdf: pypdf._reader.PdfReader._is_valid_startxref_value
#[pyfunction]
fn reader_is_valid_startxref_value(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let matched = py
        .import("re")?
        .call_method1(
            "fullmatch",
            (py_bytes(py, b"[+-]?[0-9]+(?:_[0-9]+)*"), value),
        )?;
    if matched.is_none() {
        return Ok(false);
    }
    // Respect Python's integer conversion limit without attempting int().
    let max_digits: i128 = match py.import("sys")?.getattr("get_int_max_str_digits") {
        Ok(function) => function.call0()?.extract().unwrap_or(0),
        Err(_) => 0,
    };
    let bytes = bytes_of(value)?;
    let mut start = 0;
    while start < bytes.len() && (bytes[start] == b'+' || bytes[start] == b'-') {
        start += 1;
    }
    let digits = bytes[start..].iter().filter(|byte| **byte != b'_').count();
    Ok(max_digits == 0 || digits as i128 <= max_digits)
}

// @pypdf: pypdf._reader.PdfReader._find_previous_startxref_pos
#[pyfunction]
fn reader_find_previous_startxref_pos<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<i128> {
    let read_previous_line = utils_attr(py, "read_previous_line")?;
    let int = int_class(py)?;
    let max_lines: i128 = cls
        .getattr("_MAX_STARTXREF_RECOVERY_LINES")?
        .extract()?;
    let mut offset: Option<i128> = None;
    for _ in 0..max_lines {
        if stream_tell(stream)? <= 0 {
            break;
        }
        let line = bytes_of(&read_previous_line.call1((stream,))?)?;
        if !line.starts_with(b"startxref") {
            offset = match int.call1((py_bytes(py, &line),)) {
                Ok(value) => Some(value.extract()?),
                Err(error) => {
                    if !error.is_instance_of::<PyValueError>(py) {
                        return Err(error);
                    }
                    None
                }
            };
            continue;
        }
        if line.len() > 9 {
            // 'startxref' on the same line as the offset
            let value = py_bytes(py, strip_bytes(&line[9..]));
            let valid = cls
                .getattr("_is_valid_startxref_value")?
                .call1((value.clone(),))?
                .is_truthy()?;
            if !valid {
                continue;
            }
            return int.call1((value,))?.extract();
        }
        if let Some(offset) = offset {
            warn(
                py,
                "found startxref pointing to a previous revision after a corrupt one",
            )?;
            return Ok(offset);
        }
        break;
    }
    Err(errors::pdf_read_error(py, "startxref not found"))
}

// @pypdf: pypdf._reader.PdfReader._load_recovery_cache
#[pyfunction]
fn reader_load_recovery_cache<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    data: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let cache = PyDict::new(py);
    for item in slf.call_method1("_find_pdf_objects", (data,))?.try_iter()? {
        let item = item?;
        let object_number = item.get_item(0)?;
        if cache.contains(&object_number)? {
            // Always use the first match.
            continue;
        }
        let generation_number = item.get_item(1)?;
        let object_start = item.get_item(2)?;
        cache.set_item(
            &object_number,
            py_tuple(py, vec![object_start, generation_number])?,
        )?;
    }
    Ok(cache.into_any())
}

// ---------------------------------------------------------------------------
// Cross-reference tables
// ---------------------------------------------------------------------------

// @pypdf: pypdf._reader.PdfReader._read_standard_xref_table__handle_size
#[pyfunction]
#[allow(non_snake_case)]
fn reader_read_standard_xref_table_handle_size<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let size = call_shim(py, "read_object", vec![stream.clone(), slf.clone()])?;
    if !is_instance(&size, &int_class(py)?)? {
        warn(py, "Invalid/Truncated xref table. Rebuilding it.")?;
        slf.call_method1("_rebuild_xref_table", (stream,))?;
        stream.call_method0("read")?;
        return Ok(py_none(py));
    }
    // A valid entry has a size of 20 bytes.
    let size_limit = shim_attr(py, "get_stream_size")?
        .call1((stream,))?
        .call_method1("__floordiv__", (py_int(py, 20),))?;
    if size.gt(&size_limit)? {
        return Err(errors::limit_reached_error(
            py,
            &format!(
                "Declared size {} exceeds allowed limit of {}!",
                str_of(&size)?,
                str_of(&size_limit)?
            ),
        ));
    }
    Ok(size)
}

// @pypdf: pypdf._reader.PdfReader._read_standard_xref_table
#[pyfunction]
fn reader_read_standard_xref_table(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    stream: &Bound<'_, PyAny>,
) -> PyResult<()> {
    // standard cross-reference table
    let reference = stream.call_method1("read", (3i64,))?;
    if !reference.eq(py_bytes(py, b"ref"))? {
        return Err(errors::pdf_read_error(py, "xref table read error"));
    }
    let read_non_whitespace = utils_attr(py, "read_non_whitespace")?;
    let skip_over_comment = utils_attr(py, "skip_over_comment")?;
    read_non_whitespace.call1((stream,))?;
    stream.call_method1("seek", (-1i64, 1i64))?;
    let mut first_time = true; // check if the first time looking at the xref table
    let mut recovery_cache: Option<Bound<'_, PyAny>> = None;
    let mut previous_position: Option<i128> = None;
    loop {
        let mut num = call_shim(py, "read_object", vec![stream.clone(), slf.clone()])?;
        if first_time && !num.eq(0i64)? {
            slf.setattr("xref_index", num.clone())?;
            if slf.getattr("strict")?.is_truthy()? {
                warn(
                    py,
                    "Xref table not zero-indexed. ID numbers for objects will be corrected.",
                )?;
            }
        }
        first_time = false;
        read_non_whitespace.call1((stream,))?;
        stream.call_method1("seek", (-1i64, 1i64))?;
        let size = slf.call_method1("_read_standard_xref_table__handle_size", (stream,))?;
        if size.is_none() {
            return Ok(());
        }
        read_non_whitespace.call1((stream,))?;
        stream.call_method1("seek", (-1i64, 1i64))?;
        let size_count: i128 = size.extract()?;
        let mut count: i128 = 0;
        while count < size_count {
            let mut line = bytes_of(&stream.call_method1("read", (20i64,))?)?;
            if line.is_empty() {
                return Err(errors::pdf_read_error(
                    py,
                    "Unexpected empty line in Xref table.",
                ));
            }

            // It's very clear in section 3.4.3 of the PDF spec that all
            // cross-reference table lines are a fixed 20 bytes (as of PDF
            // 1.7). However, some files have 21-byte entries (or more) due to
            // the use of \r\n (CRLF) EOL's.
            while line[0] == b'\r' || line[0] == b'\n' {
                stream.call_method1("seek", (-20i64 + 1, 1i64))?;
                line = bytes_of(&stream.call_method1("read", (20i64,))?)?;
                if line.len() != 20 {
                    return Err(errors::pdf_read_error(py, "Unexpected EOF in Xref table."));
                }
            }

            // On the other hand, some malformed PDF files use a single
            // character EOL without a preceding space.
            if matches!(line[line.len() - 1], b'0'..=b'9' | b't') {
                stream.call_method1("seek", (-1i64, 1i64))?;
            }

            let mut generation = py_int(py, 0);
            let mut offset = py_int(py, 0);
            let mut entry_type: Vec<u8> = b"n".to_vec();
            let mut parsed = false;
            if line.len() >= 16 {
                let parts: Vec<&[u8]> = line[..16].split(|byte| *byte == b' ').collect();
                if parts.len() == 2 {
                    let int = int_class(py)?;
                    let offset_result = int.call1((py_bytes(py, parts[0]),));
                    let generation_result = int.call1((py_bytes(py, parts[1]),));
                    if let (Ok(offset_value), Ok(generation_value)) =
                        (offset_result, generation_result)
                    {
                        offset = offset_value;
                        generation = generation_value;
                        entry_type = if line.len() > 17 {
                            line[17..18].to_vec()
                        } else {
                            Vec::new()
                        };
                        parsed = true;
                    }
                }
            }
            if !parsed {
                if recovery_cache.is_none() {
                    let buffer = shim_attr(py, "get_stream_data")?.call1((stream,))?;
                    recovery_cache = Some(slf.call_method1("_load_recovery_cache", (buffer,))?);
                }
                let cache = match recovery_cache.as_ref() {
                    Some(cache) => cache,
                    None => {
                        return Err(errors::pdf_read_error(
                            py,
                            "Cannot rebuild xref",
                        ))
                    }
                };
                if !cache.contains(&num)? {
                    warn_values(
                        py,
                        "entry %(num)d in Xref table invalid; object not found",
                        vec![("num", num.clone())],
                    )?;
                    generation = py_int(py, 65_535);
                    offset = py_int(py, -1);
                    entry_type = b"f".to_vec();
                } else {
                    warn_values(
                        py,
                        "entry %(num)d in Xref table invalid but object found",
                        vec![("num", num.clone())],
                    )?;
                    let recovered = cache.get_item(&num)?;
                    // Upstream unpacks `(object_start, generation_number)` as
                    // `generation, offset`; that historical quirk is preserved.
                    generation = recovered.get_item(0)?;
                    offset = recovered.get_item(1)?;
                    entry_type = b"n".to_vec();
                }
            }

            let xref = slf.getattr("xref")?;
            let xref_free_entry = slf.getattr("xref_free_entry")?;
            if !xref.contains(&generation)? {
                xref.set_item(&generation, PyDict::new(py))?;
                xref_free_entry.set_item(&generation, PyDict::new(py))?;
            }
            if xref.get_item(&generation)?.contains(&num)? {
                // It really seems like we should allow the last xref table in
                // the file to override previous ones. Since we read the file
                // backwards, assume any existing key is already set correctly.
            } else {
                if entry_type == b"n" {
                    xref.get_item(&generation)?.set_item(&num, &offset)?;
                }
                let is_free = entry_type == b"f";
                if let Ok(entry) = xref_free_entry.get_item(&generation) {
                    entry.set_item(&num, is_free)?;
                }
                if let Ok(entry) = xref_free_entry.get_item(py_int(py, 65_535)) {
                    entry.set_item(&num, is_free)?;
                }
            }
            count += 1;
            num = py_plus(&num, py_int(py, 1))?;
        }
        read_non_whitespace.call1((stream,))?;
        stream.call_method1("seek", (-1i64, 1i64))?;
        // Skip any PDF comments between xref entries and the trailer keyword.
        while stream
            .call_method1("read", (1i64,))?
            .eq(py_bytes(py, b"%" ))?
        {
            stream.call_method1("seek", (-1i64, 1i64))?;
            skip_over_comment.call1((stream,))?;
            read_non_whitespace.call1((stream,))?;
            stream.call_method1("seek", (-1i64, 1i64))?;
        }
        stream.call_method1("seek", (-1i64, 1i64))?;
        let trailer_tag = bytes_of(&stream.call_method1("read", (7i64,))?)?;
        if trailer_tag == b"trailer" {
            break;
        }

        // More xrefs should move the parser to another section.
        stream.call_method1("seek", (-7i64, 1i64))?;

        // Detect a repeated read at the same position.
        let rewound_position = stream_tell(stream)?;
        if previous_position == Some(rewound_position) {
            return Err(errors::pdf_read_error(
                py,
                &format!(
                    "Xref parsing stuck at position {}: {}",
                    rewound_position,
                    raw_repr(py, &py_bytes(py, &trailer_tag))?
                ),
            ));
        }
        previous_position = Some(rewound_position);
    }
    Ok(())
}

// @pypdf: pypdf._reader.PdfReader._read_xref_tables_and_trailers
#[pyfunction]
fn reader_read_xref_tables_and_trailers<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
    startxref: &Bound<'py, PyAny>,
    xref_issue_nr: &Bound<'py, PyAny>,
) -> PyResult<()> {
    slf.setattr("xref", PyDict::new(py))?;
    slf.setattr("xref_free_entry", PyDict::new(py))?;
    slf.setattr("xref_objStm", PyDict::new(py))?;
    slf.setattr("_xref_stream_entry_count", py_int(py, 0))?;
    slf.setattr("trailer", dictionary_object(py)?)?;
    let mut visited_xref_offsets: HashSet<i128> = HashSet::new();
    let mut startxref: Option<i128> = if startxref.is_none() {
        None
    } else {
        Some(startxref.extract()?)
    };
    let mut xref_issue_nr: i128 = xref_issue_nr.extract()?;
    while let Some(offset) = startxref {
        // Detect circular /Prev references in the xref chain
        if !visited_xref_offsets.insert(offset) {
            warn_values(
                py,
                "Circular xref chain detected at offset %(startxref)d, stopping",
                vec![("startxref", py_int(py, offset))],
            )?;
            break;
        }
        stream.call_method1("seek", (py_int(py, offset), 0i64))?;
        let mut x = bytes_of(&stream.call_method1("read", (1i64,))?)?;
        // Python `x in b"\r\n"` also accepts the empty (EOF) sequence.
        if x.is_empty() || x == b"\r" || x == b"\n" {
            x = bytes_of(&stream.call_method1("read", (1i64,))?)?;
        }
        if x == b"x" {
            let next = slf.call_method1("_read_xref", (stream,))?;
            startxref = if next.is_none() {
                None
            } else {
                Some(next.extract()?)
            };
        } else if xref_issue_nr != 0 {
            match slf.call_method1("_rebuild_xref_table", (stream,)) {
                Ok(_) => break,
                Err(error) => {
                    if error.is_instance_of::<PyMemoryError>(py) {
                        return Err(error);
                    }
                    if is_python_exception(py, &error) {
                        xref_issue_nr = 0;
                    } else {
                        return Err(error);
                    }
                }
            }
        } else if !x.is_empty() && x.iter().all(u8::is_ascii_digit) {
            match slf.call_method1("_read_pdf15_xref_stream", (stream,)) {
                Ok(xrefstream) => {
                    slf.call_method1("_process_xref_stream", (&xrefstream,))?;
                    if xrefstream.contains(py_str(py, "/Prev"))? {
                        startxref = Some(
                            xrefstream.get_item(py_str(py, "/Prev"))?.extract()?,
                        );
                    } else {
                        break;
                    }
                }
                Err(error) => {
                    if slf.getattr("trailer")?.contains(py_str(py, "/Root"))? {
                        let args = error.value(py).as_any().getattr("args")?;
                        warn_values(
                            py,
                            "Previous trailer cannot be read: %(args)s",
                            vec![("args", args)],
                        )?;
                        break;
                    }
                    let message = format!("Trailer cannot be read: {}", str_of(error.value(py).as_any())?);
                    return Err(raise_with_cause(
                        py,
                        errors::pdf_read_error(py, &message),
                        error,
                    ));
                }
            }
        } else {
            let next = slf.call_method1(
                "_read_xref_other_error",
                (stream, py_int(py, offset)),
            )?;
            startxref = if next.is_none() {
                None
            } else {
                Some(next.extract()?)
            };
        }
    }
    Ok(())
}

// @pypdf: pypdf._reader.PdfReader._XREF_STREAM_TRAILER_KEYS
// @pypdf: pypdf._reader.PdfReader._process_xref_stream
#[pyfunction]
fn reader_process_xref_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    xrefstream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let trailer = slf.getattr("trailer")?;
    let keys = slf.getattr("_XREF_STREAM_TRAILER_KEYS")?;
    for key in keys.try_iter()? {
        let key = key?;
        if xrefstream.contains(&key)? && !trailer.contains(&key)? {
            let name_class = generic_attr(py, "NameObject")?;
            trailer.set_item(name_class.call1((&key,))?, xrefstream.getattr("raw_get")?.call1((&key,))?)?;
        }
    }
    if xrefstream.contains(py_str(py, "/XRefStm"))? {
        let stream = slf.getattr("stream")?;
        let position = stream.call_method0("tell")?;
        let target = py_plus(
            &xrefstream.get_item(py_str(py, "/XRefStm"))?,
            py_int(py, 1),
        )?;
        stream.call_method1("seek", (target, 0i64))?;
        slf.call_method1("_read_pdf15_xref_stream", (&stream,))?;
        stream.call_method1("seek", (&position, 0i64))?;
    }
    Ok(())
}

// @pypdf: pypdf._reader.PdfReader._read_xref
#[pyfunction]
fn reader_read_xref<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    slf.call_method1("_read_standard_xref_table", (stream,))?;
    let next_char = stream.call_method1("read", (1i64,))?;
    if next_char.eq(py_bytes(py, b""))? {
        return Ok(py_none(py));
    }
    stream.call_method1("seek", (-1i64, 1i64))?;
    utils_attr(py, "read_non_whitespace")?.call1((stream,))?;
    stream.call_method1("seek", (-1i64, 1i64))?;
    let new_trailer = call_shim(py, "read_object", vec![stream.clone(), slf.clone()])?;
    let trailer = slf.getattr("trailer")?;
    for item in new_trailer.call_method0("items")?.try_iter()? {
        let item = item?;
        let key = item.get_item(0)?;
        if !trailer.contains(&key)? {
            trailer.set_item(key, item.get_item(1)?)?;
        }
    }
    if new_trailer.contains(py_str(py, "/XRefStm"))? {
        let position = stream.call_method0("tell")?;
        let target = py_plus(
            &new_trailer.get_item(py_str(py, "/XRefStm"))?,
            py_int(py, 1),
        )?;
        stream.call_method1("seek", (target, 0i64))?;
        let result = slf.call_method1("_read_pdf15_xref_stream", (stream,));
        let outcome = match result {
            Ok(_) => Ok(()),
            Err(error) => {
                if error.is_instance_of::<PyMemoryError>(py) {
                    Err(error)
                } else if is_instance_of_module(
                    py,
                    error.value(py),
                    "pypdf.errors",
                    "LimitReachedError",
                )? {
                    // Do not continue on reaching the resource limits.
                    Err(error)
                } else if is_python_exception(py, &error) {
                    let xref_stm = int_class(py)?.call1((
                        new_trailer.get_item(py_str(py, "/XRefStm"))?,
                    ))?;
                    warn_values(
                        py,
                        "XRef object at %(xref_stm)d can not be read, some object may be missing",
                        vec![("xref_stm", xref_stm)],
                    )?;
                    Ok(())
                } else {
                    Err(error)
                }
            }
        };
        stream.call_method1("seek", (&position, 0i64))?;
        outcome?;
    }
    if new_trailer.contains(py_str(py, "/Prev"))? {
        return new_trailer.get_item(py_str(py, "/Prev"));
    }
    Ok(py_none(py))
}

// @pypdf: pypdf._reader.PdfReader._read_xref_other_error
#[pyfunction]
fn reader_read_xref_other_error<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
    startxref: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut startxref: i128 = startxref.extract()?;
    // some PDFs have /Prev=0 in the trailer, instead of no /Prev
    if startxref == 0 {
        if slf.getattr("strict")?.is_truthy()? {
            return Err(errors::pdf_read_error(
                py,
                "/Prev=0 in the trailer (try opening with strict=False)",
            ));
        }
        warn(
            py,
            "/Prev=0 in the trailer - assuming there is no previous xref table",
        )?;
        return Ok(py_none(py));
    }
    // bad xref character at startxref. Let's see if we can find
    // the xref table nearby, as we've observed this error with an
    // off-by-one before.
    stream.call_method1("seek", (-11i64, 1i64))?;
    let tmp = bytes_of(&stream.call_method1("read", (20i64,))?)?;
    if let Some(xref_loc) = find_subslice(&tmp, b"xref", 0) {
        startxref -= 10 - xref_loc as i128;
        return Ok(py_int(py, startxref));
    }
    // No explicit xref table, try finding a cross-reference stream.
    stream.call_method1("seek", (py_int(py, startxref), 0i64))?;
    for look in 0..25i128 {
        let byte = bytes_of(&stream.call_method1("read", (1i64,))?)?;
        if !byte.is_empty() && byte.iter().all(u8::is_ascii_digit) {
            // This is not a standard PDF, consider adding a warning
            startxref += look;
            return Ok(py_int(py, startxref));
        }
    }
    // no xref table found at specified location
    if slf.getattr("trailer")?.contains(py_str(py, "/Root"))?
        && !slf.getattr("strict")?.is_truthy()?
    {
        // if Root has been already found, just raise warning
        warn(py, "Invalid parent xref., rebuild xref")?;
        match slf.call_method1("_rebuild_xref_table", (stream,)) {
            Ok(_) => Ok(py_none(py)),
            Err(error) => {
                if is_python_exception(py, &error) {
                    Err(raise_with_cause(
                        py,
                        errors::pdf_read_error(py, "Cannot rebuild xref"),
                        error,
                    ))
                } else {
                    Err(error)
                }
            }
        }
    } else {
        Err(errors::pdf_read_error(
            py,
            "Could not find xref table at specified location",
        ))
    }
}

// @pypdf: pypdf._reader.PdfReader._sanitize_pdf15_xref_stream_index_pairs
#[pyfunction]
fn reader_sanitize_pdf15_xref_stream_index_pairs<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    index_pairs: &Bound<'py, PyAny>,
    entry_sizes: &Bound<'py, PyAny>,
    xref_stream: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let entry_sizes_len = entry_sizes.len()?;
    let min_count = entry_sizes_len.min(3);
    let int = int_class(py)?;
    let mut min_entry_bytes: i128 = 0;
    for index in 0..min_count {
        // Upstream converts with Python's `int(...)` here.
        min_entry_bytes += int.call1((entry_sizes.get_item(index)?,))?.extract::<i128>()?;
    }
    if min_entry_bytes == 0 {
        let message = "Cross-reference stream encodes no entry data.";
        if slf.getattr("strict")?.is_truthy()? {
            return Err(errors::pdf_stream_error(py, message));
        }
        warn(py, message)?;
        return py_list(py, Vec::new());
    }

    // maximum number of entries that could physically fit
    let data_length = xref_stream.call_method0("get_data")?.len()?;
    let max_entries = py_plus(
        &py_int(py, data_length as i128)
            .call_method1("__floordiv__", (py_int(py, min_entry_bytes),))?,
        py_int(py, 1),
    )?;

    let mut result: Vec<Bound<'py, PyAny>> = Vec::new();
    let mut total: Bound<'py, PyAny> = py_int(py, 0);
    let int = int_class(py)?;
    for (index, pair_value) in index_pairs.try_iter()?.enumerate() {
        let mut pair_value_int = int.call1((pair_value?,))?;
        // `index_pairs` has the format `[start0, count0, start1, count1, ...]`
        // Only modify the counts here, but keep the start values.
        if index % 2 == 1 {
            if pair_value_int.lt(0i64)? {
                if slf.getattr("strict")?.is_truthy()? {
                    return Err(errors::pdf_stream_error(
                        py,
                        &format!("Negative XRef count {}.", str_of(&pair_value_int)?),
                    ));
                }
                warn_values(
                    py,
                    "Negative XRef count %(count)d, using 0.",
                    vec![("count", pair_value_int.clone())],
                )?;
                pair_value_int = py_int(py, 0);
            }
            let new_total = pyutil::py_add(&total, &pair_value_int)?;
            if new_total.gt(&max_entries)? {
                if slf.getattr("strict")?.is_truthy()? {
                    return Err(errors::limit_reached_error(
                        py,
                        &format!(
                            "Total XRef entries {} exceed maximum allowed value {}.",
                            str_of(&new_total)?,
                            str_of(&max_entries)?
                        ),
                    ));
                }
                let remaining = py_sub(&max_entries, &total)?;
                let new_value = builtins(py)?
                    .getattr("max")?
                    .call1((py_int(py, 0), remaining))?;
                warn_values(
                    py,
                    "Clamping XRef count from %(old_count)d to %(new_count)d to fit stream size.",
                    vec![
                        ("old_count", pair_value_int.clone()),
                        ("new_count", new_value.clone()),
                    ],
                )?;
                pair_value_int = new_value;
            }
            total = pyutil::py_add(&total, &pair_value_int)?;
        }
        result.push(pair_value_int);
    }
    py_list(py, result)
}

// @pypdf: pypdf._reader.PdfReader._read_pdf15_xref_stream
#[pyfunction]
fn reader_read_pdf15_xref_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    stream.call_method1("seek", (-1i64, 1i64))?;
    let header = slf.call_method1("read_object_header", (stream,))?;
    let stream_idnum = header.get_item(0)?;
    let stream_generation = header.get_item(1)?;
    let xref_stream = call_shim(py, "read_object", vec![stream.clone(), slf.clone()])?;
    let object_type = xref_stream.get_item(py_str(py, "/Type"))?;
    if !object_type.eq(py_str(py, "/XRef"))? {
        return Err(errors::pdf_read_error(
            py,
            &format!("Unexpected type {}", repr_of(&object_type)?),
        ));
    }
    slf.call_method1(
        "cache_indirect_object",
        (&stream_generation, &stream_idnum, &xref_stream),
    )?;

    // Index pairs specify the subsections in the dictionary.
    // If none, create one subsection that spans everything.
    let size_key = py_str(py, "/Size");
    if !xref_stream.contains(&size_key)? {
        // According to table 17 of the PDF 2.0 specification, this key is required.
        return Err(errors::pdf_read_error(
            py,
            &format!("Size missing from XRef stream {}!", repr_of(&xref_stream)?),
        ));
    }
    let default_index = py_list(
        py,
        vec![py_int(py, 0), xref_stream.get_item(&size_key)?],
    )?;
    let index_pairs = xref_stream.call_method1("get", (py_str(py, "/Index"), default_index))?;
    let entry_sizes = xref_stream.call_method1("get", (py_str(py, "/W"),))?;
    assert_that(py, entry_sizes.len()? >= 3, None)?;
    if slf.getattr("strict")?.is_truthy()? && entry_sizes.len()? > 3 {
        return Err(errors::pdf_read_error(
            py,
            &format!("Too many entry sizes: {}", str_of(&entry_sizes)?),
        ));
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("index_pairs", &index_pairs)?;
    kwargs.set_item("entry_sizes", &entry_sizes)?;
    kwargs.set_item("xref_stream", &xref_stream)?;
    let index_pairs = slf.call_method(
        "_sanitize_pdf15_xref_stream_index_pairs",
        (),
        Some(&kwargs),
    )?;

    let counts_slice = py_slice_of(py, Some(py_int(py, 1)), None, Some(py_int(py, 2)))?;
    let counts = index_pairs.call_method1("__getitem__", (counts_slice,))?;
    let new_count = builtins(py)?.getattr("sum")?.call1((counts,))?;
    let previous_count = slf.getattr("_xref_stream_entry_count")?;
    let entry_count = pyutil::py_add(&previous_count, &new_count)?;
    slf.setattr("_xref_stream_entry_count", &entry_count)?;
    let maximum_entries = slf.getattr("_xref_stream_maximum_entries")?;
    if entry_count.gt(&maximum_entries)? {
        return Err(errors::limit_reached_error(
            py,
            &format!(
                "Maximum XRef stream entry limit reached: {} > {}.",
                str_of(&entry_count)?,
                str_of(&maximum_entries)?
            ),
        ));
    }

    let stream_data = bytes_io(py, &xref_stream.call_method0("get_data")?)?;
    // Do not keep the decoded data cached, as the stream remains in resolved_objects.
    if is_instance_of_module(py, &xref_stream, "pypdf.generic", "EncodedStreamObject")? {
        xref_stream.setattr("decoded_self", py_none(py))?;
    }

    let stream_data_py: Py<PyAny> = stream_data.clone().unbind();
    let sizes: Vec<Py<PyAny>> = entry_sizes
        .try_iter()?
        .map(|item| item.map(|value| value.unbind()))
        .collect::<PyResult<Vec<_>>>()?;
    let convert_to_int = shim_attr(py, "convert_to_int")?.unbind();
    let get_entry = PyCFunction::new_closure(
        py,
        Some(c"get_entry"),
        None,
        move |args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>| {
            Python::attach(|py| -> PyResult<Py<PyAny>> {
                let index: usize = args.get_item(0)?.extract()?;
                let size = match sizes.get(index) {
                    Some(size) => size.bind(py).clone(),
                    None => return Err(PyIndexError::new_err("entry size index out of range")),
                };
                if size.gt(py_int(py, 0))? {
                    let data = stream_data_py.bind(py).call_method1("read", (&size,))?;
                    Ok(convert_to_int.bind(py).call1((data, size))?.unbind())
                } else if index == 0 {
                    Ok(py_int(py, 1).unbind())
                } else {
                    Ok(py_int(py, 0).unbind())
                }
            })
        },
    )?;
    let slf_py: Py<PyAny> = slf.clone().unbind();
    let used_before = PyCFunction::new_closure(
        py,
        Some(c"used_before"),
        None,
        move |args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>| {
            Python::attach(|py| -> PyResult<Py<PyAny>> {
                let num = args.get_item(0)?;
                let generation = args.get_item(1)?;
                let slf = slf_py.bind(py);
                let xref = slf.getattr("xref")?;
                let entries = xref.call_method1("get", (&generation, PyList::empty(py)))?;
                let used_before = entries.contains(&num)?
                    || slf.getattr("xref_objStm")?.contains(&num)?;
                Ok(py_bool(py, used_before).unbind())
            })
        },
    )?;

    // Iterate through each subsection
    slf.call_method1(
        "_read_xref_subsections",
        (&index_pairs, get_entry.into_any(), used_before.into_any()),
    )?;
    Ok(xref_stream)
}

// @pypdf: pypdf._reader.PdfReader._get_xref_issues
#[pyfunction]
fn reader_get_xref_issues(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    startxref: &Bound<'_, PyAny>,
) -> PyResult<i128> {
    let startxref: i128 = startxref.extract()?;
    if startxref == 0 {
        return Ok(4);
    }
    stream.call_method1("seek", (py_int(py, startxref - 1), 0i64))?;
    let mut line = bytes_of(&stream.call_method1("read", (1i64,))?)?;
    if line == b"j" {
        line = bytes_of(&stream.call_method1("read", (1i64,))?)?;
    }
    let in_whitespace = line.is_empty()
        || line == b"\r"
        || line == b"\n"
        || line == b" "
        || line == b"\t";
    if !in_whitespace {
        return Ok(1);
    }
    let xref = bytes_of(&stream.call_method1("read", (4i64,))?)?;
    if xref != b"xref" {
        // not a xref so check if it is an XREF object
        let mut line: Vec<u8> = Vec::new();
        loop {
            // Python `line in b"0123456789 \t"`: the empty bytes object is a
            // subsequence of any bytes, so the initial empty line matches.
            let matches = line.is_empty()
                || (line.len() == 1
                    && (line[0].is_ascii_digit() || line[0] == b' ' || line[0] == b'\t'));
            if !matches {
                break;
            }
            line = bytes_of(&stream.call_method1("read", (1i64,))?)?;
            if line.is_empty() {
                return Ok(2);
            }
        }
        let extra = bytes_of(&stream.call_method1("read", (2i64,))?)?;
        let mut combined = line;
        combined.extend_from_slice(&extra);
        if !combined.eq_ignore_ascii_case(b"obj") {
            return Ok(3);
        }
    }
    Ok(0)
}

// @pypdf: pypdf._reader.PdfReader._find_pdf_objects
#[pyfunction]
fn reader_find_pdf_objects<'py>(
    py: Python<'py>,
    _cls: &Bound<'py, PyAny>,
    data: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let data = bytes_of(data)?;
    let indirect_class = generic_attr(py, "IndirectObject")?;
    let max_part: i128 = indirect_class.getattr("_MAXIMUM_PART_LENGTH")?.extract()?;
    let int = int_class(py)?;
    let mut result: Vec<Bound<'py, PyAny>> = Vec::new();
    let mut index: usize = 0;
    while let Some(found) = find_subslice(&data, b" obj", index) {
        index = found + 4;

        let mut index_before_space = found as i128 - 1;

        // Skip whitespace backwards
        while index_before_space >= 0
            && is_whitespace(data[index_before_space as usize])
        {
            index_before_space -= 1;
        }

        // Read generation number
        let generation_end = index_before_space + 1;
        while index_before_space >= 0
            && data[index_before_space as usize].is_ascii_digit()
        {
            index_before_space -= 1;
        }
        let generation_start = index_before_space + 1;

        // Skip whitespace
        while index_before_space >= 0
            && is_whitespace(data[index_before_space as usize])
        {
            index_before_space -= 1;
        }

        // Read object number
        let object_end = index_before_space + 1;
        while index_before_space >= 0
            && data[index_before_space as usize].is_ascii_digit()
        {
            index_before_space -= 1;
        }
        let object_start = index_before_space + 1;

        // Validate
        if object_start < object_end
            && object_end - object_start <= max_part
            && generation_start < generation_end
            && generation_end - generation_start <= max_part
        {
            let object_bytes = &data[object_start as usize..object_end as usize];
            let generation_bytes = &data[generation_start as usize..generation_end as usize];
            let object_number = int.call1((py_bytes(py, object_bytes),))?;
            let generation_number = int.call1((py_bytes(py, generation_bytes),))?;
            result.push(py_tuple(
                py,
                vec![
                    object_number,
                    generation_number,
                    py_int(py, object_start),
                ],
            )?);
        }
    }
    py_list(py, result)
}

// @pypdf: pypdf._reader.PdfReader._find_pdf_trailers
#[pyfunction]
fn reader_find_pdf_trailers<'py>(
    py: Python<'py>,
    _cls: &Bound<'py, PyAny>,
    data: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let data = bytes_of(data)?;
    let data_length = data.len();
    let mut result: Vec<Bound<'py, PyAny>> = Vec::new();
    let mut index: usize = 0;
    while let Some(found) = find_subslice(&data, b"trailer", index) {
        index = found + 7;
        let mut index_after_trailer = found + 7; // len(b"trailer")

        // Skip whitespace
        while index_after_trailer < data_length
            && is_whitespace(data[index_after_trailer])
        {
            index_after_trailer += 1;
        }

        // Must be dictionary start
        if index_after_trailer + 1 < data_length
            && &data[index_after_trailer..index_after_trailer + 2] == b"<<"
        {
            result.push(py_int(py, index_after_trailer as i128)); // offset of '<<'
        }
    }
    py_list(py, result)
}

// @pypdf: pypdf._reader.PdfReader._rebuild_xref_table
#[pyfunction]
fn reader_rebuild_xref_table<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    slf.setattr("xref", PyDict::new(py))?;
    stream.call_method1("seek", (0i64, 0i64))?;
    let stream_data = stream.call_method1("read", (-1i64,))?;

    let xref = slf.getattr("xref")?;
    for item in slf
        .call_method1("_find_pdf_objects", (&stream_data,))?
        .try_iter()?
    {
        let item = item?;
        let object_number = item.get_item(0)?;
        let generation_number = item.get_item(1)?;
        let object_start = item.get_item(2)?;
        if !xref.contains(&generation_number)? {
            xref.set_item(&generation_number, PyDict::new(py))?;
        }
        xref.get_item(&generation_number)?
            .set_item(&object_number, object_start)?;
    }

    warn(py, "parsing for Object Streams")?;
    // PDF 1.5+ files may carry the trailer keys inside a cross-reference
    // stream instead of behind a `trailer` keyword. Collect them here, keyed
    // by their offset, to merge them below in file order.
    let mut xref_stream_trailers: Vec<(i128, Bound<'py, PyAny>)> = Vec::new();
    let trailer_keys = slf.getattr("_XREF_STREAM_TRAILER_KEYS")?;
    let name_class = generic_attr(py, "NameObject")?;
    let int = int_class(py)?;
    let indirect_class = generic_attr(py, "IndirectObject")?;
    let max_bytes = indirect_class.getattr("_MAXIMUM_PART_LENGTH")?;
    let read_until_whitespace = utils_attr(py, "read_until_whitespace")?;
    let skip_over_whitespace = utils_attr(py, "skip_over_whitespace")?;
    let read_object = shim_attr(py, "read_object")?;
    let strict = slf.getattr("strict")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("max_bytes", &max_bytes)?;
    kwargs.set_item("strict", py_bool(py, strict.is_truthy()?))?;
    for generation_item in xref.call_method0("items")?.try_iter()? {
        let generation_item = generation_item?;
        let generation_number = generation_item.get_item(0)?;
        let entries = generation_item.get_item(1)?;
        let object_numbers: Vec<Bound<'py, PyAny>> = entries
            .call_method0("keys")?
            .try_iter()?
            .collect::<PyResult<Vec<_>>>()?;
        for object_number in object_numbers {
            let object_start: i128 = entries.get_item(&object_number)?.extract()?;
            stream.call_method1("seek", (py_int(py, object_start), 0i64))?;
            let body = (|| -> PyResult<()> {
                let _ = slf.call_method1("read_object_header", (stream,))?;
                let obj = read_object.call1((stream, slf))?;
                let object_type = obj.call_method1("get", (py_str(py, "/Type"), py_str(py, "")))?;
                if object_type.eq(py_str(py, "/XRef"))? {
                    let trailer = dictionary_object(py)?;
                    for key in trailer_keys.try_iter()? {
                        let key = key?;
                        if obj.contains(&key)? {
                            trailer.set_item(
                                name_class.call1((&key,))?,
                                obj.getattr("raw_get")?.call1((&key,))?,
                            )?;
                        }
                    }
                    xref_stream_trailers.push((object_start, trailer));
                    return Ok(());
                }
                if !object_type.eq(py_str(py, "/ObjStm"))? {
                    return Ok(());
                }
                let object_stream = bytes_io(py, &obj.call_method0("get_data")?)?;
                let mut actual_count: i128 = 0;
                loop {
                    let current =
                        read_until_whitespace.call((&object_stream,), Some(&kwargs))?;
                    let current_bytes = bytes_of(&current)?;
                    if !bytes_isdigit(&current_bytes) {
                        break;
                    }
                    let inner_object_number = int.call1((&current,))?;
                    skip_over_whitespace.call1((&object_stream,))?;
                    object_stream.call_method1("seek", (-1i64, 1i64))?;
                    let current =
                        read_until_whitespace.call((&object_stream,), Some(&kwargs))?;
                    let current_bytes = bytes_of(&current)?;
                    if !bytes_isdigit(&current_bytes) {
                        break;
                    }
                    let inner_offset = int.call1((&current,))?;
                    slf.getattr("xref_objStm")?.set_item(
                        &inner_object_number,
                        py_tuple(py, vec![object_number.clone(), inner_offset])?,
                    )?;
                    actual_count += 1;
                }
                let expected_count = obj.get_item(py_str(py, "/N"))?;
                if !py_int(py, actual_count).eq(&expected_count)? {
                    warn_values(
                        py,
                        "found %(actual_count)d objects within \
                         Object(%(object_number)d,%(generation_number)d) \
                         whereas %(expected)d expected",
                        vec![
                            ("actual_count", py_int(py, actual_count)),
                            ("object_number", object_number.clone()),
                            ("generation_number", generation_number.clone()),
                            ("expected", expected_count),
                        ],
                    )?;
                }
                Ok(())
            })();
            if let Err(error) = body {
                if error.is_instance_of::<PyMemoryError>(py) {
                    return Err(error);
                }
                if is_instance_of_module(py, error.value(py).as_any(), "pypdf.errors", "LimitReachedError")?
                {
                    // Do not let the broad recovery below bypass the token-length limit.
                    return Err(error);
                }
                // could be multiple causes
            }
        }
    }

    stream.call_method1("seek", (0i64, 0i64))?;
    let mut trailers: Vec<(i128, Bound<'py, PyAny>)> = xref_stream_trailers;
    for position in slf
        .call_method1("_find_pdf_trailers", (&stream_data,))?
        .try_iter()?
    {
        let position: i128 = position?.extract()?;
        stream.call_method1("seek", (py_int(py, position), 0i64))?;
        let trailer = read_object.call1((stream, slf))?;
        trailers.push((position, trailer));
    }
    // Here, we are parsing the file from start to end, the new data have to erase the existing.
    trailers.sort_by_key(|(position, _)| *position);
    let trailer = slf.getattr("trailer")?;
    for (_, new_trailer) in trailers {
        for item in new_trailer.call_method0("items")?.try_iter()? {
            let item = item?;
            trailer.set_item(item.get_item(0)?, item.get_item(1)?)?;
        }
    }
    Ok(())
}

// @pypdf: pypdf._reader.PdfReader._read_xref_subsections
#[pyfunction]
fn reader_read_xref_subsections(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    idx_pairs: &Bound<'_, PyAny>,
    get_entry: &Bound<'_, PyAny>,
    used_before: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let strict = slf.getattr("strict")?.is_truthy()?;
    for pair in slf.call_method1("_pairs", (idx_pairs,))?.try_iter()? {
        let pair = pair?;
        let start: i128 = pair.get_item(0)?.extract()?;
        let size: i128 = pair.get_item(1)?.extract()?;
        // The subsections must increase; saturating arithmetic keeps a crafted
        // section from overflowing (upstream Python ints cannot overflow).
        let end = start.saturating_add(size);
        // The subsections must increase
        for num in start..end {
            let num = py_int(py, num);
            // The first entry is the type
            let xref_type = get_entry.call1((0i64,))?;
            let xref_type_int: Option<i128> = xref_type.extract().ok();
            match xref_type_int {
                Some(0) => {
                    // linked list of free objects
                    let _next_free_object = get_entry.call1((1i64,))?;
                    let _next_generation = get_entry.call1((2i64,))?;
                }
                Some(1) => {
                    // objects that are in use but are not compressed
                    let byte_offset = get_entry.call1((1i64,))?;
                    let generation = get_entry.call1((2i64,))?;
                    let xref = slf.getattr("xref")?;
                    if !xref.contains(&generation)? {
                        xref.set_item(&generation, PyDict::new(py))?;
                    }
                    if !used_before
                        .call1((num.clone(), generation.clone()))?
                        .is_truthy()?
                    {
                        xref.get_item(&generation)?.set_item(&num, byte_offset)?;
                    }
                }
                Some(2) => {
                    // compressed objects
                    let objstr_num = get_entry.call1((1i64,))?;
                    let obstr_idx = get_entry.call1((2i64,))?;
                    // PDF spec table 18, generation is 0
                    let generation = py_int(py, 0);
                    if !used_before
                        .call1((num.clone(), generation))?
                        .is_truthy()?
                    {
                        slf.getattr("xref_objStm")?.set_item(
                            &num,
                            py_tuple(py, vec![objstr_num, obstr_idx])?,
                        )?;
                    }
                }
                _ => {
                    if strict {
                        return Err(errors::pdf_read_error(
                            py,
                            &format!("Unknown xref type: {}", str_of(&xref_type)?),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

// @pypdf: pypdf._reader.PdfReader._pairs
#[pyfunction]
fn reader_pairs<'py>(
    py: Python<'py>,
    _slf: &Bound<'py, PyAny>,
    array: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut result: Vec<Bound<'py, PyAny>> = Vec::new();
    let length = array.len()?;
    let mut index = 0usize;
    while index + 1 < length {
        result.push(py_tuple(
            py,
            vec![array.get_item(index)?, array.get_item(index + 1)?],
        )?);
        index += 2;
    }
    py_list(py, result)
}

// ---------------------------------------------------------------------------
// Encryption / metadata
// ---------------------------------------------------------------------------

// @pypdf: pypdf._reader.PdfReader.decrypt
#[pyfunction]
fn reader_decrypt<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    password: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let encryption = slf.getattr("_encryption")?;
    if !encryption.is_truthy()? {
        return Err(errors::pdf_read_error(py, "Not encrypted file"));
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("strict", strict_flag(py, slf)?)?;
    encryption.call_method("verify", (password,), Some(&kwargs))
}

// @pypdf: pypdf._reader.PdfReader.is_encrypted
#[pyfunction]
fn reader_is_encrypted(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<bool> {
    slf.getattr("trailer")?.contains(py_str(py, "/Encrypt"))
}

// @pypdf: pypdf._reader.PdfReader.add_form_topname
#[pyfunction]
fn reader_add_form_topname<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    name: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let catalog = slf.getattr("root_object")?;
    let acroform_key = py_str(py, "/AcroForm");
    if !catalog.contains(&acroform_key)? {
        return Ok(py_none(py));
    }
    let acroform_value = catalog.get_item(&acroform_key)?;
    if !is_instance(&acroform_value, &generic_attr(py, "DictionaryObject")?)? {
        return Ok(py_none(py));
    }
    let acroform = acroform_value;
    let fields_key = py_str(py, "/Fields");
    if !acroform.contains(&fields_key)? {
        // TODO: No error but this may be extended for XFA Forms
        return Ok(py_none(py));
    }

    let name_class = generic_attr(py, "NameObject")?;
    let interim = dictionary_object(py)?;
    interim.set_item(
        name_class.call1((py_str(py, "/T"),))?,
        generic_attr(py, "TextStringObject")?.call1((name,))?,
    )?;
    let kids_key = name_class.call1((py_str(py, "/Kids"),))?;
    interim.set_item(&kids_key, acroform.get_item(&fields_key)?)?;

    // Allocate the new object_id beyond the source-file range and all cached object IDs.
    let size = slf
        .getattr("trailer")?
        .call_method1("get", (py_str(py, "/Size"), py_int(py, 0)))?;
    let mut object_id: i128 = size.extract::<i128>()? - 1;
    let mut maximum: Option<i128> = None;
    for entries in slf.getattr("xref")?.call_method0("values")?.try_iter()? {
        for idnum in entries?.call_method0("keys")?.try_iter()? {
            let value: i128 = idnum?.extract()?;
            maximum = Some(maximum.map_or(value, |current: i128| current.max(value)));
        }
    }
    object_id = object_id.max(maximum.unwrap_or(0));
    let mut maximum_objstm: Option<i128> = None;
    for key in slf.getattr("xref_objStm")?.call_method0("keys")?.try_iter()? {
        let value: i128 = key?.extract()?;
        maximum_objstm = Some(maximum_objstm.map_or(value, |current: i128| current.max(value)));
    }
    object_id = object_id.max(maximum_objstm.unwrap_or(0));
    let mut maximum_cached: Option<i128> = None;
    for key in slf
        .getattr("resolved_objects")?
        .call_method0("keys")?
        .try_iter()?
    {
        let key = key?;
        let value: i128 = key.get_item(1)?.extract()?;
        maximum_cached = Some(maximum_cached.map_or(value, |current: i128| current.max(value)));
    }
    object_id = object_id.max(maximum_cached.unwrap_or(0));
    let object_id = py_int(py, object_id + 1);
    slf.call_method1(
        "cache_indirect_object",
        (py_int(py, 0), object_id, &interim),
    )?;
    let array_class = generic_attr(py, "ArrayObject")?;
    let array = array_class.call0()?;
    array.call_method1("append", (interim.getattr("indirect_reference")?,))?;
    let fields_key_obj = name_class.call1((py_str(py, "/Fields"),))?;
    acroform.set_item(&fields_key_obj, &array)?;
    for item in interim.get_item(&kids_key)?.try_iter()? {
        let item = item?;
        let obj = item.call_method0("get_object")?;
        let parent_key = name_class.call1((py_str(py, "/Parent"),))?;
        if obj.contains(&parent_key)? {
            warn_values(
                py,
                "Top Level Form Field %(obj_ref)s has a non-expected parent",
                vec![("obj_ref", obj.getattr("indirect_reference")?)],
            )?;
        }
        obj.set_item(&parent_key, interim.getattr("indirect_reference")?)?;
    }
    Ok(interim)
}

// @pypdf: pypdf._reader.PdfReader.rename_form_topname
#[pyfunction]
fn reader_rename_form_topname<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    name: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let catalog = slf.getattr("root_object")?;
    let acroform_key = py_str(py, "/AcroForm");
    if !catalog.contains(&acroform_key)? {
        return Ok(py_none(py));
    }
    let acroform_value = catalog.get_item(&acroform_key)?;
    if !is_instance(&acroform_value, &generic_attr(py, "DictionaryObject")?)? {
        return Ok(py_none(py));
    }
    let acroform = acroform_value;
    let fields_key = py_str(py, "/Fields");
    if !acroform.contains(&fields_key)? {
        return Ok(py_none(py));
    }

    let interim = acroform
        .get_item(&fields_key)?
        .get_item(0)?
        .call_method0("get_object")?;
    let name_class = generic_attr(py, "NameObject")?;
    interim.set_item(
        name_class.call1((py_str(py, "/T"),))?,
        generic_attr(py, "TextStringObject")?.call1((name,))?,
    )?;
    Ok(interim)
}

// @pypdf: pypdf._reader.PdfReader._repr_mimebundle_
#[pyfunction]
#[pyo3(signature = (slf, include=None, exclude=None))]
fn reader_repr_mimebundle<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    include: Option<&Bound<'py, PyAny>>,
    exclude: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let stream = slf.getattr("stream")?;
    stream.call_method1("seek", (0i64,))?;
    let pdf_data = stream.call_method0("read")?;
    let mut data = PyDict::new(py);
    data.set_item("application/pdf", pdf_data)?;

    if let Some(include) = include {
        // Filter representations based on include list
        if !include.is_none() {
            let filtered = PyDict::new(py);
            for (key, value) in data.iter() {
                if include.contains(&key)? {
                    filtered.set_item(key, value)?;
                }
            }
            data = filtered;
        }
    }
    if let Some(exclude) = exclude {
        // Remove representations based on exclude list
        if !exclude.is_none() {
            let filtered = PyDict::new(py);
            for (key, value) in data.iter() {
                if !exclude.contains(&key)? {
                    filtered.set_item(key, value)?;
                }
            }
            data = filtered;
        }
    }
    Ok(data.into_any())
}

// @pypdf: pypdf._reader.PdfReader._get_named_destinations
#[pyfunction]
#[pyo3(signature = (slf, tree=None, retval=None, visited=None, depth=None, traversal_state=None))]
fn reader_get_named_destinations<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    tree: Option<&Bound<'py, PyAny>>,
    retval: Option<&Bound<'py, PyAny>>,
    visited: Option<&Bound<'py, PyAny>>,
    depth: Option<&Bound<'py, PyAny>>,
    traversal_state: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let tree_truthy = match tree {
        Some(tree) => tree.is_truthy()?,
        None => false,
    };
    let retval_truthy = match retval {
        Some(retval) => retval.is_truthy()?,
        None => false,
    };
    let base = py
        .import("pypdf._doc_common")?
        .getattr("PdfDocCommon")?
        .getattr("_get_named_destinations")?;
    if tree_truthy || retval_truthy {
        let kwargs = PyDict::new(py);
        kwargs.set_item(
            "tree",
            match tree {
                Some(tree) => tree.clone(),
                None => py_none(py),
            },
        )?;
        kwargs.set_item(
            "retval",
            match retval {
                Some(retval) => retval.clone(),
                None => py_none(py),
            },
        )?;
        kwargs.set_item(
            "visited",
            match visited {
                Some(visited) => visited.clone(),
                None => py_none(py),
            },
        )?;
        kwargs.set_item(
            "depth",
            match depth {
                Some(depth) => depth.clone(),
                None => py_int(py, 0),
            },
        )?;
        kwargs.set_item(
            "traversal_state",
            match traversal_state {
                Some(state) => state.clone(),
                None => py_none(py),
            },
        )?;
        return base.call((slf,), Some(&kwargs));
    }

    let cache = slf.getattr("_named_destinations_cache")?;
    if cache.is_none() {
        let destinations = base.call1((slf,))?;
        slf.setattr("_named_destinations_cache", destinations.clone())?;
        return Ok(destinations);
    }
    Ok(cache)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(reader_init, module)?)?;
    module.add_function(wrap_pyfunction!(reader_initialize_stream, module)?)?;
    module.add_function(wrap_pyfunction!(reader_handle_encryption, module)?)?;
    module.add_function(wrap_pyfunction!(reader_enter, module)?)?;
    module.add_function(wrap_pyfunction!(reader_exit, module)?)?;
    module.add_function(wrap_pyfunction!(reader_close, module)?)?;
    module.add_function(wrap_pyfunction!(reader_root_object, module)?)?;
    module.add_function(wrap_pyfunction!(reader_info, module)?)?;
    module.add_function(wrap_pyfunction!(reader_id, module)?)?;
    module.add_function(wrap_pyfunction!(reader_pdf_header, module)?)?;
    module.add_function(wrap_pyfunction!(reader_xmp_metadata, module)?)?;
    module.add_function(wrap_pyfunction!(
        reader_get_page_number_by_indirect,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(reader_get_object_from_stream, module)?)?;
    module.add_function(wrap_pyfunction!(reader_get_object, module)?)?;
    module.add_function(wrap_pyfunction!(reader_read_object_header, module)?)?;
    module.add_function(wrap_pyfunction!(
        reader_cache_get_indirect_object,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(reader_cache_indirect_object, module)?)?;
    module.add_function(wrap_pyfunction!(reader_replace_object, module)?)?;
    module.add_function(wrap_pyfunction!(reader_read, module)?)?;
    module.add_function(wrap_pyfunction!(reader_basic_validation, module)?)?;
    module.add_function(wrap_pyfunction!(reader_find_eof_marker, module)?)?;
    module.add_function(wrap_pyfunction!(reader_find_startxref_pos, module)?)?;
    module.add_function(wrap_pyfunction!(
        reader_is_valid_startxref_value,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        reader_find_previous_startxref_pos,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(reader_load_recovery_cache, module)?)?;
    module.add_function(wrap_pyfunction!(
        reader_read_standard_xref_table_handle_size,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(reader_read_standard_xref_table, module)?)?;
    module.add_function(wrap_pyfunction!(
        reader_read_xref_tables_and_trailers,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(reader_process_xref_stream, module)?)?;
    module.add_function(wrap_pyfunction!(reader_read_xref, module)?)?;
    module.add_function(wrap_pyfunction!(reader_read_xref_other_error, module)?)?;
    module.add_function(wrap_pyfunction!(
        reader_sanitize_pdf15_xref_stream_index_pairs,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(reader_read_pdf15_xref_stream, module)?)?;
    module.add_function(wrap_pyfunction!(reader_get_xref_issues, module)?)?;
    module.add_function(wrap_pyfunction!(reader_find_pdf_objects, module)?)?;
    module.add_function(wrap_pyfunction!(reader_find_pdf_trailers, module)?)?;
    module.add_function(wrap_pyfunction!(reader_rebuild_xref_table, module)?)?;
    module.add_function(wrap_pyfunction!(reader_read_xref_subsections, module)?)?;
    module.add_function(wrap_pyfunction!(reader_pairs, module)?)?;
    module.add_function(wrap_pyfunction!(reader_decrypt, module)?)?;
    module.add_function(wrap_pyfunction!(reader_is_encrypted, module)?)?;
    module.add_function(wrap_pyfunction!(reader_add_form_topname, module)?)?;
    module.add_function(wrap_pyfunction!(reader_rename_form_topname, module)?)?;
    module.add_function(wrap_pyfunction!(reader_repr_mimebundle, module)?)?;
    module.add_function(wrap_pyfunction!(
        reader_get_named_destinations,
        module
    )?)?;
    Ok(())
}
