//! `pypdf._text_extraction` module functions (upstream `_text_extraction/__init__.py`).
//!
//! Every function mirrors the upstream body line by line.  The module-global RTL
//! state (`CUSTOM_RTL_MIN`, `CUSTOM_RTL_MAX`, `CUSTOM_RTL_SPECIAL_CHARS`,
//! `RTL_CHARACTER_PATTERN`) and the `is_char_neutral` / `is_char_rtl` helpers are
//! resolved from the shim module at call time, exactly like upstream's global
//! lookups, so that `set_custom_rtl()` and `monkeypatch.setattr()` behave the same.

use pyo3::exceptions::PyUnicodeDecodeError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};

use super::common;

/// Whether a module-level text-extraction helper is still the default one.
///
/// The first object seen for `name` is remembered; a replaced attribute falls
/// back to the Python call so monkeypatching behaves like upstream.
pub(crate) fn is_default_function(py: Python<'_>, name: &str, current: &Bound<'_, PyAny>) -> PyResult<bool> {
    use pyo3::sync::PyOnceLock;
    static DEFAULT_MULT: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    static DEFAULT_ORIENT: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    static DEFAULT_CRLF: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    let cell = match name {
        "mult" => &DEFAULT_MULT,
        "orient" => &DEFAULT_ORIENT,
        "crlf_space_check" => &DEFAULT_CRLF,
        _ => return Ok(false),
    };
    let default = cell.get_or_try_init(py, || Ok::<_, PyErr>(current.clone().unbind()))?;
    Ok(default.bind(py).is(current))
}

/// Whether `font.get_text_width` is still the class implementation (no instance
/// or class monkeypatching), so the Rust implementation may be called directly.
fn font_text_width_is_default(py: Python<'_>, font: &Bound<'_, PyAny>) -> PyResult<bool> {
    use pyo3::sync::PyOnceLock;
    if let Ok(instance_dict) = font.getattr("__dict__") {
        if let Ok(dict) = instance_dict.cast::<PyDict>() {
            if dict.contains("get_text_width")? {
                return Ok(false);
            }
        }
    }
    let class = font.get_type();
    let current = class.getattr("get_text_width")?;
    static DEFAULT: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    let default = DEFAULT.get_or_try_init(py, || Ok::<_, PyErr>(current.clone().unbind()))?;
    Ok(default.bind(py).is(&current))
}

/// Sum `font.get_text_width` over `source` with a Rust-side cache.
///
/// Returns `None` when a width is not a plain number, so the caller can run the
/// original Python-level loop (which preserves exotic numeric types).
fn text_widths_fast(
    py: Python<'_>,
    font: &Bound<'_, PyAny>,
    source: &Bound<'_, PyAny>,
) -> PyResult<Option<f64>> {
    // Fast path: `source` is a str, so iterate code points natively and only
    // create Python characters when `get_text_width` has to be called.
    let Ok(text) = source.extract::<String>() else {
        return Ok(None);
    };
    let space_char = match font.getattr("space_char").and_then(|value| value.extract::<String>()) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let space_width = match font.getattr("space_width").and_then(|value| value.extract::<f64>()) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let single_space = {
        let mut chars = space_char.chars();
        match (chars.next(), chars.next()) {
            (Some(character), None) => Some(character),
            _ => None,
        }
    };
    let default_width = font_text_width_is_default(py, font)?;
    let mut cache: std::collections::HashMap<char, f64> = std::collections::HashMap::new();
    let mut total = 0.0f64;
    for character in text.chars() {
        if Some(character) == single_space {
            total += space_width;
            continue;
        }
        let width = match cache.get(&character) {
            Some(width) => *width,
            None => {
                let mut buffer = [0u8; 4];
                let character_object = common::py_str_val(py, character.encode_utf8(&mut buffer));
                let value = if default_width {
                    crate::font_mod::font_get_text_width(py, font, &character_object)?
                } else {
                    font.call_method1("get_text_width", (&character_object,))?
                        .unbind()
                };
                let Ok(width) = value.extract::<f64>(py) else {
                    return Ok(None);
                };
                cache.insert(character, width);
                width
            }
        };
        total += width;
    }
    Ok(Some(total))
}

