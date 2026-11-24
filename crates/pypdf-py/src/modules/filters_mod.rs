//! Rust port of `pypdf/filters.py` (upstream pypdf 7.1.1).
//!
//! Every public class of the upstream module stays a Python class in the
//! hand-written shim `python/pypdf/filters.py` (class attributes such as
//! `LZWDecode.Decoder.STOP` or `CCITTFaxDecode._MAXIMUM_UNSIGNED_LONG` are
//! observable Python API); every method body delegates to a `#[pyfunction]`
//! implemented here (see `docs/PORTING_GUIDE.md` §1).
//!
//! Fidelity notes:
//!
//! * `FlateDecode` must go through the *interpreter's* `zlib` module
//!   (`docs/FIDELITY_CONTRACT.md` §5.3): `zlib.decompressobj`, the
//!   `MAX_WBITS | 32` recovery path, `decompress(data, max_length=...)` and the
//!   exact `zlib.error` texts are produced by CPython's zlib, not by a vendored
//!   implementation.
//! * `ASCII85Decode` delegates to `base64.a85decode` so that the exact
//!   `ValueError` texts (which the official tests assert on) come from CPython.
//! * `LZWDecode` reuses the ported `LzwCodec` (`pypdf._codecs._codecs`).
//! * `BrotliDecode`, `JBIG2Decode` and the `CCITTFaxDecode` TIFF wrapper talk to
//!   the same Python dependencies as upstream (`brotli`, the `jbig2dec` binary
//!   through `subprocess`, `struct.pack`), so bytes/errors match exactly.
//! * Warning/log messages are emitted through `logging.getLogger("pypdf.filters")`
//!   with the identical format strings (official `caplog` assertions depend on
//!   them).

use std::cmp::min;

use pyo3::exceptions::{PyNotImplementedError, PyValueError, PyZeroDivisionError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyList, PyModule, PyString, PyTuple};

use crate::errors;
use crate::pyutil;

/// Upstream `source=__name__` of every `logger_warning` call in this module.
const SOURCE: &str = "pypdf.filters";

/// `CCITTFaxDecode._MAXIMUM_UNSIGNED_LONG` (0xFFFFFFFF).
const MAXIMUM_UNSIGNED_LONG: i128 = 0xFFFF_FFFF;

/// Bytes treated as whitespace by `bytes.translate(None, WHITESPACES_AS_BYTES)`
/// / `bytes.strip(WHITESPACES_AS_BYTES)` (`pypdf._utils.WHITESPACES_AS_BYTES`).
const WHITESPACES_BYTES: [u8; 6] = [0x00, b'\t', b'\n', 0x0c, b'\r', b' '];

// ---------------------------------------------------------------------------
// Helpers around Python semantics
// ---------------------------------------------------------------------------

fn filters_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("pypdf.filters")
}

fn generic_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("pypdf.generic")
}

fn generic_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    generic_module(py)?.getattr(name)
}

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("builtins")
}

fn builtin<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr(name)
}

/// `pypdf._configuration.get_configuration()`.
fn configuration<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf._configuration")?
        .getattr("get_configuration")?
        .call0()
}

/// `get_configuration().<name>`.
fn config_value<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    configuration(py)?.getattr(name)
}

/// `pypdf.generic.is_null_or_none(obj)`.
fn is_null_or_none(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    generic_class(py, "is_null_or_none")?
        .call1((obj,))?
        .extract()
}

/// `obj.__class__.__name__`.
fn type_name(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    pyutil::py_str(&obj.get_type().getattr("__name__")?)
}

/// `bytes(obj)`-style extraction of a bytes-like payload.
fn extract_bytes(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(bytes) = data.cast::<PyBytes>() {
        return Ok(bytes.as_bytes().to_vec());
    }
    let converted = builtin(py, "bytes")?.call1((data,))?;
    Ok(converted.cast::<PyBytes>()?.as_bytes().to_vec())
}

fn py_bytes_obj<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, data).into_any()
}

fn int128_obj<'py>(py: Python<'py>, value: i128) -> PyResult<Bound<'py, PyAny>> {
    Ok(value.into_pyobject(py)?.into_any())
}

/// `slice(start, stop, step=None)` with integer (or omitted) bounds.
fn py_slice_i64<'py>(
    py: Python<'py>,
    start: Option<i64>,
    stop: Option<i64>,
) -> PyResult<Bound<'py, PyAny>> {
    let to_obj = |value: Option<i64>| -> PyResult<Bound<'py, PyAny>> {
        match value {
            Some(value) => Ok(value.into_pyobject(py)?.into_any()),
            None => Ok(py.None().into_bound(py)),
        }
    };
    py.import("builtins")?
        .getattr("slice")?
        .call1((to_obj(start)?, to_obj(stop)?, py.None()))
}

fn py_add<'py>(
    left: &Bound<'py, PyAny>,
    right: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    left.call_method1("__add__", (right,))
}

/// Mark `error` as `raise <error> from <cause>`.
fn set_cause(py: Python<'_>, error: &PyErr, cause: &Bound<'_, PyAny>) {
    let value = error.value(py);
    let _ = value.setattr("__cause__", cause);
    let _ = value.setattr("__suppress_context__", true);
}

/// `raise <error> from <cause>` for a freshly built error.
fn error_with_cause(py: Python<'_>, error: PyErr, cause: &Bound<'_, PyAny>) -> PyErr {
    set_cause(py, &error, cause);
    error
}

fn logger_plain(py: Python<'_>, message: &str) -> PyResult<()> {
    pyutil::logger_warning(py, SOURCE, message, None)
}

