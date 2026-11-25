//! Rust port of `pypdf/generic/_font.py` (upstream pypdf 7.1.1).
//!
//! # Design: Python dataclasses, Rust logic
//!
//! `FontDescriptor`, `CoreFontMetrics` and `Font` are `@dataclass`es whose
//! generated `__init__`/`__eq__`/`__repr__` and `dataclasses.asdict()` output are
//! observable Python API (`tests/test_text_extraction.py::test_font_class_to_dict`),
//! so the class skeleton, fields, defaults and decorators stay verbatim in the
//! shim `python/pypdf/generic/_font.py`.  Every method body delegates to a
//! `#[pyfunction]` implemented here; no algorithm lives in the shim.
//!
//! # fontTools parity
//!
//! pypdf only *reads* fonts via `fontTools` (`TTFont` table access,
//! `cffLib.CFFFontSet`), so the port calls the installed `fontTools` package
//! through PyO3 at exactly the same points as upstream instead of
//! reimplementing it (`docs/FIDELITY_CONTRACT.md` §5.4).  In particular:
//!
//! * `TTFont`, `BytesIO` and the `TTFont` context manager are reached through
//!   Python, so `mock.patch("fontTools.ttLib...buildReversedMin")` and the
//!   "old fontTools" fallback (`buildReversed` + `min`) keep working.
//! * `HAS_FONTTOOLS` is read lazily from the shim module on every call, so
//!   `mock.patch("pypdf.generic._font.HAS_FONTTOOLS", False)` is observed and
//!   the `ImportError` message is raised exactly where upstream raises it.
//! * Table loading keeps upstream's critical/optional split and its double
//!   wrapping of a missing critical table
//!   (`"Font table 'cmap' is corrupt or truncated: Font file does not have a 'cmap' table"`),
//!   with the original exception attached as `__cause__`.
//!
//! # Determinism
//!
//! Every mapping that reaches the output is an insertion-ordered Python `dict`
//! created and iterated in upstream order; no Rust `HashMap`/`HashSet`
//! participates.  Arithmetic that can be observed as output (rounding, floor
//! division, `int()` truncation, `%d`/`%s` log formatting) is executed through
//! Python's own numeric protocol because upstream's results depend on the
//! Python operand types (`int` vs `float`, `IntFlag`, arbitrary precision).
//!
//! The module has no classes: all 21 entry points are `#[pyfunction]`s bound by
//! the hand-written shim.

use pyo3::exceptions::{PyAttributeError, PyMemoryError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyFloat, PyInt, PyList, PyModule, PyString, PyTuple};

use crate::errors;
use crate::pyutil;

/// Upstream module name; used as `logger_warning(..., source=__name__)`.
const SOURCE: &str = "pypdf.generic._font";

// Limits (upstream module constants; the Python-visible copies live in the shim).
const MAX_CID_WIDTH_ENTRY_COUNT: i64 = 65_536;
const MAX_WIDTH_ENTRY_COUNT: i64 = 100_000;
const MAX_SIMPLE_FONT_CHARACTER_CODE: i64 = 255;

// Some constants from truetype font tables that we use (upstream module constants).
const HEADER_MACSTYLE_ITALIC: i64 = 0x02;
const OS2_FSSELECTION_ITALIC: i64 = 0x01;
const OS2_PANOSE_BFAMILYTYPE_SCRIPT: i64 = 3;
const OS2_PANOSE_BFAMILYTYPE_DECORATIVE: i64 = 4;
const OS2_PANOSE_BFAMILYTYPE_PICTORIAL: i64 = 5;
const OS2_PANOSE_BPROPORTION_MONOSPACED: i64 = 9;
const OS2_SFAMILYSCLASS_SCRIPTS: i64 = 10;
const OS2_SFAMILYSCLASS_SYMBOLIC: i64 = 12;
const CMAP_MAX_ENTRIES_PER_GROUP: usize = 100;

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    crate::pyutil::cached_module(py, "builtins")
}

fn font_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("pypdf.generic._font")
}

/// Fetch a name from the shim module.  Upstream's functions read module globals
/// (`HAS_FONTTOOLS`, `TTFont`, `BytesIO`, `get_encoding`, `unicode_to_bytes`, ...)
/// and the official tests monkeypatch some of them, so they are resolved by name.
fn font_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    font_module(py)?.getattr(name)
}

fn generic_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf.generic")?.getattr(name)
}

fn builtin<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr(name)
}

fn py_none<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    py.None().into_bound(py)
}

fn py_int_obj<'py>(py: Python<'py>, value: i64) -> Bound<'py, PyAny> {
    PyInt::new(py, value).into_any()
}

fn py_str_obj<'py>(py: Python<'py>, value: &str) -> Bound<'py, PyAny> {
    PyString::new(py, value).into_any()
}

/// `isinstance(obj, cls)`.
fn is_instance(obj: &Bound<'_, PyAny>, class: &Bound<'_, PyAny>) -> PyResult<bool> {
    obj.is_instance(class)
}

/// `isinstance(obj, (int, float))`.
fn is_number(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    let int_class = builtin(py, "int")?;
    let float_class = builtin(py, "float")?;
    Ok(is_instance(obj, &int_class)? || is_instance(obj, &float_class)?)
}

/// Emulate `logger_warning(message, source=__name__, **values)`.
fn logger_warning(
    py: Python<'_>,
    message: &str,
    values: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    pyutil::logger_warning(py, SOURCE, message, values)
}

/// Emulate `logger_warning(message, source=__name__, **values)` from a Python dict.
fn logger_warning_dict(py: Python<'_>, message: &str, values: Bound<'_, PyDict>) -> PyResult<()> {
    logger_warning(py, message, Some(&values))
}

fn limit_reached(py: Python<'_>, message: &str) -> PyErr {
    errors::limit_reached_error(py, message)
}

fn pdf_read_error(py: Python<'_>, message: &str) -> PyErr {
    errors::pdf_read_error(py, message)
}

/// `str(exception)` for a Python exception instance.
fn exception_args_str(py: Python<'_>, error: &PyErr) -> PyResult<String> {
    pyutil::py_str(error.value(py))
}

/// `except Exception:` -- `KeyboardInterrupt`/`SystemExit`/`GeneratorExit` are
/// not caught by upstream's handlers and must keep propagating.
fn is_python_exception(py: Python<'_>, error: &PyErr) -> bool {
    match builtin(py, "Exception") {
        Ok(class) => error.value(py).is_instance(&class).unwrap_or(false),
        Err(_) => false,
    }
}

/// Attach `__cause__` like `raise ... from exception`.
fn with_cause(py: Python<'_>, error: PyErr, cause: PyErr) -> PyErr {
    error.set_cause(py, Some(cause));
    error
}

fn py_int_of<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtin(py, "int")?.call1((value,))
}

fn py_round<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
    ndigits: Option<i64>,
) -> PyResult<Bound<'py, PyAny>> {
    let round = builtin(py, "round")?;
    match ndigits {
        Some(ndigits) => round.call1((value, ndigits)),
        None => round.call1((value,)),
    }
}

fn py_chr<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtin(py, "chr")?.call1((value,))
}

fn py_ord<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtin(py, "ord")?.call1((value,))
}

fn py_min<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtin(py, "min")?.call1((value,))
}

/// `bytes((character_code,))` (raises the same `ValueError` for out-of-range codes).
fn py_single_byte<'py>(
    py: Python<'py>,
    character_code: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let args = PyTuple::new(py, [character_code.clone()])?;
    builtin(py, "bytes")?.call1((args,))
}

fn tuple_of<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyTuple::new(py, items)?.into_any())
}

fn name_object<'py>(py: Python<'py>, value: &str) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "NameObject")?.call1((value,))
}

fn number_object<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "NumberObject")?.call1((value,))
}

fn float_object<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "FloatObject")?.call1((value,))
}

fn array_object<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "ArrayObject")?.call1((value,))
}

fn dict_object<'py>(
    py: Python<'py>,
    pairs: Vec<(Bound<'py, PyAny>, Bound<'py, PyAny>)>,
) -> PyResult<Bound<'py, PyAny>> {
    let mapping = PyDict::new(py);
    for (key, value) in pairs {
        mapping.set_item(key, value)?;
    }
    generic_class(py, "DictionaryObject")?.call1((mapping,))
}

fn stream_object<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "StreamObject")?.call0()
}

/// `FontDescriptor(**kwargs)` (module-global class, resolved by name).
fn font_descriptor_from_kwargs<'py>(
    py: Python<'py>,
    kwargs: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let kwargs: &Bound<'py, PyDict> = kwargs.cast()?;
    font_attr(py, "FontDescriptor")?.call((), Some(kwargs))
}

/// `<font>[name]`-style key format for the shim's `Font` dataclass call.
fn call_with_kwargs<'py>(
    callable: &Bound<'py, PyAny>,
    kwargs: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyAny>> {
    callable.call((), Some(kwargs))
}

/// `value & mask`, using Python's integer protocol (so `IntFlag`/`NumberObject`
/// behave exactly like upstream).
fn bit_and<'py>(
    value: &Bound<'py, PyAny>,
    mask: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    value.bitand(mask)
}

// ---------------------------------------------------------------------------
// FontDescriptor.as_font_descriptor_resource
// ---------------------------------------------------------------------------

fn as_font_descriptor_resource_impl<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let name = self_.getattr("name")?;
    let font_name = format!("/{}", pyutil::py_str(&name)?);
    let bbox = self_.getattr("bbox")?;
    let bbox_list = PyList::empty(py);
    for item in bbox.try_iter()? {
        bbox_list.append(float_object(py, &item?)?)?;
    }
    let mapping = PyDict::new(py);
    mapping.set_item(
        name_object(py, "/Type")?,
        name_object(py, "/FontDescriptor")?,
    )?;
    mapping.set_item(name_object(py, "/FontName")?, name_object(py, &font_name)?)?;
    mapping.set_item(
        name_object(py, "/Flags")?,
        number_object(py, &self_.getattr("flags")?)?,
    )?;
    mapping.set_item(
        name_object(py, "/FontBBox")?,
        array_object(py, bbox_list.as_any())?,
    )?;
    mapping.set_item(
        name_object(py, "/ItalicAngle")?,
        float_object(py, &self_.getattr("italic_angle")?)?,
    )?;
    mapping.set_item(
        name_object(py, "/Ascent")?,
        float_object(py, &self_.getattr("ascent")?)?,
    )?;
    mapping.set_item(
        name_object(py, "/Descent")?,
        float_object(py, &self_.getattr("descent")?)?,
    )?;
    mapping.set_item(
        name_object(py, "/CapHeight")?,
        float_object(py, &self_.getattr("cap_height")?)?,
    )?;
    mapping.set_item(
        name_object(py, "/XHeight")?,
        float_object(py, &self_.getattr("x_height")?)?,
    )?;
    let resource = generic_class(py, "DictionaryObject")?.call1((mapping,))?;

    let font_file = self_.getattr("font_file")?;
    if font_file.is_truthy()? {
        // Add the stream. For now, we assume a TrueType font (FontFile2)
        resource.set_item(name_object(py, "/FontFile2")?, font_file)?;
    }
    Ok(resource)
}

