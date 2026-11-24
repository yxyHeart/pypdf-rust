//! Rust port of `pypdf/_cmap.py` (upstream pypdf 7.1.1).
//!
//! # Design
//!
//! The module has no classes: every module-level function is a `#[pyfunction]`
//! in this file, bound by the hand-written shim `python/pypdf/_cmap.py`.  The
//! module-level constants and the `_predefined_cmap` table stay Python
//! definitions (they are part of the observable module surface), and the
//! `_FontFileCharacterMap` container is reused from
//! `pypdf.generic._data_structures`.
//!
//! Upstream mixes Python byte manipulation (`bytes.replace`, `binascii`,
//! `str.decode("utf-16-be", "surrogatepass")`), integer arithmetic with
//! arbitrary precision and `fontTools` parsing.  This port performs the same
//! Python-level operations where their exact semantics are observable (error
//! messages, `OverflowError`, lone surrogates, `mock.patch` targets) and only
//! uses Rust for byte scanning where the result is byte-identical.  In
//! particular:
//!
//! * `binascii.unhexlify`, `int(token, 16)`, `bytes.decode` and
//!   `fontTools.cffLib.CFFFontSet` are reached through Python at call time, so
//!   exceptions (`binascii.Error`, `ValueError`, `OverflowError`) and
//!   monkey-patching behave exactly like upstream.
//! * `pypdf.generic._font.HAS_FONTTOOLS` is read lazily on every
//!   `_parse_to_unicode` call, like upstream's function-local import, so
//!   `mock.patch("pypdf.generic._font.HAS_FONTTOOLS", False)` is observed.
//! * The per-font-program cache is stored on the stream attribute
//!   `_font_file_character_map` as an upstream `_FontFileCharacterMap`.
//!
//! No Rust `HashMap`/`HashSet` participates in any output: insertion-ordered
//! Python dicts are used wherever upstream relies on dict order.

use pyo3::exceptions::{PyIndexError, PyKeyError, PyMemoryError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyList, PyModule, PyString, PyTuple};

use crate::errors;
use crate::pyutil;

/// Upstream module name; used as `logger_warning(..., source=__name__)`.
const SOURCE: &str = "pypdf._cmap";

/// `MAPPING_DICTIONARY_SIZE_LIMIT` (upstream module constant).
const MAPPING_DICTIONARY_SIZE_LIMIT: i64 = 100_000;

/// `MAX_CMAP_CODE_BYTES_LIMIT` = `MAX_CMAP_CODE_BYTES * 2` (upstream).
const MAX_CMAP_CODE_BYTES_LIMIT: i64 = 16;

/// `MAX_CMAP_STRING_BYTES_LIMIT` = `MAX_CMAP_STRING_BYTES * 2` (upstream).
const MAX_CMAP_STRING_BYTES_LIMIT: i64 = 1024;

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("builtins")
}

fn shim<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("pypdf._cmap")
}

/// Fetch a name from the module's Python shim (upstream references module
/// globals such as `_predefined_cmap`, `_glyph_name_to_unicode` and
/// `parse_bfrange` by name).
fn shim_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim(py)?.getattr(name)
}

fn generic_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf.generic")?.getattr(name)
}

fn builtin_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr(name)
}

fn py_int<'py>(py: Python<'py>, value: i64) -> Bound<'py, PyAny> {
    pyo3::types::PyInt::new(py, value).into_any()
}

fn py_none<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    py.None().into_bound(py)
}

fn py_str<'py>(py: Python<'py>, value: &str) -> Bound<'py, PyAny> {
    PyString::new(py, value).into_any()
}

fn py_bytes<'py>(py: Python<'py>, value: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, value).into_any()
}

fn py_tuple<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyTuple::new(py, items)?.into_any())
}

/// `isinstance(obj, <class>)`.
fn is_instance<'py>(obj: &Bound<'py, PyAny>, class: &Bound<'py, PyAny>) -> PyResult<bool> {
    obj.is_instance(class)
}

/// Emulate `logger_warning(message, source=__name__, **values)`.
fn logger_warning(
    py: Python<'_>,
    message: &str,
    values: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    pyutil::logger_warning(py, SOURCE, message, values)
}

/// Emulate `logger_error(message, source=__name__, **values)` from `pypdf._utils`.
fn logger_error(py: Python<'_>, message: &str, values: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let logger = py.import("logging")?.call_method1("getLogger", (SOURCE,))?;
    match values {
        Some(values) if !values.is_empty() => {
            logger.call_method1("error", (message, values))?;
        }
        _ => {
            logger.call_method1("error", (message,))?;
        }
    }
    Ok(())
}

fn limit_reached(py: Python<'_>, message: &str) -> PyErr {
    errors::limit_reached_error(py, message)
}

/// `binascii.unhexlify(value)`.
fn unhexlify<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    py.import("binascii")?.getattr("unhexlify")?.call1((value,))
}

/// `int(token, 16)`; accepts the leading `-` that CPython allows.
fn int_hex<'py>(py: Python<'py>, token: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("int")?.call1((token, 16))
}

/// `int(token)`.
fn int_decimal<'py>(py: Python<'py>, token: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("int")?.call1((token,))
}

/// `chr(value)`.
fn chr_value<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("chr")?.call1((value,))
}

// --- byte scanning helpers (byte-identical to the Python bytes methods) -----

fn is_bytes_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// `data.split()` for `bytes` (ASCII whitespace, empty tokens removed).
fn split_whitespace(data: &[u8]) -> Vec<&[u8]> {
    data.split(|byte| is_bytes_whitespace(*byte))
        .filter(|token| !token.is_empty())
        .collect()
}

/// `[token for token in data.split(b" ") if token]`.
fn split_space(data: &[u8]) -> Vec<&[u8]> {
    data.split(|byte| *byte == b' ')
        .filter(|token| !token.is_empty())
        .collect()
}