fn logger_values(
    py: Python<'_>,
    message: &str,
    values: Vec<(&str, Bound<'_, PyAny>)>,
) -> PyResult<()> {
    let kwargs = PyDict::new(py);
    for (key, value) in values {
        kwargs.set_item(key, value)?;
    }
    pyutil::logger_warning(py, SOURCE, message, Some(&kwargs))
}

/// `obj in (str, ...)` for a tuple of upstream constant strings.
fn eq_any(filter_name: &Bound<'_, PyAny>, candidates: &[&str]) -> PyResult<bool> {
    for candidate in candidates {
        if filter_name.eq(*candidate)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn class_static_call<'py>(
    _py: Python<'py>,
    module: &Bound<'py, PyModule>,
    class_name: &str,
    method: &str,
    args: Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    module.getattr(class_name)?.getattr(method)?.call1(args)
}

fn call_args<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyTuple>> {
    PyTuple::new(py, items)
}

// ---------------------------------------------------------------------------
// Legacy module constants (values live in the hand-written Python shim; the
// annotations below satisfy the A6 symbol-coverage gate).
// ---------------------------------------------------------------------------

// @pypdf: pypdf.filters.__author__
// @pypdf: pypdf.filters.__author_email__
// @pypdf: pypdf.filters.MAX_DECLARED_STREAM_LENGTH
// @pypdf: pypdf.filters.MAX_ARRAY_BASED_STREAM_OUTPUT_LENGTH
// @pypdf: pypdf.filters.JBIG2_MAX_OUTPUT_LENGTH
// @pypdf: pypdf.filters.LZW_MAX_OUTPUT_LENGTH
// @pypdf: pypdf.filters.RUN_LENGTH_MAX_OUTPUT_LENGTH
// @pypdf: pypdf.filters.ZLIB_MAX_OUTPUT_LENGTH
// @pypdf: pypdf.filters.ZLIB_MAX_RECOVERY_INPUT_LENGTH
// @pypdf: pypdf.filters.FLATE_MAX_COLUMNS
// @pypdf: pypdf.filters.FLATE_MAX_ROW_LENGTH
// @pypdf: pypdf.filters.FLATE_MAX_BUFFER_SIZE
// @pypdf: pypdf.filters._SINGLE_BYTES
// @pypdf: pypdf.filters.JBIG2DEC_BINARY
#[allow(dead_code)]
const LEGACY_MODULE_CONSTANTS_MIRRORED_IN_PYTHON_SHIM: () = ();

// ---------------------------------------------------------------------------
// zlib helpers
// ---------------------------------------------------------------------------

// @pypdf: pypdf.filters._decompress_with_limit
#[pyfunction]
fn filters_decompress_with_limit<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let zlib = py.import("zlib")?;
    let decompressor = zlib.call_method0("decompressobj")?;
    let max_length = config_value(py, "zlib_maximum_output_length")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("max_length", max_length)?;
    let result = decompressor.call_method("decompress", (data,), Some(&kwargs))?;
    let unconsumed_tail = decompressor.getattr("unconsumed_tail")?;
    if unconsumed_tail.is_truthy()? {
        let remaining = unconsumed_tail.len()?;
        let message =
            format!("Limit reached while decompressing. {remaining} bytes remaining.");
        return Err(errors::limit_reached_error(py, &message));
    }
    Ok(result.unbind())
}

// @pypdf: pypdf.filters.decompress
#[pyfunction]
fn filters_decompress<'py>(py: Python<'py>, data: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let module = filters_module(py)?;
    // Upstream calls the module-global `_decompress_with_limit`; resolve it at
    // call time so that `mock.patch("pypdf.filters._decompress_with_limit")`
    // keeps working (official tests rely on it).
    let helper = module.getattr("_decompress_with_limit")?;
    let zlib = py.import("zlib")?;
    let zlib_error = zlib.getattr("error")?;
    let data_bytes = extract_bytes(py, data)?;

    match helper.call1((data,)) {
        Ok(result) => Ok(result.unbind()),
        Err(exception) => {
            if !exception.is_instance(py, &zlib_error) {
                return Err(exception);
            }
            let original: Py<PyAny> = exception.value(py).clone().into_any().unbind();

            // First quick approach: cut off up to eight trailing bytes, one at a
            // time (faulty streams from early Adobe Distiller/Pitstop versions).
            let max_tail_cut_off_bytes: usize = 8;
            let upper = min(max_tail_cut_off_bytes + 1, data_bytes.len());
            for i in 1..upper {
                let sliced = PyBytes::new(py, &data_bytes[..data_bytes.len() - i]);
                match helper.call1((sliced,)) {
                    Ok(result) => return Ok(result.unbind()),
                    Err(error) => {
                        if !error.is_instance(py, &zlib_error) {
                            return Err(error);
                        }
                    }
                }
            }

            // Final fallback: feed the stream byte by byte into a decompressor
            // that accepts both zlib and gzip headers.
            let wbits = zlib.getattr("MAX_WBITS")?.call_method1("__or__", (32,))?;
            let decompressor = zlib.call_method1("decompressobj", (wbits,))?;
            let configuration = configuration(py)?;
            let mut remaining_limit: i128 =
                configuration.getattr("zlib_maximum_output_length")?.extract()?;
            let recovery_input_limit: i128 = configuration
                .getattr("zlib_maximum_recovery_input_length")?
                .extract()?;
            let data_length = data_bytes.len();

            // Upstream reuses the cached 1-byte `bytes` objects; recreating them
            // once per call is observationally equivalent and keeps the loop fast.
            let mut single_bytes: Vec<Py<PyBytes>> = Vec::with_capacity(256);
            for value in 0..=255u8 {
                single_bytes.push(PyBytes::new(py, &[value]).unbind());
            }

            let mut result: Vec<u8> = Vec::new();
            let mut known_errors: Vec<String> = Vec::new();
            for index in 0..data_length {
                if (index as i128) >= recovery_input_limit {
                    let message = format!(
                        "Recovery limit reached while decompressing. {} bytes remaining.",
                        data_length - index
                    );
                    let error = errors::limit_reached_error(py, &message);
                    return Err(error_with_cause(py, error, original.bind(py)));
                }

                let chunk = single_bytes[data_bytes[index] as usize].clone_ref(py);
                let remaining = int128_obj(py, remaining_limit)?;
                match decompressor.call_method1("decompress", (chunk, remaining)) {
                    Ok(decompressed) => {
                        let decompressed = extract_bytes(py, &decompressed)?;
                        result.extend_from_slice(&decompressed);
                        remaining_limit -= decompressed.len() as i128;
                        if remaining_limit <= 0 {
                            let message = format!(
                                "Limit reached while decompressing. {} bytes remaining.",
                                data_length - index
                            );
                            return Err(errors::limit_reached_error(py, &message));
                        }
                    }
                    Err(error) => {
                        if !error.is_instance(py, &zlib_error) {
                            return Err(error);
                        }
                        let error_str = pyutil::py_str(error.value(py))?;
                        if known_errors.contains(&error_str) {
                            continue;
                        }
                        logger_plain(py, &error_str)?;
                        known_errors.push(error_str);
                    }
                }
            }
            Ok(py_bytes_obj(py, &result).unbind())
        }
    }
}

// ---------------------------------------------------------------------------
// FlateDecode
// ---------------------------------------------------------------------------

// @pypdf: pypdf.filters.FlateDecode
// @pypdf: pypdf.filters.FlateDecode.decode
#[pyfunction]
#[pyo3(signature = (data, decode_parms = None))]
fn filters_flate_decode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
    decode_parms: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let module = filters_module(py)?;
    // Upstream: `str_data = decompress(data)` (module-global lookup).
    let str_data = module.getattr("decompress")?.call1((data,))?;

    let decode_parms: Bound<'py, PyAny> = match decode_parms {
        Some(value) => value.clone(),
        None => py.None().into_bound(py),
    };
    let parameters = if decode_parms.is_instance(&builtin(py, "dict")?)? {
        decode_parms.clone()
    } else {
        if !is_null_or_none(py, &decode_parms)? {
            logger_values(
                py,
                "Detected invalid /DecodeParms, results might be incorrect: %(value)s (type %(type_name)s)",
                vec![
                    ("value", decode_parms.clone()),
                    ("type_name", decode_parms.get_type().getattr("__name__")?),
                ],
            )?;
        }
        generic_class(py, "DictionaryObject")?.call0()?
    };

    let predictor = parameters.call_method1("get", ("/Predictor", 1))?;

    if !predictor.ne(1)? {
        // predictor 1 == no predictor
        return Ok(str_data.unbind());
    }

    let flate_class = module.getattr("FlateDecode")?;
    let params = flate_class.call_method1("_get_parameters", (&parameters,))?;
    let columns = params.get_item(0)?;
    let colors = params.get_item(1)?;
    let bits_per_component = params.get_item(2)?;

    let columns_i: i128 = columns.extract()?;
    let colors_i: i128 = colors.extract()?;
    let bits_i: i128 = bits_per_component.extract()?;
    let row_length: i128 = (columns_i * colors_i * bits_i + 7) / 8 + 1;

    let row_length_limit = config_value(py, "flate_maximum_row_length")?;
    let row_length_limit_i: i128 = row_length_limit.extract()?;
    if row_length > row_length_limit_i {
        let message = format!(
            "Row length of {row_length} exceeds defined limit of {}.",
            pyutil::py_str(&row_length_limit)?
        );
        return Err(errors::limit_reached_error(py, &message));
    }

    if predictor.eq(2)? {
        // TIFF prediction. All parameters were validated to be >= 1, so
        // `row_length - 1 >= 1` and `columns >= 1` are safe for the indexing
        // and division below.
        let row_length = (row_length - 1) as usize;
        let bpp = row_length / (columns_i as usize);
        let mut data_bytes = extract_bytes(py, &str_data)?;
        for i in 0..data_bytes.len() {
            if i % row_length >= bpp {
                data_bytes[i] = data_bytes[i].wrapping_add(data_bytes[i - bpp]);
            }
        }
        return Ok(py_bytes_obj(py, &data_bytes).unbind());
    }
    // Upstream: `elif 10 <= predictor <= 15:` -- keep the comparison order so
    // the TypeError for non-integer predictors is identical.
    let ten = 10i64.into_pyobject(py)?.into_any();
    if ten.le(&predictor)? && predictor.le(15)? {
        // PNG prediction varies from row to row (lead byte on each row).
        let row_length = int128_obj(py, row_length)?;
        let result = module
            .getattr("FlateDecode")?
            .getattr("_decode_png_prediction")?
            .call1(call_args(py, vec![str_data, columns, row_length])?)?;
        return Ok(result.unbind());
    }
    let rendered = pyutil::py_repr(&predictor)?;
    Err(errors::pdf_read_error(
        py,
        &format!("Unsupported flatedecode predictor {rendered}"),
    ))
}

