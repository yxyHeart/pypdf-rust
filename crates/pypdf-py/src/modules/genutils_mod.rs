//! Rust port of `pypdf/generic/_utils.py` (upstream pypdf 7.1.1).
//!
//! All logic lives here; `python/pypdf/generic/_utils.py` keeps the upstream
//! module surface (imports, `_PDFDOC_DECODE_MAP` / `__ESCAPE_DICT__` /
//! `__BACKSLASH_CODE__` constants, the `StreamParsingBudget` dataclass and
//! one-line delegating functions).  The classes used to build string objects
//! (`TextStringObject` / `ByteStringObject`) and the module-level helpers
//! (`read_non_whitespace`, `create_string_object`, ...) are looked up in the
//! `pypdf.generic._utils` module namespace at call time, exactly like the
//! upstream globals, so monkeypatching keeps working.
//!
//! The stream readers deliberately drive the Python stream object through its
//! `read` / `seek` / `tell` methods (one byte at a time), matching upstream
//! control flow byte for byte, including the octal-escape overflow rule.

use pyo3::exceptions::{PyKeyError, PyTypeError, PyUnicodeDecodeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyInt, PyList, PyModule, PyString, PyTuple};

use crate::errors;
use crate::pyutil;

/// Upstream module name; used as `logger_warning(..., source=__name__)`.
const SOURCE: &str = "pypdf.generic._utils";

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    crate::pyutil::cached_module(py, "builtins")
}

/// The `pypdf.generic._utils` module; upstream reads/writes these globals.
fn utils_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("pypdf.generic._utils")
}

/// A module-level global of `pypdf.generic._utils` (class, constant, function).
fn utils_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    utils_module(py)?.getattr(name)
}

fn py_bytes<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, data).into_any()
}

/// `obj[start:stop]` with integer bounds (missing bounds mean `None`).
fn slice_range<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    start: Option<i64>,
    stop: Option<i64>,
) -> PyResult<Bound<'py, PyAny>> {
    let start = start.map(|value| PyInt::new(py, value).into_any());
    let stop = stop.map(|value| PyInt::new(py, value).into_any());
    let slice = pyutil::py_slice(py, start.as_ref(), stop.as_ref(), None)?;
    obj.get_item(&slice)
}

/// `pypdf.errors.STREAM_TRUNCATED_PREMATURELY` (the upstream constant).
fn stream_truncated_message(py: Python<'_>) -> PyResult<String> {
    py.import("pypdf.errors")?
        .getattr("STREAM_TRUNCATED_PREMATURELY")?
        .extract()
}

fn stream_truncated_error(py: Python<'_>) -> PyResult<PyErr> {
    Ok(errors::pdf_stream_error(py, &stream_truncated_message(py)?))
}

/// `low <= token <= high` for a `bytes` token (Python comparison semantics).
fn token_between(
    py: Python<'_>,
    token: &Bound<'_, PyAny>,
    low: &[u8],
    high: &[u8],
) -> PyResult<bool> {
    let low = py_bytes(py, low);
    let high = py_bytes(py, high);
    Ok(low.le(token.clone())? && token.le(high)?)
}

/// `token in needle` for `bytes` (a substring test, like upstream).
fn token_in(py: Python<'_>, token: &Bound<'_, PyAny>, needle: &[u8]) -> PyResult<bool> {
    py_bytes(py, needle)
        .call_method1("__contains__", (token.clone(),))?
        .is_truthy()
}

/// `ord(tok)` for the one-byte tokens returned by `stream.read(1)`.
fn ord_token(py: Python<'_>, token: &Bound<'_, PyAny>) -> PyResult<u8> {
    let raw: Vec<u8> = token.extract()?;
    if raw.len() == 1 {
        return Ok(raw[0]);
    }
    // Upstream calls the builtin, so multi-byte/empty tokens raise CPython's
    // exact `TypeError` (e.g. "ord() expected a character, but string of
    // length 2 found"); real streams never take this path.
    builtins(py)?
        .getattr("ord")?
        .call1((token.clone(),))?
        .extract()
}