fn contains_sub(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn find_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// `bytes.replace(old, new)` (all non-overlapping occurrences).
fn replace_all(data: &[u8], old: &[u8], new: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(data.len());
    let mut position = 0;
    while position < data.len() {
        if !old.is_empty()
            && position + old.len() <= data.len()
            && &data[position..position + old.len()] == old
        {
            output.extend_from_slice(new);
            position += old.len();
        } else {
            output.push(data[position]);
            position += 1;
        }
    }
    output
}

/// `bytes.strip()` (no argument: ASCII whitespace on both ends).
fn strip_whitespace(data: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = data.len();
    while start < end && is_bytes_whitespace(data[start]) {
        start += 1;
    }
    while end > start && is_bytes_whitespace(data[end - 1]) {
        end -= 1;
    }
    &data[start..end]
}

/// `bytes.strip(chars)` for a set of single-byte characters.
fn strip_bytes<'a>(data: &'a [u8], chars: &'a [u8]) -> &'a [u8] {
    let mut start = 0;
    let mut end = data.len();
    while start < end && chars.contains(&data[start]) {
        start += 1;
    }
    while end > start && chars.contains(&data[end - 1]) {
        end -= 1;
    }
    &data[start..end]
}

/// `token_list[index]` with the upstream `IndexError` message.
fn token_at<'a>(tokens: &'a [&'a [u8]], index: usize) -> PyResult<&'a [u8]> {
    tokens
        .get(index)
        .copied()
        .ok_or_else(|| PyIndexError::new_err("list index out of range"))
}

// ---------------------------------------------------------------------------
// Size limits
// ---------------------------------------------------------------------------

// @pypdf: pypdf._cmap._check_mapping_size
#[pyfunction]
fn cmap_check_mapping_size<'py>(py: Python<'py>, size: &Bound<'py, PyAny>) -> PyResult<()> {
    check_mapping_size(py, size)
}

fn check_mapping_size<'py>(py: Python<'py>, size: &Bound<'py, PyAny>) -> PyResult<()> {
    if size.gt(MAPPING_DICTIONARY_SIZE_LIMIT)? {
        let rendered = pyutil::py_str(size)?;
        return Err(limit_reached(
            py,
            &format!(
                "Maximum /ToUnicode size limit reached: {rendered} > {MAPPING_DICTIONARY_SIZE_LIMIT}."
            ),
        ));
    }
    Ok(())
}

/// `_check_token_length(token, limit)` on a Python `bytes` object.
// @pypdf: pypdf._cmap._check_token_length
#[pyfunction]
fn cmap_check_token_length<'py>(
    py: Python<'py>,
    token: &Bound<'py, PyAny>,
    limit: i64,
) -> PyResult<()> {
    let token_length = token.len()? as i64;
    check_token_length_value(py, token_length, limit)
}

fn check_token_length_bytes(py: Python<'_>, token: &[u8], limit: i64) -> PyResult<()> {
    check_token_length_value(py, token.len() as i64, limit)
}

fn check_token_length_value(py: Python<'_>, token_length: i64, limit: i64) -> PyResult<()> {
    if token_length > limit {
        let description = if limit == MAX_CMAP_CODE_BYTES_LIMIT {
            "code"
        } else if limit == MAX_CMAP_STRING_BYTES_LIMIT {
            "string"
        } else {
            "token"
        };
        return Err(limit_reached(
            py,
            &format!("Maximum /ToUnicode {description} length exceeded: {token_length} > {limit}."),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ToUnicode CMap parsing
// ---------------------------------------------------------------------------

/// `__parse_bfrange__decode(map_dict, code)` implementation.
fn parse_bfrange_decode_impl<'py>(
    _py: Python<'py>,
    map_dict: &Bound<'py, PyAny>,
    code: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let width = map_dict.get_item(-1)?;
    let encoded = code.call_method1("to_bytes", (&width, "big"))?;
    let encoding = if width.eq(1)? { "charmap" } else { "utf-16-be" };
    encoded.call_method1("decode", (encoding, "surrogatepass"))
}

// @pypdf: pypdf._cmap.__parse_bfrange__decode
#[pyfunction]
fn cmap_parse_bfrange_decode<'py>(
    py: Python<'py>,
    map_dict: &Bound<'py, PyAny>,
    code: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(parse_bfrange_decode_impl(py, map_dict, code)?.unbind())
}

/// `decode_utf16 = partial(bytes.decode, encoding="utf-16-be", errors="surrogatepass")`.
fn decode_utf16<'py>(value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    value.call_method1("decode", ("utf-16-be", "surrogatepass"))
}

/// `_parse_to_unicode`'s mapping of destinations: `unhexlify(sq)` + `decode_utf16`.
fn decode_utf16_hex<'py>(py: Python<'py>, token: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    let raw = unhexlify(py, &py_bytes(py, token))?;
    decode_utf16(&raw)
}