// @pypdf: pypdf.generic._font.FontDescriptor
// @pypdf: pypdf.generic._font.FontDescriptor._DEFAULT_BBOX
// @pypdf: pypdf.generic._font.FontDescriptor.name
// @pypdf: pypdf.generic._font.FontDescriptor.family
// @pypdf: pypdf.generic._font.FontDescriptor.weight
// @pypdf: pypdf.generic._font.FontDescriptor.ascent
// @pypdf: pypdf.generic._font.FontDescriptor.descent
// @pypdf: pypdf.generic._font.FontDescriptor.cap_height
// @pypdf: pypdf.generic._font.FontDescriptor.x_height
// @pypdf: pypdf.generic._font.FontDescriptor.italic_angle
// @pypdf: pypdf.generic._font.FontDescriptor.flags
// @pypdf: pypdf.generic._font.FontDescriptor.bbox
// @pypdf: pypdf.generic._font.FontDescriptor.font_file
// @pypdf: pypdf.generic._font.FontDescriptor.as_font_descriptor_resource
#[pyfunction]
fn font_font_descriptor_as_font_descriptor_resource(
    py: Python<'_>,
    self_: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(as_font_descriptor_resource_impl(py, self_)?.unbind())
}

// ---------------------------------------------------------------------------
// Width collection: simple fonts
// ---------------------------------------------------------------------------

fn check_range_length_impl(
    py: Python<'_>,
    start: &Bound<'_, PyAny>,
    end: &Bound<'_, PyAny>,
) -> PyResult<()> {
    if end.lt(start)? {
        return Err(limit_reached(
            py,
            &format!(
                "Invalid CID width range: {}..{}.",
                pyutil::py_str(start)?,
                pyutil::py_str(end)?
            ),
        ));
    }
    let count = end.sub(start)?;
    if count.gt(MAX_CID_WIDTH_ENTRY_COUNT)? {
        return Err(limit_reached(
            py,
            &format!(
                "CID width range too large: {} > {}.",
                pyutil::py_str(&count)?,
                MAX_CID_WIDTH_ENTRY_COUNT
            ),
        ));
    }
    Ok(())
}

fn check_entry_count_impl(
    py: Python<'_>,
    count: &Bound<'_, PyAny>,
    limit: &Bound<'_, PyAny>,
) -> PyResult<()> {
    if count.gt(limit)? {
        return Err(limit_reached(
            py,
            &format!(
                "Too many character widths: {} > {}.",
                pyutil::py_str(count)?,
                pyutil::py_str(limit)?
            ),
        ));
    }
    Ok(())
}

fn check_entry_count_value(py: Python<'_>, count: i64, limit: i64) -> PyResult<()> {
    if count > limit {
        return Err(limit_reached(
            py,
            &format!("Too many character widths: {count} > {limit}."),
        ));
    }
    Ok(())
}

fn collect_tt_t1_character_widths_impl(
    py: Python<'_>,
    pdf_font_dict: &Bound<'_, PyAny>,
    _char_map: &Bound<'_, PyAny>,
    _encoding: &Bound<'_, PyAny>,
    current_widths: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let widths_array = pdf_font_dict
        .get_item("/Widths")?
        .call_method0("get_object")?;
    let array_class = generic_class(py, "ArrayObject")?;
    if !is_instance(&widths_array, &array_class)? {
        let values = PyDict::new(py);
        values.set_item("widths_array", &widths_array)?;
        logger_warning_dict(
            py,
            "Expected an array for /Widths, got %(widths_array)s. Ignoring it.",
            values,
        )?;
        return Ok(());
    }
    check_entry_count_value(
        py,
        widths_array.len()? as i64,
        MAX_SIMPLE_FONT_CHARACTER_CODE + 1,
    )?;
    let first_char = pdf_font_dict.call_method1("get", ("/FirstChar", 0))?;
    let int_class = builtin(py, "int")?;
    let float_class = builtin(py, "float")?;
    if !is_instance(&first_char, &int_class)? && !is_instance(&first_char, &float_class)? {
        let values = PyDict::new(py);
        values.set_item("code", &first_char)?;
        logger_warning_dict(py, "Ignoring invalid /FirstChar %(code)s.", values)?;
        return Ok(());
    }
    let first_char = py_int_of(py, &first_char)?;
    if first_char.lt(0)? {
        let values = PyDict::new(py);
        values.set_item("code", &first_char)?;
        logger_warning_dict(py, "Ignoring invalid /FirstChar %(code)d < 0.", values)?;
        return Ok(());
    }
    let mut character_code = first_char;
    for width in widths_array.try_iter()? {
        let width = width?;
        if character_code.gt(MAX_SIMPLE_FONT_CHARACTER_CODE)? {
            let values = PyDict::new(py);
            values.set_item("code", &character_code)?;
            values.set_item("limit", MAX_SIMPLE_FONT_CHARACTER_CODE)?;
            logger_warning_dict(
                py,
                "Ignoring invalid character codes > %(limit)d (starting at %(code)d).",
                values,
            )?;
            break;
        }
        current_widths.set_item(py_chr(py, &character_code)?, py_int_of(py, &width)?)?;
        character_code = character_code.add(1)?;
    }
    Ok(())
}

// @pypdf: pypdf.generic._font.Font._collect_tt_t1_character_widths
#[pyfunction]
fn font_collect_tt_t1_character_widths(
    py: Python<'_>,
    pdf_font_dict: &Bound<'_, PyAny>,
    char_map: &Bound<'_, PyAny>,
    encoding: &Bound<'_, PyAny>,
    current_widths: &Bound<'_, PyAny>,
) -> PyResult<()> {
    collect_tt_t1_character_widths_impl(py, pdf_font_dict, char_map, encoding, current_widths)
}

// @pypdf: pypdf.generic._font.Font.__check_range_length
#[pyfunction]
fn font_check_range_length(
    py: Python<'_>,
    start: &Bound<'_, PyAny>,
    end: &Bound<'_, PyAny>,
) -> PyResult<()> {
    check_range_length_impl(py, start, end)
}

// @pypdf: pypdf.generic._font.Font.__check_entry_count
#[pyfunction]
fn font_check_entry_count(
    py: Python<'_>,
    count: &Bound<'_, PyAny>,
    limit: &Bound<'_, PyAny>,
) -> PyResult<()> {
    check_entry_count_impl(py, count, limit)
}

// ---------------------------------------------------------------------------
// Width collection: CID fonts
// ---------------------------------------------------------------------------

fn collect_cid_character_widths_impl(
    py: Python<'_>,
    d_font: &Bound<'_, PyAny>,
    current_widths: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let sequence_class = py.import("collections.abc")?.getattr("Sequence")?;
    let array_class = generic_class(py, "ArrayObject")?;
    // /W width definitions have two valid formats which can be mixed and matched:
    //   (1) A character start index followed by a list of widths, e.g.
    //       `45 [500 600 700]` applies widths 500, 600, 700 to characters 45-47.
    //   (2) A character start index, a character stop index, and a width, e.g.
    //       `45 65 500` applies width 500 to characters 45-65.
    let mut skip_count: usize = 0;
    let mut entry_count: i64 = 0;
    let widths = d_font
        .call_method1("get", ("/W", array_class.call0()?))?
        .call_method0("get_object")?;
    let widths_length = widths.len()?;
    let mut idx: usize = 0;
    while idx < widths_length {
        if skip_count != 0 {
            skip_count -= 1;
            idx += 1;
            continue;
        }
        let w_entry = widths.get_item(idx)?.call_method0("get_object")?;
        if !is_number(py, &w_entry)? {
            // We should never get here due to skip_count above. But
            // sometimes we do.
            let values = PyDict::new(py);
            values.set_item("w_entry", &w_entry)?;
            logger_warning_dict(
                py,
                "Expected numeric value for width, got %(w_entry)s. Ignoring it.",
                values,
            )?;
            idx += 1;
            continue;
        }
        let w_next_entry = if idx + 1 < widths_length {
            widths.get_item(idx + 1)?.call_method0("get_object")?
        } else {
            py_none(py)
        };
        if is_instance(&w_next_entry, &sequence_class)? {
            // check for format (1): `int [int int int int ...]`
            let start_idx = py_int_of(py, &w_entry)?;
            let width_list = w_next_entry;
            let width_list_length = width_list.len()?;
            let stop_idx = start_idx.add(width_list_length)?;
            check_range_length_impl(py, &start_idx, &stop_idx)?;
            let delta: i64 = stop_idx.sub(&start_idx)?.extract()?;
            entry_count += delta;
            check_entry_count_value(py, entry_count, MAX_WIDTH_ENTRY_COUNT)?;
            let update = PyDict::new(py);
            for offset in 0..width_list_length {
                let cidx = start_idx.add(offset)?;
                let width = width_list.get_item(offset)?.call_method0("get_object")?;
                update.set_item(py_chr(py, &cidx)?, width)?;
            }
            current_widths.call_method1("update", (update,))?;
            skip_count = 1;
        } else if is_number(py, &w_next_entry)?
            && idx + 2 < widths_length
            && is_number(py, &widths.get_item(idx + 2)?.call_method0("get_object")?)?
        {
            // check for format (2): `int int int`
            let start_idx = py_int_of(py, &w_entry)?;
            let stop_idx = py_int_of(py, &w_next_entry)?;
            let const_width = widths.get_item(idx + 2)?.call_method0("get_object")?;
            let stop_idx_plus_one = stop_idx.add(1)?;
            check_range_length_impl(py, &start_idx, &stop_idx_plus_one)?;
            let delta: i64 = stop_idx_plus_one.sub(&start_idx)?.extract()?;
            entry_count += delta;
            check_entry_count_value(py, entry_count, MAX_WIDTH_ENTRY_COUNT)?;
            let update = PyDict::new(py);
            let mut cidx = start_idx;
            let mut count: i64 = 0;
            while cidx.le(&stop_idx)? && count < delta {
                update.set_item(py_chr(py, &cidx)?, &const_width)?;
                cidx = cidx.add(1)?;
                count += 1;
            }
            current_widths.call_method1("update", (update,))?;
            skip_count = 2;
        } else {
            // This handles the case of out of bounds (reaching the end of the width definitions
            // while expecting more elements).
            let values = PyDict::new(py);
            values.set_item("w_entry", &w_entry)?;
            logger_warning_dict(
                py,
                "Invalid font width definition. Last element: %(w_entry)s.",
                values,
            )?;
        }
        idx += 1;
    }
    Ok(())
}