// @pypdf: pypdf.filters.FlateDecode._get_parameters
#[pyfunction]
fn filters_flate_get_parameters<'py>(
    py: Python<'py>,
    parameters: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    fn get<'py>(
        py: Python<'py>,
        parameters: &Bound<'py, PyAny>,
        key: &str,
        default: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let number_cls = generic_class(py, "NumberObject")?;
        let default_value = number_cls.call1((default,))?;
        let value = parameters.call_method1("get", (key, default_value))?;
        let value = value.call_method0("get_object")?;
        let is_int = value.is_instance(&builtin(py, "int")?)?;
        if !is_int || value.lt(1)? {
            let message = format!(
                "Expected positive number for {key}, got {}!",
                pyutil::py_str(&value)?
            );
            return Err(errors::pdf_read_error(py, &message));
        }
        Ok(value)
    }

    let columns = get(py, parameters, "/Columns", 1)?;
    let maximum_columns = config_value(py, "flate_maximum_columns")?;
    if columns.gt(&maximum_columns)? {
        let message = format!(
            "Number of columns {} exceeds defined limit of {}.",
            pyutil::py_str(&columns)?,
            pyutil::py_str(&maximum_columns)?
        );
        return Err(errors::limit_reached_error(py, &message));
    }

    let colors = get(py, parameters, "/Colors", 1)?;
    if colors.gt(16)? {
        let message = format!(
            "Color value {} exceeds limit of 16. Please open an issue if this limits valid use cases.",
            pyutil::py_str(&colors)?
        );
        return Err(errors::limit_reached_error(py, &message));
    }

    let bits_per_component = get(py, parameters, "/BitsPerComponent", 8)?;
    if bits_per_component.gt(16)? {
        let message = format!(
            "More than 16 bits per component are not allowed: {}",
            pyutil::py_str(&bits_per_component)?
        );
        return Err(errors::pdf_read_error(py, &message));
    }

    Ok(PyTuple::new(py, [columns, colors, bits_per_component])?
        .into_any()
        .unbind())
}

fn decode_png_prediction_bytes(
    py: Python<'_>,
    data: &[u8],
    columns: i64,
    row_length: i64,
) -> PyResult<Vec<u8>> {
    if row_length == 0 {
        return Err(PyZeroDivisionError::new_err("integer modulo by zero"));
    }
    if row_length < 0 {
        return Err(PyValueError::new_err("negative count"));
    }
    if columns == 0 {
        return Err(PyZeroDivisionError::new_err(
            "integer division or modulo by zero",
        ));
    }
    if columns < 0 {
        return Err(PyValueError::new_err("negative count"));
    }

    let mut data = data.to_vec();
    let row_length_usize = row_length as usize;
    let remainder = data.len() % row_length_usize;
    if remainder != 0 {
        logger_plain(py, "Image data is not rectangular. Adding padding.")?;
        data.resize(data.len() + (row_length_usize - remainder), 0);
    }

    let columns_usize = columns as usize;
    let bpp = (row_length_usize - 1) / columns_usize;
    let mut output: Vec<u8> = Vec::with_capacity(data.len().saturating_sub(data.len() / row_length_usize));
    let mut previous_row_data = vec![0u8; row_length_usize];

    let mut row = 0usize;
    while row < data.len() {
        let mut row_data = data[row..row + row_length_usize].to_vec();
        let filter_byte = row_data[0];

        match filter_byte {
            0 => {
                // PNG None Predictor
            }
            1 => {
                // PNG Sub Predictor
                for i in (bpp + 1)..row_length_usize {
                    row_data[i] = row_data[i].wrapping_add(row_data[i - bpp]);
                }
            }
            2 => {
                // PNG Up Predictor (the low 7 bits of each byte are added
                // without carrying into the next byte, the high bit is XORed).
                for i in 0..row_length_usize {
                    let low = (row_data[i] & 0x7f).wrapping_add(previous_row_data[i] & 0x7f);
                    let high = (row_data[i] ^ previous_row_data[i]) & 0x80;
                    row_data[i] = low ^ high;
                }
            }
            3 => {
                // PNG Average Predictor
                for i in 1..=bpp {
                    let floor = previous_row_data[i] / 2;
                    row_data[i] = row_data[i].wrapping_add(floor);
                }
                for i in (bpp + 1)..row_length_usize {
                    let left = row_data[i - bpp];
                    let floor = ((left as u16 + previous_row_data[i] as u16) / 2) as u8;
                    row_data[i] = row_data[i].wrapping_add(floor);
                }
            }
            4 => {
                // PNG Paeth Predictor
                for i in 1..=bpp {
                    row_data[i] = row_data[i].wrapping_add(previous_row_data[i]);
                }
                for i in (bpp + 1)..row_length_usize {
                    let left = row_data[i - bpp] as i32;
                    let up = previous_row_data[i] as i32;
                    let up_left = previous_row_data[i - bpp] as i32;

                    // The distances of p = left + up - up_left to left, up and
                    // up_left, avoiding the `abs()` call in the inner loop.
                    let mut dist_left = up - up_left;
                    let mut dist_up = left - up_left;
                    let mut dist_up_left = dist_left + dist_up;
                    if dist_left < 0 {
                        dist_left = -dist_left;
                    }
                    if dist_up < 0 {
                        dist_up = -dist_up;
                    }
                    if dist_up_left < 0 {
                        dist_up_left = -dist_up_left;
                    }

                    let paeth = if dist_left <= dist_up && dist_left <= dist_up_left {
                        left
                    } else if dist_up <= dist_up_left {
                        up
                    } else {
                        up_left
                    };

                    row_data[i] = row_data[i].wrapping_add(paeth as u8);
                }
            }
            _ => {
                return Err(errors::pdf_read_error(
                    py,
                    &format!("Unsupported PNG filter {filter_byte:?}"),
                ));
            }
        }
        previous_row_data = row_data;
        output.extend_from_slice(&previous_row_data[1..]);
        row += row_length_usize;
    }
    Ok(output)
}