// @pypdf: pypdf._text_extraction.set_custom_rtl
#[pyfunction]
pub fn text_extraction_set_custom_rtl<'py>(
    py: Python<'py>,
    _min: &Bound<'py, PyAny>,
    _max: &Bound<'py, PyAny>,
    specials: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let module = common::init_module(py)?;
    let lower = common::init_attr(py, "UNICODE_LOWER_LIMIT")?;
    let upper = common::init_attr(py, "UNICODE_UPPER_LIMIT")?;
    let mut min_value = common::init_attr(py, "CUSTOM_RTL_MIN")?;
    let mut max_value = common::init_attr(py, "CUSTOM_RTL_MAX")?;
    let mut specials_value = common::init_attr(py, "CUSTOM_RTL_SPECIAL_CHARS")?;

    if common::is_int(py, _min)? {
        min_value = if lower.le(_min)? && _min.le(&upper)? {
            common::chr_of(py, _min)?
        } else {
            common::py_str_val(py, "")
        };
    } else if common::is_str(py, _min)? {
        min_value = _min.clone();
    }
    if common::is_int(py, _max)? {
        max_value = if lower.le(_max)? && _max.le(&upper)? {
            common::chr_of(py, _max)?
        } else {
            common::py_str_val(py, "")
        };
    } else if common::is_str(py, _max)? {
        max_value = _max.clone();
    }
    if common::is_str(py, specials)? {
        specials_value = specials.clone();
    } else if common::is_list(py, specials)? {
        let mut codes: Vec<Bound<'py, PyAny>> = Vec::new();
        for char in specials.try_iter()? {
            let char = char?;
            // `UNICODE_LOWER_LIMIT <= char <= UNICODE_UPPER_LIMIT`
            if lower.le(&char)? && char.le(&upper)? {
                codes.push(common::chr_of(py, &char)?);
            }
        }
        specials_value = common::py_str_val(py, "")
            .call_method1("join", (pyo3::types::PyList::new(py, codes)?,))?;
    }

    module.setattr("CUSTOM_RTL_MIN", &min_value)?;
    module.setattr("CUSTOM_RTL_MAX", &max_value)?;
    module.setattr("CUSTOM_RTL_SPECIAL_CHARS", &specials_value)?;
    common::py_tuple(py, vec![min_value, max_value, specials_value])
}

// @pypdf: pypdf._text_extraction.mult
#[pyfunction]
pub fn text_extraction_mult<'py>(
    py: Python<'py>,
    m: &Bound<'py, PyAny>,
    n: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    common::mult(py, m, n)
}

// @pypdf: pypdf._text_extraction.orient
#[pyfunction]
pub fn text_extraction_orient(py: Python<'_>, m: &Bound<'_, PyAny>) -> PyResult<i32> {
    common::orient(py, m)
}


/// Multiply two 6-element affine matrices with native doubles (same IEEE
/// operations and order as `pypdf._text_extraction.mult`).
fn matrix_mult(m: [f64; 6], n: [f64; 6]) -> [f64; 6] {
    [
        m[0] * n[0] + m[1] * n[2],
        m[0] * n[1] + m[1] * n[3],
        m[2] * n[0] + m[3] * n[2],
        m[2] * n[1] + m[3] * n[3],
        m[4] * n[0] + m[5] * n[2] + n[4],
        m[4] * n[1] + m[5] * n[3] + n[5],
    ]
}

fn matrix_orient(m: [f64; 6]) -> i64 {
    if m[3] > 1e-6 {
        return 0;
    }
    if m[3] < -1e-6 {
        return 180;
    }
    if m[1] > 0.0 {
        return 90;
    }
    270
}

fn extract_matrix(value: &Bound<'_, PyAny>) -> Option<[f64; 6]> {
    let values = value.extract::<Vec<f64>>().ok()?;
    if values.len() != 6 {
        return None;
    }
    let mut matrix = [0.0f64; 6];
    matrix.copy_from_slice(&values);
    Some(matrix)
}