/// `parse_bfrange(line, map_dict, int_entry, multiline_rg)` implementation.
fn parse_bfrange_impl<'py>(
    py: Python<'py>,
    line: &Bound<'py, PyAny>,
    map_dict: &Bound<'py, PyAny>,
    int_entry: &Bound<'py, PyAny>,
    multiline_rg: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let line_bytes: Vec<u8> = line.extract()?;
    let tokens = split_whitespace(&line_bytes);
    let mut closure_found = false;
    let mut entry_count = int_entry.len()? as i64;
    check_mapping_size(py, &py_int(py, entry_count))?;
    if !multiline_rg.is_none() {
        let mut a = multiline_rg.get_item(0)?;
        let b = multiline_rg.get_item(1)?;
        for token in &tokens {
            if *token == b"]" {
                closure_found = true;
                break;
            }
            check_token_length_bytes(py, token, MAX_CMAP_STRING_BYTES_LIMIT)?;
            entry_count += 1;
            check_mapping_size(py, &py_int(py, entry_count))?;
            // Python evaluates the assignment value before the subscript key.
            let destination = decode_utf16_hex(py, token)?;
            let key = parse_bfrange_decode_impl(py, map_dict, &a)?;
            map_dict.set_item(key, destination)?;
            int_entry.call_method1("append", (&a,))?;
            a = a.add(1)?;
        }
        if closure_found {
            return Ok(py_none(py));
        }
        return py_tuple(py, vec![a, b]);
    }

    let token_a = token_at(&tokens, 0)?;
    check_token_length_bytes(py, token_a, MAX_CMAP_CODE_BYTES_LIMIT)?;
    let token_b = token_at(&tokens, 1)?;
    check_token_length_bytes(py, token_b, MAX_CMAP_CODE_BYTES_LIMIT)?;
    let mut a = int_hex(py, &py_bytes(py, token_a))?;
    let b = int_hex(py, &py_bytes(py, token_b))?;
    if a.lt(0)? || b.lt(0)? {
        // int(token, 16) accepts a leading "-"; negative range bounds would leak
        // an OverflowError from the to_bytes encoding below.
        return Err(PyValueError::new_err(format!(
            "Negative bfrange source range: {} {}",
            pyutil::py_repr(&py_bytes(py, token_a))?,
            pyutil::py_repr(&py_bytes(py, token_b))?,
        )));
    }
    let nbi = token_a.len().max(token_b.len());
    map_dict.set_item(-1, py_int(py, nbi.div_ceil(2) as i64))?;

    let token_c = token_at(&tokens, 2)?;
    if token_c == b"[" {
        for token in tokens.iter().skip(3) {
            if *token == b"]" {
                closure_found = true;
                break;
            }
            check_token_length_bytes(py, token, MAX_CMAP_STRING_BYTES_LIMIT)?;
            entry_count += 1;
            check_mapping_size(py, &py_int(py, entry_count))?;
            let destination = decode_utf16_hex(py, token)?;
            let key = parse_bfrange_decode_impl(py, map_dict, &a)?;
            map_dict.set_item(key, destination)?;
            int_entry.call_method1("append", (&a,))?;
            a = a.add(1)?;
        }
    } else {
        check_token_length_bytes(py, token_c, MAX_CMAP_STRING_BYTES_LIMIT)?;
        let mut c = int_hex(py, &py_bytes(py, token_c))?;
        let width = 4.max(token_c.len() as i64);
        let fmt = py_bytes(py, b"%%0%dX").rem(width)?;
        closure_found = true;
        let difference = b.sub(&a)?;
        let range_size = difference.add(1)?;
        let range_size = if range_size.lt(0)? {
            py_int(py, 0)
        } else {
            range_size
        };
        let total = py_int(py, entry_count).add(&range_size)?;
        check_mapping_size(py, &total)?;
        while a.le(&b)? {
            let formatted = fmt.rem(&c)?;
            let destination = unhexlify(py, &formatted)?;
            check_token_length_value(py, destination.len()? as i64, MAX_CMAP_CODE_BYTES_LIMIT)?;
            let decoded = decode_utf16(&destination)?;
            let key = parse_bfrange_decode_impl(py, map_dict, &a)?;
            map_dict.set_item(key, decoded)?;
            int_entry.call_method1("append", (&a,))?;
            a = a.add(1)?;
            c = c.add(1)?;
        }
    }
    if closure_found {
        return Ok(py_none(py));
    }
    py_tuple(py, vec![a, b])
}

// @pypdf: pypdf._cmap.parse_bfrange
#[pyfunction]
fn cmap_parse_bfrange<'py>(
    py: Python<'py>,
    line: &Bound<'py, PyAny>,
    map_dict: &Bound<'py, PyAny>,
    int_entry: &Bound<'py, PyAny>,
    multiline_rg: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    parse_bfrange_impl(py, line, map_dict, int_entry, multiline_rg).map(Bound::unbind)
}

/// `parse_bfchar(line, map_dict, int_entry)` implementation.
fn parse_bfchar_impl<'py>(
    py: Python<'py>,
    line: &Bound<'py, PyAny>,
    map_dict: &Bound<'py, PyAny>,
    int_entry: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let line_bytes: Vec<u8> = line.extract()?;
    let tokens = split_space(&line_bytes);
    let lst_length = tokens.len();
    if lst_length == 0 {
        let values = PyDict::new(py);
        values.set_item("line", line)?;
        logger_warning(
            py,
            "Skipping broken line %(line)r: Line is empty.",
            Some(&values),
        )?;
        return Ok(());
    }
    if !lst_length.is_multiple_of(2) {
        let values = PyDict::new(py);
        values.set_item("line", line)?;
        logger_warning(
            py,
            "Ignoring final token of odd-length line %(line)r.",
            Some(&values),
        )?;
    }

    let new_count = (lst_length / 2) as i64;
    check_mapping_size(py, &py_int(py, int_entry.len()? as i64 + new_count))?;
    map_dict.set_item(-1, py_int(py, (tokens[0].len() / 2) as i64))?;

    let binascii_error = py.import("binascii")?.getattr("Error")?;
    for index in (0..lst_length.saturating_sub(1)).step_by(2) {
        let source = tokens[index];
        let destination = tokens[index + 1];
        check_token_length_bytes(py, source, MAX_CMAP_CODE_BYTES_LIMIT)?;

        let mut map_to = py_str(py, "");
        // placeholder (see above) means empty string
        if destination != b"." {
            check_token_length_bytes(py, destination, MAX_CMAP_STRING_BYTES_LIMIT)?;
            let destination_bytes = py_bytes(py, destination);
            match unhexlify(py, &destination_bytes) {
                Ok(raw) => {
                    let encoding = if destination.len() < 4 {
                        "charmap"
                    } else {
                        "utf-16-be"
                    };
                    map_to = raw.call_method1("decode", (encoding, "surrogatepass"))?;
                }
                Err(error) => {
                    if error
                        .value(py)
                        .is_instance(&binascii_error)
                        .unwrap_or(false)
                    {
                        let values = PyDict::new(py);
                        values.set_item("exception", error.value(py))?;
                        values.set_item("destination", &destination_bytes)?;
                        logger_warning(
                            py,
                            "Got invalid hex string: %(exception)s (%(destination)r)",
                            Some(&values),
                        )?;
                    } else {
                        return Err(error);
                    }
                }
            }
        }
        let width = map_dict.get_item(-1)?;
        let encoding = if width.eq(1)? { "charmap" } else { "utf-16-be" };
        let source_bytes = py_bytes(py, source);
        let key =
            unhexlify(py, &source_bytes)?.call_method1("decode", (encoding, "surrogatepass"))?;
        map_dict.set_item(key, map_to)?;
        int_entry.call_method1("append", (int_hex(py, &source_bytes)?,))?;
    }
    Ok(())
}

