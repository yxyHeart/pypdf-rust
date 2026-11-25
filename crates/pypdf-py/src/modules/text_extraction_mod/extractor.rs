//! `pypdf._text_extraction._text_extractor` (upstream `_text_extractor.py`).
//!
//! `TextExtraction` is a Python class in the shim; this module holds the bodies of
//! its methods plus the module-level `_get_number_operand` helper.  The instance
//! state lives in ordinary Python attributes, which are read and written exactly
//! where upstream does, so the `visitor_*` hooks and `pypdf._page` see the same
//! intermediate values (matrices, `text`, `output`, `memo_*`, ...).

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyTuple};

use super::common;

fn get_attr<'py>(slf: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    slf.getattr(name)
}

fn set_attr<'py>(
    slf: &Bound<'py, PyAny>,
    name: &str,
    value: impl IntoPyObject<'py>,
) -> PyResult<()> {
    slf.setattr(name, value)
}

fn empty_dict<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyDict::new(py).into_any())
}

/// `_get_number_operand(operands, index, default)`.
pub fn get_number_operand<'py>(
    py: Python<'py>,
    operands: &Bound<'py, PyAny>,
    index: i64,
    default: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let length = common::len_of(operands)? as i64;
    if index < length {
        let operand = operands.get_item(index)?;
        if common::is_number(py, &operand)? {
            return common::float_of(py, &operand);
        }
    }
    Ok(default.clone())
}