/// Native `crlf_space_check` used when `mult`/`orient` are the defaults.
///
/// `None` means the inputs are not plain numeric matrices: the caller falls
/// back to the object-based implementation.
pub(crate) fn crlf_space_check_native<'py>(
    py: Python<'py>,
    text: &Bound<'py, PyAny>,
    cm_prev: &Bound<'py, PyAny>,
    tm_prev: &Bound<'py, PyAny>,
    cm_matrix: &Bound<'py, PyAny>,
    tm_matrix: &Bound<'py, PyAny>,
    memo_cm: &Bound<'py, PyAny>,
    memo_tm: &Bound<'py, PyAny>,
    font_resource: &Bound<'py, PyAny>,
    orientations: &Bound<'py, PyAny>,
    output: &Bound<'py, PyAny>,
    font_size: &Bound<'py, PyAny>,
    visitor_text: &Bound<'py, PyAny>,
    str_widths: &Bound<'py, PyAny>,
    spacewidth: &Bound<'py, PyAny>,
    str_height: &Bound<'py, PyAny>,
    line_span: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    if std::env::var_os("PYPDF_RS_LEGACY_CRLF").is_some() {
        return Ok(None);
    }
    let Some(cm_prev_matrix) = extract_matrix(cm_prev) else {
        return Ok(None);
    };
    let Some(tm_prev_matrix) = extract_matrix(tm_prev) else {
        return Ok(None);
    };
    let Some(cm_matrix_values) = extract_matrix(cm_matrix) else {
        return Ok(None);
    };
    let Some(tm_matrix_values) = extract_matrix(tm_matrix) else {
        return Ok(None);
    };
    let (Ok(font_size_value), Ok(str_widths_value), Ok(spacewidth_value), Ok(str_height_value)) = (
        font_size.extract::<f64>(),
        str_widths.extract::<f64>(),
        spacewidth.extract::<f64>(),
        str_height.extract::<f64>(),
    ) else {
        return Ok(None);
    };

    let m_prev = matrix_mult(tm_prev_matrix, cm_prev_matrix);
    let m = matrix_mult(tm_matrix_values, cm_matrix_values);
    let orientation = matrix_orient(m);
    let delta_x = m[4] - m_prev[4];
    let delta_y = m[5] - m_prev[5];
    let scale_prev_x = (m_prev[0] * m_prev[0] + m_prev[1] * m_prev[1]).sqrt();
    let scale_prev_y = (m_prev[2] * m_prev[2] + m_prev[3] * m_prev[3]).sqrt();
    let scale_y = (m[2] * m[2] + m[3] * m[3]).sqrt();

    let orientation_object = common::py_int(py, orientation);
    if !orientations.contains(&orientation_object)? {
        return Err(common::raise_orientation_not_found(py));
    }
    let is_horizontal = orientation == 0 || orientation == 180;
    let (moved_height, moved_width) = if is_horizontal {
        (delta_y, delta_x)
    } else {
        (delta_x, delta_y)
    };
    let axis_index = if is_horizontal { 5usize } else { 4usize };
    let axis = m[axis_index];

    // line_span: None or (axis index, lower, upper).
    let mut span: Option<(i64, f64, f64)> = None;
    if !line_span.is_none() {
        let index: i64 = match line_span.get_item(0usize).and_then(|value| value.extract()) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        let lower: f64 = match line_span.get_item(1usize).and_then(|value| value.extract()) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        let upper: f64 = match line_span.get_item(2usize).and_then(|value| value.extract()) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        span = Some((index, lower, upper));
    }
    let index_matches = span.is_some_and(|(index, _, _)| index == axis_index as i64);
    let distance = if !index_matches {
        moved_height.abs()
    } else {
        let (_, lower, upper) = span.expect("checked above");
        if lower <= axis && axis <= upper {
            0.0
        } else {
            (axis - lower).abs().min((axis - upper).abs())
        }
    };

    let empty = common::py_str_val(py, "");
    let newline = common::py_str_val(py, "\n");
    let last = common::concat(output, text)?.get_item(common::slice_from_end(py)?)?;
    let last_is_empty = last.eq(&empty)?;
    let last_is_newline = last.eq(&newline)?;
    let threshold =
        0.8 * (str_height_value * scale_prev_y).min(font_size_value * scale_y);

    let mut text = text.clone();
    let mut output = output.clone();
    let mut new_span: Bound<'py, PyAny> = line_span.clone();
    if distance > threshold {
        if !(last_is_empty || last_is_newline) {
            let piece = common::concat(&text, &newline)?;
            output = common::concat(&output, &piece)?;
            if !visitor_text.is_none() {
                visitor_text.call1((&piece, memo_cm, memo_tm, font_resource, font_size))?;
            }
            text = empty.clone();
        }
        new_span = common::py_tuple(
            py,
            vec![
                common::py_int(py, axis_index as i64),
                common::py_float(py, axis),
                common::py_float(py, axis),
            ],
        )?;
    } else {
        let moved_to_other_baseline = moved_height.abs() > threshold;
        let space = common::py_str_val(py, " ");
        let gap_needed = moved_to_other_baseline
            || moved_width >= (spacewidth_value + str_widths_value) * scale_prev_x;
        let last_is_space = last.eq(&space)?;
        if gap_needed && !(last_is_empty || last_is_space) {
            text = common::concat(&text, &space)?;
        }
        match span {
            Some((index, lower, upper)) if index == axis_index as i64 && !last_is_empty && !last_is_newline => {
                new_span = common::py_tuple(
                    py,
                    vec![
                        common::py_int(py, index),
                        common::py_float(py, lower.min(axis)),
                        common::py_float(py, upper.max(axis)),
                    ],
                )?;
            }
            _ => {
                new_span = common::py_tuple(
                    py,
                    vec![
                        common::py_int(py, axis_index as i64),
                        common::py_float(py, axis),
                        common::py_float(py, axis),
                    ],
                )?;
            }
        }
    }
    let tm_prev_result = tm_matrix.call_method0("copy")?;
    let cm_prev_result = cm_matrix.call_method0("copy")?;
    Ok(Some(common::py_tuple(
        py,
        vec![text, output, cm_prev_result, tm_prev_result, new_span],
    )?))
}