// @pypdf: pypdf._cmap.parse_bfchar
#[pyfunction]
fn cmap_parse_bfchar<'py>(
    py: Python<'py>,
    line: &Bound<'py, PyAny>,
    map_dict: &Bound<'py, PyAny>,
    int_entry: &Bound<'py, PyAny>,
) -> PyResult<()> {
    parse_bfchar_impl(py, line, map_dict, int_entry)
}

/// `prepare_cm(ft)` implementation.
fn prepare_cm_impl<'py>(py: Python<'py>, ft: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let stream_class = generic_class(py, "StreamObject")?;
    let tu = ft.get_item("/ToUnicode")?;
    let mut cm: Vec<u8> = if is_instance(&tu, &stream_class)? {
        let data = ft.get_item("/ToUnicode")?.call_method0("get_data")?;
        if is_instance(&data, &builtin_class(py, "str")?)? {
            data.call_method0("encode")?.extract()?
        } else {
            data.extract()?
        }
    } else {
        b"beginbfrange\n<0000> <0001> <0000>\nendbfrange".to_vec()
    };
    // we need to prepare cm before due to missing return line in pdf printed
    // to pdf from word
    cm = strip_whitespace(&cm).to_vec();
    for (from, to) in [
        (b"beginbfchar".as_slice(), b"\nbeginbfchar\n".as_slice()),
        (b"endbfchar".as_slice(), b"\nendbfchar\n".as_slice()),
        (b"beginbfrange".as_slice(), b"\nbeginbfrange\n".as_slice()),
        (b"endbfrange".as_slice(), b"\nendbfrange\n".as_slice()),
        (b"<<".as_slice(), b"\n{\n".as_slice()),
        (b">>".as_slice(), b"\n}\n".as_slice()),
    ] {
        cm = replace_all(&cm, from, to);
    }
    let mut pieces: Vec<Vec<u8>> = cm
        .split(|byte| *byte == b'<')
        .map(|piece| piece.to_vec())
        .collect();
    for piece in &mut pieces {
        if let Some(position) = find_sub(piece, b">") {
            let content = if position == 0 {
                // string is empty: stash a placeholder here (see below)
                // see https://github.com/py-pdf/pypdf/issues/1111
                b".".to_vec()
            } else {
                let head = &piece[..position];
                replace_all(head, b" ", b"")
            };
            let mut rebuilt = content;
            rebuilt.push(b' ');
            rebuilt.extend_from_slice(&piece[position + 1..]);
            *piece = rebuilt;
        }
    }
    let mut joined: Vec<u8> = Vec::new();
    for (index, piece) in pieces.iter().enumerate() {
        if index > 0 {
            joined.push(b' ');
        }
        joined.extend_from_slice(piece);
    }
    let mut cm = joined;
    cm = replace_all(&cm, b"[", b" [ ");
    cm = replace_all(&cm, b"]", b" ]\n ");
    cm = replace_all(&cm, b"\r", b"\n");
    Ok(py_bytes(py, &cm))
}

// @pypdf: pypdf._cmap.prepare_cm
#[pyfunction]
fn cmap_prepare_cm<'py>(py: Python<'py>, ft: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(prepare_cm_impl(py, ft)?.unbind())
}