// @pypdf: pypdf._text_extraction._text_extractor._get_number_operand
#[pyfunction]
pub fn text_extractor_get_number_operand<'py>(
    py: Python<'py>,
    operands: &Bound<'py, PyAny>,
    index: i64,
    default: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    get_number_operand(py, operands, index, default)
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction.__init__
#[pyfunction]
pub fn text_extractor_init(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<()> {
    set_attr(slf, "cm_matrix", common::identity_matrix(py)?)?;
    set_attr(slf, "tm_matrix", common::identity_matrix(py)?)?;
    set_attr(slf, "cm_stack", PyList::empty(py))?;
    // Store the last modified matrices; can be an intermediate position
    set_attr(slf, "cm_prev", common::identity_matrix(py)?)?;
    set_attr(slf, "tm_prev", common::identity_matrix(py)?)?;
    // Store the position at the beginning of building the text
    set_attr(slf, "memo_cm", common::identity_matrix(py)?)?;
    set_attr(slf, "memo_tm", common::identity_matrix(py)?)?;
    set_attr(slf, "char_scale", common::py_float(py, 1.0))?;
    set_attr(slf, "space_scale", common::py_float(py, 1.0))?;
    set_attr(slf, "char_spacing", common::py_float(py, 0.0))?;
    // will be set correctly at first Tf
    set_attr(slf, "_space_width", common::py_float(py, 500.0))?;
    // Right after a Tf, a word gap is a gap in either the previous or the new font.
    set_attr(slf, "_previous_font_space", common::none(py))?;
    // will be set to string length calculation result
    let actual_str_size = PyDict::new(py);
    actual_str_size.set_item("str_widths", common::py_float(py, 0.0))?;
    actual_str_size.set_item("str_height", common::py_float(py, 0.0))?;
    set_attr(slf, "_actual_str_size", actual_str_size)?;
    set_attr(slf, "TL", common::py_float(py, 0.0))?;
    // init just in case of
    set_attr(slf, "font_size", common::py_float(py, 12.0))?;
    // (axis index, min, max) of baselines kept on the current extracted line.
    set_attr(slf, "_line_span", common::none(py))?;

    // Text extraction variables
    set_attr(slf, "text", common::py_str_val(py, ""))?;
    set_attr(slf, "output", common::py_str_val(py, ""))?;
    // right-to-left
    set_attr(slf, "rtl_dir", common::py_bool(py, false))?;
    set_attr(slf, "font_resource", common::none(py))?;

    let encoding = py
        .import("pypdf._codecs")?
        .getattr("encoding_dict_from_named_encoding")?
        .call1(("cp1252",))?;
    let font_descriptor = common::font_descriptor_class(py)?.call0()?;
    let font_kwargs = PyDict::new(py);
    font_kwargs.set_item("name", "NotInitialized")?;
    font_kwargs.set_item("sub_type", "Unknown")?;
    font_kwargs.set_item("encoding", encoding)?;
    font_kwargs.set_item("font_descriptor", font_descriptor)?;
    let font = common::font_class(py)?.call((), Some(&font_kwargs))?;
    set_attr(slf, "font", font)?;
    set_attr(
        slf,
        "orientations",
        common::py_tuple(
            py,
            vec![
                common::py_int(py, 0),
                common::py_int(py, 90),
                common::py_int(py, 180),
                common::py_int(py, 270),
            ],
        )?,
    )?;
    set_attr(slf, "visitor_text", common::none(py))?;
    set_attr(slf, "font_resources", empty_dict(py)?)?;
    set_attr(slf, "fonts", empty_dict(py)?)?;

    let handlers = PyDict::new(py);
    for (operator, method) in [
        (b"BT".as_slice(), "_handle_bt"),
        (b"ET".as_slice(), "_handle_et"),
        (b"q".as_slice(), "_handle_save_graphics_state"),
        (b"Q".as_slice(), "_handle_restore_graphics_state"),
        (b"cm".as_slice(), "_handle_cm"),
        (b"Tz".as_slice(), "_handle_tz"),
        (b"Tc".as_slice(), "_handle_tc"),
        (b"Tw".as_slice(), "_handle_tw"),
        (b"TL".as_slice(), "_handle_tl"),
        (b"Tf".as_slice(), "_handle_tf"),
        (b"Td".as_slice(), "_handle_td"),
        (b"Tm".as_slice(), "_handle_tm"),
        (b"T*".as_slice(), "_handle_t_star"),
        (b"Tj".as_slice(), "_handle_tj_operation"),
    ] {
        handlers.set_item(common::py_bytes_val(py, operator), slf.getattr(method)?)?;
    }
    set_attr(slf, "operation_handlers", handlers.clone())?;
    // Used to detect monkeypatched dispatch tables: the fast path in
    // `process_operation` only applies while this is still the default table.
    set_attr(slf, "_rs_default_handlers", handlers)?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction.initialize_extraction
#[pyfunction]
pub fn text_extractor_initialize_extraction<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    orientations: &Bound<'py, PyAny>,
    visitor_text: &Bound<'py, PyAny>,
    font_resources: &Bound<'py, PyAny>,
    fonts: &Bound<'py, PyAny>,
) -> PyResult<()> {
    set_attr(slf, "orientations", orientations)?;
    set_attr(slf, "visitor_text", visitor_text)?;
    let font_resources = if font_resources.is_truthy()? {
        font_resources.clone()
    } else {
        empty_dict(py)?
    };
    let fonts = if fonts.is_truthy()? {
        fonts.clone()
    } else {
        empty_dict(py)?
    };
    set_attr(slf, "font_resources", font_resources)?;
    set_attr(slf, "fonts", fonts)?;

    // Reset state
    set_attr(slf, "text", common::py_str_val(py, ""))?;
    set_attr(slf, "output", common::py_str_val(py, ""))?;
    set_attr(slf, "rtl_dir", common::py_bool(py, false))?;
    set_attr(slf, "_line_span", common::none(py))?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction.compute_str_widths
#[pyfunction]
pub fn text_extractor_compute_str_widths<'py>(
    _py: Python<'py>,
    _slf: &Bound<'py, PyAny>,
    str_widths: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    str_widths.div(1000i64)
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction.process_operation
#[pyfunction]
pub fn text_extractor_process_operation<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    operator: &Bound<'py, PyAny>,
    operands: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let handlers = get_attr(slf, "operation_handlers")?;
    // Fast path: dispatch on the operator bytes and call the Rust handler
    // directly instead of going through the Python bound-method table (the text
    // extraction loop calls this once per content-stream operation).
    let default_handlers = get_attr(slf, "_rs_default_handlers").ok();
    let fast = match &default_handlers {
        Some(default) => handlers.is(default) && handlers.len()? == 14,
        None => false,
    };
    if fast {
        // `operator` is always `bytes`; borrow instead of allocating a Vec per
        // content-stream operation.
        let operator_bytes: &[u8] = operator.cast::<pyo3::types::PyBytes>()?.as_bytes();
        let widths: Option<Bound<'py, PyAny>> = match operator_bytes {
            b"BT" => { text_extractor_handle_bt(py, slf, operands)?; None }
            b"ET" => { text_extractor_handle_et(py, slf, operands)?; None }
            b"q" => { text_extractor_handle_save_graphics_state(py, slf, operands)?; None }
            b"Q" => { text_extractor_handle_restore_graphics_state(py, slf, operands)?; None }
            b"cm" => { text_extractor_handle_cm(py, slf, operands)?; None }
            b"Tz" => { text_extractor_handle_tz(py, slf, operands)?; None }
            b"Tc" => { text_extractor_handle_tc(py, slf, operands)?; None }
            b"Tw" => { text_extractor_handle_tw(py, slf, operands)?; None }
            b"TL" => { text_extractor_handle_tl(py, slf, operands)?; None }
            b"Tf" => { text_extractor_handle_tf(py, slf, operands)?; None }
            b"Td" => Some(text_extractor_handle_td(py, slf, operands)?),
            b"Tm" => Some(text_extractor_handle_tm(py, slf, operands)?),
            b"T*" => Some(text_extractor_handle_t_star(py, slf, operands)?),
            b"Tj" => Some(text_extractor_handle_tj_operation(py, slf, operands)?),
            _ => None,
        };
        if matches!(operator_bytes, b"Td" | b"Tm" | b"T*" | b"Tj") {
            let value = match widths {
                Some(widths) if widths.is_truthy()? => widths,
                _ => common::py_float(py, 0.0),
            };
            text_extractor_post_process_text_operation(py, slf, &value)?;
        }
        return Ok(());
    }
    if handlers.contains(operator)? {
        let handler = handlers.get_item(operator)?;
        let str_widths = handler.call1((operands,))?;
        // Post-process operations that affect text positioning
        let is_text_positioning = operator.eq(common::py_bytes_val(py, b"Td"))?
            || operator.eq(common::py_bytes_val(py, b"Tm"))?
            || operator.eq(common::py_bytes_val(py, b"T*"))?
            || operator.eq(common::py_bytes_val(py, b"Tj"))?;
        if is_text_positioning {
            let value = if str_widths.is_truthy()? {
                str_widths
            } else {
                common::py_float(py, 0.0)
            };
            slf.call_method1("_post_process_text_operation", (value,))?;
        }
    }
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction.process_tj_offset
#[pyfunction]
pub fn text_extractor_process_tj_offset<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    offset: &Bound<'py, PyAny>,
    space_width: &Bound<'py, PyAny>,
) -> PyResult<()> {
    // The offset moved the pen, so it counts towards the width and the added space does not.
    let actual_str_size = get_attr(slf, "_actual_str_size")?;
    let previous_width = actual_str_size.get_item("str_widths")?;
    let str_widths = previous_width.sub(offset.mul(get_attr(slf, "font_size")?)?)?;
    // A Tf just before this TJ has already moved the text to the output.
    let text = get_attr(slf, "text")?;
    let previous = if text.is_truthy()? {
        text
    } else {
        get_attr(slf, "output")?
    };
    // A positive offset moves the next glyph back (kerning), so only a negative one can be a gap.
    let negated = offset.neg()?;
    if negated.ge(space_width)? && previous.is_truthy()? {
        let last_character = previous.get_item(common::slice_from_end(py)?)?;
        let whitespace = common::py_str_val(py, " \n");
        if !whitespace.contains(&last_character)? {
            slf.call_method1(
                "process_operation",
                (
                    common::py_bytes_val(py, b"Tj"),
                    common::py_list(py, vec![common::py_str_val(py, " ")])?,
                ),
            )?;
        }
    }
    // `self._actual_str_size` may have been replaced by the `Tj` above.
    get_attr(slf, "_actual_str_size")?.set_item("str_widths", str_widths)?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._post_process_text_operation
#[pyfunction]
pub fn text_extractor_post_process_text_operation<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    str_widths: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let text_before = get_attr(slf, "text")?;
    let text_was_empty = text_before.eq(common::py_str_val(py, ""))?;
    let font_size = get_attr(slf, "font_size")?;
    let mut space_width = font_size.mul(get_attr(slf, "_space_width")?)?;
    let previous_font_space = get_attr(slf, "_previous_font_space")?;
    if !previous_font_space.is_none() {
        // Native double arithmetic for the common case (both operands floats).
        let fast = (|| -> Option<Bound<'py, PyAny>> {
            let font_size_value = font_size.extract::<f64>().ok()?;
            let base_value = get_attr(slf, "_space_width").ok()?.extract::<f64>().ok()?;
            let previous_value = previous_font_space.extract::<f64>().ok()?;
            let product = font_size_value * base_value;
            Some(common::py_float(py, product.min(previous_value)))
        })();
        space_width = match fast {
            Some(value) => value,
            None => common::min_of(py, &space_width, &previous_font_space)?,
        };
    }
    let cm_prev = get_attr(slf, "cm_prev")?;
    let tm_prev = get_attr(slf, "tm_prev")?;
    let cm_matrix = get_attr(slf, "cm_matrix")?;
    let tm_matrix = get_attr(slf, "tm_matrix")?;
    let memo_cm = get_attr(slf, "memo_cm")?;
    let memo_tm = get_attr(slf, "memo_tm")?;
    let actual_str_size = get_attr(slf, "_actual_str_size")?;
    // `TextExtraction.compute_str_widths` is a thin shim over this Rust
    // function; call it directly (once per text-positioning operation).
    let spacewidth = crate::text_extraction_mod::extractor::text_extractor_compute_str_widths(
        py, slf, &space_width,
    )?;
    let crlf_fn = common::init_attr(py, "crlf_space_check")?;
    let crlf_is_default = crate::text_extraction_mod::init_mod::is_default_function(
        py,
        "crlf_space_check",
        &crlf_fn,
    )?;
    let call_object_based = |py: Python<'py>| -> PyResult<Bound<'py, PyAny>> {
        let arguments = PyTuple::new(
            py,
            vec![
                text_before.clone(),
                common::py_tuple(py, vec![cm_prev.clone(), tm_prev.clone()])?,
                common::py_tuple(py, vec![cm_matrix.clone(), tm_matrix.clone()])?,
                common::py_tuple(py, vec![memo_cm.clone(), memo_tm.clone()])?,
                get_attr(slf, "font_resource")?,
                get_attr(slf, "orientations")?,
                get_attr(slf, "output")?,
                font_size.clone(),
                get_attr(slf, "visitor_text")?,
                str_widths.clone(),
                spacewidth.clone(),
                actual_str_size.get_item("str_height")?,
                get_attr(slf, "_line_span")?,
            ],
        )?;
        if crlf_is_default {
            crate::text_extraction_mod::init_mod::text_extraction_crlf_space_check(
                py,
                &arguments.get_item(0)?,
                &arguments.get_item(1)?,
                &arguments.get_item(2)?,
                &arguments.get_item(3)?,
                &arguments.get_item(4)?,
                &arguments.get_item(5)?,
                &arguments.get_item(6)?,
                &arguments.get_item(7)?,
                &arguments.get_item(8)?,
                &arguments.get_item(9)?,
                &arguments.get_item(10)?,
                &arguments.get_item(11)?,
                &arguments.get_item(12)?,
            )
        } else {
            crlf_fn.call1(arguments)
        }
    };
    let result = if crlf_is_default {
        // Native path: matrices stay doubles and no intermediate tuples are
        // built (this runs for every text-positioning operator).
        match crate::text_extraction_mod::init_mod::crlf_space_check_native(
            py,
            &text_before,
            &cm_prev,
            &tm_prev,
            &cm_matrix,
            &tm_matrix,
            &memo_cm,
            &memo_tm,
            &get_attr(slf, "font_resource")?,
            &get_attr(slf, "orientations")?,
            &get_attr(slf, "output")?,
            &font_size,
            &get_attr(slf, "visitor_text")?,
            &str_widths,
            &spacewidth,
            &actual_str_size.get_item("str_height")?,
            &get_attr(slf, "_line_span")?,
        ) {
            // The orientation error is handled by the `match result` below,
            // exactly like the object-based implementation.
            Ok(Some(values)) => Ok(values),
            Ok(None) => call_object_based(py),
            Err(error) => Err(error),
        }
    } else {
        call_object_based(py)
    };
    match result {
        Ok(values) => {
            set_attr(slf, "text", values.get_item(0usize)?)?;
            set_attr(slf, "output", values.get_item(1usize)?)?;
            set_attr(slf, "cm_prev", values.get_item(2usize)?)?;
            set_attr(slf, "tm_prev", values.get_item(3usize)?)?;
            set_attr(slf, "_line_span", values.get_item(4usize)?)?;
            if text_was_empty || get_attr(slf, "text")?.eq(common::py_str_val(py, ""))? {
                set_attr(
                    slf,
                    "memo_cm",
                    get_attr(slf, "cm_matrix")?.call_method0("copy")?,
                )?;
                set_attr(
                    slf,
                    "memo_tm",
                    get_attr(slf, "tm_matrix")?.call_method0("copy")?,
                )?;
            }
        }
        Err(error) => {
            let orientation_error = common::orientation_error(py)?;
            if !error.is_instance(py, &orientation_error) {
                return Err(error);
            }
        }
    }
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_tj
#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn text_extractor_handle_tj<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    text: &Bound<'py, PyAny>,
    operands: &Bound<'py, PyAny>,
    cm_matrix: &Bound<'py, PyAny>,
    tm_matrix: &Bound<'py, PyAny>,
    font_resource: &Bound<'py, PyAny>,
    font: &Bound<'py, PyAny>,
    orientations: &Bound<'py, PyAny>,
    font_size: &Bound<'py, PyAny>,
    rtl_dir: &Bound<'py, PyAny>,
    visitor_text: &Bound<'py, PyAny>,
    actual_str_size: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let operands_result = crate::text_extraction_mod::init_mod::text_extraction_get_text_operands(
        py,
        operands,
        cm_matrix,
        tm_matrix,
        font,
        orientations,
    )?;
    let text_operands = operands_result.get_item(0usize)?;
    let is_str_operands = operands_result.get_item(1usize)?;
    let font_widths = operands_result.get_item(2usize)?;
    let mut new_text = text.clone();
    let mut new_rtl_dir = rtl_dir.clone();
    if is_str_operands.extract::<bool>()? {
        new_text = common::concat(&new_text, &text_operands)?;
    } else {
        let display_result = crate::text_extraction_mod::init_mod::text_extraction_get_display_str(
            py,
            &new_text,
            cm_matrix,
            tm_matrix,
            font_resource,
            font,
            &text_operands,
            font_size,
            &new_rtl_dir,
            visitor_text,
        )?;
        new_text = display_result.get_item(0usize)?;
        new_rtl_dir = display_result.get_item(1usize)?;
        let completed = display_result.get_item(2usize)?;
        let output = common::concat(&get_attr(slf, "output")?, &completed)?;
        set_attr(slf, "output", output)?;
    }
    // Tc is in text space units and applies to every glyph.
    let added = font_widths.mul(font_size)?.add(
        common::py_int(py, 1000)
            .mul(get_attr(slf, "char_spacing")?)?
            .mul(common::len_of(&text_operands)? as i64)?,
    )?;
    let widths = actual_str_size.get_item("str_widths")?;
    actual_str_size.set_item("str_widths", widths.add(&added)?)?;
    actual_str_size.set_item("str_height", font_size)?;
    common::py_tuple(py, vec![new_text, new_rtl_dir, actual_str_size.clone()])
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._flush_text
#[pyfunction]
pub fn text_extractor_flush_text(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<()> {
    let text = get_attr(slf, "text")?;
    let output = common::concat(&get_attr(slf, "output")?, &text)?;
    set_attr(slf, "output", output)?;
    let visitor_text = get_attr(slf, "visitor_text")?;
    if !visitor_text.is_none() {
        visitor_text.call1((
            &text,
            get_attr(slf, "memo_cm")?,
            get_attr(slf, "memo_tm")?,
            get_attr(slf, "font_resource")?,
            get_attr(slf, "font_size")?,
        ))?;
    }
    set_attr(slf, "text", common::py_str_val(py, ""))?;
    set_attr(
        slf,
        "memo_cm",
        get_attr(slf, "cm_matrix")?.call_method0("copy")?,
    )?;
    set_attr(
        slf,
        "memo_tm",
        get_attr(slf, "tm_matrix")?.call_method0("copy")?,
    )?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_bt
#[pyfunction]
pub fn text_extractor_handle_bt(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    _operands: &Bound<'_, PyAny>,
) -> PyResult<()> {
    set_attr(slf, "tm_matrix", common::identity_matrix(py)?)?;
    text_extractor_flush_text(py, slf)
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_et
#[pyfunction]
pub fn text_extractor_handle_et(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    _operands: &Bound<'_, PyAny>,
) -> PyResult<()> {
    text_extractor_flush_text(py, slf)
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_save_graphics_state
#[pyfunction]
pub fn text_extractor_handle_save_graphics_state(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    _operands: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let entry = common::py_tuple(
        py,
        vec![
            get_attr(slf, "cm_matrix")?,
            get_attr(slf, "font_resource")?,
            get_attr(slf, "font")?,
            get_attr(slf, "font_size")?,
            get_attr(slf, "char_scale")?,
            get_attr(slf, "space_scale")?,
            get_attr(slf, "char_spacing")?,
            get_attr(slf, "TL")?,
        ],
    )?;
    get_attr(slf, "cm_stack")?.call_method1("append", (entry,))?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_restore_graphics_state
#[pyfunction]
pub fn text_extractor_handle_restore_graphics_state(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    _operands: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let popped = get_attr(slf, "cm_stack")?.call_method0("pop");
    match popped {
        Ok(entry) => {
            let unpacked: PyResult<Vec<Bound<'_, PyAny>>> = (|| {
                let mut values = Vec::with_capacity(8);
                for index in 0..8 {
                    values.push(entry.get_item(index)?);
                }
                Ok(values)
            })();
            match unpacked {
                Ok(values) => {
                    set_attr(slf, "cm_matrix", values[0].clone())?;
                    set_attr(slf, "font_resource", values[1].clone())?;
                    set_attr(slf, "font", values[2].clone())?;
                    set_attr(slf, "font_size", values[3].clone())?;
                    set_attr(slf, "char_scale", values[4].clone())?;
                    set_attr(slf, "space_scale", values[5].clone())?;
                    set_attr(slf, "char_spacing", values[6].clone())?;
                    set_attr(slf, "TL", values[7].clone())?;
                }
                Err(error) => {
                    if common::is_memory_error(py, &error) {
                        return Err(error);
                    }
                    set_attr(slf, "cm_matrix", common::identity_matrix(py)?)?;
                }
            }
        }
        Err(error) => {
            if common::is_memory_error(py, &error) {
                return Err(error);
            }
            set_attr(slf, "cm_matrix", common::identity_matrix(py)?)?;
        }
    }
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_cm
#[pyfunction]
pub fn text_extractor_handle_cm(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    operands: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let text = get_attr(slf, "text")?;
    let output = common::concat(&get_attr(slf, "output")?, &text)?;
    set_attr(slf, "output", output)?;
    let visitor_text = get_attr(slf, "visitor_text")?;
    if !visitor_text.is_none() {
        visitor_text.call1((
            &text,
            get_attr(slf, "memo_cm")?,
            get_attr(slf, "memo_tm")?,
            get_attr(slf, "font_resource")?,
            get_attr(slf, "font_size")?,
        ))?;
    }
    set_attr(slf, "text", common::py_str_val(py, ""))?;
    let attempt: PyResult<Bound<'_, PyAny>> = (|| {
        let sliced = operands.get_item(common::slice_of(
            py,
            Some(&common::py_int(py, 0)),
            Some(&common::py_int(py, 6)),
            None,
        )?)?;
        let mut values: Vec<Bound<'_, PyAny>> = Vec::new();
        for operand in sliced.try_iter()? {
            values.push(common::float_of(py, &operand?)?);
        }
        let mult_fn = common::init_attr(py, "mult")?;
        if crate::text_extraction_mod::init_mod::is_default_function(py, "mult", &mult_fn)? {
            crate::text_extraction_mod::init_mod::text_extraction_mult(
                py,
                &common::py_list(py, values)?,
                &get_attr(slf, "cm_matrix")?,
            )
        } else {
            mult_fn.call1((common::py_list(py, values)?, get_attr(slf, "cm_matrix")?))
        }
    })();
    match attempt {
        Ok(matrix) => set_attr(slf, "cm_matrix", matrix)?,
        Err(error) => {
            if common::is_memory_error(py, &error) {
                return Err(error);
            }
            set_attr(slf, "cm_matrix", common::identity_matrix(py)?)?;
        }
    }
    set_attr(
        slf,
        "memo_cm",
        get_attr(slf, "cm_matrix")?.call_method0("copy")?,
    )?;
    set_attr(
        slf,
        "memo_tm",
        get_attr(slf, "tm_matrix")?.call_method0("copy")?,
    )?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_tz
#[pyfunction]
pub fn text_extractor_handle_tz(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    operands: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let value = get_number_operand(py, operands, 0, &common::py_float(py, 100.0))?;
    set_attr(slf, "char_scale", value.div(100i64)?)?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_tc
#[pyfunction]
pub fn text_extractor_handle_tc(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    operands: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let value = get_number_operand(py, operands, 0, &common::py_float(py, 0.0))?;
    set_attr(slf, "char_spacing", value)?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_tw
#[pyfunction]
pub fn text_extractor_handle_tw(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    operands: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let value = get_number_operand(py, operands, 0, &common::py_float(py, 0.0))?;
    set_attr(slf, "space_scale", common::py_float(py, 1.0).add(value)?)?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_tl
#[pyfunction]
pub fn text_extractor_handle_tl(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    operands: &Bound<'_, PyAny>,
) -> PyResult<()> {
    // The leading is measured in unscaled text space units (PDF 32000-1, 9.3.5)
    // and T* applies the text matrix to it, so it must not be scaled here.
    let value = get_number_operand(py, operands, 0, &common::py_float(py, 0.0))?;
    set_attr(slf, "TL", value)?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_tf
#[pyfunction]
pub fn text_extractor_handle_tf(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    operands: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let text = get_attr(slf, "text")?;
    if !text.eq(common::py_str_val(py, ""))? {
        let font_space = get_attr(slf, "font_size")?.mul(get_attr(slf, "_space_width")?)?;
        set_attr(slf, "_previous_font_space", font_space)?;
        let output = common::concat(&get_attr(slf, "output")?, &text)?;
        set_attr(slf, "output", output)?;
        let visitor_text = get_attr(slf, "visitor_text")?;
        if !visitor_text.is_none() {
            visitor_text.call1((
                &text,
                get_attr(slf, "memo_cm")?,
                get_attr(slf, "memo_tm")?,
                get_attr(slf, "font_resource")?,
                get_attr(slf, "font_size")?,
            ))?;
        }
    }
    set_attr(slf, "text", common::py_str_val(py, ""))?;
    set_attr(
        slf,
        "memo_cm",
        get_attr(slf, "cm_matrix")?.call_method0("copy")?,
    )?;
    set_attr(
        slf,
        "memo_tm",
        get_attr(slf, "tm_matrix")?.call_method0("copy")?,
    )?;
    // Upstream assigns `font_resource` before looking up the font, so a failing
    // second lookup keeps the first assignment; reproduce that order.
    let attempt: PyResult<()> = (|| {
        let name = operands.get_item(0usize)?;
        let font_resource = get_attr(slf, "font_resources")?.get_item(&name)?;
        set_attr(slf, "font_resource", font_resource)?;
        let font = get_attr(slf, "fonts")?.get_item(&name)?;
        set_attr(slf, "font", font)?;
        Ok(())
    })();
    match attempt {
        Ok(()) => {}
        Err(error) => {
            if !common::is_key_or_index_error(py, &error) {
                return Err(error);
            }
            // font not found / operand missing
            set_attr(slf, "font_resource", common::none(py))?;
            let font_descriptor = common::font_descriptor_class(py)?.call0()?;
            let range = common::builtins(py)?.getattr("range")?.call1((256,))?;
            let encoding = common::builtins(py)?
                .getattr("dict")?
                .getattr("fromkeys")?
                .call1((range, "\u{FFFD}"))?;
            let font_kwargs = PyDict::new(py);
            font_kwargs.set_item("space_width", 250)?;
            font_kwargs.set_item("encoding", encoding)?;
            font_kwargs.set_item("font_descriptor", font_descriptor)?;
            font_kwargs.set_item("character_map", PyDict::new(py))?;
            let font = common::font_class(py)?.call(("Unknown",), Some(&font_kwargs))?;
            set_attr(slf, "font", font)?;
        }
    }

    // Actually the width of _half_ a space...
    let space_width = get_attr(slf, "font")?.getattr("space_width")?.div(2i64)?;
    set_attr(slf, "_space_width", space_width)?;
    let parsed_size: PyResult<Bound<'_, PyAny>> = (|| {
        let size = operands.get_item(1usize)?;
        common::float_of(py, &size)
    })();
    match parsed_size {
        Ok(size) => set_attr(slf, "font_size", size)?,
        Err(error) => {
            if common::is_memory_error(py, &error) {
                return Err(error);
            }
            // keep previous size
        }
    }
    Ok(())
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_td
#[pyfunction]
pub fn text_extractor_handle_td<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    operands: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // A special case is a translating only tm:
    // tm = [1, 0, 0, 1, e, f]
    // i.e. tm[4] += tx, tm[5] += ty.
    let tx = get_number_operand(py, operands, 0, &common::py_float(py, 0.0))?;
    let ty = get_number_operand(py, operands, 1, &common::py_float(py, 0.0))?;
    let tm = get_attr(slf, "tm_matrix")?;
    let t0 = tm.get_item(0usize)?;
    let t1 = tm.get_item(1usize)?;
    let t2 = tm.get_item(2usize)?;
    let t3 = tm.get_item(3usize)?;
    let new4 = tm.get_item(4usize)?.add(tx.mul(&t0)?.add(ty.mul(&t2)?)?)?;
    let new5 = tm.get_item(5usize)?.add(tx.mul(&t1)?.add(ty.mul(&t3)?)?)?;
    tm.set_item(4usize, new4)?;
    tm.set_item(5usize, new5)?;
    let actual_str_size = get_attr(slf, "_actual_str_size")?;
    let str_widths = slf.call_method1(
        "compute_str_widths",
        (actual_str_size.get_item("str_widths")?,),
    )?;
    actual_str_size.set_item("str_widths", common::py_float(py, 0.0))?;
    Ok(str_widths)
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_tm
#[pyfunction]
pub fn text_extractor_handle_tm<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    operands: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let attempt: PyResult<Vec<Bound<'_, PyAny>>> = (|| {
        let sliced = operands.get_item(common::slice_of(
            py,
            Some(&common::py_int(py, 0)),
            Some(&common::py_int(py, 6)),
            None,
        )?)?;
        let mut values: Vec<Bound<'_, PyAny>> = Vec::new();
        for operand in sliced.try_iter()? {
            values.push(common::float_of(py, &operand?)?);
        }
        Ok(values)
    })();
    let tm_matrix = match attempt {
        Ok(values) => values,
        Err(error) => {
            if !common::is_type_or_value_error(py, &error) {
                return Err(error);
            }
            Vec::new()
        }
    };
    // Fall back to the identity matrix when Tm carries the wrong number of
    // operands, mirroring _handle_cm, so the text matrix stays a six-element
    // list and later positioning operators do not read past its end.
    if tm_matrix.len() == 6 {
        set_attr(slf, "tm_matrix", common::py_list(py, tm_matrix)?)?;
    } else {
        set_attr(slf, "tm_matrix", common::identity_matrix(py)?)?;
    }
    let actual_str_size = get_attr(slf, "_actual_str_size")?;
    let str_widths = slf.call_method1(
        "compute_str_widths",
        (actual_str_size.get_item("str_widths")?,),
    )?;
    actual_str_size.set_item("str_widths", common::py_float(py, 0.0))?;
    Ok(str_widths)
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_t_star
#[pyfunction]
pub fn text_extractor_handle_t_star<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    _operands: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let tm = get_attr(slf, "tm_matrix")?;
    let leading = get_attr(slf, "TL")?;
    let new4 = tm
        .get_item(4usize)?
        .sub(leading.mul(tm.get_item(2usize)?)?)?;
    let new5 = tm
        .get_item(5usize)?
        .sub(leading.mul(tm.get_item(3usize)?)?)?;
    tm.set_item(4usize, new4)?;
    tm.set_item(5usize, new5)?;
    let actual_str_size = get_attr(slf, "_actual_str_size")?;
    let str_widths = slf.call_method1(
        "compute_str_widths",
        (actual_str_size.get_item("str_widths")?,),
    )?;
    actual_str_size.set_item("str_widths", common::py_float(py, 0.0))?;
    Ok(str_widths)
}

// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction._handle_tj_operation
#[pyfunction]
pub fn text_extractor_handle_tj_operation<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    operands: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let zero = common::py_float(py, 0.0);
    if !operands.is_truthy()? {
        return Ok(zero);
    }
    let first = operands.get_item(0usize)?;
    if !common::is_str_or_bytes(py, &first)? {
        return Ok(zero);
    }
    set_attr(slf, "_previous_font_space", common::none(py))?;
    let result = text_extractor_handle_tj(
        py,
        slf,
        &get_attr(slf, "text")?,
        operands,
        &get_attr(slf, "cm_matrix")?,
        &get_attr(slf, "tm_matrix")?,
        &get_attr(slf, "font_resource")?,
        &get_attr(slf, "font")?,
        &get_attr(slf, "orientations")?,
        &get_attr(slf, "font_size")?,
        &get_attr(slf, "rtl_dir")?,
        &get_attr(slf, "visitor_text")?,
        &get_attr(slf, "_actual_str_size")?,
    )?;
    set_attr(slf, "text", result.get_item(0usize)?)?;
    set_attr(slf, "rtl_dir", result.get_item(1usize)?)?;
    set_attr(slf, "_actual_str_size", result.get_item(2usize)?)?;
    // str_widths will be handled in post-processing
    Ok(zero)
}