// @pypdf: pypdf._text_extraction.crlf_space_check
#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn text_extraction_crlf_space_check<'py>(
    py: Python<'py>,
    text: &Bound<'py, PyAny>,
    cmtm_prev: &Bound<'py, PyAny>,
    cmtm_matrix: &Bound<'py, PyAny>,
    memo_cmtm: &Bound<'py, PyAny>,
    font_resource: &Bound<'py, PyAny>,
    orientations: &Bound<'py, PyAny>,
    output: &Bound<'py, PyAny>,
    font_size: &Bound<'py, PyAny>,
    visitor_text: &Bound<'py, PyAny>,
    str_widths: &Bound<'py, PyAny>,
    spacewidth: &Bound<'py, PyAny>,
    str_height: &Bound<'py, PyAny>,
    line_span: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mult_fn = common::init_attr(py, "mult")?;
    let orient_fn = common::init_attr(py, "orient")?;

    let cm_prev = cmtm_prev.get_item(0usize)?;
    let tm_prev = cmtm_prev.get_item(1usize)?;
    let cm_matrix = cmtm_matrix.get_item(0usize)?;
    let tm_matrix = cmtm_matrix.get_item(1usize)?;
    let memo_cm = memo_cmtm.get_item(0usize)?;
    let memo_tm = memo_cmtm.get_item(1usize)?;

    let direct = is_default_function(py, "mult", &mult_fn)?
        && is_default_function(py, "orient", &orient_fn)?;
    let m_prev = if direct {
        text_extraction_mult(py, &tm_prev, &cm_prev)?
    } else {
        mult_fn.call1((&tm_prev, &cm_prev))?
    };
    let m = if direct {
        text_extraction_mult(py, &tm_matrix, &cm_matrix)?
    } else {
        mult_fn.call1((&tm_matrix, &cm_matrix))?
    };
    let orientation = if direct {
        common::py_int(py, i64::from(text_extraction_orient(py, &m)?))
    } else {
        orient_fn.call1((&m,))?
    };
    let delta_x = m.get_item(4usize)?.sub(m_prev.get_item(4usize)?)?;
    let delta_y = m.get_item(5usize)?.sub(m_prev.get_item(5usize)?)?;
    // Table 108 of the 1.7 reference ("Text positioning operators")
    // delta_x/delta_y are expressed in the coordinate system produced by
    // text matrix x current transformation matrix, so the scaling factors
    // they get compared against have to be taken from the same combined
    // matrices instead of the text matrices alone.
    let scale_prev_x = common::sqrt_of(py, &squared_sum(py, &m_prev, 0)?)?;
    let scale_prev_y = common::sqrt_of(py, &squared_sum(py, &m_prev, 2)?)?;
    let scale_y = common::sqrt_of(py, &squared_sum(py, &m, 2)?)?;

    if !orientations.contains(&orientation)? {
        return Err(common::raise_orientation_not_found(py));
    }
    let zero = common::py_int(py, 0);
    let ninety = common::py_int(py, 90);
    let one_eighty = common::py_int(py, 180);
    let is_horizontal = orientation.eq(&zero)? || orientation.eq(&one_eighty)?;
    let (moved_height, moved_width) = if is_horizontal {
        (delta_y.clone(), delta_x.clone())
    } else if orientation.eq(&ninety)? || orientation.eq(common::py_int(py, 270))? {
        (delta_x.clone(), delta_y.clone())
    } else {
        // `orient()` only returns 0/90/180/270, so this mirrors upstream's
        // (unreachable) fall-through without leaving a variable unbound.
        (delta_y.clone(), delta_x.clone())
    };
    // Keep track of the baseline range of the current extracted line. A centered
    // row label can fall between the two baselines of a wrapped cell (#4130), so
    // comparing only with the immediately preceding fragment can incorrectly
    // insert a line break. Use the established baseline range for that case.
    let axis_index: i64 = if is_horizontal { 5 } else { 4 };
    let axis_index_obj = common::py_int(py, axis_index);
    let axis = m.get_item(axis_index as usize)?;
    let distance = if line_span.is_none() || line_span.get_item(0usize)?.ne(&axis_index_obj)? {
        common::abs_of(py, &moved_height)?
    } else {
        let lower = line_span.get_item(1usize)?;
        let upper = line_span.get_item(2usize)?;
        if lower.le(&axis)? && axis.le(&upper)? {
            common::py_float(py, 0.0)
        } else {
            let a = common::abs_of(py, &axis.sub(&lower)?)?;
            let b = common::abs_of(py, &axis.sub(&upper)?)?;
            common::min_of(py, &a, &b)?
        }
    };
    let concatenated = common::concat(output, text)?;
    let last = concatenated.get_item(common::slice_from_end(py)?)?;
    let empty = common::py_str_val(py, "");
    let newline = common::py_str_val(py, "\n");
    let last_is_empty = last.eq(&empty)?;
    let last_is_newline = last.eq(&newline)?;
    let threshold = common::py_float(py, 0.8).mul(common::min_of(
        py,
        &str_height.mul(&scale_prev_y)?,
        &font_size.mul(&scale_y)?,
    )?)?;
    let mut text = text.clone();
    let mut output = output.clone();
    let mut line_span = line_span.clone();
    let more_than_threshold = distance.gt(&threshold)?;
    if more_than_threshold {
        if !(last_is_empty || last_is_newline) {
            let piece = common::concat(&text, &newline)?;
            output = common::concat(&output, &piece)?;
            if !visitor_text.is_none() {
                visitor_text.call1((&piece, &memo_cm, &memo_tm, font_resource, font_size))?;
            }
            text = empty.clone();
        }
        line_span = common::py_tuple(py, vec![axis_index_obj.clone(), axis.clone(), axis.clone()])?;
    } else {
        // A fragment kept on this line only by the line span moved to another
        // baseline, so comparing its horizontal gap with the previous fragment
        // would miss the word separation (#4254).
        let moved_to_other_baseline = common::abs_of(py, &moved_height)?.gt(&threshold)?;
        let space = common::py_str_val(py, " ");
        let gap_needed = moved_to_other_baseline
            || moved_width.ge(&spacewidth.add(str_widths)?.mul(&scale_prev_x)?)?;
        let last_is_space = last.eq(&space)?;
        if gap_needed && !(last_is_empty || last_is_space) {
            text = common::concat(&text, &space)?;
        }
        if last_is_empty
            || last_is_newline
            || line_span.is_none()
            || line_span.get_item(0usize)?.ne(&axis_index_obj)?
        {
            line_span =
                common::py_tuple(py, vec![axis_index_obj.clone(), axis.clone(), axis.clone()])?;
        } else {
            let new_lower = common::min_of(py, &line_span.get_item(1usize)?, &axis)?;
            let new_upper = common::max_of(py, &line_span.get_item(2usize)?, &axis)?;
            line_span = common::py_tuple(py, vec![axis_index_obj.clone(), new_lower, new_upper])?;
        }
    }
    let tm_prev = tm_matrix.call_method0("copy")?;
    let cm_prev = cm_matrix.call_method0("copy")?;
    common::py_tuple(py, vec![text, output, cm_prev, tm_prev, line_span])
}