/// `process_cm_line(...)` implementation.
fn process_cm_line_impl<'py>(
    py: Python<'py>,
    line: &Bound<'py, PyAny>,
    process_rg: bool,
    process_char: bool,
    multiline_rg: &Bound<'py, PyAny>,
    map_dict: &Bound<'py, PyAny>,
    int_entry: &Bound<'py, PyAny>,
) -> PyResult<(bool, bool, Bound<'py, PyAny>)> {
    let original: Vec<u8> = line.extract()?;
    if original.is_empty() || original[0] == 37 {
        return Ok((process_rg, process_char, multiline_rg.clone()));
    }
    let mut process_rg = process_rg;
    let mut process_char = process_char;
    let mut multiline_rg = multiline_rg.clone();
    let replaced = replace_all(&original, b"\t", b" ");
    let replaced_line = py_bytes(py, &replaced);
    if contains_sub(&replaced, b"beginbfrange") {
        process_rg = true;
    } else if contains_sub(&replaced, b"endbfrange") {
        process_rg = false;
    } else if contains_sub(&replaced, b"beginbfchar") {
        process_char = true;
    } else if contains_sub(&replaced, b"endbfchar") {
        process_char = false;
    } else if process_rg {
        match shim_attr(py, "parse_bfrange")?.call1((
            &replaced_line,
            map_dict,
            int_entry,
            &multiline_rg,
        )) {
            Ok(value) => multiline_rg = value,
            Err(error)
                if error.is_instance_of::<PyValueError>(py)
                    || error.is_instance_of::<PyIndexError>(py) =>
            {
                let values = PyDict::new(py);
                values.set_item("line", &replaced_line)?;
                values.set_item("error", error.value(py))?;
                logger_warning(
                    py,
                    "Skipping broken line %(line)r: %(error)s",
                    Some(&values),
                )?;
            }
            Err(error) => return Err(error),
        }
    } else if process_char {
        match shim_attr(py, "parse_bfchar")?.call1((&replaced_line, map_dict, int_entry)) {
            Ok(_) => {}
            Err(error) if error.is_instance_of::<PyValueError>(py) => {
                let values = PyDict::new(py);
                values.set_item("line", &replaced_line)?;
                values.set_item("error", error.value(py))?;
                logger_warning(
                    py,
                    "Skipping broken line %(line)r: %(error)s",
                    Some(&values),
                )?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok((process_rg, process_char, multiline_rg))
}

// @pypdf: pypdf._cmap.process_cm_line
#[pyfunction]
fn cmap_process_cm_line<'py>(
    py: Python<'py>,
    line: &Bound<'py, PyAny>,
    process_rg: bool,
    process_char: bool,
    multiline_rg: &Bound<'py, PyAny>,
    map_dict: &Bound<'py, PyAny>,
    int_entry: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let (process_rg, process_char, multiline_rg) = process_cm_line_impl(
        py,
        line,
        process_rg,
        process_char,
        multiline_rg,
        map_dict,
        int_entry,
    )?;
    py_tuple(
        py,
        vec![
            pyo3::types::PyBool::new(py, process_rg)
                .to_owned()
                .into_any(),
            pyo3::types::PyBool::new(py, process_char)
                .to_owned()
                .into_any(),
            multiline_rg,
        ],
    )
    .map(Bound::unbind)
}

// ---------------------------------------------------------------------------
// Glyph names and embedded font programs
// ---------------------------------------------------------------------------

/// `_glyph_name_to_unicode(glyph_name)` implementation.
fn glyph_name_to_unicode_impl<'py>(
    py: Python<'py>,
    glyph_name: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let adobe_glyphs = py.import("pypdf._codecs")?.getattr("adobe_glyphs")?;
    match adobe_glyphs.get_item(glyph_name) {
        Ok(value) => return Ok(Some(value)),
        Err(error) if error.is_instance_of::<PyKeyError>(py) => {}
        Err(error) => return Err(error),
    }
    let starts_with_uni = glyph_name
        .call_method1("startswith", ("/uni",))?
        .is_truthy()?;
    let code_point = if starts_with_uni {
        glyph_name.get_item(py_slice_from(py, 4)?)?
    } else if glyph_name
        .call_method1("startswith", ("/u",))?
        .is_truthy()?
    {
        glyph_name.get_item(py_slice_from(py, 2)?)?
    } else {
        return Ok(None);
    };
    let code = match int_hex(py, &code_point) {
        Ok(code) => code,
        Err(error) if error.is_instance_of::<PyValueError>(py) => return Ok(None),
        Err(error) => return Err(error),
    };
    match chr_value(py, &code) {
        Ok(character) => Ok(Some(character)),
        Err(error) if error.is_instance_of::<PyValueError>(py) => Ok(None),
        Err(error) => Err(error),
    }
}

/// `slice(start, None)` as a Python slice object.
fn py_slice_from<'py>(py: Python<'py>, start: i64) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("slice")?.call1((start, py_none(py)))
}

// @pypdf: pypdf._cmap._glyph_name_to_unicode
#[pyfunction]
fn cmap_glyph_name_to_unicode<'py>(
    py: Python<'py>,
    glyph_name: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(match glyph_name_to_unicode_impl(py, glyph_name)? {
        Some(value) => value.unbind(),
        None => py_none(py).unbind(),
    })
}

/// `_derive_character_map_from_font_file(...)` implementation.
fn derive_character_map_from_font_file_impl<'py>(
    py: Python<'py>,
    font_file: &Bound<'py, PyAny>,
    font_data: &Bound<'py, PyAny>,
    font_file_processor: &Bound<'py, PyAny>,
    map_dict: &Bound<'py, PyAny>,
    int_entry: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let digest = py
        .import("hashlib")?
        .getattr("sha256")?
        .call1((font_data,))?
        .call_method0("digest")?;
    let cached = font_file.getattr("_font_file_character_map")?;
    let mut cached_value = cached;
    let mut valid = !cached_value.is_none();
    if valid {
        let cached_digest = cached_value.getattr("digest")?;
        valid = !cached_digest.ne(&digest)?;
    }
    if !valid {
        let produced =
            font_file_processor.call1((font_data, PyDict::new(py), PyList::empty(py)))?;
        let produced_map = produced.get_item(0)?;
        let produced_entries = produced.get_item(1)?;
        let character_map_class = py
            .import("pypdf.generic._data_structures")?
            .getattr("_FontFileCharacterMap")?;
        cached_value = character_map_class.call1((digest, produced_map, produced_entries))?;
        font_file.setattr("_font_file_character_map", &cached_value)?;
    }
    map_dict.call_method1("update", (cached_value.getattr("map_dict")?,))?;
    int_entry.call_method1("extend", (cached_value.getattr("int_entry")?,))?;
    Ok((map_dict.clone(), int_entry.clone()))
}

// @pypdf: pypdf._cmap._derive_character_map_from_font_file
#[pyfunction]
fn cmap_derive_character_map_from_font_file<'py>(
    py: Python<'py>,
    font_file: &Bound<'py, PyAny>,
    font_data: &Bound<'py, PyAny>,
    font_file_processor: &Bound<'py, PyAny>,
    map_dict: &Bound<'py, PyAny>,
    int_entry: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let (map_dict, int_entry) = derive_character_map_from_font_file_impl(
        py,
        font_file,
        font_data,
        font_file_processor,
        map_dict,
        int_entry,
    )?;
    py_tuple(py, vec![map_dict, int_entry]).map(Bound::unbind)
}