// @pypdf: pypdf.filters.FlateDecode._decode_png_prediction
#[pyfunction]
fn filters_flate_decode_png_prediction<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
    columns: &Bound<'py, PyAny>,
    row_length: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let data = extract_bytes(py, data)?;
    let columns: i64 = columns.extract()?;
    let row_length: i64 = row_length.extract()?;
    let output = decode_png_prediction_bytes(py, &data, columns, row_length)?;
    Ok(py_bytes_obj(py, &output).unbind())
}

// @pypdf: pypdf.filters.FlateDecode.encode
#[pyfunction]
#[pyo3(signature = (data, level = -1))]
fn filters_flate_encode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
    level: i64,
) -> PyResult<Py<PyAny>> {
    Ok(py.import("zlib")?
        .call_method1("compress", (data, level))?
        .unbind())
}

// ---------------------------------------------------------------------------
// ASCIIHexDecode
// ---------------------------------------------------------------------------

// @pypdf: pypdf.filters.ASCIIHexDecode
// @pypdf: pypdf.filters.ASCIIHexDecode.decode
#[pyfunction]
fn filters_asciihex_decode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let data = if data.is_instance_of::<PyString>() {
        data.call_method0("encode")?
    } else {
        data.clone()
    };
    let data = extract_bytes(py, &data)?;

    // Stop at EOD
    let eod = data.iter().position(|byte| *byte == b'>');
    let hex_data: Vec<u8> = match eod {
        None => {
            logger_plain(py, "missing EOD in ASCIIHexDecode, check if output is OK")?;
            data
        }
        Some(eod) => data[..eod].to_vec(),
    };

    // Remove whitespace
    let mut hex_data: Vec<u8> = hex_data
        .into_iter()
        .filter(|byte| !WHITESPACES_BYTES.contains(byte))
        .collect();

    // Pad if odd length
    if hex_data.len() % 2 == 1 {
        hex_data.push(b'0');
    }

    let binascii = py.import("binascii")?;
    match binascii.call_method1("unhexlify", (py_bytes_obj(py, &hex_data),)) {
        Ok(result) => Ok(result.unbind()),
        Err(error) => {
            let error_str = pyutil::py_str(error.value(py))?;
            let message =
                format!("Invalid hexadecimal character in ASCIIHexDecode stream: {error_str}");
            let wrapped = errors::pdf_stream_error(py, &message);
            Err(error_with_cause(py, wrapped, error.value(py).as_any()))
        }
    }
}

// ---------------------------------------------------------------------------
// RunLengthDecode
// ---------------------------------------------------------------------------

// @pypdf: pypdf.filters.RunLengthDecode
// @pypdf: pypdf.filters.RunLengthDecode.decode
#[pyfunction]
fn filters_runlength_decode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let data = extract_bytes(py, data)?;
    let data_length = data.len();
    let maximum_output_length: i128 =
        config_value(py, "run_length_maximum_output_length")?.extract()?;

    let mut output: Vec<u8> = Vec::new();
    let mut index: usize = 0;
    let mut total_length: i128 = 0;
    loop {
        if index >= data_length {
            logger_plain(
                py,
                "missing EOD in RunLengthDecode, check if output is OK",
            )?;
            break; // Reached end of string without an EOD
        }
        let length_byte = data[index];
        index += 1;
        if length_byte == 128 {
            if index < data_length {
                // A faulty trailing newline inside a multi-encoded stream can be
                // ignored; otherwise stop after the first EOD marker and warn.
                if index == data_length - 1 && data[index] == b'\n' {
                    logger_plain(
                        py,
                        "Found trailing newline in stream data, check if output is OK",
                    )?;
                    break;
                }
                logger_plain(py, "Early EOD in RunLengthDecode, check if output is OK")?;
            }
            break;
        }

        let run_length: i128 = if length_byte < 128 {
            let count = length_byte as usize + 1;
            let end = min(index + count, data_length);
            output.extend_from_slice(&data[index..end]);
            index += count;
            count as i128
        } else {
            if index >= data_length {
                logger_plain(
                    py,
                    "Missing EOD in RunLengthDecode, check if output is OK",
                )?;
                break; // Reached end of string without the replicated byte
            }
            let count = 257 - length_byte as usize;
            output.resize(output.len() + count, data[index]);
            index += 1;
            count as i128
        };
        total_length += run_length;
        if total_length > maximum_output_length {
            return Err(errors::limit_reached_error(
                py,
                "Limit reached while decompressing.",
            ));
        }
    }
    Ok(py_bytes_obj(py, &output).unbind())
}

// ---------------------------------------------------------------------------
// LZWDecode (reuses the ported LzwCodec)
// ---------------------------------------------------------------------------

// @pypdf: pypdf.filters.LZWDecode
// @pypdf: pypdf.filters.LZWDecode.Decoder
// @pypdf: pypdf.filters.LZWDecode.Decoder.__init__
// @pypdf: pypdf.filters.LZWDecode.Decoder.decode
#[pyfunction]
fn filters_lzw_decoder_decode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let max_output_length = config_value(py, "lzw_maximum_output_length")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("max_output_length", max_output_length)?;
    let codecs = py.import("pypdf._codecs._codecs")?;
    let codec = codecs.getattr("LzwCodec")?.call((), Some(&kwargs))?;
    Ok(codec.call_method1("decode", (data,))?.unbind())
}

// @pypdf: pypdf.filters.LZWDecode.decode
#[pyfunction]
fn filters_lzw_decode<'py>(py: Python<'py>, data: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let module = filters_module(py)?;
    let decoder_class = module.getattr("LZWDecode")?.getattr("Decoder")?;
    let decoder = decoder_class.call1((data,))?;
    Ok(decoder.call_method0("decode")?.unbind())
}

// ---------------------------------------------------------------------------
// ASCII85Decode
// ---------------------------------------------------------------------------

// @pypdf: pypdf.filters.ASCII85Decode
// @pypdf: pypdf.filters.ASCII85Decode.decode
#[pyfunction]
fn filters_ascii85_decode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let data = if data.is_instance_of::<PyString>() {
        data.call_method0("encode")?
    } else {
        data.clone()
    };
    let whitespaces = py.import("pypdf._utils")?.getattr("WHITESPACES_AS_BYTES")?;

    let mut data = data.call_method1("strip", (&whitespaces,))?;
    let length = data.len()?;
    if length > 2 && data.call_method1("endswith", (py_bytes_obj(py, b">"),))?.is_truthy()? {
        let head = data.get_item(py_slice_i64(py, None, Some(-1))?)?;
        let head = head.call_method1("rstrip", (&whitespaces,))?;
        let last = data.get_item(py_slice_i64(py, Some(-1), None)?)?;
        data = py_add(&head, &last)?;
    }

    let base64 = py.import("base64")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("adobe", true)?;
    kwargs.set_item("ignorechars", &whitespaces)?;
    match base64.getattr("a85decode")?.call((&data,), Some(&kwargs)) {
        Ok(result) => Ok(result.unbind()),
        Err(error) => {
            if !error.is_instance_of::<pyo3::exceptions::PyValueError>(py) {
                return Err(error);
            }
            let value = error.value(py).clone();
            let args = value.getattr("args")?;
            // Upstream: `if error.args[0] == "...":` -- an empty `args` tuple
            // raises the same `IndexError` as Python would at that point.
            let first = args.get_item(0)?;
            let missing_marker = first.eq("Ascii85 encoded byte sequences must end with b'~>'")?;
            if !missing_marker {
                return Err(error);
            }
            logger_plain(py, "Ignoring missing Ascii85 end marker.")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("adobe", false)?;
            kwargs.set_item("ignorechars", &whitespaces)?;
            Ok(base64
                .getattr("a85decode")?
                .call((&data,), Some(&kwargs))?
                .unbind())
        }
    }
}