/// `m[i] ** 2 + m[i + 1] ** 2`.
fn squared_sum<'py>(
    py: Python<'py>,
    m: &Bound<'py, PyAny>,
    start: usize,
) -> PyResult<Bound<'py, PyAny>> {
    let a = m.get_item(start)?.pow(2i64, py.None())?;
    let b = m.get_item(start + 1)?.pow(2i64, py.None())?;
    a.add(&b)
}

// @pypdf: pypdf._text_extraction.get_text_operands
#[pyfunction]
pub fn text_extraction_get_text_operands<'py>(
    py: Python<'py>,
    operands: &Bound<'py, PyAny>,
    cm_matrix: &Bound<'py, PyAny>,
    tm_matrix: &Bound<'py, PyAny>,
    font: &Bound<'py, PyAny>,
    orientations: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // Direct Rust calls unless the module-level functions were replaced by a
    // user (monkeypatching must keep working).
    let mult_fn = common::init_attr(py, "mult")?;
    let m = if is_default_function(py, "mult", &mult_fn)? {
        text_extraction_mult(py, tm_matrix, cm_matrix)?
    } else {
        mult_fn.call1((tm_matrix, cm_matrix))?
    };
    let orient_fn = common::init_attr(py, "orient")?;
    let orientation = if is_default_function(py, "orient", &orient_fn)? {
        common::py_int(py, i64::from(text_extraction_orient(py, &m)?))
    } else {
        orient_fn.call1((&m,))?
    };

    let mut text = common::py_str_val(py, "");
    let mut is_str_operands = false;
    let mut widths = common::py_float(py, 0.0);
    let width_cache = PyDict::new(py);
    let mut raw_characters = common::py_str_val(py, "");
    if orientations.contains(&orientation)? && common::len_of(operands)? > 0 {
        let first = operands.get_item(0usize)?;
        if common::is_str(py, &first)? {
            text = first;
            is_str_operands = true;
        } else {
            // Upstream's conditional `encode_pdfdocencoding(...)` branch is
            // unreachable here (this branch is exactly `not isinstance(..., str)`).
            let tt = first;
            let encoding = font.getattr("encoding")?;
            if common::is_str(py, &encoding)? {
                match tt.call_method1("decode", (&encoding, "surrogatepass")) {
                    Ok(decoded) => text = decoded,
                    Err(error) if error.is_instance_of::<PyUnicodeDecodeError>(py) => {
                        // Fallback for odd byte counts or unmapped 16-bit GIDs/CIDs
                        text = tt.call_method1("decode", (&encoding, "replace"))?;
                    }
                    Err(error) => return Err(error),
                }
            } else {
                // Apply dict encoding
                let mut chars: Vec<Bound<'py, PyAny>> = Vec::new();
                let mut raw: Vec<Bound<'py, PyAny>> = Vec::new();
                for byte in tt.try_iter()? {
                    let byte = byte?;
                    let char = common::chr_of(py, &byte)?;
                    chars.push(encoding.call_method1("get", (&byte, &char))?);
                    raw.push(char);
                }
                text = common::py_str_val(py, "")
                    .call_method1("join", (pyo3::types::PyList::new(py, chars)?,))?;
                raw_characters = common::py_str_val(py, "")
                    .call_method1("join", (pyo3::types::PyList::new(py, raw)?,))?;
            }
        }
        let source = if raw_characters.is_truthy()? {
            raw_characters.clone()
        } else {
            text.clone()
        };
        if let Some(total) = text_widths_fast(py, font, &source)? {
            widths = common::py_float(py, total);
            width_cache.clear();
            return common::py_tuple(py, vec![text, common::py_bool(py, is_str_operands), widths]);
        }
        let space_char = font.getattr("space_char")?;
        let space_width = font.getattr("space_width")?;
        for char in source.try_iter()? {
            let char = char?;
            if char.eq(&space_char)? {
                widths = widths.add(&space_width)?;
            } else {
                if !width_cache.contains(&char)? {
                    // `font.get_text_width` is looked up on the object so that
                    // instance-level monkeypatching keeps working (upstream test).
                    let width = font.call_method1("get_text_width", (&char,))?;
                    width_cache.set_item(&char, width)?;
                }
                widths = widths.add(width_cache.get_item(&char)?.expect("width cached"))?;
            }
        }
    }
    width_cache.clear();
    common::py_tuple(py, vec![text, common::py_bool(py, is_str_operands), widths])
}