/// `int(token, base)` with an arbitrary-precision result.
fn parse_int_base(py: Python<'_>, token: &[u8], base: u32) -> PyResult<i64> {
    builtins(py)?
        .getattr("int")?
        .call1((py_bytes(py, token), base))?
        .extract()
}

/// The `forced_encoding` argument as a Python object (`None` when omitted).
fn forced_encoding_or_none<'py>(
    py: Python<'py>,
    forced_encoding: Option<&Bound<'py, PyAny>>,
) -> Bound<'py, PyAny> {
    match forced_encoding {
        Some(value) => value.clone(),
        None => py.None().into_bound(py),
    }
}

// ---------------------------------------------------------------------------
// hex_to_rgb
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._utils.hex_to_rgb
#[pyfunction]
fn genutils_hex_to_rgb<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    // `value.lstrip("#")[i : i + 2]` for `i in (0, 2, 4)`, then `int(..., 16)`.
    let stripped = value.call_method1("lstrip", ("#",))?;
    let int_type = builtins(py)?.getattr("int")?;
    let mut components: Vec<Bound<'py, PyAny>> = Vec::with_capacity(3);
    for (start, stop) in [(0_i64, 2_i64), (2, 4), (4, 6)] {
        let part = slice_range(py, &stripped, Some(start), Some(stop))?;
        let number = int_type.call1((part, 16))?;
        // `int / 255.0`: true division through the full binary-op protocol
        // (`int.__truediv__(float)` alone returns NotImplemented).
        components.push(number.div(255.0)?);
    }
    Ok(PyTuple::new(py, components)?.unbind().into())
}

// ---------------------------------------------------------------------------
// Hex strings
// ---------------------------------------------------------------------------

/// `int(digits, base=16)` for validated hexadecimal digits, with the
/// `bytearray.append()` range error upstream would get for oversized tokens.
fn hex_value(digits: &[u8]) -> PyResult<u8> {
    let mut value: u32 = 0;
    for digit in digits {
        let digit = match digit {
            b'0'..=b'9' => u32::from(digit - b'0'),
            b'a'..=b'f' => u32::from(digit - b'a') + 10,
            b'A'..=b'F' => u32::from(digit - b'A') + 10,
            _ => return Err(PyValueError::new_err("invalid hexadecimal digit")),
        };
        value = value * 16 + digit;
    }
    if value > 255 {
        // `bytearray.append()` raises exactly this message for out-of-range
        // values (only reachable for a non-conforming stream returning more
        // than one byte per `read(1)` call).
        return Err(PyValueError::new_err("byte must be in range(0, 256)"));
    }
    Ok(value as u8)
}

// @pypdf: pypdf.generic._utils.read_hex_string_from_stream
#[pyfunction]
#[pyo3(signature = (stream, forced_encoding=None))]
fn genutils_read_hex_string_from_stream<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
    forced_encoding: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    stream.call_method1("read", (1,))?;
    let read_non_whitespace = utils_attr(py, "read_non_whitespace")?;
    let mut arr: Vec<u8> = Vec::new();
    let mut x: Vec<u8> = Vec::new();
    loop {
        let tok = read_non_whitespace.call1((stream,))?;
        if !tok.is_truthy()? {
            return Err(stream_truncated_error(py)?);
        }
        if tok.eq(py_bytes(py, b">"))? {
            break;
        }
        let raw: Vec<u8> = tok.extract()?;
        // `tok not in b"0123456789abcdefABCDEF"` (a substring test upstream).
        if !raw.iter().all(|byte| byte.is_ascii_hexdigit()) {
            let message = format!(
                "Invalid hexadecimal character {} in hex string",
                pyutil::py_repr(&tok)?
            );
            return Err(errors::pdf_stream_error(py, &message));
        }
        x.extend_from_slice(&raw);
        if x.len() == 2 {
            arr.push(hex_value(&x)?);
            x.clear();
        }
    }
    if x.len() == 1 {
        x.push(b'0');
    }
    if !x.is_empty() {
        arr.push(hex_value(&x)?);
    }
    let string = py_bytes(py, &arr);
    let forced = forced_encoding_or_none(py, forced_encoding);
    Ok(utils_attr(py, "create_string_object")?
        .call1((string, forced))?
        .unbind())
}