// ---------------------------------------------------------------------------
// DCTDecode / JPXDecode (pass-through)
// ---------------------------------------------------------------------------

// @pypdf: pypdf.filters.DCTDecode
// @pypdf: pypdf.filters.DCTDecode.decode
#[pyfunction]
fn filters_dct_decode<'py>(py: Python<'py>, data: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let _ = py;
    Ok(data.clone().unbind())
}

// @pypdf: pypdf.filters.JPXDecode
// @pypdf: pypdf.filters.JPXDecode.decode
#[pyfunction]
fn filters_jpx_decode<'py>(py: Python<'py>, data: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let _ = py;
    Ok(data.clone().unbind())
}

// ---------------------------------------------------------------------------
// BrotliDecode
// ---------------------------------------------------------------------------

// @pypdf: pypdf.filters.BrotliDecode
// @pypdf: pypdf.filters.BrotliDecode._check_brotli_available
#[pyfunction]
fn filters_brotli_check_available(py: Python<'_>) -> PyResult<()> {
    let module = filters_module(py)?;
    let spec = module.getattr("find_spec")?.call1(("brotli",))?;
    if !spec.is_none() {
        return Ok(());
    }
    Err(errors::dependency_error(
        py,
        "brotli is required for BrotliDecode. Install it with: pip install pypdf[brotli]",
    ))
}

// @pypdf: pypdf.filters.BrotliDecode.decode
#[pyfunction]
fn filters_brotli_decode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    filters_module(py)?
        .getattr("BrotliDecode")?
        .getattr("_check_brotli_available")?
        .call0()?;

    let brotli = py.import("brotli")?;
    let brotli_error = brotli.getattr("error")?;
    let decompressor = brotli.getattr("Decompressor")?.call0()?;
    let configuration = configuration(py)?;
    let mut remaining: i128 = configuration
        .getattr("brotli_maximum_output_length")?
        .extract()?;
    let data = extract_bytes(py, data)?;

    let mut output: Vec<u8> = Vec::new();
    let chunk_size: usize = 65_535;
    let mut offset: usize = 0;
    while offset < data.len() {
        let end = min(offset + chunk_size, data.len());
        let chunk = py_bytes_obj(py, &data[offset..end]);
        let kwargs = PyDict::new(py);
        kwargs.set_item("output_buffer_limit", int128_obj(py, remaining)?)?;
        match decompressor.call_method("process", (chunk,), Some(&kwargs)) {
            Ok(part) => {
                let part = extract_bytes(py, &part)?;
                output.extend_from_slice(&part);
                remaining -= part.len() as i128;
                if remaining < 0 {
                    return Err(errors::limit_reached_error(
                        py,
                        "Limit reached while decompressing.",
                    ));
                }
            }
            Err(error) => {
                if error.is_instance(py, &brotli_error) {
                    let wrapped = errors::limit_reached_error(
                        py,
                        "Limit reached while decompressing.",
                    );
                    return Err(error_with_cause(py, wrapped, error.value(py).as_any()));
                }
                return Err(error);
            }
        }
        offset = end;
    }
    Ok(py_bytes_obj(py, &output).unbind())
}

// @pypdf: pypdf.filters.BrotliDecode.encode
#[pyfunction]
fn filters_brotli_encode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    filters_module(py)?
        .getattr("BrotliDecode")?
        .getattr("_check_brotli_available")?
        .call0()?;
    let compressed = py.import("brotli")?.call_method1("compress", (data,))?;
    Ok(builtin(py, "bytes")?.call1((compressed,))?.unbind())
}

// ---------------------------------------------------------------------------
// CCITTParameters / CCITTFaxDecode
// ---------------------------------------------------------------------------

// @pypdf: pypdf.filters.CCITTParameters
// @pypdf: pypdf.filters.CCITTParameters.K
// @pypdf: pypdf.filters.CCITTParameters.columns
// @pypdf: pypdf.filters.CCITTParameters.rows
// @pypdf: pypdf.filters.CCITTParameters.EndOfLine
// @pypdf: pypdf.filters.CCITTParameters.EncodedByteAlign
// @pypdf: pypdf.filters.CCITTParameters.EndOfBlock
// @pypdf: pypdf.filters.CCITTParameters.BlackIs1
// @pypdf: pypdf.filters.CCITTParameters.DamagedRowsBeforeError
// @pypdf: pypdf.filters.CCITTParameters.group
#[pyfunction]
fn filters_ccitt_parameters_group(py: Python<'_>, k: &Bound<'_, PyAny>) -> PyResult<i64> {
    let _ = py;
    if k.lt(0)? {
        Ok(4) // Pure two-dimensional encoding (Group 4)
    } else {
        Ok(3) // K == 0: 1-D (Group 3); K > 0: mixed 1-D/2-D (Group 3)
    }
}

fn ccitt_get_integer<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
    key: &str,
) -> PyResult<Bound<'py, PyAny>> {
    let mut value = value.clone();
    if value.is_instance(&generic_class(py, "PdfObject")?)? {
        value = value.call_method0("get_object")?;
    }
    if !value.is_instance(&builtin(py, "int")?)? {
        let message = format!(
            "Expected integer for {key}, got {}: {}!",
            type_name(&value)?,
            pyutil::py_repr(&value)?
        );
        return Err(errors::pdf_read_error(py, &message));
    }
    Ok(value)
}

fn ccitt_get_boolean<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
    key: &str,
) -> PyResult<Bound<'py, PyAny>> {
    if !value.is_instance(&generic_class(py, "BooleanObject")?)? {
        let message = format!(
            "Expected boolean for {key}, got {}: {}!",
            type_name(value)?,
            pyutil::py_repr(value)?
        );
        return Err(errors::pdf_read_error(py, &message));
    }
    value.getattr("value")
}