// @pypdf: pypdf.generic._font.Font._collect_cid_character_widths
#[pyfunction]
fn font_collect_cid_character_widths(
    py: Python<'_>,
    d_font: &Bound<'_, PyAny>,
    current_widths: &Bound<'_, PyAny>,
) -> PyResult<()> {
    collect_cid_character_widths_impl(py, d_font, current_widths)
}

// ---------------------------------------------------------------------------
// Space character / default width helpers
// ---------------------------------------------------------------------------

fn get_space_char_impl<'py>(
    py: Python<'py>,
    encoding: &Bound<'py, PyAny>,
    character_map: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let space_char = py_str_obj(py, " ");
    for item in character_map.call_method0("items")?.try_iter()? {
        let item = item?;
        let glyph_id = item.get_item(0)?;
        let char_str = item.get_item(1)?;
        if char_str.eq(&space_char)? {
            return glyph_id.str().map(|value| value.into_any());
        }
    }
    let dict_class = builtin(py, "dict")?;
    if is_instance(encoding, &dict_class)? {
        for item in encoding.call_method0("items")?.try_iter()? {
            let item = item?;
            let char_code = item.get_item(0)?;
            let char_str = item.get_item(1)?;
            if char_str.eq(&space_char)? {
                return py_chr(py, &char_code);
            }
        }
    }
    Ok(space_char)
}

// @pypdf: pypdf.generic._font.Font._get_space_char
#[pyfunction]
fn font_get_space_char<'py>(
    py: Python<'py>,
    encoding: &Bound<'py, PyAny>,
    character_map: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(get_space_char_impl(py, encoding, character_map)?.unbind())
}

fn add_default_width_impl(
    py: Python<'_>,
    current_widths: &Bound<'_, PyAny>,
    flags: &Bound<'_, PyAny>,
    space_char: &Bound<'_, PyAny>,
) -> PyResult<()> {
    if !current_widths.is_truthy()? {
        current_widths.set_item("default", 500)?;
        return Ok(());
    }
    let fixed_pitch = py
        .import("pypdf.constants")?
        .getattr("FontFlags")?
        .getattr("FIXED_PITCH")?;
    if current_widths.contains(space_char)? {
        let space_width = current_widths.get_item(space_char)?;
        if !space_width.eq(0)? {
            // Setting default to once or twice the space width, depending on fixed pitch
            if bit_and(flags, &fixed_pitch)?.eq(&fixed_pitch)? {
                current_widths.set_item("default", &space_width)?;
                return Ok(());
            }
            current_widths.set_item("default", py_int_of(py, &space_width.mul(2)?)?)?;
            return Ok(());
        }
    }
    // Use the average width of existing glyph widths
    let valid_widths = PyList::empty(py);
    for width in current_widths.call_method0("values")?.try_iter()? {
        let width = width?;
        if width.gt(0)? {
            valid_widths.append(width)?;
        }
    }
    if valid_widths.is_empty() {
        current_widths.set_item("default", 500)?;
    } else {
        let valid_widths_length = valid_widths.len();
        let total = builtin(py, "sum")?.call1((valid_widths,))?;
        current_widths.set_item("default", total.floor_div(valid_widths_length)?)?;
    }
    Ok(())
}

// @pypdf: pypdf.generic._font.Font._add_default_width
#[pyfunction]
fn font_add_default_width(
    py: Python<'_>,
    current_widths: &Bound<'_, PyAny>,
    flags: &Bound<'_, PyAny>,
    space_char: &Bound<'_, PyAny>,
) -> PyResult<()> {
    add_default_width_impl(py, current_widths, flags, space_char)
}

fn add_space_width_impl<'py>(
    py: Python<'py>,
    character_widths: &Bound<'py, PyAny>,
    flags: &Bound<'py, PyAny>,
    space_char: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let space_width = character_widths.call_method1("get", (space_char, 0))?;
    if !space_width.eq(0)? {
        return Ok(space_width);
    }
    let fixed_pitch = py
        .import("pypdf.constants")?
        .getattr("FontFlags")?
        .getattr("FIXED_PITCH")?;
    if bit_and(flags, &fixed_pitch)?.eq(&fixed_pitch)? {
        return character_widths.get_item("default");
    }
    character_widths.get_item("default")?.floor_div(2)
}

// @pypdf: pypdf.generic._font.Font._add_space_width
#[pyfunction]
fn font_add_space_width<'py>(
    py: Python<'py>,
    character_widths: &Bound<'py, PyAny>,
    flags: &Bound<'py, PyAny>,
    space_char: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(add_space_width_impl(py, character_widths, flags, space_char)?.unbind())
}

// ---------------------------------------------------------------------------
// Font descriptor / bbox parsing
// ---------------------------------------------------------------------------

fn parse_bbox_impl<'py>(
    py: Python<'py>,
    raw_bbox: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let float_class = builtin(py, "float")?;
    let iterator = match raw_bbox.try_iter() {
        Ok(iterator) => iterator,
        Err(error) if error.is_instance_of::<PyTypeError>(py) => return Ok(py_none(py)),
        Err(error) => return Err(error),
    };
    let mut bbox: Vec<Bound<'py, PyAny>> = Vec::new();
    for item in iterator {
        let item = item?;
        match float_class.call1((&item,)) {
            Ok(value) => bbox.push(value),
            Err(error)
                if error.is_instance_of::<PyTypeError>(py)
                    || error.is_instance_of::<PyValueError>(py) =>
            {
                return Ok(py_none(py));
            }
            Err(error) => return Err(error),
        }
    }
    if bbox.len() != 4 {
        return Ok(py_none(py));
    }
    tuple_of(py, bbox)
}

// @pypdf: pypdf.generic._font.Font._parse_bbox
#[pyfunction]
fn font_parse_bbox<'py>(py: Python<'py>, raw_bbox: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(parse_bbox_impl(py, raw_bbox)?.unbind())
}

fn parse_font_descriptor_impl<'py>(
    py: Python<'py>,
    font_descriptor_obj: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let font_descriptor_kwargs = PyDict::new(py);
    for (source_key, target_key) in [
        ("/FontName", "name"),
        ("/FontFamily", "family"),
        ("/FontWeight", "weight"),
        ("/Ascent", "ascent"),
        ("/Descent", "descent"),
        ("/CapHeight", "cap_height"),
        ("/XHeight", "x_height"),
        ("/ItalicAngle", "italic_angle"),
        ("/Flags", "flags"),
        ("/FontBBox", "bbox"),
    ] {
        if font_descriptor_obj.contains(source_key)? {
            font_descriptor_kwargs
                .set_item(target_key, font_descriptor_obj.get_item(source_key)?)?;
        }
    }
    // Handle missing or malformed bbox gracefully - PDFs may have fonts without valid bounding boxes
    if font_descriptor_kwargs.contains("bbox")? {
        let bbox_raw = font_descriptor_kwargs
            .get_item("bbox")?
            .unwrap_or_else(|| py_none(py));
        let bbox = parse_bbox_impl(py, &bbox_raw)?;
        if bbox.is_none() {
            font_descriptor_kwargs.call_method1("__delitem__", ("bbox",))?;
        } else {
            font_descriptor_kwargs.set_item("bbox", bbox)?;
        }
    }

    // Find the binary stream for this font if there is one
    let pdf_read_error_class = py.import("pypdf.errors")?.getattr("PdfReadError")?;
    for source_key in ["/FontFile", "/FontFile2", "/FontFile3"] {
        if font_descriptor_obj.contains(source_key)? {
            if font_descriptor_kwargs.contains("font_file")? {
                return Err(pdf_read_error(
                    py,
                    &format!(
                        "More than one /FontFile found in {}",
                        pyutil::py_str(font_descriptor_obj)?
                    ),
                ));
            }
            // Upstream wraps the whole `font_descriptor_obj[source_key].get_object()`
            // expression in try/except; `DictionaryObject.__getitem__` already calls
            // `get_object()` internally, so the PdfReadError can come from `get_item`
            // itself and must be caught here (not propagated by `?`).
            let attempt = font_descriptor_obj
                .get_item(source_key)
                .and_then(|value| value.call_method0("get_object"));
            match attempt {
                Ok(font_file) => {
                    font_descriptor_kwargs.set_item("font_file", font_file)?;
                }
                Err(error) if error.value(py).is_instance(&pdf_read_error_class)? => {
                    let values = PyDict::new(py);
                    values.set_item("source_key", py_str_obj(py, source_key))?;
                    values.set_item("font_descriptor_obj", font_descriptor_obj)?;
                    values.set_item("error", error.value(py))?;
                    logger_warning_dict(
                        py,
                        "Failed to get %(source_key)r in %(font_descriptor_obj)s: %(error)s",
                        values,
                    )?;
                }
                Err(error) => return Err(error),
            }
        }
    }
    Ok(font_descriptor_kwargs)
}

// @pypdf: pypdf.generic._font.Font._parse_font_descriptor
#[pyfunction]
fn font_parse_font_descriptor<'py>(
    py: Python<'py>,
    font_descriptor_obj: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(parse_font_descriptor_impl(py, font_descriptor_obj)?
        .into_any()
        .unbind())
}

// ---------------------------------------------------------------------------
// Font.from_font_resource
// ---------------------------------------------------------------------------