// @pypdf: pypdf._text_extraction.get_display_str
#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn text_extraction_get_display_str<'py>(
    py: Python<'py>,
    text: &Bound<'py, PyAny>,
    cm_matrix: &Bound<'py, PyAny>,
    tm_matrix: &Bound<'py, PyAny>,
    font_resource: &Bound<'py, PyAny>,
    font: &Bound<'py, PyAny>,
    text_operands: &Bound<'py, PyAny>,
    font_size: &Bound<'py, PyAny>,
    rtl_dir: &Bound<'py, PyAny>,
    visitor_text: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let character_map = font.getattr("character_map")?;
    // `character_map.get(char, char)` through the C-level dict API avoids
    // creating a bound method for every character.
    let character_map_dict = character_map.cast::<PyDict>().ok().cloned();
    let mut characters: Vec<Bound<'py, PyAny>> = Vec::new();
    for raw_character in text_operands.try_iter()? {
        let raw_character = raw_character?;
        let mapped = match &character_map_dict {
            Some(dict) => match dict.get_item(&raw_character)? {
                Some(value) => value,
                None => raw_character,
            },
            None => character_map.call_method1("get", (&raw_character, &raw_character))?,
        };
        characters.push(mapped);
    }

    let custom_min = common::init_attr(py, "CUSTOM_RTL_MIN")?;
    let custom_max = common::init_attr(py, "CUSTOM_RTL_MAX")?;
    let custom_specials = common::init_attr(py, "CUSTOM_RTL_SPECIAL_CHARS")?;
    let pattern = common::init_attr(py, "RTL_CHARACTER_PATTERN")?;
    let is_char_neutral = common::init_attr(py, "is_char_neutral")?;
    let is_char_rtl = common::init_attr(py, "is_char_rtl")?;

    let empty = common::py_str_val(py, "");
    if !rtl_dir.is_truthy()? && !(custom_min.is_truthy()? && custom_max.is_truthy()?) {
        // Without a right-to-left character, all characters are appended in order.
        let appended_text =
            empty.call_method1("join", (pyo3::types::PyList::new(py, &characters)?,))?;
        if pattern.call_method1("search", (&appended_text,))?.is_none() {
            let new_text = common::concat(text, &appended_text)?;
            return common::py_tuple(py, vec![new_text, rtl_dir.clone(), empty]);
        }
    }

    let mut completed_text: Vec<Bound<'py, PyAny>> = Vec::new();
    let neutral_cache = PyDict::new(py);
    let rtl_cache = PyDict::new(py);
    // Characters are only ever prepended in a right-to-left run and appended in a
    // left-to-right run. Collect them and join once, as building the string one
    // prepended character at a time takes quadratic time.
    let mut run_parts: Vec<Bound<'py, PyAny>> = vec![text.clone()];
    let mut current_rtl = rtl_dir.clone();

    for x in characters {
        // Test whether x is a sequence of bytes; ex: habibi.pdf
        if common::len_of(&x)? == 1 {
            if !neutral_cache.contains(&x)? {
                let neutral = is_char_neutral.call1((&x, &custom_specials))?;
                neutral_cache.set_item(&x, neutral)?;
            }
            if common::dict_get(&neutral_cache, &x)?.is_truthy()? {
                // Cases where the current inserting order is kept
                run_parts.push(x);
            } else {
                if !rtl_cache.contains(&x)? {
                    let rtl = is_char_rtl.call1((&x, &custom_min, &custom_max))?;
                    rtl_cache.set_item(&x, rtl)?;
                }
                let char_rtl = common::dict_get(&rtl_cache, &x)?;
                if char_rtl.ne(&current_rtl)? {
                    // The direction changes: right-to-left or left-to-right characters
                    let run_text = get_run_text(py, &run_parts, &current_rtl)?;
                    current_rtl = char_rtl;
                    if !visitor_text.is_none() {
                        visitor_text.call1((
                            &run_text,
                            cm_matrix,
                            tm_matrix,
                            font_resource,
                            font_size,
                        ))?;
                        neutral_cache.clear();
                        rtl_cache.clear();
                    }
                    completed_text.push(run_text);
                    run_parts = Vec::new();
                }
                run_parts.push(x);
            }
        } else {
            // Treat a sequence of bytes as a neutral character.
            run_parts.push(x);
        }
    }
    let final_text = get_run_text(py, &run_parts, &current_rtl)?;
    let completed = empty.call_method1("join", (pyo3::types::PyList::new(py, completed_text)?,))?;
    common::py_tuple(py, vec![final_text, current_rtl, completed])
}