/// `_derive_character_map_from_cff_type1_font_file(...)` implementation.
fn derive_cff_impl<'py>(
    py: Python<'py>,
    font_data: &Bound<'py, PyAny>,
    map_dict: &Bound<'py, PyAny>,
    int_entry: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let attempt: PyResult<()> = (|| {
        let cff_font_set_class = py.import("fontTools.cffLib")?.getattr("CFFFontSet")?;
        let cff_set = cff_font_set_class.call0()?;
        let bytes_io = py.import("io")?.getattr("BytesIO")?.call1((font_data,))?;
        cff_set.call_method1("decompile", (bytes_io, py_none(py)))?;
        let cff_font = cff_set.getattr("topDictIndex")?.get_item(0)?;
        let cff_encoding = cff_font.getattr("Encoding")?;
        // Encoding can fall back to literal strings "StandardEncoding" or
        // "ExpertEncoding", which we do not parse.
        if is_instance(&cff_encoding, &builtin_class(py, "str")?)? {
            return Ok(());
        }
        let count = (cff_encoding.len()?).min(256);
        let glyph_name_to_unicode = shim_attr(py, "_glyph_name_to_unicode")?;
        for index in 0..count {
            let glyph_name = cff_encoding.get_item(index)?;
            if !glyph_name.is_truthy()? || glyph_name.eq(".notdef")? {
                continue;
            }
            let prefixed = PyString::new(py, "/").into_any().add(&glyph_name)?;
            let unipoint = glyph_name_to_unicode.call1((prefixed,))?;
            if unipoint.is_truthy()? {
                map_dict.set_item(chr_value(py, &py_int(py, index as i64))?, unipoint)?;
                int_entry.call_method1("append", (py_int(py, index as i64),))?;
            }
        }
        Ok(())
    })();
    match attempt {
        Ok(()) => Ok((map_dict.clone(), int_entry.clone())),
        Err(error) if error.is_instance_of::<PyMemoryError>(py) => Err(error),
        Err(_) => Ok((map_dict.clone(), int_entry.clone())),
    }
}

// @pypdf: pypdf._cmap._derive_character_map_from_cff_type1_font_file
#[pyfunction]
fn cmap_derive_character_map_from_cff_type1_font_file<'py>(
    py: Python<'py>,
    font_data: &Bound<'py, PyAny>,
    map_dict: &Bound<'py, PyAny>,
    int_entry: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let (map_dict, int_entry) = derive_cff_impl(py, font_data, map_dict, int_entry)?;
    py_tuple(py, vec![map_dict, int_entry]).map(Bound::unbind)
}

/// `_derive_character_map_from_type1_font_file(...)` implementation.
fn derive_type1_impl<'py>(
    py: Python<'py>,
    font_data: &Bound<'py, PyAny>,
    map_dict: &Bound<'py, PyAny>,
    int_entry: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let data: Vec<u8> = font_data.extract()?;
    // Only the clear part
    let clear_part = match find_sub(&data, b"eexec\n") {
        Some(position) => &data[..position],
        None => &data[..],
    };
    // `txt.split(b"/Encoding")[1]` == everything after the first occurrence.
    let Some(position) = find_sub(clear_part, b"/Encoding") else {
        return Ok((map_dict.clone(), int_entry.clone()));
    };
    let encoding_part = &clear_part[position + b"/Encoding".len()..];
    // To get the encoding part
    let normalized = replace_all(encoding_part, b"\r", b"\n");
    let glyph_name_to_unicode = shim_attr(py, "_glyph_name_to_unicode")?;
    for line in normalized.split(|byte| *byte == b'\n') {
        if !line.starts_with(b"dup") {
            continue;
        }
        let words: Vec<&[u8]> = line
            .split(|byte| *byte == b' ')
            .filter(|word| !word.is_empty())
            .collect();
        if words.len() < 3 || (words.len() > 3 && words[3] != b"put") {
            continue;
        }
        let index = match int_decimal(py, &py_bytes(py, words[1])) {
            Ok(value) => value,
            Err(error) if error.is_instance_of::<PyValueError>(py) => continue,
            Err(error) => return Err(error),
        };
        let glyph_name = py_bytes(py, words[2]).call_method0("decode")?;
        let unipoint = glyph_name_to_unicode.call1((glyph_name,))?;
        if unipoint.is_truthy()? {
            map_dict.set_item(chr_value(py, &index)?, unipoint)?;
            int_entry.call_method1("append", (&index,))?;
        }
    }
    Ok((map_dict.clone(), int_entry.clone()))
}

// @pypdf: pypdf._cmap._derive_character_map_from_type1_font_file
#[pyfunction]
fn cmap_derive_character_map_from_type1_font_file<'py>(
    py: Python<'py>,
    font_data: &Bound<'py, PyAny>,
    map_dict: &Bound<'py, PyAny>,
    int_entry: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let (map_dict, int_entry) = derive_type1_impl(py, font_data, map_dict, int_entry)?;
    py_tuple(py, vec![map_dict, int_entry]).map(Bound::unbind)
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/// Build `dict(zip(range(256), values))` from a Python sequence.
fn dict_from_sequence<'py>(
    py: Python<'py>,
    values: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let dict = PyDict::new(py);
    let count = values.len()?.min(256);
    for index in 0..count {
        dict.set_item(index as i64, values.get_item(index)?)?;
    }
    Ok(dict.into_any())
}