fn from_font_resource_impl<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    pdf_font_dict: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let core_font_metrics = py
        .import("pypdf._codecs.core_font_metrics")?
        .getattr("CORE_FONT_METRICS")?;

    // Can collect base_font, name and encoding directly from font resource.
    // A malformed resource may carry a non-string value here, so fall back
    // to "Unknown" rather than failing on it.
    let base_font = pdf_font_dict.call_method1("get", ("/BaseFont", "Unknown"))?;
    let string_class = builtin(py, "str")?;
    let unknown = || py_str_obj(py, "Unknown");
    let name = if is_instance(&base_font, &string_class)? {
        base_font.call_method1("removeprefix", ("/",))?
    } else {
        unknown()
    };
    let sub_type_value = pdf_font_dict.call_method1("get", ("/Subtype",))?;
    let sub_type = if is_instance(&sub_type_value, &string_class)? {
        sub_type_value.call_method1("removeprefix", ("/",))?
    } else {
        unknown()
    };
    let encoding_and_map = font_attr(py, "get_encoding")?.call1((pdf_font_dict,))?;
    let encoding = encoding_and_map.get_item(0)?;
    let character_map = encoding_and_map.get_item(1)?;
    let mut font_descriptor: Option<Bound<'py, PyAny>> = None;
    let character_widths = PyDict::new(py);
    let mut interpretable = true;

    // Deal with fonts by type; Type1, TrueType and certain Type3
    let subtype_value = pdf_font_dict.call_method1("get", ("/Subtype",))?;
    let subtype_choices = PyTuple::new(py, ["/Type1", "/MMType1", "/TrueType", "/Type3"])?;
    if subtype_choices.contains(&subtype_value)? {
        // Type3 fonts that do not specify a "/ToUnicode" mapping cannot be
        // reliably converted into character codes unless all named chars
        // in /CharProcs map to a standard adobe glyph. See §9.10.2 of the
        // PDF 1.7 standard.
        if sub_type.eq("Type3")? && !pdf_font_dict.contains("/ToUnicode")? {
            let char_procs = pdf_font_dict.call_method1("get", ("/CharProcs",))?;
            let iterable = if char_procs.is_truthy()? {
                char_procs
            } else {
                PyList::empty(py).into_any()
            };
            let adobe_glyphs = py
                .import("pypdf._codecs.adobe_glyphs")?
                .getattr("adobe_glyphs")?;
            let mut all_known = true;
            for item in iterable.try_iter()? {
                let cname = item?;
                if !adobe_glyphs.contains(&cname)? {
                    all_known = false;
                    break;
                }
            }
            interpretable = all_known;
        }
        if interpretable {
            // Save some overhead if font is not interpretable
            if pdf_font_dict.contains("/Widths")? {
                cls.getattr("_collect_tt_t1_character_widths")?.call1((
                    pdf_font_dict,
                    &character_map,
                    &encoding,
                    &character_widths,
                ))?;
            } else if core_font_metrics.contains(&name)? {
                let core_font_metrics_entry = core_font_metrics.get_item(&name)?;
                font_descriptor = Some(core_font_metrics_entry.getattr("font_descriptor")?);
                let core_font_widths = core_font_metrics_entry.getattr("character_widths")?;
                for item in encoding.call_method0("items")?.try_iter()? {
                    let item = item?;
                    let code = item.get_item(0)?;
                    let character = item.get_item(1)?;
                    // Look up the width using the glyph name from the encoding
                    if core_font_widths.contains(&character)? {
                        character_widths
                            .set_item(py_chr(py, &code)?, core_font_widths.get_item(&character)?)?;
                    }
                }
            }
            if pdf_font_dict.contains("/FontDescriptor")? {
                let dict_class = generic_class(py, "DictionaryObject")?;
                let font_descriptor_obj = pdf_font_dict
                    .call_method1("get", ("/FontDescriptor", dict_class.call0()?))?
                    .call_method0("get_object")?;
                if font_descriptor_obj.contains("/MissingWidth")? {
                    character_widths.set_item(
                        "default",
                        font_descriptor_obj
                            .get_item("/MissingWidth")?
                            .call_method0("get_object")?,
                    )?;
                }
                let kwargs = cls
                    .getattr("_parse_font_descriptor")?
                    .call1((&font_descriptor_obj,))?;
                font_descriptor = Some(font_descriptor_from_kwargs(py, &kwargs)?);
            } else if pdf_font_dict.contains("/FontBBox")? {
                // For Type3 without Font Descriptor but with FontBBox, see Table 110 in the PDF specification 2.0
                let font_descriptor_kwargs = PyDict::new(py);
                font_descriptor_kwargs.set_item("name", &name)?;
                let bbox = cls
                    .getattr("_parse_bbox")?
                    .call1((pdf_font_dict.get_item("/FontBBox")?,))?;
                if !bbox.is_none() {
                    font_descriptor_kwargs.set_item("bbox", bbox)?;
                }
                font_descriptor = Some(font_descriptor_from_kwargs(
                    py,
                    font_descriptor_kwargs.as_any(),
                )?);
            }
        }
    } else {
        // Composite font or CID font - CID fonts have a /W array mapping character codes
        // to widths stashed in /DescendantFonts.
        let array_class = generic_class(py, "ArrayObject")?;
        let mut descendant_fonts = pdf_font_dict
            .call_method1("get", ("/DescendantFonts", array_class.call0()?))?
            .call_method0("get_object")?;
        if !is_instance(&descendant_fonts, &array_class)? {
            let values = PyDict::new(py);
            values.set_item("descendant_fonts", &descendant_fonts)?;
            logger_warning_dict(
                py,
                "Expected an array for /DescendantFonts, got %(descendant_fonts)s. Ignoring it.",
                values,
            )?;
            descendant_fonts = array_class.call0()?;
        }
        let dict_class = generic_class(py, "DictionaryObject")?;
        for (descendant_index, item) in (0_i64..).zip(descendant_fonts.try_iter()?) {
            let d_font = item?.call_method0("get_object")?;
            descendant_fonts.set_item(descendant_index, &d_font)?;
            let collect_kwargs = PyDict::new(py);
            collect_kwargs.set_item("d_font", &d_font)?;
            collect_kwargs.set_item("current_widths", &character_widths)?;
            cls.getattr("_collect_cid_character_widths")?
                .call((), Some(&collect_kwargs))?;
            if d_font.contains("/DW")? {
                character_widths.set_item(
                    "default",
                    d_font.get_item("/DW")?.call_method0("get_object")?,
                )?;
            }
            let font_descriptor_obj = d_font
                .call_method1("get", ("/FontDescriptor", dict_class.call0()?))?
                .call_method0("get_object")?;
            let kwargs = cls
                .getattr("_parse_font_descriptor")?
                .call1((&font_descriptor_obj,))?;
            font_descriptor = Some(font_descriptor_from_kwargs(py, &kwargs)?);
        }
    }

    let font_descriptor = match font_descriptor {
        Some(font_descriptor) => font_descriptor,
        None => {
            let kwargs = PyDict::new(py);
            kwargs.set_item("name", &name)?;
            font_descriptor_from_kwargs(py, kwargs.as_any())?
        }
    };

    let space_char = cls
        .getattr("_get_space_char")?
        .call1((&encoding, &character_map))?;
    let default_width = character_widths.call_method1("get", ("default", 0))?;
    let flags = || -> PyResult<Bound<'py, PyAny>> { font_descriptor.getattr("flags") };
    if default_width.eq(0)? {
        cls.getattr("_add_default_width")?
            .call1((&character_widths, flags()?, &space_char))?;
    }
    let space_width =
        cls.getattr("_add_space_width")?
            .call1((&character_widths, flags()?, &space_char))?;

    let kwargs = PyDict::new(py);
    kwargs.set_item("name", &name)?;
    kwargs.set_item("sub_type", &sub_type)?;
    kwargs.set_item("encoding", &encoding)?;
    kwargs.set_item("font_descriptor", &font_descriptor)?;
    kwargs.set_item("character_map", &character_map)?;
    kwargs.set_item("character_widths", &character_widths)?;
    kwargs.set_item("space_width", &space_width)?;
    kwargs.set_item("space_char", &space_char)?;
    kwargs.set_item("interpretable", interpretable)?;
    call_with_kwargs(cls, &kwargs)
}

// @pypdf: pypdf.generic._font.Font
// @pypdf: pypdf.generic._font.Font.name
// @pypdf: pypdf.generic._font.Font.encoding
// @pypdf: pypdf.generic._font.Font.character_map
// @pypdf: pypdf.generic._font.Font.sub_type
// @pypdf: pypdf.generic._font.Font.font_descriptor
// @pypdf: pypdf.generic._font.Font.character_widths
// @pypdf: pypdf.generic._font.Font.space_width
// @pypdf: pypdf.generic._font.Font.space_char
// @pypdf: pypdf.generic._font.Font.interpretable
// @pypdf: pypdf.generic._font.Font.from_font_resource
#[pyfunction]
fn font_from_font_resource<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    pdf_font_dict: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(from_font_resource_impl(py, cls, pdf_font_dict)?.unbind())
}

// ---------------------------------------------------------------------------
// fontTools table access
// ---------------------------------------------------------------------------

fn load_fonttools_table_impl<'py>(
    py: Python<'py>,
    table_name: &Bound<'py, PyAny>,
    is_critical: bool,
    tt_font_object: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // `tt_font.get()` automatically triggers `table.decompile()` under the hood
    let attempt: PyResult<Bound<'py, PyAny>> = (|| {
        let table = tt_font_object.call_method1("get", (table_name,))?;
        if table.is_none() {
            if is_critical {
                return Err(pdf_read_error(
                    py,
                    &format!(
                        "Font file does not have a {} table",
                        pyutil::py_repr(table_name)?
                    ),
                ));
            }
            let values = PyDict::new(py);
            values.set_item("table_name", table_name)?;
            logger_warning_dict(
                py,
                "Optional font table %(table_name)r is missing from font file",
                values,
            )?;
            return Ok(py_none(py));
        }
        Ok(table)
    })();
    match attempt {
        Ok(table) => Ok(table),
        Err(error) => {
            if !is_python_exception(py, &error) {
                return Err(error);
            }
            if is_critical {
                let message = format!(
                    "Font table {} is corrupt or truncated: {}",
                    pyutil::py_repr(table_name)?,
                    exception_args_str(py, &error)?
                );
                return Err(with_cause(py, pdf_read_error(py, &message), error));
            }
            let values = PyDict::new(py);
            values.set_item("table_name", table_name)?;
            values.set_item("exception", error.value(py))?;
            logger_warning_dict(
                py,
                "Optional font table %(table_name)r is corrupt and will be ignored: %(exception)s",
                values,
            )?;
            Ok(py_none(py))
        }
    }
}

// @pypdf: pypdf.generic._font.Font._load_fonttools_table
#[pyfunction]
fn font_load_fonttools_table<'py>(
    py: Python<'py>,
    table_name: &Bound<'py, PyAny>,
    is_critical: bool,
    tt_font_object: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(load_fonttools_table_impl(py, table_name, is_critical, tt_font_object)?.unbind())
}