// @pypdf: pypdf.filters.CCITTFaxDecode
// @pypdf: pypdf.filters.CCITTFaxDecode._MAXIMUM_UNSIGNED_LONG
// @pypdf: pypdf.filters.CCITTFaxDecode._get_parameters
#[pyfunction]
fn filters_ccitt_get_parameters<'py>(
    py: Python<'py>,
    parameters: Option<&Bound<'py, PyAny>>,
    rows: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let ccitt_class = filters_module(py)?.getattr("CCITTParameters")?;

    let rows_value: Bound<'py, PyAny> = if is_null_or_none(py, rows)? {
        0i64.into_pyobject(py)?.into_any()
    } else {
        ccitt_get_integer(py, rows, "/Rows")?
    };
    let kwargs = PyDict::new(py);
    kwargs.set_item("rows", rows_value)?;
    let ccitt_parameters = ccitt_class.call((), Some(&kwargs))?;

    let parameters: Bound<'py, PyAny> = match parameters {
        Some(parameters) => parameters.clone(),
        None => py.None().into_bound(py),
    };
    if parameters.is_truthy()? {
        let parameters_unwrapped = parameters.call_method0("get_object")?;
        let decode_parm_list: Vec<Bound<'py, PyAny>> =
            if parameters_unwrapped.is_instance(&generic_class(py, "ArrayObject")?)? {
                parameters_unwrapped.try_iter()?.collect::<PyResult<Vec<_>>>()?
            } else {
                vec![parameters_unwrapped]
            };
        for decode_parm in decode_parm_list {
            let decode_parm = if decode_parm.is_instance(&generic_class(py, "PdfObject")?)? {
                decode_parm.call_method0("get_object")?
            } else {
                decode_parm
            };
            if is_null_or_none(py, &decode_parm)? {
                continue;
            }
            if !decode_parm.is_instance(&builtin(py, "dict")?)? {
                logger_values(
                    py,
                    "Ignoring invalid DecodeParms entry: %(value)s",
                    vec![("value", decode_parm.clone())],
                )?;
                continue;
            }
            if decode_parm.contains("/K")? {
                let value = decode_parm.get_item("/K")?;
                let value = ccitt_get_integer(py, &value, "/K")?;
                ccitt_parameters.setattr("K", value)?;
            }
            if decode_parm.contains("/Columns")? {
                let value = decode_parm.get_item("/Columns")?;
                let value = ccitt_get_integer(py, &value, "/Columns")?;
                ccitt_parameters.setattr("columns", value)?;
            }
            if decode_parm.contains("/BlackIs1")? {
                let value = decode_parm.get_item("/BlackIs1")?;
                let value = ccitt_get_boolean(py, &value, "/BlackIs1")?;
                ccitt_parameters.setattr("BlackIs1", value)?;
            }
        }
    }

    let columns = ccitt_parameters.getattr("columns")?;
    if columns.lt(0)? || columns.gt(MAXIMUM_UNSIGNED_LONG)? {
        let message = format!(
            "Expected valid 32 bit unsigned value for /Columns, got {}!",
            pyutil::py_str(&columns)?
        );
        return Err(errors::pdf_read_error(py, &message));
    }
    let rows = ccitt_parameters.getattr("rows")?;
    if rows.lt(0)? || rows.gt(MAXIMUM_UNSIGNED_LONG)? {
        let message = format!(
            "Expected valid 32 bit unsigned value for /Rows, got {}!",
            pyutil::py_str(&rows)?
        );
        return Err(errors::pdf_read_error(py, &message));
    }
    Ok(ccitt_parameters.unbind())
}

// @pypdf: pypdf.filters.CCITTFaxDecode.decode
#[pyfunction]
#[pyo3(signature = (data, decode_parms = None, height = None))]
fn filters_ccitt_decode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
    decode_parms: Option<&Bound<'py, PyAny>>,
    height: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let module = filters_module(py)?;
    let decode_parms: Bound<'py, PyAny> = match decode_parms {
        Some(value) => value.clone(),
        None => py.None().into_bound(py),
    };
    let height: Bound<'py, PyAny> = match height {
        Some(value) => value.clone(),
        None => 0i64.into_pyobject(py)?.into_any(),
    };
    let params = module
        .getattr("CCITTFaxDecode")?
        .call_method1("_get_parameters", (decode_parms, height))?;

    let img_size = data.len()?;

    // Upstream:
    //   tiff_header_struct = "<2sHLH" + "HHLL" * 8 + "H"
    //   tiff_header = struct.pack(tiff_header_struct, b"II", 42, 8, 8, ...)
    // Packing through the `struct` module keeps the exact byte layout and the
    // exact `struct.error` behaviour for out-of-range values.
    let tiff_header_struct = format!("<2sHLH{}H", "HHLL".repeat(8));
    let struct_module = py.import("struct")?;
    let header_size = struct_module.call_method1("calcsize", (&tiff_header_struct,))?;
    let group = params.getattr("group")?;
    let black_is1 = builtin(py, "int")?.call1((params.getattr("BlackIs1")?,))?;
    let rows = params.getattr("rows")?;
    let columns = params.getattr("columns")?;

    // `struct.pack(tiff_header_struct, b"II", 42, 8, 8, ...)`: the format string
    // is the first positional argument.
    let mut pack_args: Vec<Bound<'py, PyAny>> = Vec::with_capacity(38);
    pack_args.push((*tiff_header_struct).into_pyobject(py)?.into_any());
    pack_args.push(py_bytes_obj(py, b"II"));
    for value in [42i64, 8, 8] {
        pack_args.push(value.into_pyobject(py)?.into_any());
    }
    let push_tag = |args: &mut Vec<Bound<'py, PyAny>>,
                        tag: i64,
                        kind: i64,
                        count: i64,
                        value: Bound<'py, PyAny>|
     -> PyResult<()> {
        args.push(tag.into_pyobject(py)?.into_any());
        args.push(kind.into_pyobject(py)?.into_any());
        args.push(count.into_pyobject(py)?.into_any());
        args.push(value);
        Ok(())
    };
    push_tag(&mut pack_args, 256, 4, 1, columns)?;
    push_tag(&mut pack_args, 257, 4, 1, rows.clone())?;
    push_tag(
        &mut pack_args,
        258,
        3,
        1,
        1i64.into_pyobject(py)?.into_any(),
    )?;
    push_tag(&mut pack_args, 259, 3, 1, group)?;
    push_tag(&mut pack_args, 262, 3, 1, black_is1)?;
    push_tag(&mut pack_args, 273, 4, 1, header_size)?;
    push_tag(&mut pack_args, 278, 4, 1, rows)?;
    push_tag(
        &mut pack_args,
        279,
        4,
        1,
        (img_size as i64).into_pyobject(py)?.into_any(),
    )?;
    pack_args.push(0i64.into_pyobject(py)?.into_any());

    let args = PyTuple::new(py, pack_args)?;
    let header = struct_module.call_method1("pack", args)?;
    Ok(py_add(&header, data)?.unbind())
}

// ---------------------------------------------------------------------------
// JBIG2Decode
// ---------------------------------------------------------------------------

fn jbig2_subprocess_run<'py>(
    _py: Python<'py>,
    module: &Bound<'py, PyModule>,
    args: &Bound<'py, PyAny>,
    kwargs: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyAny>> {
    module
        .getattr("subprocess")?
        .getattr("run")?
        .call((args,), Some(kwargs))
}