/// `_parse_encoding(ft)` implementation.
fn parse_encoding_impl<'py>(
    py: Python<'py>,
    ft: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut encoding: Bound<'py, PyAny> = PyList::empty(py).into_any();
    let charset_encoding = py.import("pypdf._codecs")?.getattr("charset_encoding")?;
    let _predefined_cmap = shim_attr(py, "_predefined_cmap")?;
    let standard_encoding = || -> PyResult<Bound<'py, PyAny>> {
        charset_encoding
            .get_item("/StandardEncoding")?
            .call_method0("copy")
    };
    // If ft["/Encoding"] exists, then use that for encoding. Otherwise, use StandardEncoding as a basis,
    // and add what the embedded font file says, if present. See Table 114, PDF Reference 1.7 / 2.0
    if !ft.contains("/Encoding")? {
        if ft.contains("/BaseFont")? {
            let base_font = ft.get_item("/BaseFont")?;
            if charset_encoding.contains(&base_font)? {
                // This will match Symbol and ZapfDingbats
                return dict_from_sequence(py, &charset_encoding.get_item(&base_font)?);
            }
        }
        // Return StandardEncoding as fallback option. Note that a font's internal
        // encoding can be used to overwrite this, which we do for Type1 fonts in
        // _derive_character_map_from_(cff_)type1_font_file.
        return dict_from_sequence(py, &charset_encoding.get_item("/StandardEncoding")?);
    }

    let enc = ft.get_item("/Encoding")?.call_method0("get_object")?;
    let str_class = builtin_class(py, "str")?;
    let dict_class = generic_class(py, "DictionaryObject")?;
    if is_instance(&enc, &str_class)? {
        if charset_encoding.contains(&enc)? {
            encoding = charset_encoding.get_item(&enc)?.call_method0("copy")?;
        } else if _predefined_cmap.contains(&enc)? {
            encoding = _predefined_cmap.get_item(&enc)?;
        } else if enc.contains("-UCS2-")? {
            encoding = py_str(py, "utf-16-be");
        } else {
            let subtype_is_type0 = ft.call_method1("get", ("/Subtype", ""))?.eq("/Type0")?;
            encoding = if subtype_is_type0 {
                py_str(py, "utf-16-be")
            } else {
                standard_encoding()?
            };
            let values = PyDict::new(py);
            values.set_item("encoding", &enc)?;
            values.set_item("new_encoding", &encoding)?;
            logger_error(
                py,
                "Advanced encoding %(encoding)s not implemented yet, using %(new_encoding)s instead.",
                Some(&values),
            )?;
        }
    } else if is_instance(&enc, &dict_class)? && enc.contains("/BaseEncoding")? {
        match charset_encoding.get_item(enc.get_item("/BaseEncoding")?) {
            Ok(value) => encoding = value.call_method0("copy")?,
            Err(error) if error.is_instance_of::<PyKeyError>(py) => {
                let values = PyDict::new(py);
                values.set_item("encoding", &encoding)?;
                logger_error(
                    py,
                    "Advanced encoding %(encoding)s not implemented yet",
                    Some(&values),
                )?;
                encoding = standard_encoding()?;
            }
            Err(error) => return Err(error),
        }
    } else {
        encoding = standard_encoding()?;
    }

    if is_instance(&enc, &dict_class)? && enc.contains("/Differences")? {
        let mut x: Bound<'py, PyAny> = py_int(py, 0);
        let differences = enc.get_item("/Differences")?.call_method0("get_object")?;
        let array_class = generic_class(py, "ArrayObject")?;
        let differences = if is_instance(&differences, &array_class)? {
            differences
        } else {
            let values = PyDict::new(py);
            values.set_item("differences", &differences)?;
            logger_warning(
                py,
                "Font encoding differences are not an array: %(differences)s",
                Some(&values),
            )?;
            array_class.call0()?
        };
        let glyph_name_to_unicode = shim_attr(py, "_glyph_name_to_unicode")?;
        for entry in differences.try_iter()? {
            let entry = entry?;
            if is_instance(&entry, &builtin_class(py, "int")?)? {
                x = entry;
            } else {
                let assigned: PyResult<Bound<'py, PyAny>> = (|| {
                    if x.lt(encoding.len()? as i64)? {
                        let converted = glyph_name_to_unicode.call1((&entry,))?;
                        let value = if converted.is_truthy()? {
                            converted
                        } else {
                            entry.clone()
                        };
                        encoding.set_item(&x, &value)?;
                    }
                    Ok(py_none(py))
                })();
                match assigned {
                    Ok(_) => {}
                    Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                    Err(_) => {
                        encoding.set_item(&x, &entry)?;
                    }
                }
                x = x.add(1)?;
            }
        }
    }
    if is_instance(&encoding, &builtin_class(py, "list")?)? {
        encoding = dict_from_sequence(py, &encoding)?;
    }
    Ok(encoding)
}

// @pypdf: pypdf._cmap._parse_encoding
#[pyfunction]
fn cmap_parse_encoding<'py>(py: Python<'py>, ft: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(parse_encoding_impl(py, ft)?.unbind())
}