// ---------------------------------------------------------------------------
// Literal strings
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._utils.read_string_from_stream
#[pyfunction]
#[pyo3(signature = (stream, forced_encoding=None))]
fn genutils_read_string_from_stream<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
    forced_encoding: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    stream.call_method1("read", (1,))?;
    let mut parens: i64 = 1;
    let mut txt: Vec<u8> = Vec::new();
    let escape_dict = utils_attr(py, "__ESCAPE_DICT__")?;
    let backslash_code: u8 = utils_attr(py, "__BACKSLASH_CODE__")?.extract()?;
    loop {
        let mut tok = stream.call_method1("read", (1,))?;
        if !tok.is_truthy()? {
            return Err(stream_truncated_error(py)?);
        }
        if tok.eq(py_bytes(py, b"("))? {
            parens += 1;
        } else if tok.eq(py_bytes(py, b")"))? {
            parens -= 1;
            if parens == 0 {
                break;
            }
        } else if tok.eq(py_bytes(py, b"\\"))? {
            tok = stream.call_method1("read", (1,))?;
            // `try: txt.append(__ESCAPE_DICT__[tok]) except KeyError: ...`
            match escape_dict.get_item(&tok) {
                Ok(value) => {
                    txt.push(value.extract()?);
                    continue;
                }
                Err(error) if error.is_instance_of::<PyKeyError>(py) => {}
                Err(error) => return Err(error),
            }
            if token_between(py, &tok, b"0", b"7")? {
                // "The number ddd may consist of one, two, or three octal
                // digits; high-order overflow shall be ignored." (PDF reference
                // 7.3.4.2, p 16)
                let saved: i64 = stream.call_method0("tell")?.extract::<i64>()? - 1;
                let mut octal: Vec<u8> = tok.extract()?;
                for _ in 0..2 {
                    let next = stream.call_method1("read", (1,))?;
                    if token_between(py, &next, b"0", b"7")? {
                        octal.extend_from_slice(&next.extract::<Vec<u8>>()?);
                    } else {
                        // `ntok` has to be analyzed again.
                        stream.call_method1("seek", (-1_i64, 1))?;
                        break;
                    }
                }
                let value = parse_int_base(py, &octal, 8)?;
                if value > 255 {
                    txt.push(backslash_code);
                    stream.call_method1("seek", (saved,))?;
                } else {
                    txt.push(value as u8);
                }
                continue;
            }
            if token_in(py, &tok, b"\n\r")? {
                // A backslash followed by a line break: consume the second
                // character of a multi-char EOL, then keep nothing.
                let next = stream.call_method1("read", (1,))?;
                if !token_in(py, &next, b"\n\r")? {
                    stream.call_method1("seek", (-1_i64, 1))?;
                }
                continue;
            }
            let values = PyDict::new(py);
            values.set_item(
                "token",
                tok.call_method("decode", ("utf-8", "ignore"), None)?,
            )?;
            pyutil::logger_warning(
                py,
                SOURCE,
                "Unexpected escaped string: %(token)s",
                Some(&values),
            )?;
            txt.push(backslash_code);
        }
        txt.push(ord_token(py, &tok)?);
    }
    let string = py_bytes(py, &txt);
    let forced = forced_encoding_or_none(py, forced_encoding);
    Ok(utils_attr(py, "create_string_object")?
        .call1((string, forced))?
        .unbind())
}