fn get_font_flags_from_truetype_font_tables_impl<'py>(
    py: Python<'py>,
    header_table: &Bound<'py, PyAny>,
    postscript_table: &Bound<'py, PyAny>,
    os2_table: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let os2_present = os2_table.is_truthy()?;
    let postscript_present = postscript_table.is_truthy()?;
    let mut panose: Option<Bound<'_, PyAny>> = None;
    let mut family_class: i64 = 0;
    // Get the font flags
    if os2_present {
        panose = Some(os2_table.getattr("panose")?);
        // sFamilyClass is a two-byte field. The high byte describes the family class, whereas the low
        // byte only describes the subclass. We only need the high byte, hence the bit shift below:
        family_class = os2_table.getattr("sFamilyClass")?.extract::<i64>()? >> 8;
    }
    // Upstream imports `FontFlags` into the module namespace and combines the
    // flag members with `|=`.  `0 | FontFlags.X` must go through Python: CPython
    // gives the *reflected* operation of the int subclass precedence, so the
    // accumulated value is a `FontFlags` member (not a plain `int`), which is
    // observable through `repr(FontDescriptor(...))`.
    let font_flags = font_attr(py, "FontFlags")?;
    let flag = |name: &str| -> PyResult<Bound<'py, PyAny>> { font_flags.getattr(name) };
    let or_flag = |flags: Bound<'py, PyAny>,
                   flag: &Bound<'py, PyAny>|
     -> PyResult<Bound<'py, PyAny>> { flags.bitor(flag) };
    let italic_flag = flag("ITALIC")?;
    let fixed_pitch_flag = flag("FIXED_PITCH")?;
    let script_flag = flag("SCRIPT")?;
    let serif_flag = flag("SERIF")?;
    let symbolic_flag = flag("SYMBOLIC")?;
    let nonsymbolic_flag = flag("NONSYMBOLIC")?;
    let panose_field = |field: &str| -> PyResult<i64> {
        // `panose` is only read when the OS/2 table is present, mirroring
        // upstream's `panose = os2_table.panose` inside `if os2_table:`.
        match panose.as_ref() {
            Some(panose) => panose.getattr(field)?.extract::<i64>(),
            None => Err(PyAttributeError::new_err(field.to_string())),
        }
    };

    let mut flags: Bound<'py, PyAny> = py_int_obj(py, 0);

    // ITALIC
    let mac_style = header_table.getattr("macStyle")?.extract::<i64>()?;
    let mut italic = (mac_style & HEADER_MACSTYLE_ITALIC) != 0;
    if !italic && os2_present {
        let fs_selection = os2_table.getattr("fsSelection")?.extract::<i64>()?;
        italic = (fs_selection & OS2_FSSELECTION_ITALIC) != 0;
    }
    if italic {
        flags = or_flag(flags, &italic_flag)?;
    }
    if postscript_present && !postscript_table.getattr("italicAngle")?.eq(0.0)? {
        flags = or_flag(flags, &italic_flag)?;
    }

    // FIXED_PITCH
    if (os2_present && panose_field("bProportion")? == OS2_PANOSE_BPROPORTION_MONOSPACED)
        || (postscript_present && postscript_table.getattr("isFixedPitch")?.gt(0)?)
    {
        flags = or_flag(flags, &fixed_pitch_flag)?;
    }

    // SCRIPT
    if os2_present
        && (family_class == OS2_SFAMILYSCLASS_SCRIPTS
            || panose_field("bFamilyType")? == OS2_PANOSE_BFAMILYTYPE_SCRIPT)
    {
        flags = or_flag(flags, &script_flag)?;
    }

    // SERIF
    if os2_present {
        let serif_style = panose_field("bSerifStyle")?;
        if (2..=10).contains(&serif_style) || (1..=5).contains(&family_class) || family_class == 7
        // 6 is reserved, all 8 and above are not serif
        {
            flags = or_flag(flags, &serif_flag)?;
        }
    }

    // SYMBOLIC
    if os2_present
        && (family_class == OS2_SFAMILYSCLASS_SYMBOLIC
            || [
                OS2_PANOSE_BFAMILYTYPE_DECORATIVE,
                OS2_PANOSE_BFAMILYTYPE_PICTORIAL,
            ]
            .contains(&panose_field("bFamilyType")?))
    {
        flags = or_flag(flags, &symbolic_flag)?;
    } else {
        flags = or_flag(flags, &nonsymbolic_flag)?;
    }

    Ok(flags)
}

// @pypdf: pypdf.generic._font.Font._get_font_flags_from_truetype_font_tables
#[pyfunction]
fn font_get_font_flags_from_truetype_font_tables<'py>(
    py: Python<'py>,
    header_table: &Bound<'py, PyAny>,
    postscript_table: &Bound<'py, PyAny>,
    os2_table: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(get_font_flags_from_truetype_font_tables_impl(
        py,
        header_table,
        postscript_table,
        os2_table,
    )?
    .unbind())
}

/// `gid.to_bytes(2, "big").decode("utf-16-be", "surrogatepass")`.
fn gid_key_string<'py>(gid: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let raw = gid.call_method1("to_bytes", (2, "big"))?;
    raw.call_method1("decode", ("utf-16-be", "surrogatepass"))
}

/// `int.to_bytes(2, "big")` + decode for a Rust integer glyph id.
fn gid_key_string_from_i64<'py>(py: Python<'py>, gid: i64) -> PyResult<Bound<'py, PyAny>> {
    gid_key_string(&py_int_obj(py, gid))
}

struct TrueTypeFontData<'py> {
    font_descriptor: Bound<'py, PyAny>,
    encoding: Bound<'py, PyAny>,
    character_widths: Bound<'py, PyDict>,
    character_map: Bound<'py, PyDict>,
    space_char: Bound<'py, PyAny>,
    space_width: Bound<'py, PyAny>,
}

/// The body of the `with tt_font_object:` block of `Font.from_truetype_font_file`.
fn read_truetype_font_data<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    tt_font_object: &Bound<'py, PyAny>,
    font_file: &Bound<'py, PyAny>,
) -> PyResult<TrueTypeFontData<'py>> {
    // See Chapter 6 of the TrueType reference manual for the definition of the head, OS/2 and post tables:
    // https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6head.html
    // https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6OS2.html
    // https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6post.html

    // Collect all font tables.
    let loader = cls.getattr("_load_fonttools_table")?;
    let mut tables: Vec<Bound<'py, PyAny>> = Vec::with_capacity(7);
    for table_name in ["cmap", "head", "hmtx", "hhea", "name", "OS/2", "post"] {
        let kwargs = PyDict::new(py);
        kwargs.set_item("table_name", py_str_obj(py, table_name))?;
        kwargs.set_item(
            "is_critical",
            ["head", "hmtx", "cmap"].contains(&table_name),
        )?;
        kwargs.set_item("tt_font_object", tt_font_object)?;
        tables.push(loader.call((), Some(&kwargs))?);
    }
    let cmap_table = tables[0].clone();
    let header_table = tables[1].clone();
    let metrics_table = tables[2].clone();
    let horizontal_header_table = tables[3].clone();
    let name_table = tables[4].clone();
    let os2_table = tables[5].clone();
    let postscript_table = tables[6].clone();

    // Get the scaling factor to convert font file's units per em to PDF's 1000 units per em
    let units_per_em = header_table.getattr("unitsPerEm")?;
    if units_per_em.le(0)? {
        return Err(pdf_read_error(
            py,
            &format!(
                "Font file has an invalid unitsPerEm of {}",
                pyutil::py_str(&units_per_em)?
            ),
        ));
    }
    let scale_factor = PyFloat::new(py, 1000.0).into_any().div(&units_per_em)?;

    // Get the font descriptor
    let font_descriptor_kwargs = PyDict::new(py);

    if name_table.is_truthy()? {
        for (key, getter) in [
            ("name", "getBestFullName"),
            ("family", "getBestFamilyName"),
            ("weight", "getBestSubFamilyName"),
        ] {
            let value = name_table.call_method0(getter)?;
            if !value.is_none() {
                font_descriptor_kwargs.set_item(key, value)?;
            }
        }
    }

    if horizontal_header_table.is_truthy()? {
        let ascent = horizontal_header_table
            .getattr("ascent")?
            .mul(&scale_factor)?;
        font_descriptor_kwargs
            .set_item("ascent", py_int_of(py, &py_round(py, &ascent, Some(0))?)?)?;
        let descent = horizontal_header_table
            .getattr("descent")?
            .mul(&scale_factor)?;
        font_descriptor_kwargs
            .set_item("descent", py_int_of(py, &py_round(py, &descent, Some(0))?)?)?;
    }

    if os2_table.is_truthy()? {
        let cap_height: PyResult<Bound<'py, PyAny>> = (|| {
            let scaled = os2_table.getattr("sCapHeight")?.mul(&scale_factor)?;
            py_int_of(py, &py_round(py, &scaled, Some(0))?)
        })();
        match cap_height {
            Ok(value) => font_descriptor_kwargs.set_item("cap_height", value)?,
            Err(error) if error.is_instance_of::<PyAttributeError>(py) => {}
            Err(error) => return Err(error),
        }
        let x_height: PyResult<Bound<'py, PyAny>> = (|| {
            let scaled = os2_table.getattr("sxHeight")?.mul(&scale_factor)?;
            py_int_of(py, &py_round(py, &scaled, Some(0))?)
        })();
        match x_height {
            Ok(value) => font_descriptor_kwargs.set_item("x_height", value)?,
            Err(error) if error.is_instance_of::<PyAttributeError>(py) => {}
            Err(error) => return Err(error),
        }
    }

    font_descriptor_kwargs.set_item(
        "flags",
        cls.getattr("_get_font_flags_from_truetype_font_tables")?
            .call1((&header_table, &postscript_table, &os2_table))?,
    )?;

    let mut bbox_values: Vec<Bound<'py, PyAny>> = Vec::with_capacity(4);
    for attribute in ["xMin", "yMin", "xMax", "yMax"] {
        let scaled = header_table.getattr(attribute)?.mul(&scale_factor)?;
        bbox_values.push(py_round(py, &scaled, Some(0))?);
    }
    font_descriptor_kwargs.set_item("bbox", tuple_of(py, bbox_values)?)?;

    let font_file_data = stream_object(py)?;
    let font_file_raw_bytes = font_file.call_method0("getvalue")?;
    font_file_data.call_method1("set_data", (&font_file_raw_bytes,))?;
    let length_mapping = PyDict::new(py);
    length_mapping.set_item(
        name_object(py, "/Length1")?,
        number_object(py, &py_int_obj(py, font_file_raw_bytes.len()? as i64))?,
    )?;
    font_file_data.call_method1("update", (length_mapping,))?;
    font_descriptor_kwargs.set_item("font_file", &font_file_data)?;

    let font_descriptor = font_descriptor_from_kwargs(py, font_descriptor_kwargs.as_any())?;
    let encoding = py_str_obj(py, "utf_16_be"); // Assume unicode

    let character_widths = PyDict::new(py);
    let character_map = PyDict::new(py);

    // Note that one glyph can be mapped to multiple unicode code points. However, buildReversedMin()
    // creates a dictionary mapping glyphs to the minimum Unicode codepoint.
    let reverse_cmap: Bound<'py, PyAny> = match cmap_table.call_method0("buildReversedMin") {
        Ok(reverse_cmap) => reverse_cmap,
        Err(error) if error.is_instance_of::<PyAttributeError>(py) => {
            // Use buildReversed on fonttools < 4.57 and build a list of minimums from it
            let build_reversed = cmap_table.call_method0("buildReversed")?;
            let minimums = PyDict::new(py);
            for item in build_reversed.call_method0("items")?.try_iter()? {
                let item = item?;
                minimums.set_item(item.get_item(0)?, py_min(py, &item.get_item(1)?)?)?;
            }
            minimums.into_any()
        }
        Err(error) => return Err(error),
    };
    let metrics = metrics_table.getattr("metrics")?;
    let glyph_order = tt_font_object.call_method0("getGlyphOrder")?;
    for (glyph_id, glyph) in (0_i64..).zip(glyph_order.try_iter()?) {
        let glyph = glyph?;
        // The following is to comply with how font_glyph_byte_map works in _appearance_stream.py
        let gid_key = gid_key_string_from_i64(py, glyph_id)?;
        // Always map character width
        let advance = metrics.get_item(&glyph)?.get_item(0)?.mul(&scale_factor)?;
        character_widths.set_item(&gid_key, py_round(py, &advance, None)?)?;
        // Add GID to character_map when we can find it in the cmap
        let char_code = reverse_cmap.call_method1("get", (&glyph,))?;
        if !char_code.is_none() {
            character_map.set_item(&gid_key, py_chr(py, &char_code)?)?;
        }
    }

    let space_char = cls
        .getattr("_get_space_char")?
        .call1((&encoding, &character_map))?;
    let flags = font_descriptor_kwargs
        .get_item("flags")?
        .unwrap_or_else(|| py_none(py));
    cls.getattr("_add_default_width")?
        .call1((&character_widths, &flags, &space_char))?;
    let space_width =
        cls.getattr("_add_space_width")?
            .call1((&character_widths, &flags, &space_char))?;

    Ok(TrueTypeFontData {
        font_descriptor,
        encoding,
        character_widths,
        character_map,
        space_char,
        space_width,
    })
}