/// `_parse_to_unicode(ft)` implementation.
fn parse_to_unicode_impl<'py>(
    py: Python<'py>,
    ft: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    // We store all character mappings in map_dict. In map_dict[-1] we store the byte length
    // of the character codes (or CIDs) encoded inside the ToUnicode stream.
    let map_dict = PyDict::new(py);
    // We provide the list of cmap keys in int_entry to correct encoding later on in get_encoding().
    let int_entry = PyList::empty(py);

    if !ft.contains("/ToUnicode")? {
        if ft.call_method1("get", ("/Subtype", ""))?.eq("/Type1")? {
            let font_descriptor = ft.call_method1("get", ("/FontDescriptor",))?;
            if !font_descriptor.is_truthy()? {
                return Ok((map_dict.into_any(), int_entry.into_any()));
            }

            // We try to read encoding from an embedded font file, if we can. See Table 126 about embedded font
            // file organization in the PDF specification 1.7 for details.
            let stream_class = generic_class(py, "StreamObject")?;
            let has_fonttools: bool = py
                .import("pypdf.generic._font")?
                .getattr("HAS_FONTTOOLS")?
                .extract()?;
            // A normal Type1 font file, can be part of a Type1 or MMType1 font dictionary.
            // A CFF Type1 font file, as part of a Type1 or MMType1 font dictionary, when subtype is Type1C.
            let handlers: [(&str, Option<&str>, &str); 2] = [
                (
                    "/FontFile",
                    None,
                    "_derive_character_map_from_type1_font_file",
                ),
                (
                    "/FontFile3",
                    Some("/Type1C"),
                    "_derive_character_map_from_cff_type1_font_file",
                ),
            ];
            for (font_file, required_subtype, processor_name) in handlers {
                if !font_descriptor.contains(font_file)? {
                    continue;
                }
                let font_file_dict = font_descriptor.get_item(font_file)?;
                if !is_instance(&font_file_dict, &stream_class)? {
                    continue;
                }
                if let Some(required_subtype) = required_subtype {
                    if !font_file_dict
                        .call_method1("get", ("/Subtype",))?
                        .eq(required_subtype)?
                    {
                        continue;
                    }
                }
                if font_file == "/FontFile3" && !has_fonttools {
                    let values = PyDict::new(py);
                    values.set_item("ft", ft)?;
                    logger_warning(
                        py,
                        concat!(
                            "fontTools is required to fully parse the encoding of a CFF Type1 font in font ",
                            "dictionary %(ft)s, but is not installed. Consider installing fontTools if you ",
                            "encounter encoding problems."
                        ),
                        Some(&values),
                    )?;
                    return Ok((map_dict.into_any(), int_entry.into_any()));
                }

                let font_file_data = font_file_dict.call_method0("get_data")?;
                if !font_file_data.is_truthy()? {
                    return Ok((map_dict.into_any(), int_entry.into_any()));
                }

                let processor = shim_attr(py, processor_name)?;
                let derived = shim_attr(py, "_derive_character_map_from_font_file")?.call1((
                    &font_file_dict,
                    &font_file_data,
                    &processor,
                    map_dict.as_any(),
                    int_entry.as_any(),
                ))?;
                return Ok((derived.get_item(0)?, derived.get_item(1)?));
            }

            return Ok((map_dict.into_any(), int_entry.into_any()));
        }

        return Ok((PyDict::new(py).into_any(), PyList::empty(py).into_any()));
    }

    let mut process_rg: bool = false;
    let mut process_char: bool = false;
    // tuple = (current_char, remaining size) ; cf #1285 for example of file
    let mut multiline_rg: Bound<'py, PyAny> = py_none(py);
    let cm = shim_attr(py, "prepare_cm")?.call1((ft,))?;
    let cm_bytes: Vec<u8> = cm.extract()?;
    let process_cm_line = shim_attr(py, "process_cm_line")?;
    for line in cm_bytes.split(|byte| *byte == b'\n') {
        let stripped = strip_bytes(line, b" \t");
        let state = process_cm_line.call1((
            py_bytes(py, stripped),
            process_rg,
            process_char,
            &multiline_rg,
            map_dict.as_any(),
            int_entry.as_any(),
        ))?;
        process_rg = state.get_item(0)?.extract()?;
        process_char = state.get_item(1)?.extract()?;
        multiline_rg = state.get_item(2)?;
    }

    // Don't pass the -1 key, we only used it to temporarily store encoding length
    map_dict.call_method1("pop", (-1, py_none(py)))?;

    Ok((map_dict.into_any(), int_entry.into_any()))
}

// @pypdf: pypdf._cmap._parse_to_unicode
#[pyfunction]
fn cmap_parse_to_unicode<'py>(py: Python<'py>, ft: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let (map_dict, int_entry) = parse_to_unicode_impl(py, ft)?;
    py_tuple(py, vec![map_dict, int_entry]).map(Bound::unbind)
}

// @pypdf: pypdf._cmap.get_encoding
#[pyfunction]
fn cmap_get_encoding<'py>(py: Python<'py>, ft: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    // Upstream calls the module-global functions; doing the same keeps
    // monkey-patching of `pypdf._cmap._parse_encoding` observable.
    let encoding = shim_attr(py, "_parse_encoding")?.call1((ft,))?;
    let parsed = shim_attr(py, "_parse_to_unicode")?.call1((ft,))?;
    let map_dict = parsed.get_item(0)?;
    let int_entry = parsed.get_item(1)?;

    // Apply rule from PDF ref 1.7 §5.9.1, 1st bullet:
    //   if cmap not empty encoding should be discarded
    //   (here transformed into identity for those characters)
    // If encoding is a string, it is expected to be an identity translation.
    if is_instance(&encoding, &builtin_class(py, "dict")?)? {
        for entry in int_entry.try_iter()? {
            let entry = entry?;
            if entry.le(255)? {
                encoding.set_item(&entry, chr_value(py, &entry)?)?;
            }
        }
    }

    py_tuple(py, vec![encoding, map_dict]).map(Bound::unbind)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Symbols implemented in the hand-written shim (`python/pypdf/_cmap.py`): the
/// module constants, the `_predefined_cmap` table, the imported helpers
/// (`unhexlify`, `sha256`, `BytesIO`, `partial`, ...) and the
/// `_FontFileCharacterMap` re-export.
///
// @pypdf: pypdf._cmap._predefined_cmap
// @pypdf: pypdf._cmap.MAPPING_DICTIONARY_SIZE_LIMIT
// @pypdf: pypdf._cmap.MAX_CMAP_CODE_BYTES
// @pypdf: pypdf._cmap.MAX_CMAP_STRING_BYTES
// @pypdf: pypdf._cmap.MAX_CMAP_CODE_BYTES_LIMIT
// @pypdf: pypdf._cmap.MAX_CMAP_STRING_BYTES_LIMIT
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(cmap_get_encoding, module)?)?;
    module.add_function(wrap_pyfunction!(cmap_parse_encoding, module)?)?;
    module.add_function(wrap_pyfunction!(cmap_parse_to_unicode, module)?)?;
    module.add_function(wrap_pyfunction!(cmap_prepare_cm, module)?)?;
    module.add_function(wrap_pyfunction!(cmap_process_cm_line, module)?)?;
    module.add_function(wrap_pyfunction!(cmap_check_mapping_size, module)?)?;
    module.add_function(wrap_pyfunction!(cmap_check_token_length, module)?)?;
    module.add_function(wrap_pyfunction!(cmap_parse_bfrange_decode, module)?)?;
    module.add_function(wrap_pyfunction!(cmap_parse_bfrange, module)?)?;
    module.add_function(wrap_pyfunction!(cmap_parse_bfchar, module)?)?;
    module.add_function(wrap_pyfunction!(cmap_glyph_name_to_unicode, module)?)?;
    module.add_function(wrap_pyfunction!(
        cmap_derive_character_map_from_font_file,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        cmap_derive_character_map_from_cff_type1_font_file,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        cmap_derive_character_map_from_type1_font_file,
        module
    )?)?;
    Ok(())
}