// ---------------------------------------------------------------------------
// Object creation / decoding
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._utils._create_unicode_string_object
#[pyfunction]
fn genutils_create_unicode_string_object<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let codecs = py.import("codecs")?;

    // UTF-8 text strings have been added with PDF 2.0, see ISO 32000-2:2020,
    // 7.9.2.2.1. Data which is not valid UTF-8 is still decoded as
    // PDFDocEncoding.
    let bom_utf8 = codecs.getattr("BOM_UTF8")?;
    if data
        .call_method1("startswith", (bom_utf8.clone(),))?
        .is_truthy()?
    {
        let bom_length: i64 = bom_utf8.len()?.try_into().map_err(|_| {
            PyValueError::new_err("UTF-8 BOM length does not fit in a Python index")
        })?;
        let trimmed = slice_range(py, data, Some(bom_length), None)?;
        match trimmed.call_method1("decode", ("utf-8",)) {
            Ok(decoded) => {
                let retval = utils_attr(py, "TextStringObject")?.call1((decoded,))?;
                retval.setattr("_original_bytes", data)?;
                return Ok(retval.unbind());
            }
            Err(error) if error.is_instance_of::<PyUnicodeDecodeError>(py) => {
                return Ok(py.None());
            }
            Err(error) => return Err(error),
        }
    }

    let bom_be = codecs.getattr("BOM_UTF16_BE")?;
    let bom_le = codecs.getattr("BOM_UTF16_LE")?;
    let utf16_boms = PyTuple::new(py, [bom_be.clone(), bom_le.clone()])?;
    if data
        .call_method1("startswith", (utf16_boms,))?
        .is_truthy()?
    {
        let decoded = data.call_method1("decode", ("utf-16",))?;
        let retval = utils_attr(py, "TextStringObject")?.call1((decoded,))?;
        retval.setattr("_original_bytes", data)?;
        retval.setattr("autodetect_utf16", true)?;
        retval.setattr("utf16_bom", slice_range(py, data, Some(0), Some(2))?)?;
        return Ok(retval.unbind());
    }

    if data
        .call_method1("startswith", (py_bytes(py, b"\x00"),))?
        .is_truthy()?
    {
        let decoded = data.call_method1("decode", ("utf-16be",))?;
        let retval = utils_attr(py, "TextStringObject")?.call1((decoded,))?;
        retval.setattr("_original_bytes", data)?;
        retval.setattr("autodetect_utf16", true)?;
        retval.setattr("utf16_bom", bom_be)?;
        return Ok(retval.unbind());
    }

    let second_byte = slice_range(py, data, Some(1), Some(2))?;
    if second_byte.eq(py_bytes(py, b"\x00"))? {
        let decoded = data.call_method1("decode", ("utf-16le",))?;
        let retval = utils_attr(py, "TextStringObject")?.call1((decoded,))?;
        retval.setattr("_original_bytes", data)?;
        retval.setattr("autodetect_utf16", true)?;
        retval.setattr("utf16_bom", bom_le)?;
        return Ok(retval.unbind());
    }

    Ok(py.None())
}

/// `TextStringObject(decode_pdfdocencoding(string))`.
fn pdfdoc_string_object<'py>(
    py: Python<'py>,
    string: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let decoded = utils_attr(py, "decode_pdfdocencoding")?.call1((string,))?;
    utils_attr(py, "TextStringObject")?.call1((decoded,))
}