fn from_truetype_font_file_impl<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    font_file: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let has_fonttools: bool = font_attr(py, "HAS_FONTTOOLS")?.extract()?;
    if !has_fonttools {
        return Err(pyo3::exceptions::PyImportError::new_err(
            "The 'fontTools' library is required to use 'from_truetype_font_file'",
        ));
    }
    let tt_font_class = font_attr(py, "TTFont")?;
    let tt_font_object = match tt_font_class.call1((font_file,)) {
        Ok(tt_font_object) => tt_font_object,
        Err(error) if is_python_exception(py, &error) => {
            let message = format!(
                "Could not open font file: {}",
                exception_args_str(py, &error)?
            );
            return Err(with_cause(py, pdf_read_error(py, &message), error));
        }
        Err(error) => return Err(error),
    };

    tt_font_object.call_method0("__enter__")?;
    let body_result = read_truetype_font_data(py, cls, &tt_font_object, font_file);
    let data = match body_result {
        Ok(data) => {
            tt_font_object.call_method1("__exit__", (py.None(), py.None(), py.None()))?;
            data
        }
        Err(error) => {
            let value = error.value(py);
            let exception_type = value.get_type();
            match tt_font_object.call_method1("__exit__", (exception_type, value, py.None())) {
                Ok(_) => {}
                Err(exit_error) => return Err(exit_error),
            }
            return Err(error);
        }
    };

    let kwargs = PyDict::new(py);
    kwargs.set_item("name", data.font_descriptor.getattr("name")?)?;
    kwargs.set_item("sub_type", py_str_obj(py, "Type0"))?;
    kwargs.set_item("encoding", &data.encoding)?;
    kwargs.set_item("font_descriptor", &data.font_descriptor)?;
    kwargs.set_item("character_map", &data.character_map)?;
    kwargs.set_item("character_widths", &data.character_widths)?;
    kwargs.set_item("space_width", &data.space_width)?;
    kwargs.set_item("space_char", &data.space_char)?;
    kwargs.set_item("interpretable", true)?;
    call_with_kwargs(cls, &kwargs)
}

// @pypdf: pypdf.generic._font.Font.from_truetype_font_file
#[pyfunction]
fn font_from_truetype_font_file<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    font_file: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(from_truetype_font_file_impl(py, cls, font_file)?.unbind())
}

// @pypdf: pypdf.generic._font.Font.from_core_font_name
#[pyfunction]
fn font_from_core_font_name<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    core_font_name: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let core_font_metrics = py
        .import("pypdf._codecs.core_font_metrics")?
        .getattr("CORE_FONT_METRICS")?;
    let font_name = core_font_name.call_method1("removeprefix", ("/",))?;
    let core_font_metrics_entry = core_font_metrics.get_item(&font_name)?;
    // WinAnsiEncoding
    let win_ansi_encoding =
        font_attr(py, "encoding_dict_from_named_encoding")?.call1(("cp1252",))?;
    let core_font_widths = core_font_metrics_entry.getattr("character_widths")?;

    let character_widths = PyDict::new(py);
    for item in win_ansi_encoding.call_method0("items")?.try_iter()? {
        let item = item?;
        let code = item.get_item(0)?;
        let character = item.get_item(1)?;
        let char = py_chr(py, &code)?;
        if core_font_widths.contains(&char)? {
            character_widths.set_item(char, core_font_widths.get_item(&character)?)?;
        }
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("name", &font_name)?;
    kwargs.set_item("character_map", PyDict::new(py))?;
    kwargs.set_item("encoding", &win_ansi_encoding)?;
    kwargs.set_item("sub_type", py_str_obj(py, "Type1"))?;
    kwargs.set_item(
        "font_descriptor",
        core_font_metrics_entry.getattr("font_descriptor")?,
    )?;
    kwargs.set_item("character_widths", &character_widths)?;
    let font = call_with_kwargs(cls, &kwargs)?;
    font.getattr("character_widths")?
        .set_item("default", core_font_widths.get_item("default")?)?;
    Ok(font.unbind())
}

// ---------------------------------------------------------------------------
// Font._get_typographic_maps
// ---------------------------------------------------------------------------