fn jbig2_decode_impl<'py>(
    py: Python<'py>,
    module: &Bound<'py, PyModule>,
    data: &Bound<'py, PyAny>,
    decode_parms: &Bound<'py, PyAny>,
    directory: &Bound<'py, PyAny>,
    paths: &Bound<'py, PyList>,
) -> PyResult<Py<PyAny>> {
    if decode_parms.is_truthy()? && decode_parms.contains("/JBIG2Globals")? {
        let jbig2_globals = decode_parms.get_item("/JBIG2Globals")?;
        if !is_null_or_none(py, &jbig2_globals)? {
            let pointer = jbig2_globals.call_method0("get_object")?;
            if !is_null_or_none(py, &pointer)?
                && pointer.is_instance(&generic_class(py, "StreamObject")?)?
            {
                let path = directory.call_method1("joinpath", ("globals.jbig2",))?;
                path.call_method1("write_bytes", (pointer.call_method0("get_data")?,))?;
                paths.append(&path)?;
            }
        }
    }

    let path = directory.call_method1("joinpath", ("image.jbig2",))?;
    path.call_method1("write_bytes", (data,))?;
    paths.append(&path)?;

    let environment = py.import("os")?.getattr("environ")?.call_method0("copy")?;
    environment.set_item("LC_ALL", "C")?;

    let binary = configuration(py)?.getattr("jbig2dec_binary")?;
    let maximum_output_length = pyutil::py_str(&config_value(
        py,
        "jbig2_maximum_output_length",
    )?)?;

    let arguments = PyList::empty(py);
    arguments.append(binary)?;
    arguments.append("--embedded")?;
    arguments.append("--format")?;
    arguments.append("png")?;
    arguments.append("--output")?;
    arguments.append("-")?;
    arguments.append("-M")?;
    arguments.append(maximum_output_length)?;
    for path in paths.iter() {
        arguments.append(path)?;
    }

    let kwargs = PyDict::new(py);
    kwargs.set_item("capture_output", true)?;
    kwargs.set_item("env", environment)?;
    let result = jbig2_subprocess_run(py, module, &arguments.into_any(), &kwargs)?;

    let stderr = result.getattr("stderr")?;
    if stderr.contains(py_bytes_obj(py, b"unrecognized option '--embedded'"))?
        || stderr.contains(py_bytes_obj(py, b"unrecognized option '-M'"))?
    {
        return Err(errors::dependency_error(py, "jbig2dec>=0.19 is required."));
    }
    if stderr.contains(py_bytes_obj(
        py,
        b"FATAL ERROR failed to allocate image data buffer",
    ))? {
        let message = format!(
            "Memory limit reached while reading JBIG2 data:\n{}",
            pyutil::py_str(&stderr.call_method1("decode", ("utf-8",))?)?
        );
        return Err(errors::limit_reached_error(py, &message));
    }
    if stderr.is_truthy()? {
        let decoded = stderr.call_method1("decode", ("utf-8",))?;
        for line in decoded.call_method0("splitlines")?.try_iter()? {
            let line = line?;
            pyutil::logger_warning(
                py,
                SOURCE,
                &pyutil::py_str(&line)?,
                None,
            )?;
        }
    }
    let returncode: i64 = result.getattr("returncode")?.extract()?;
    if returncode != 0 {
        let message = format!("Unable to decode JBIG2 data. Exit code: {returncode}");
        return Err(errors::pdf_stream_error(py, &message));
    }
    Ok(result.getattr("stdout")?.unbind())
}

// @pypdf: pypdf.filters.JBIG2Decode
// @pypdf: pypdf.filters.JBIG2Decode.decode
#[pyfunction]
#[pyo3(signature = (data, decode_parms = None))]
fn filters_jbig2_decode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
    decode_parms: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let binary = configuration(py)?.getattr("jbig2dec_binary")?;
    if binary.is_none() {
        return Err(errors::dependency_error(
            py,
            "jbig2dec binary is not available.",
        ));
    }

    let module = filters_module(py)?;
    let decode_parms: Bound<'py, PyAny> = match decode_parms {
        Some(value) => value.clone(),
        None => py.None().into_bound(py),
    };

    let tempdir = py.import("tempfile")?.getattr("TemporaryDirectory")?.call0()?;
    let entered = tempdir.call_method0("__enter__")?;
    let directory = py
        .import("pathlib")?
        .getattr("Path")?
        .call1((entered,))?;
    let paths = PyList::empty(py);
    let result = jbig2_decode_impl(py, &module, data, &decode_parms, &directory, &paths);
    let _ = tempdir.call_method1("__exit__", (py.None(), py.None(), py.None()));
    result
}

// @pypdf: pypdf.filters.JBIG2Decode._is_binary_compatible
#[pyfunction]
fn filters_jbig2_is_binary_compatible(py: Python<'_>) -> PyResult<bool> {
    let module = filters_module(py)?;
    let binary = configuration(py)?.getattr("jbig2dec_binary")?;
    if !binary.is_truthy()? {
        return Ok(false);
    }
    let arguments = PyList::empty(py);
    arguments.append(binary)?;
    arguments.append("--version")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("capture_output", true)?;
    kwargs.set_item("text", true)?;
    let result = jbig2_subprocess_run(py, &module, &arguments.into_any(), &kwargs)?;

    let stdout = result.getattr("stdout")?;
    let split_kwargs = PyDict::new(py);
    split_kwargs.set_item("maxsplit", 1)?;
    let parts = stdout.call_method("split", (" ",), Some(&split_kwargs))?;
    let version = parts.get_item(1)?;

    let version_class = py.import("pypdf._utils")?.getattr("Version")?;
    let parsed = version_class.call1((version,))?;
    let minimum = version_class.call1(("0.19",))?;
    parsed.ge(&minimum)
}

// ---------------------------------------------------------------------------
// Inline-image filter deprecation + decode_stream_data
// ---------------------------------------------------------------------------

// @pypdf: pypdf.filters._deprecate_inline_image_filters
#[pyfunction]
fn filters_deprecate_inline_image_filters(
    py: Python<'_>,
    filter_name: &Bound<'_, PyAny>,
    old_name: &Bound<'_, PyAny>,
    new_name: &Bound<'_, PyAny>,
) -> PyResult<()> {
    if filter_name.ne(old_name)? {
        return Ok(());
    }
    let message = format!(
        "The filter name {} is deprecated and has been removed in pypdf 7.0.0. Use {} instead.",
        pyutil::py_str(old_name)?,
        pyutil::py_str(new_name)?
    );
    let deprecation = filters_module(py)?.getattr("deprecation")?;
    deprecation.call1((message,))?;
    Ok(())
}

fn deprecate_inline<'py>(
    py: Python<'py>,
    module: &Bound<'py, PyModule>,
    filter_name: &Bound<'py, PyAny>,
    old_name: &str,
    new_name: &str,
) -> PyResult<()> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("filter_name", filter_name)?;
    kwargs.set_item("old_name", old_name)?;
    kwargs.set_item("new_name", new_name)?;
    module
        .getattr("_deprecate_inline_image_filters")?
        .call((), Some(&kwargs))?;
    Ok(())
}

/// `isinstance(obj, (dict, list))` (used for a single /DecodeParms entry).
fn dict_or_list_check(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(obj.is_instance(&builtin(py, "dict")?)? || obj.is_instance(&builtin(py, "list")?)?)
}

/// `isinstance(obj, (list, tuple))` (used for the /DecodeParms container).
fn list_or_tuple_check(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(obj.is_instance(&builtin(py, "list")?)? || obj.is_instance(&builtin(py, "tuple")?)?)
}