// @pypdf: pypdf.generic._utils.create_string_object
#[pyfunction]
#[pyo3(signature = (string, forced_encoding=None))]
pub(crate) fn genutils_create_string_object<'py>(
    py: Python<'py>,
    string: &Bound<'py, PyAny>,
    forced_encoding: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let builtins = builtins(py)?;
    let str_type = builtins.getattr("str")?;
    let bytes_type = builtins.getattr("bytes")?;

    if string.is_instance(&str_type)? {
        return Ok(utils_attr(py, "TextStringObject")?
            .call1((string,))?
            .unbind());
    }

    if string.is_instance(&bytes_type)? {
        if let Some(forced) = forced_encoding {
            let list_type = builtins.getattr("list")?;
            let dict_type = builtins.getattr("dict")?;
            let is_list = forced.is_instance(&list_type)?;
            if is_list || forced.is_instance(&dict_type)? {
                let list_length: Option<usize> = if is_list { Some(forced.len()?) } else { None };
                let mut out: Vec<Bound<'py, PyAny>> = Vec::new();
                for item in string.try_iter()? {
                    let x = item?;
                    let use_forced = match list_length {
                        Some(length) => x.lt(length)?,
                        None => forced.contains(x.clone())?,
                    };
                    let value = if use_forced {
                        forced.get_item(&x)?
                    } else {
                        let byte: u8 = x.extract()?;
                        py_bytes(py, &[byte]).call_method1("decode", ("charmap",))?
                    };
                    out.push(value);
                }
                let joined =
                    PyString::new(py, "").call_method1("join", (PyList::new(py, out)?,))?;
                let retval = utils_attr(py, "TextStringObject")?.call1((joined,))?;
                retval.setattr("_original_bytes", string)?;
                return Ok(retval.unbind());
            }
            if forced.is_instance(&str_type)? {
                if forced.eq("bytes")? {
                    return Ok(utils_attr(py, "ByteStringObject")?
                        .call1((string,))?
                        .unbind());
                }
                let decoded = string.call_method1("decode", (forced,))?;
                let retval = utils_attr(py, "TextStringObject")?.call1((decoded,))?;
                retval.setattr("_original_bytes", string)?;
                return Ok(retval.unbind());
            }
        }

        // try:
        //     unicode_string = _create_unicode_string_object(string)
        //     ...
        // except UnicodeDecodeError:
        //     return ByteStringObject(string)
        match utils_attr(py, "_create_unicode_string_object")?.call1((string,)) {
            Ok(unicode_string) => {
                if !unicode_string.is_none() {
                    return Ok(unicode_string.unbind());
                }
                match pdfdoc_string_object(py, string) {
                    Ok(retval) => {
                        retval.setattr("_original_bytes", string)?;
                        retval.setattr("autodetect_pdfdocencoding", true)?;
                        return Ok(retval.unbind());
                    }
                    Err(error) if error.is_instance_of::<PyUnicodeDecodeError>(py) => {
                        return Ok(utils_attr(py, "ByteStringObject")?
                            .call1((string,))?
                            .unbind());
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) if error.is_instance_of::<PyUnicodeDecodeError>(py) => {
                return Ok(utils_attr(py, "ByteStringObject")?
                    .call1((string,))?
                    .unbind());
            }
            Err(error) => return Err(error),
        }
    }

    Err(PyTypeError::new_err(
        "create_string_object should have str or unicode arg",
    ))
}

// @pypdf: pypdf.generic._utils.decode_pdfdocencoding
#[pyfunction]
fn genutils_decode_pdfdocencoding<'py>(
    py: Python<'py>,
    byte_array: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let builtins = builtins(py)?;
    let raw = builtins.getattr("bytes")?.call1((byte_array,))?;
    // latin1 maps each byte to the code point of the same value, so translate()
    // applies the PDFDocEncoding table in C instead of a Python loop.
    let latin1 = raw.call_method1("decode", ("latin1",))?;
    let table = utils_attr(py, "_PDFDOC_DECODE_MAP")?;
    let retval = latin1.call_method1("translate", (table,))?;
    let nul = PyString::new(py, "\u{0}");
    let index: i64 = retval.call_method1("find", (nul,))?.extract()?;
    if index != -1 {
        let exception = builtins.getattr("UnicodeDecodeError")?.call1((
            "pdfdocencoding",
            raw.clone(),
            index,
            index + 1,
            "does not exist in translation table",
        ))?;
        return Err(PyErr::from_value(exception));
    }
    Ok(retval.unbind())
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(genutils_hex_to_rgb, module)?)?;
    module.add_function(wrap_pyfunction!(
        genutils_read_hex_string_from_stream,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(genutils_read_string_from_stream, module)?)?;
    module.add_function(wrap_pyfunction!(
        genutils_create_unicode_string_object,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(genutils_create_string_object, module)?)?;
    module.add_function(wrap_pyfunction!(genutils_decode_pdfdocencoding, module)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Symbols implemented in the hand-written shim
// (`python/pypdf/generic/_utils.py`).
//
// These are pure data definitions: upstream's module-level constants and the
// `StreamParsingBudget` dataclass.  The Rust logic above reads the constants
// from the module namespace, so the shim stays the single definition point.
//
// @pypdf: pypdf.generic._utils._PDFDOC_DECODE_MAP
// @pypdf: pypdf.generic._utils.__ESCAPE_DICT__
// @pypdf: pypdf.generic._utils.__BACKSLASH_CODE__
// @pypdf: pypdf.generic._utils.StreamParsingBudget
// @pypdf: pypdf.generic._utils.StreamParsingBudget.remaining_budget