fn get_typographic_maps_impl<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let reverse_cmap = PyDict::new(py);
    let encoding_cmap = PyDict::new(py);
    let mut resolved = false;

    let has_fonttools: bool = font_attr(py, "HAS_FONTTOOLS")?.extract()?;
    if has_fonttools {
        let font_descriptor = self_.getattr("font_descriptor")?;
        let font_file = match font_descriptor.getattr("font_file") {
            Ok(font_file) => font_file,
            Err(error) if error.is_instance_of::<PyAttributeError>(py) => py_none(py),
            Err(error) => return Err(error),
        };
        let encoding = self_.getattr("encoding")?;
        let string_class = builtin(py, "str")?;
        if font_file.is_truthy()? && is_instance(&encoding, &string_class)? {
            let font_file_data = font_file.call_method0("get_data")?;
            if !font_file_data.is_none() {
                let bytes_io_class = font_attr(py, "BytesIO")?;
                let tt_font_class = font_attr(py, "TTFont")?;
                let stream = bytes_io_class.call1((&font_file_data,))?;
                match tt_font_class.call1((&stream,)) {
                    Ok(tt_font_object) => {
                        tt_font_object.call_method0("__enter__")?;
                        let body: PyResult<bool> = (|| {
                            // Read reverse_cmap and encoding_cmap from the font file if we can get a cmap
                            let kwargs = PyDict::new(py);
                            kwargs.set_item("table_name", py_str_obj(py, "cmap"))?;
                            kwargs.set_item("is_critical", false)?;
                            kwargs.set_item("tt_font_object", &tt_font_object)?;
                            let tt_font_cmap_table = self_
                                .getattr("_load_fonttools_table")?
                                .call((), Some(&kwargs))?;
                            if tt_font_cmap_table.is_truthy()? {
                                let best_cmap = tt_font_cmap_table.call_method0("getBestCmap")?;
                                if best_cmap.is_truthy()? {
                                    for item in best_cmap.call_method0("items")?.try_iter()? {
                                        let item = item?;
                                        let unicode_int = item.get_item(0)?;
                                        let glyph_name = item.get_item(1)?;
                                        let glyph_id = tt_font_object
                                            .call_method1("getGlyphID", (&glyph_name,))?;
                                        let gid_key = gid_key_string(&glyph_id)?;
                                        let unicode_char = py_chr(py, &unicode_int)?;
                                        reverse_cmap.set_item(&unicode_char, &gid_key)?;
                                        encoding_cmap.set_item(
                                            &gid_key,
                                            gid_key.call_method1("encode", (&encoding,))?,
                                        )?;
                                    }
                                    return Ok(true);
                                }
                            }
                            Ok(false)
                        })();
                        match body {
                            Ok(did_resolve) => {
                                tt_font_object
                                    .call_method1("__exit__", (py.None(), py.None(), py.None()))?;
                                resolved = did_resolve;
                            }
                            Err(error) => {
                                let value = error.value(py);
                                let exception_type = value.get_type();
                                tt_font_object
                                    .call_method1("__exit__", (exception_type, value, py.None()))?;
                                return Err(error);
                            }
                        }
                    }
                    Err(error) if error.is_instance_of::<PyMemoryError>(py) => {
                        return Err(error);
                    }
                    Err(error) if is_python_exception(py, &error) => {
                        // Font file data is corrupt.
                        let values = PyDict::new(py);
                        values.set_item("exception", error.value(py))?;
                        logger_warning_dict(py, "Could not open font file: %(exception)s", values)?;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
    }

    if resolved {
        return tuple_of(py, vec![reverse_cmap.into_any(), encoding_cmap.into_any()]);
    }

    let encoding = self_.getattr("encoding")?;
    let character_map = self_.getattr("character_map")?;
    let string_class = builtin(py, "str")?;
    if is_instance(&encoding, &string_class)? {
        for item in character_map.call_method0("items")?.try_iter()? {
            let item = item?;
            let glyph_id = item.get_item(0)?;
            let unicode_char = item.get_item(1)?;
            let glyph_id_str = glyph_id.str()?;
            reverse_cmap.set_item(&unicode_char, &glyph_id_str)?;
            encoding_cmap.set_item(
                &glyph_id_str,
                glyph_id_str.call_method1("encode", (&encoding,))?,
            )?;
        }
    } else {
        // Encoding is a dict, which means we are dealing with a simple font
        let unicode_to_bytes = PyDict::new(py);
        for item in encoding.call_method0("items")?.try_iter()? {
            let item = item?;
            let character_code = item.get_item(0)?;
            let unicode_char = item.get_item(1)?;
            let character_str = py_chr(py, &character_code)?;
            let encoded = py_single_byte(py, &character_code)?;
            reverse_cmap.set_item(&unicode_char, &character_str)?;
            encoding_cmap.set_item(&character_str, &encoded)?;
            unicode_to_bytes.set_item(&unicode_char, encoded)?;
        }
        for item in character_map.call_method0("items")?.try_iter()? {
            let item = item?;
            let character_code_str = item.get_item(0)?;
            let unicode_char = item.get_item(1)?;
            reverse_cmap.set_item(&unicode_char, &character_code_str)?;
            let default = py_single_byte(py, &py_ord(py, &character_code_str)?)?;
            let encoded = match unicode_to_bytes.get_item(&unicode_char)? {
                Some(encoded) => encoded,
                None => default,
            };
            encoding_cmap.set_item(&character_code_str, encoded)?;
        }
    }

    tuple_of(py, vec![reverse_cmap.into_any(), encoding_cmap.into_any()])
}

// @pypdf: pypdf.generic._font.Font._get_typographic_maps
#[pyfunction]
fn font_get_typographic_maps<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(get_typographic_maps_impl(py, self_)?.unbind())
}

// ---------------------------------------------------------------------------
// Font._create_widths_list_and_unicode_stream
// ---------------------------------------------------------------------------

fn create_widths_list_and_unicode_stream_impl<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let core_font_metrics = py
        .import("pypdf._codecs.core_font_metrics")?
        .getattr("CORE_FONT_METRICS")?;
    let string_class = builtin(py, "str")?;
    let number_class = generic_class(py, "NumberObject")?;
    let array_class = generic_class(py, "ArrayObject")?;

    let widths_list = PyList::empty(py);
    let mut unicode_map: Vec<String> = Vec::new();
    let mut bfchar_map: Vec<String> = Vec::new();

    let sub_type = self_.getattr("sub_type")?;
    let is_type0 = sub_type.eq("Type0")?;
    // Composite/CID fonts use 4-hex digits, simple fonts use 2-hex digits
    let src_hex_format = py_str_obj(py, if is_type0 { "{cid:04X}" } else { "{cid:02X}" });
    let codespace_min = if is_type0 { "0000" } else { "00" };
    let codespace_max = if is_type0 { "FFFF" } else { "FF" };
    let cmap_name = if is_type0 {
        "Adobe-Identity-UCS"
    } else {
        "Custom-Simple-8Bit"
    };
    let cid_system_info = if is_type0 {
        "/CIDSystemInfo <<\n/Registry (Adobe)\n/Ordering (UCS)\n/Supplement 0\n>> def\n"
    } else {
        ""
    };

    let character_map = self_.getattr("character_map")?;
    let encoding = self_.getattr("encoding")?;
    let name = self_.getattr("name")?;
    let name_in_core_metrics = core_font_metrics.contains(&name)?;
    let character_widths = self_.getattr("character_widths")?;
    let default_width = character_widths.get_item("default")?;

    // If we have self.character_map then use that. Otherwise fall back to self.encoding.
    let mapping_source = if character_map.is_truthy()? {
        character_map
    } else {
        encoding
    };
    for item in mapping_source.call_method0("items")?.try_iter()? {
        let item = item?;
        let src_id = item.get_item(0)?;
        let actual_char = item.get_item(1)?;
        // Make sure that we do not include characters such as arabic presentation form characters.
        // Note that, in some cases, unicodedata.normalize() might split a ligature, resulting
        // in multiple characters.
        let normalized_chars =
            font_attr(py, "unicodedata")?.call_method1("normalize", ("NFKC", &actual_char))?;
        let cid = if is_instance(&src_id, &string_class)? {
            py_ord(py, &src_id)?
        } else {
            src_id.clone()
        };
        let format_kwargs = PyDict::new(py);
        format_kwargs.set_item("cid", &cid)?;
        let cid_hex = src_hex_format.call_method("format", (), Some(&format_kwargs))?;
        let uni_hex = normalized_chars
            .call_method1("encode", ("utf-16-be",))?
            .call_method0("hex")?
            .call_method0("upper")?;
        bfchar_map.push(format!(
            "<{}> <{}>",
            pyutil::py_str(&cid_hex)?,
            pyutil::py_str(&uni_hex)?
        ));

        // Width mapping, but not for the 14 Adobe code fonts, which are dealt with elsewhere.
        if !name_in_core_metrics {
            // The widths (/W) array can have two formats:
            //    [first_cid [w1 w2 w3]] or [first last width]
            // Here we choose the first format and simply provide one array with one width for every cid.
            let width = character_widths.call_method1("get", (&src_id, &default_width))?;
            let cid_number = number_class.call1((&cid,))?;
            let width_array =
                array_class.call1((PyList::new(py, [number_class.call1((&width,))?])?,))?;
            widths_list.call_method1("extend", (PyList::new(py, [cid_number, width_array])?,))?;
        }
    }

    let mut index = 0;
    while index < bfchar_map.len() {
        let end = (index + CMAP_MAX_ENTRIES_PER_GROUP).min(bfchar_map.len());
        let partial_list = &bfchar_map[index..end];
        unicode_map.push(format!("{} beginbfchar", partial_list.len()));
        for line in partial_list {
            unicode_map.push(line.clone());
        }
        unicode_map.push("endbfchar".to_string());
        index = end;
    }

    let mut payload = String::new();
    payload.push_str("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n");
    payload.push_str(cid_system_info);
    payload.push_str(&format!("/CMapName /{cmap_name} def\n"));
    payload.push_str("/CMapType 2 def\n");
    payload.push_str(&format!(
        "1 begincodespacerange <{codespace_min}> <{codespace_max}> endcodespacerange\n"
    ));
    payload.push_str(&unicode_map.join("\n"));
    payload.push_str("\nendcmap\nCMapName currentdict /CMap defineresource pop\nend end");

    // Create the /ToUnicode CMap Stream
    let to_unicode_stream = stream_object(py)?;
    to_unicode_stream.call_method1(
        "set_data",
        (py_str_obj(py, &payload).call_method1("encode", ("ascii",))?,),
    )?;

    tuple_of(py, vec![widths_list.into_any(), to_unicode_stream])
}

// @pypdf: pypdf.generic._font.Font._create_widths_list_and_unicode_stream
#[pyfunction]
fn font_create_widths_list_and_unicode_stream<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(create_widths_list_and_unicode_stream_impl(py, self_)?.unbind())
}

// ---------------------------------------------------------------------------
// Font.as_font_resource
// ---------------------------------------------------------------------------