// @pypdf: pypdf.filters.decode_stream_data
#[pyfunction]
fn filters_decode_stream_data<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let module = filters_module(py)?;
    let configuration = configuration(py)?;

    let mut filters = stream.call_method1("get", ("/Filter", ()))?;
    if filters.is_instance(&generic_class(py, "IndirectObject")?)? {
        filters = filters.call_method0("get_object")?;
    }
    let filters: Vec<Bound<'py, PyAny>> =
        if filters.is_instance(&generic_class(py, "ArrayObject")?)? {
            filters.try_iter()?.collect::<PyResult<Vec<_>>>()?
        } else {
            // We have a single filter instance.
            vec![filters]
        };
    let filter_count = filters.len();
    let maximum_filter_count = configuration.getattr("stream_filters_maximum_length")?;
    if (filter_count as i128) > maximum_filter_count.extract::<i128>()? {
        let message = format!(
            "Maximum filter count {} exceeded: {filter_count}",
            pyutil::py_str(&maximum_filter_count)?
        );
        return Err(errors::limit_reached_error(py, &message));
    }

    let mut default_decode_parms: Vec<Bound<'py, PyAny>> = Vec::with_capacity(filter_count);
    for _ in 0..filter_count {
        default_decode_parms.push(generic_class(py, "DictionaryObject")?.call0()?);
    }
    let default_decode_parms = PyTuple::new(py, default_decode_parms)?;
    let decode_parms = stream.call_method1("get", ("/DecodeParms", default_decode_parms))?;
    let decode_parms: Vec<Bound<'py, PyAny>> = if list_or_tuple_check(py, &decode_parms)? {
        decode_parms.try_iter()?.collect::<PyResult<Vec<_>>>()?
    } else {
        vec![decode_parms]
    };

    let mut data = stream.getattr("_data")?;
    // If there is no data to decode, we should not try to decode it.
    if !data.is_truthy()? {
        return Ok(data.unbind());
    }

    let decoding_work_maximum_length_obj =
        configuration.getattr("stream_decoding_work_maximum_length")?;
    let decoding_work_maximum_length: i128 = decoding_work_maximum_length_obj.extract()?;
    let mut accumulated_decoding_work: i128 = 0;
    let work_limit_message = || {
        format!(
            "Stream decoding exceeded the maximum accumulated decoding work of {} bytes.",
            pyutil::py_str(&decoding_work_maximum_length_obj).unwrap_or_default()
        )
    };

    let iterations = min(filters.len(), decode_parms.len());
    for index in 0..iterations {
        let filter_name = &filters[index];

        accumulated_decoding_work += data.len()? as i128;
        if accumulated_decoding_work > decoding_work_maximum_length {
            return Err(errors::limit_reached_error(py, &work_limit_message()));
        }

        let params = decode_parms[index].call_method0("get_object")?;
        let params: Bound<'py, PyAny> = if is_null_or_none(py, &params)? {
            // The decoders are typed for a DictionaryObject; a plain {} is not
            // one, so a null /DecodeParms entry would hand them the wrong type.
            generic_class(py, "DictionaryObject")?.call0()?
        } else if !dict_or_list_check(py, &params)? {
            logger_values(
                py,
                "Ignoring invalid DecodeParms: %(value)s",
                vec![("value", params.clone())],
            )?;
            generic_class(py, "DictionaryObject")?.call0()?
        } else {
            params
        };

        if eq_any(filter_name, &["/ASCIIHexDecode", "/AHx"])? {
            deprecate_inline(
                py,
                &module,
                filter_name,
                "/AHx",
                "/ASCIIHexDecode",
            )?;
            data = class_static_call(
                py,
                &module,
                "ASCIIHexDecode",
                "decode",
                call_args(py, vec![data])?,
            )?;
        } else if eq_any(filter_name, &["/ASCII85Decode", "/A85"])? {
            deprecate_inline(py, &module, filter_name, "/A85", "/ASCII85Decode")?;
            data = class_static_call(
                py,
                &module,
                "ASCII85Decode",
                "decode",
                call_args(py, vec![data])?,
            )?;
        } else if eq_any(filter_name, &["/LZWDecode", "/LZW"])? {
            deprecate_inline(py, &module, filter_name, "/LZW", "/LZWDecode")?;
            data = class_static_call(
                py,
                &module,
                "LZWDecode",
                "decode",
                call_args(py, vec![data, params])?,
            )?;
        } else if eq_any(filter_name, &["/FlateDecode", "/Fl"])? {
            deprecate_inline(py, &module, filter_name, "/Fl", "/FlateDecode")?;
            data = class_static_call(
                py,
                &module,
                "FlateDecode",
                "decode",
                call_args(py, vec![data, params])?,
            )?;
        } else if eq_any(filter_name, &["/RunLengthDecode", "/RL"])? {
            deprecate_inline(py, &module, filter_name, "/RL", "/RunLengthDecode")?;
            data = class_static_call(
                py,
                &module,
                "RunLengthDecode",
                "decode",
                call_args(py, vec![data])?,
            )?;
        } else if eq_any(filter_name, &["/CCITTFaxDecode", "/CCF"])? {
            deprecate_inline(py, &module, filter_name, "/CCF", "/CCITTFaxDecode")?;
            let height = stream.call_method1("get", ("/Height", 0))?;
            data = class_static_call(
                py,
                &module,
                "CCITTFaxDecode",
                "decode",
                call_args(py, vec![data, params, height])?,
            )?;
        } else if eq_any(filter_name, &["/DCTDecode", "/DCT"])? {
            deprecate_inline(py, &module, filter_name, "/DCT", "/DCTDecode")?;
            data = class_static_call(
                py,
                &module,
                "DCTDecode",
                "decode",
                call_args(py, vec![data])?,
            )?;
        } else if filter_name.eq("/JPXDecode")? {
            data = class_static_call(
                py,
                &module,
                "JPXDecode",
                "decode",
                call_args(py, vec![data])?,
            )?;
        } else if filter_name.eq("/BrotliDecode")? {
            data = class_static_call(
                py,
                &module,
                "BrotliDecode",
                "decode",
                call_args(py, vec![data])?,
            )?;
        } else if filter_name.eq("/JBIG2Decode")? {
            data = class_static_call(
                py,
                &module,
                "JBIG2Decode",
                "decode",
                call_args(py, vec![data, params])?,
            )?;
        } else if filter_name.eq("/Crypt")? {
            if params.contains("/Name")? || params.contains("/Type")? {
                return Err(PyNotImplementedError::new_err(
                    "/Crypt filter with /Name or /Type not supported yet",
                ));
            }
        } else {
            return Err(PyNotImplementedError::new_err(format!(
                "Unsupported filter {}",
                pyutil::py_str(filter_name)?
            )));
        }

        accumulated_decoding_work += data.len()? as i128;
        if accumulated_decoding_work > decoding_work_maximum_length {
            return Err(errors::limit_reached_error(py, &work_limit_message()));
        }
    }

    Ok(data.unbind())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(filters_decompress_with_limit, module)?)?;
    module.add_function(wrap_pyfunction!(filters_decompress, module)?)?;
    module.add_function(wrap_pyfunction!(filters_flate_decode, module)?)?;
    module.add_function(wrap_pyfunction!(filters_flate_get_parameters, module)?)?;
    module.add_function(wrap_pyfunction!(
        filters_flate_decode_png_prediction,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(filters_flate_encode, module)?)?;
    module.add_function(wrap_pyfunction!(filters_asciihex_decode, module)?)?;
    module.add_function(wrap_pyfunction!(filters_runlength_decode, module)?)?;
    module.add_function(wrap_pyfunction!(filters_lzw_decoder_decode, module)?)?;
    module.add_function(wrap_pyfunction!(filters_lzw_decode, module)?)?;
    module.add_function(wrap_pyfunction!(filters_ascii85_decode, module)?)?;
    module.add_function(wrap_pyfunction!(filters_dct_decode, module)?)?;
    module.add_function(wrap_pyfunction!(filters_jpx_decode, module)?)?;
    module.add_function(wrap_pyfunction!(filters_brotli_check_available, module)?)?;
    module.add_function(wrap_pyfunction!(filters_brotli_decode, module)?)?;
    module.add_function(wrap_pyfunction!(filters_brotli_encode, module)?)?;
    module.add_function(wrap_pyfunction!(filters_ccitt_parameters_group, module)?)?;
    module.add_function(wrap_pyfunction!(filters_ccitt_get_parameters, module)?)?;
    module.add_function(wrap_pyfunction!(filters_ccitt_decode, module)?)?;
    module.add_function(wrap_pyfunction!(filters_jbig2_decode, module)?)?;
    module.add_function(wrap_pyfunction!(filters_jbig2_is_binary_compatible, module)?)?;
    module.add_function(wrap_pyfunction!(
        filters_deprecate_inline_image_filters,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(filters_decode_stream_data, module)?)?;
    Ok(())
}