/// `get_run_text()`: reversed join in an RTL run, plain join in an LTR run.
fn get_run_text<'py>(
    py: Python<'py>,
    run_parts: &[Bound<'py, PyAny>],
    rtl_dir: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut parts: Vec<Bound<'py, PyAny>> = run_parts.to_vec();
    if rtl_dir.is_truthy()? {
        parts.reverse();
    }
    common::py_str_val(py, "").call_method1("join", (pyo3::types::PyList::new(py, parts)?,))
}

// @pypdf: pypdf._text_extraction._has_text_operator
#[pyfunction]
pub fn text_extraction_has_text_operator<'py>(
    _py: Python<'py>,
    data: &Bound<'py, PyAny>,
) -> PyResult<bool> {
    let bytes = data.cast::<pyo3::types::PyBytes>()?.as_bytes();
    let mut found = false;
    for (start, byte) in bytes.iter().enumerate() {
        if *byte == b'T' || *byte == b'D' || *byte == b'\'' || *byte == b'"' {
            let end = (start + 2).min(bytes.len());
            let two = &bytes[start..end];
            if two == b"Tj" || two == b"TJ" || two == b"Do" {
                found = true;
                break;
            }
            if *byte == b'\'' || *byte == b'"' {
                found = true;
                break;
            }
        }
    }
    Ok(found)
}

// @pypdf: pypdf._text_extraction._get_content_stream_for_text
#[pyfunction]
pub fn text_extraction_get_content_stream_for_text<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
    content_key: &Bound<'py, PyAny>,
    has_visitor: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let content_stream_class = common::generic_class(py, "ContentStream")?;
    let attempt: PyResult<Bound<'py, PyAny>> = (|| {
        let mut content = if common::is_str(py, content_key)? {
            obj.get_item(content_key)?.call_method0("get_object")?
        } else {
            obj.clone()
        };
        if !content.is_instance(&content_stream_class)? {
            content = content_stream_class.call1((&content, pdf, "bytes"))?;
        }
        Ok(content)
    })();
    let content = match attempt {
        Ok(content) => content,
        Err(error) if common::is_attribute_or_key_error(py, &error) => {
            // no content can be extracted (certainly empty page)
            return Ok(common::none(py));
        }
        Err(error) => return Err(error),
    };
    if !has_visitor {
        let data = content.call_method0("get_data")?;
        let has_operator = text_extraction_has_text_operator(py, &data)?;
        if !has_operator {
            return Ok(common::none(py));
        }
    }
    Ok(content)
}