fn as_font_resource_impl<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let font_descriptor = self_.getattr("font_descriptor")?;
    let font_file = font_descriptor.getattr("font_file")?;
    let sub_type = self_.getattr("sub_type")?;
    // If we have an embedded Truetype font, we assume that we need to produce a Type 2 CID font resource.
    // We check that we are 16-bit encoded, that is, Type0.
    if font_file.is_truthy()? && sub_type.eq("Type0")? {
        // Begin with creating the widths array (part of the descendant font) and the unicode cmap (part
        // of the Type 0 font object).
        let widths_and_stream = self_.call_method0("_create_widths_list_and_unicode_stream")?;
        let widths_list = widths_and_stream.get_item(0)?;
        let to_unicode_stream = widths_and_stream.get_item(1)?;
        let name = self_.getattr("name")?;
        let base_font_name = format!("/{}", pyutil::py_str(&name)?);
        let default_width = self_.getattr("character_widths")?.get_item("default")?;

        // Create the descendant font object
        let cid_font = dict_object(
            py,
            vec![
                (name_object(py, "/Type")?, name_object(py, "/Font")?),
                (
                    name_object(py, "/Subtype")?,
                    name_object(py, "/CIDFontType2")?,
                ),
                (
                    name_object(py, "/BaseFont")?,
                    name_object(py, &base_font_name)?,
                ),
                (
                    name_object(py, "/CIDSystemInfo")?,
                    dict_object(
                        py,
                        vec![
                            (
                                name_object(py, "/Registry")?,
                                generic_class(py, "TextStringObject")?.call1(("Adobe",))?,
                            ),
                            (
                                name_object(py, "/Ordering")?,
                                generic_class(py, "TextStringObject")?.call1(("Identity",))?,
                            ),
                            (
                                name_object(py, "/Supplement")?,
                                number_object(py, &py_int_obj(py, 0))?,
                            ),
                        ],
                    )?,
                ),
                (
                    name_object(py, "/FontDescriptor")?,
                    font_descriptor.call_method0("as_font_descriptor_resource")?,
                ),
                (name_object(py, "/W")?, array_object(py, &widths_list)?),
                (name_object(py, "/DW")?, number_object(py, &default_width)?),
                (
                    name_object(py, "/CIDToGIDMap")?,
                    name_object(py, "/Identity")?,
                ),
            ],
        )?;

        // Create the Type 0 font object
        return dict_object(
            py,
            vec![
                (name_object(py, "/Type")?, name_object(py, "/Font")?),
                (name_object(py, "/Subtype")?, name_object(py, "/Type0")?),
                (
                    name_object(py, "/BaseFont")?,
                    name_object(py, &base_font_name)?,
                ),
                (
                    name_object(py, "/Encoding")?,
                    name_object(py, "/Identity-H")?,
                ),
                (
                    name_object(py, "/DescendantFonts")?,
                    array_object(py, PyList::new(py, [cid_font])?.as_any())?,
                ),
                (name_object(py, "/ToUnicode")?, to_unicode_stream),
            ],
        );
    }

    // Fallback: Return a font resource for one of the 14 Adobe Core fonts.
    let win_ansi_encoding =
        font_attr(py, "encoding_dict_from_named_encoding")?.call1(("cp1252",))?;
    let differences_list = PyList::empty(py);
    let adobe_glyphs = py
        .import("pypdf._codecs.adobe_glyphs")?
        .getattr("adobe_glyphs")?;
    let reverse_adobe_glyphs = PyDict::new(py);
    for item in adobe_glyphs.call_method0("items")?.try_iter()? {
        let item = item?;
        reverse_adobe_glyphs.set_item(item.get_item(1)?, item.get_item(0)?)?;
    }
    let own_encoding = self_.getattr("encoding")?;
    let number_class = generic_class(py, "NumberObject")?;
    let name_class = generic_class(py, "NameObject")?;
    for item in win_ansi_encoding.call_method0("items")?.try_iter()? {
        let item = item?;
        let idx = item.get_item(0)?;
        let character_code = item.get_item(1)?;
        let encoding_char = own_encoding.call_method1("get", (&idx,))?;
        if encoding_char.is_truthy()? && !encoding_char.eq(&character_code)? {
            differences_list.call_method1(
                "extend",
                (PyList::new(
                    py,
                    [
                        number_class.call1((&idx,))?,
                        name_class.call1((reverse_adobe_glyphs.get_item(&encoding_char)?,))?,
                    ],
                )?,),
            )?;
        }
    }

    let differences_present = !differences_list.is_empty();
    let encoding: Bound<'py, PyAny> = if differences_present {
        dict_object(
            py,
            vec![
                (
                    name_object(py, "/BaseEncoding")?,
                    name_object(py, "/WinAnsiEncoding")?,
                ),
                (
                    name_object(py, "/Differences")?,
                    array_object(py, differences_list.as_any())?,
                ),
            ],
        )?
    } else {
        name_object(py, "/WinAnsiEncoding")?
    };

    let name = self_.getattr("name")?;
    let font_name = format!("/{}", pyutil::py_str(&name)?);
    let simple_font = dict_object(
        py,
        vec![
            (name_object(py, "/Type")?, name_object(py, "/Font")?),
            (name_object(py, "/Subtype")?, name_object(py, "/Type1")?),
            (name_object(py, "/Name")?, name_object(py, &font_name)?),
            (name_object(py, "/BaseFont")?, name_object(py, &font_name)?),
            (name_object(py, "/Encoding")?, encoding),
        ],
    )?;

    if differences_present {
        let widths_and_stream = self_.call_method0("_create_widths_list_and_unicode_stream")?;
        simple_font.set_item(
            name_object(py, "/ToUnicode")?,
            widths_and_stream.get_item(1)?,
        )?;
    }

    Ok(simple_font)
}

// @pypdf: pypdf.generic._font.Font.as_font_resource
#[pyfunction]
fn font_as_font_resource<'py>(py: Python<'py>, self_: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(as_font_resource_impl(py, self_)?.unbind())
}

// ---------------------------------------------------------------------------
// Font._add_to_writer
// ---------------------------------------------------------------------------

fn add_to_writer_impl<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
    writer: &Bound<'py, PyAny>,
    target_resource_dict: &Bound<'py, PyAny>,
    font_resource_name: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let font_resource = self_.call_method0("as_font_resource")?;
    if font_resource.contains("/ToUnicode")? {
        font_resource.set_item(
            name_object(py, "/ToUnicode")?,
            writer.call_method1("_add_object", (font_resource.get_item("/ToUnicode")?,))?,
        )?;
    }

    let font_resource_dict = if font_resource.contains("/DescendantFonts")? {
        font_resource.get_item("/DescendantFonts")?.get_item(0)?
    } else {
        font_resource.clone()
    };

    if font_resource_dict.contains("/FontDescriptor")? {
        let font_descriptor_obj = font_resource_dict.get_item("/FontDescriptor")?;
        for key in ["/FontFile", "/FontFile2", "/FontFile3"] {
            if font_descriptor_obj.contains(key)? {
                font_descriptor_obj.set_item(
                    name_object(py, key)?,
                    writer.call_method1("_add_object", (font_descriptor_obj.get_item(key)?,))?,
                )?;
            }
        }
        font_resource_dict.set_item(
            name_object(py, "/FontDescriptor")?,
            writer.call_method1(
                "_add_object",
                (font_resource_dict.get_item("/FontDescriptor")?,),
            )?,
        )?;
    }
    let font_resource_ref = writer.call_method1("_add_object", (&font_resource,))?;
    target_resource_dict.set_item(font_resource_name, &font_resource_ref)?;
    Ok(font_resource_ref)
}

// @pypdf: pypdf.generic._font.Font._add_to_writer
#[pyfunction]
fn font_add_to_writer<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
    writer: &Bound<'py, PyAny>,
    target_resource_dict: &Bound<'py, PyAny>,
    font_resource_name: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(add_to_writer_impl(py, self_, writer, target_resource_dict, font_resource_name)?.unbind())
}

// ---------------------------------------------------------------------------
// Text width / encoding support
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._font.Font.get_text_width
#[pyfunction]
pub(crate) fn font_get_text_width<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
    text: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let character_widths = self_.getattr("character_widths")?;
    let default_width = character_widths.get_item("default")?;
    let widths = PyList::empty(py);
    for character in text.try_iter()? {
        let character = character?;
        widths.append(character_widths.call_method1("get", (&character, &default_width))?)?;
    }
    let total = builtin(py, "sum")?.call1((widths, 0.0))?;
    Ok(total.unbind())
}

// @pypdf: pypdf.generic._font.Font.can_encode
#[pyfunction]
fn font_can_encode(
    py: Python<'_>,
    self_: &Bound<'_, PyAny>,
    text: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let character_map = self_.getattr("character_map")?;
    if character_map.is_truthy()? {
        let supported = builtin(py, "set")?.call1((character_map.call_method0("values")?,))?;
        for character in text.try_iter()? {
            let character = character?;
            if !supported.contains(&character)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    let encoding = self_.getattr("encoding")?;
    let dict_class = builtin(py, "dict")?;
    if is_instance(&encoding, &dict_class)? {
        let supported = builtin(py, "set")?.call1((encoding.call_method0("values")?,))?;
        for character in text.try_iter()? {
            let character = character?;
            if !supported.contains(&character)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    // Not a simple font (encoding is not a dict), and missing ToUnicode cmap (no character_map).
    // Assume we cannot use this font for text encoding.
    Ok(false)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Symbols implemented in the hand-written shim (`python/pypdf/generic/_font.py`):
/// the module constants, `HAS_FONTTOOLS`, the `CoreFontMetrics` dataclass and the
/// re-exported helpers (`get_encoding`, `adobe_glyphs`,
/// `encoding_dict_from_named_encoding`, `logger_warning`, `FontFlags`,
/// `LimitReachedError`, `PdfReadError`, `TTFont`, `BytesIO`, dataclasses, ...).
///
// @pypdf: pypdf.generic._font.HAS_FONTTOOLS
// @pypdf: pypdf.generic._font.HAS_FONTTOOLS
// @pypdf: pypdf.generic._font.MAX_CID_WIDTH_ENTRY_COUNT
// @pypdf: pypdf.generic._font.MAX_WIDTH_ENTRY_COUNT
// @pypdf: pypdf.generic._font.MAX_SIMPLE_FONT_CHARACTER_CODE
// @pypdf: pypdf.generic._font.HEADER_MACSTYLE_ITALIC
// @pypdf: pypdf.generic._font.OS2_FSSELECTION_ITALIC
// @pypdf: pypdf.generic._font.OS2_PANOSE_BFAMILYTYPE_SCRIPT
// @pypdf: pypdf.generic._font.OS2_PANOSE_BFAMILYTYPE_DECORATIVE
// @pypdf: pypdf.generic._font.OS2_PANOSE_BFAMILYTYPE_PICTORIAL
// @pypdf: pypdf.generic._font.OS2_PANOSE_BPROPORTION_MONOSPACED
// @pypdf: pypdf.generic._font.OS2_SFAMILYSCLASS_SCRIPTS
// @pypdf: pypdf.generic._font.OS2_SFAMILYSCLASS_SYMBOLIC
// @pypdf: pypdf.generic._font.CMAP_MAX_ENTRIES_PER_GROUP
// @pypdf: pypdf.generic._font.CoreFontMetrics
// @pypdf: pypdf.generic._font.CoreFontMetrics.font_descriptor
// @pypdf: pypdf.generic._font.CoreFontMetrics.character_widths
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(
        font_font_descriptor_as_font_descriptor_resource,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        font_collect_tt_t1_character_widths,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(font_check_range_length, module)?)?;
    module.add_function(wrap_pyfunction!(font_check_entry_count, module)?)?;
    module.add_function(wrap_pyfunction!(font_collect_cid_character_widths, module)?)?;
    module.add_function(wrap_pyfunction!(font_get_space_char, module)?)?;
    module.add_function(wrap_pyfunction!(font_add_default_width, module)?)?;
    module.add_function(wrap_pyfunction!(font_add_space_width, module)?)?;
    module.add_function(wrap_pyfunction!(font_parse_bbox, module)?)?;
    module.add_function(wrap_pyfunction!(font_parse_font_descriptor, module)?)?;
    module.add_function(wrap_pyfunction!(font_from_font_resource, module)?)?;
    module.add_function(wrap_pyfunction!(font_load_fonttools_table, module)?)?;
    module.add_function(wrap_pyfunction!(
        font_get_font_flags_from_truetype_font_tables,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(font_from_truetype_font_file, module)?)?;
    module.add_function(wrap_pyfunction!(font_from_core_font_name, module)?)?;
    module.add_function(wrap_pyfunction!(font_get_typographic_maps, module)?)?;
    module.add_function(wrap_pyfunction!(
        font_create_widths_list_and_unicode_stream,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(font_as_font_resource, module)?)?;
    module.add_function(wrap_pyfunction!(font_add_to_writer, module)?)?;
    module.add_function(wrap_pyfunction!(font_get_text_width, module)?)?;
    module.add_function(wrap_pyfunction!(font_can_encode, module)?)?;
    Ok(())
}
