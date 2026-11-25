//! Stream helpers ported from `pypdf/_utils.py`.

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyByteArray, PyBytes, PyDict};

use crate::errors;
use crate::pyutil;

use super::common;

/// `io.DEFAULT_BUFFER_SIZE` used by `read_previous_line`.
fn default_buffer_size(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    py.import("io")?.getattr("DEFAULT_BUFFER_SIZE")
}

/// `bytes(txt)` where `txt` is assembled from one-byte reads.
///
/// `txt` is a `bytearray` upstream; appending a non-bytes token raises the
/// same `TypeError` here by delegating to `bytearray.__iadd__`.
fn append_token(py: Python<'_>, txt: &mut Vec<u8>, tok: &Bound<'_, PyAny>) -> PyResult<()> {
    if let Ok(bytes) = tok.cast::<PyBytes>() {
        txt.extend_from_slice(bytes.as_bytes());
        return Ok(());
    }
    let buffer = PyByteArray::new(py, txt);
    buffer.call_method1("__iadd__", (tok.clone(),))?;
    *txt = buffer.to_vec();
    Ok(())
}

// @pypdf: pypdf._utils.read_until_whitespace
#[pyfunction]
#[pyo3(signature = (stream, max_bytes=None, strict=false))]
pub(super) fn read_until_whitespace(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    max_bytes: Option<&Bound<'_, PyAny>>,
    strict: bool,
) -> PyResult<Py<PyBytes>> {
    let mut txt: Vec<u8> = Vec::new();
    loop {
        let tok = common::read_one(stream)?;
        let empty = !tok.is_truthy()?;
        if empty || common::is_whitespace_token(py, &tok)? {
            break;
        }
        if let Some(max_bytes) = max_bytes {
            if common::len_ge_limit(py, txt.len(), max_bytes)? {
                if strict {
                    return Err(errors::limit_reached_error(
                        py,
                        &format!(
                            "Token exceeds maximum length of {} bytes.",
                            pyutil::py_str(max_bytes)?
                        ),
                    ));
                }
                logger_warning_token_limit(py, max_bytes)?;
                break;
            }
        }
        append_token(py, &mut txt, &tok)?;
    }
    Ok(PyBytes::new(py, &txt).unbind())
}

fn logger_warning_token_limit(py: Python<'_>, max_bytes: &Bound<'_, PyAny>) -> PyResult<()> {
    let values = PyDict::new(py);
    values.set_item("max_bytes", max_bytes)?;
    pyutil::logger_warning(
        py,
        common::SOURCE,
        "Token exceeds maximum length of %(max_bytes)d bytes.",
        Some(&values),
    )
}

// @pypdf: pypdf._utils.read_non_whitespace
#[pyfunction]
pub(super) fn read_non_whitespace(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let mut tok = common::read_one(stream)?;
    while common::in_whitespaces(py, &tok)? {
        tok = common::read_one(stream)?;
    }
    Ok(tok.unbind())
}

// @pypdf: pypdf._utils.skip_over_whitespace
#[pyfunction]
pub(super) fn skip_over_whitespace(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<bool> {
    let mut tok = common::read_one(stream)?;
    let mut count: i64 = 0;
    while common::in_whitespaces(py, &tok)? {
        count += 1;
        tok = common::read_one(stream)?;
    }
    Ok(count > 0)
}

// @pypdf: pypdf._utils.check_if_whitespace_only
#[pyfunction]
pub(super) fn check_if_whitespace_only(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    if let Ok(bytes) = value.cast::<PyBytes>() {
        return Ok(bytes
            .as_bytes()
            .iter()
            .all(|byte| common::is_whitespace_byte(*byte)));
    }
    // `all(b in WHITESPACES_AS_BYTES for b in value)` for any iterable.
    let whitespaces_bytes = PyBytes::new(py, &common::WHITESPACES_BYTES);
    for item in value.try_iter()? {
        let item = item?;
        if !whitespaces_bytes
            .call_method1("__contains__", (item,))?
            .is_truthy()?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

// @pypdf: pypdf._utils.skip_over_comment
#[pyfunction]
pub(super) fn skip_over_comment(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<()> {
    let mut tok = common::read_one(stream)?;
    stream.call_method1("seek", (-1i64, 1))?;
    let marker = PyBytes::new(py, b"%");
    if tok.eq(&marker)? {
        let newline = PyBytes::new(py, b"\n");
        let carriage_return = PyBytes::new(py, b"\r");
        let empty = PyBytes::new(py, b"");
        while !(tok.eq(&newline)? || tok.eq(&carriage_return)?) {
            tok = common::read_one(stream)?;
            if tok.eq(&empty)? {
                return Err(errors::pdf_stream_error(py, "File ended unexpectedly."));
            }
        }
    }
    Ok(())
}

// @pypdf: pypdf._utils.skip_over_comments
#[pyfunction]
#[pyo3(signature = (stream, *, limit=None))]
pub(super) fn skip_over_comments(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    limit: Option<&Bound<'_, PyAny>>,
) -> PyResult<()> {
    let mut tok = common::read_one(stream)?;
    let tok_len = pyutil::py_len(&tok)?;
    stream.call_method1("seek", (-(tok_len as i64), 1))?;
    let mut comment_count: i64 = 0;
    let marker = PyBytes::new(py, b"%");
    while tok.eq(&marker)? {
        if let Some(limit) = limit {
            if common::int_ge_limit(py, comment_count, limit)? {
                return Err(errors::limit_reached_error(
                    py,
                    &format!(
                        "Maximum number of leading comments reached: {}.",
                        pyutil::py_str(limit)?
                    ),
                ));
            }
        }
        skip_over_comment(py, stream)?;
        tok = read_non_whitespace(py, stream)?.into_bound(py);
        stream.call_method1("seek", (-1i64, 1))?;
        comment_count += 1;
    }
    Ok(())
}

// @pypdf: pypdf._utils.read_until_regex
#[pyfunction]
#[pyo3(signature = (*, stream, regex, length=9223372036854775807))]
pub(super) fn read_until_regex(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    regex: &Bound<'_, PyAny>,
    length: i64,
) -> PyResult<Py<PyBytes>> {
    let mut parts: Vec<Vec<u8>> = Vec::new();
    let mut total_length: i64 = 0;
    let mut tail: Vec<u8> = Vec::new();
    let mut chunk_size: i64 = 16;
    loop {
        let token = stream.call_method1("read", (chunk_size,))?;
        let token_bytes: Vec<u8> = token.extract()?;
        if token_bytes.is_empty() {
            return Ok(PyBytes::new(py, &join_parts(&parts)).unbind());
        }
        let token_length = token_bytes.len() as i64;
        let current_length = total_length + token_length;
        if current_length >= length {
            return Err(errors::limit_reached_error(
                py,
                &format!(
                    "Read stream length of {current_length} exceeds maximum allowed length of {length}."
                ),
            ));
        }

        // Search the overlap of the previous tail with the new chunk so that
        // multi-byte regex matches spanning chunk boundaries are found.
        let mut current_buffer = tail.clone();
        current_buffer.extend_from_slice(&token_bytes);
        let search_match = regex.call_method1("search", (PyBytes::new(py, &current_buffer),))?;
        parts.push(token_bytes.clone());
        if !search_match.is_none() {
            let overlap = tail.len() as i64;
            let start: i64 = search_match.call_method0("start")?.extract()?;
            let actual_start = total_length - overlap + start;
            stream.call_method1("seek", (actual_start - total_length - token_length, 1))?;
            let joined = join_parts(&parts);
            let cut = actual_start.clamp(0, joined.len() as i64) as usize;
            return Ok(PyBytes::new(py, &joined[..cut]).unbind());
        }
        total_length += token_length;

        let tail_start = token_bytes.len().saturating_sub(16);
        tail = token_bytes[tail_start..].to_vec();
        if chunk_size < 8192 {
            chunk_size <<= 1;
        }
    }
}

fn join_parts(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut joined = Vec::new();
    for part in parts {
        joined.extend_from_slice(part);
    }
    joined
}

// @pypdf: pypdf._utils.read_block_backwards
#[pyfunction]
pub(super) fn read_block_backwards(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    to_read: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let position = stream.call_method0("tell")?;
    if position.lt(to_read)? {
        return Err(errors::pdf_stream_error(
            py,
            "Could not read malformed PDF file",
        ));
    }
    let negated = to_read.neg()?;
    stream.call_method1("seek", (negated.clone(), 1))?;
    let data = stream.call_method1("read", (to_read.clone(),))?;
    stream.call_method1("seek", (negated, 1))?;
    Ok(data.unbind())
}

// @pypdf: pypdf._utils.read_previous_line
#[pyfunction]
pub(super) fn read_previous_line(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
) -> PyResult<Py<PyBytes>> {
    let buffer_size = default_buffer_size(py)?;
    let mut line_content: Vec<Vec<u8>> = Vec::new();
    let mut found_crlf = false;
    if stream.call_method0("tell")?.eq(0i64)? {
        return Err(errors::pdf_stream_error(
            py,
            common::STREAM_TRUNCATED_PREMATURELY,
        ));
    }
    loop {
        let position = stream.call_method0("tell")?;
        let to_read = if buffer_size.lt(&position)? {
            buffer_size.clone()
        } else {
            position
        };
        if to_read.eq(0i64)? {
            break;
        }
        let block = read_block_backwards(py, stream, &to_read)?;
        let block_bytes: Vec<u8> = block.bind(py).extract()?;
        let mut idx: i64 = block_bytes.len() as i64 - 1;
        if !found_crlf {
            while idx >= 0 && !matches!(block_bytes[idx as usize], b'\r' | b'\n') {
                idx -= 1;
            }
            if idx >= 0 {
                found_crlf = true;
            }
        }
        if found_crlf {
            line_content.push(block_bytes[(idx + 1) as usize..].to_vec());
            while idx >= 0 && matches!(block_bytes[idx as usize], b'\r' | b'\n') {
                idx -= 1;
            }
        } else {
            line_content.push(block_bytes);
        }
        if idx >= 0 {
            stream.call_method1("seek", (idx + 1, 1))?;
            break;
        }
    }
    line_content.reverse();
    let mut line = Vec::new();
    for block in line_content {
        line.extend_from_slice(&block);
    }
    Ok(PyBytes::new(py, &line).unbind())
}

// @pypdf: pypdf._utils.get_stream_size
#[pyfunction]
pub(super) fn get_stream_size(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if stream.hasattr("getbuffer")? {
        let buffer_object = stream.call_method0("getbuffer")?;
        let view = buffer_object.call_method0("__enter__")?;
        let nbytes = view.getattr("nbytes")?;
        buffer_object.call_method1("__exit__", (py.None(), py.None(), py.None()))?;
        return Ok(nbytes.unbind());
    }
    let position = stream.call_method0("tell")?;
    // os.SEEK_END
    stream.call_method1("seek", (0i64, 2i64))?;
    let size = stream.call_method0("tell")?;
    stream.call_method1("seek", (position,))?;
    Ok(size.unbind())
}

// @pypdf: pypdf._utils.get_stream_data
#[pyfunction]
pub(super) fn get_stream_data(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if stream.hasattr("getbuffer")? {
        let buffer_object = stream.call_method0("getbuffer")?;
        let view = buffer_object.call_method0("__enter__")?;
        let data = crate::pyutil::cached_module(py, "builtins")?.getattr("bytes")?.call1((view,))?;
        buffer_object.call_method1("__exit__", (py.None(), py.None(), py.None()))?;
        return Ok(data.unbind());
    }
    let position = stream.call_method0("tell")?;
    // os.SEEK_SET
    stream.call_method1("seek", (0i64, 0i64))?;
    let data = stream.call_method1("read", (-1i64,))?;
    stream.call_method1("seek", (position,))?;
    Ok(data.unbind())
}

// @pypdf: pypdf._utils.mark_location
#[pyfunction]
pub(super) fn mark_location(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<()> {
    const RADIUS: i64 = 5000;
    stream.call_method1("seek", (-RADIUS, 1))?;
    let file_context = py
        .import("builtins")?
        .getattr("open")?
        .call1(("pypdf_pdfLocation.txt", "wb"))?;
    let file_handle = file_context.call_method0("__enter__")?;
    file_handle.call_method1("write", (stream.call_method1("read", (RADIUS,))?,))?;
    file_handle.call_method1("write", (PyBytes::new(py, b"HERE"),))?;
    file_handle.call_method1("write", (stream.call_method1("read", (RADIUS,))?,))?;
    file_context.call_method1("__exit__", (py.None(), py.None(), py.None()))?;
    stream.call_method1("seek", (-RADIUS, 1))?;
    Ok(())
}
