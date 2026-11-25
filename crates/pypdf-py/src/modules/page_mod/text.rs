//! Text extraction entry points of `pypdf._page.PageObject` (upstream pypdf
//! 7.1.1).
//!
//! The heavy lifting lives in `pypdf._text_extraction` (ported separately); this
//! module reproduces the orchestration that upstream keeps in `_page.py`,
//! including the dynamic `visitor_*` callbacks and the exact warning texts.

use pyo3::exceptions::{PyMemoryError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyTuple};

use super::common::{
    self, constants_attr, dict_object, generic_class, globals, is_null_or_none, name_object,
    null_object, op_add, op_mul, op_neg, py_int, py_len, py_list, py_str, py_tuple,
};

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyModule>> {
    common::builtins(py)
}

fn get_attr<'py>(obj: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    obj.getattr(name)
}

fn add_assign<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    name: &str,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let current = obj.getattr(name)?;
    obj.setattr(name, op_add(py, &current, value)?)?;
    Ok(())
}

fn is_instance_int_or_float(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    let builtins = builtins(py)?;
    Ok(value.is_instance(&builtins.getattr("int")?)?
        || value.is_instance(&builtins.getattr("float")?)?)
}

fn is_instance_tuple_or_int(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    let builtins = builtins(py)?;
    Ok(value.is_instance(&builtins.getattr("tuple")?)?
        || value.is_instance(&builtins.getattr("int")?)?)
}

fn invocation<'py>(value: Option<&Bound<'py, PyAny>>) -> Option<Bound<'py, PyAny>> {
    value.cloned()
}

// @pypdf: pypdf._page.PageObject.extract_text
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (slf, args, orientations, space_width, visitor_operand_before, visitor_operand_after, visitor_text, extraction_mode, kwargs))]
pub fn page_extract_text<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    args: &Bound<'py, PyTuple>,
    orientations: &Bound<'py, PyAny>,
    space_width: &Bound<'py, PyAny>,
    visitor_operand_before: &Bound<'py, PyAny>,
    visitor_operand_after: &Bound<'py, PyAny>,
    visitor_text: &Bound<'py, PyAny>,
    extraction_mode: &Bound<'py, PyAny>,
    kwargs: &Bound<'py, PyDict>,
) -> PyResult<String> {
    let builtins = builtins(py)?;
    let mode: String = extraction_mode.extract()?;
    if mode != "plain" && mode != "layout" {
        return Err(PyValueError::new_err(format!(
            "Invalid text extraction mode '{}'",
            mode
        )));
    }
    if mode == "layout" {
        for (visitor_name, visitor) in [
            ("visitor_operand_before", visitor_operand_before),
            ("visitor_operand_after", visitor_operand_after),
            ("visitor_text", visitor_text),
        ] {
            if !visitor.is_none() {
                common::logger_warning(
                    py,
                    "Argument %(visitor)s is ignored in layout mode",
                    &[("visitor", common::py_str_type(py, visitor_name))],
                )?;
            }
        }
        let space_vertically =
            kwargs.call_method1("get", ("layout_mode_space_vertically", true))?;
        let scale_weight = kwargs.call_method1("get", ("layout_mode_scale_weight", 1.25))?;
        let strip_rotated = kwargs.call_method1("get", ("layout_mode_strip_rotated", true))?;
        let debug_path = kwargs.call_method1("get", ("layout_mode_debug_path", py.None()))?;
        let font_height_weight =
            kwargs.call_method1("get", ("layout_mode_font_height_weight", 1))?;
        let call_kwargs = PyDict::new(py);
        call_kwargs.set_item("space_vertically", space_vertically)?;
        call_kwargs.set_item("scale_weight", scale_weight)?;
        call_kwargs.set_item("strip_rotated", strip_rotated)?;
        call_kwargs.set_item("debug_path", debug_path)?;
        call_kwargs.set_item("font_height_weight", font_height_weight)?;
        let result = slf.call_method("_layout_mode_text", (), Some(&call_kwargs))?;
        return result.extract();
    }
    let mut orientations = orientations.clone();
    let mut space_width = space_width.clone();
    let args_len = args.len();
    if args_len >= 1 {
        let first = args.get_item(0)?;
        if first.is_instance(&builtins.getattr("str")?)? {
            if args_len >= 3 {
                let third = args.get_item(2)?;
                if is_instance_tuple_or_int(py, &third)? {
                    orientations = third;
                } else {
                    return Err(PyTypeError::new_err(format!(
                        "Invalid positional parameter {}",
                        py_str(&third)?
                    )));
                }
            }
            if args_len >= 4 {
                let fourth = args.get_item(3)?;
                if is_instance_int_or_float(py, &fourth)? {
                    space_width = fourth;
                } else {
                    return Err(PyTypeError::new_err(format!(
                        "Invalid positional parameter {}",
                        py_str(&fourth)?
                    )));
                }
            }
        } else if is_instance_tuple_or_int(py, &first)? {
            orientations = first;
            if args_len >= 2 {
                let second = args.get_item(1)?;
                if is_instance_int_or_float(py, &second)? {
                    space_width = second;
                } else {
                    return Err(PyTypeError::new_err(format!(
                        "Invalid positional parameter {}",
                        py_str(&second)?
                    )));
                }
            }
        } else {
            return Err(PyTypeError::new_err(format!(
                "Invalid positional parameter {}",
                py_str(&first)?
            )));
        }
    }
    if orientations.is_instance(&builtins.getattr("int")?)? {
        orientations = py_tuple(py, vec![orientations])?;
    }
    let pdf = slf.getattr("pdf")?;
    let contents_key = constants_attr(py, "PageAttributes", "CONTENTS")?;
    slf.call_method1(
        "_extract_text",
        (
            slf.clone(),
            pdf,
            orientations,
            space_width,
            contents_key,
            visitor_operand_before.clone(),
            visitor_operand_after.clone(),
            visitor_text.clone(),
        ),
    )?
    .extract()
}

// @pypdf: pypdf._page.PageObject._extract_text
#[pyfunction]
#[pyo3(signature = (slf, obj, pdf, orientations, space_width, content_key, visitor_operand_before, visitor_operand_after, visitor_text, known_ids=None, traversal_state=None))]
#[allow(clippy::too_many_arguments)]
pub fn page_extract_text_inner<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    obj: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
    orientations: &Bound<'py, PyAny>,
    space_width: &Bound<'py, PyAny>,
    content_key: &Bound<'py, PyAny>,
    visitor_operand_before: &Bound<'py, PyAny>,
    visitor_operand_after: &Bound<'py, PyAny>,
    visitor_text: &Bound<'py, PyAny>,
    known_ids: Option<&Bound<'py, PyAny>>,
    traversal_state: Option<&Bound<'py, PyAny>>,
) -> PyResult<String> {
    let known_ids = match known_ids {
        Some(value) if !value.is_none() => value.clone(),
        _ => builtins(py)?.getattr("set")?.call0()?,
    };
    let traversal_state = match traversal_state {
        Some(value) if !value.is_none() => value.clone(),
        _ => globals(py, "_TraversalState")?.call0()?,
    };
    let resources_dict = globals(py, "_get_page_resources")?.call1((obj.clone(),))?;
    if !resources_dict.is_truthy()? {
        // No resources means no text is possible (no font); we consider the
        // file as not damaged, no need to check for TJ or Tj
        return Ok(String::new());
    }
    let font_resources = PyDict::new(py);
    let fonts = PyDict::new(py);
    let font_resources_dict =
        globals(py, "_get_font_resources")?.call1((resources_dict.clone(),))?;
    if font_resources_dict.is_truthy()? {
        for font_resource in font_resources_dict.try_iter()? {
            let font_resource = font_resource?;
            let attempt: PyResult<()> = (|| {
                let font_resource_object =
                    get_attr(&font_resources_dict.get_item(&font_resource)?, "get_object")?
                        .call0()?;
                font_resources.set_item(&font_resource, &font_resource_object)?;
                let font = globals(py, "Font")?
                    .getattr("from_font_resource")?
                    .call1((font_resource_object,))?;
                fonts.set_item(&font_resource, &font)?;
                // Override space width, if applicable
                let character_widths = font.getattr("character_widths")?;
                let space_char = font.getattr("space_char")?;
                let width = character_widths.call_method1("get", (space_char, 0))?;
                if width.eq(py_int(py, 0))? {
                    font.setattr("space_width", space_width.clone())?;
                }
                Ok(())
            })();
            match attempt {
                Ok(()) => {}
                Err(error)
                    if error.is_instance_of::<pyo3::exceptions::PyAttributeError>(py)
                        || error.is_instance_of::<PyTypeError>(py) => {}
                Err(error) => return Err(error),
            }
        }
    }
    let text_extraction = crate::pyutil::cached_module(py, "pypdf._text_extraction")?;
    let has_visitor = !(visitor_operand_before.is_none()
        && visitor_operand_after.is_none()
        && visitor_text.is_none());
    let call_kwargs = PyDict::new(py);
    call_kwargs.set_item("has_visitor", has_visitor)?;
    let content = text_extraction
        .getattr("_get_content_stream_for_text")?
        .call(
            (obj.clone(), pdf.clone(), content_key.clone()),
            Some(&call_kwargs),
        )?;
    if content.is_none() {
        return Ok(String::new());
    }
    let extractor = globals(py, "TextExtraction")?.call0()?;
    extractor.call_method1(
        "initialize_extraction",
        (
            orientations.clone(),
            visitor_text.clone(),
            font_resources,
            fonts,
        ),
    )?;
    let operations = content.getattr("operations")?;
    // When the extractor uses its default handler table, operators that no
    // handler covers can be skipped without entering `process_operation`
    // (most content-stream operators are pure graphics operators).
    let fast_dispatch = {
        let handlers = extractor.getattr("operation_handlers")?;
        let default = extractor.getattr("_rs_default_handlers")?;
        handlers.is(&default) && handlers.len()? == 14
    };
    const HANDLED_OPERATORS: [&[u8]; 14] = [
        b"BT", b"ET", b"q", b"Q", b"cm", b"Tz", b"Tc", b"Tw", b"TL", b"Tf", b"Td", b"Tm",
        b"T*", b"Tj",
    ];
    for pair in operations.try_iter()? {
        let pair = pair?;
        let operands = pair.get_item(0)?;
        let operator = pair.get_item(1)?;
        if !visitor_operand_before.is_none() {
            visitor_operand_before.call1((
                operator.clone(),
                operands.clone(),
                extractor.getattr("cm_matrix")?,
                extractor.getattr("tm_matrix")?,
            ))?;
        }
        // Compare the operator bytes without allocating a fresh `bytes` object
        // for every comparison (this loop runs once per content-stream op).
        let operator_bytes: &[u8] = match operator.cast::<pyo3::types::PyBytes>() {
            Ok(bytes) => bytes.as_bytes(),
            Err(_) => b"",
        };
        if operator_bytes == b"'" {
            crate::text_extraction_mod::extractor::text_extractor_process_operation(py, &extractor, &(common::py_bytes(py, b"T*")), &(py_list(py, vec![])?)) ?;
            crate::text_extraction_mod::extractor::text_extractor_process_operation(py, &extractor, &(common::py_bytes(py, b"Tj")), &(operands.clone())) ?;
        } else if operator_bytes == b"\"" && py_len(&operands)? >= 3 {
            crate::text_extraction_mod::extractor::text_extractor_process_operation(py, &extractor, &(common::py_bytes(py, b"Tw")), &(py_list(py, vec![operands.get_item(0)?])?)) ?;
            crate::text_extraction_mod::extractor::text_extractor_process_operation(py, &extractor, &(common::py_bytes(py, b"Tc")), &(py_list(py, vec![operands.get_item(1)?])?)) ?;
            crate::text_extraction_mod::extractor::text_extractor_process_operation(py, &extractor, &(common::py_bytes(py, b"T*")), &(py_list(py, vec![])?)) ?;
            crate::text_extraction_mod::extractor::text_extractor_process_operation(py, &extractor, &(common::py_bytes(py, b"Tj")), &(operands.get_item(super::common::slice_object(
                        py,
                        Some(py_int(py, 2)),
                        None,
                        None,
                    )?)?)) ?;
        } else if operator_bytes == b"TJ" {
            // The space width may be smaller than the font width, so the width should be 95%.
            let space_width_value = extractor.getattr("_space_width")?;
            let confirm_space_width = op_mul(py, &space_width_value, &common::py_float(py, 0.95))?;
            let array_class = generic_class(py, "ArrayObject")?;
            let name_class = generic_class(py, "NameObject")?;
            let builtins = builtins(py)?;
            let str_type = builtins.getattr("str")?;
            let bytes_type = builtins.getattr("bytes")?;
            let int_type = builtins.getattr("int")?;
            let float_type = builtins.getattr("float")?;
            if operands.is_truthy()? && operands.get_item(0)?.is_instance(&array_class)? {
                for op in operands.get_item(0)?.try_iter()? {
                    let op = op?;
                    let is_text = (op.is_instance(&str_type)? || op.is_instance(&bytes_type)?)
                        && !op.is_instance(&name_class)?;
                    if is_text {
                        crate::text_extraction_mod::extractor::text_extractor_process_operation(py, &extractor, &(common::py_bytes(py, b"Tj")), &(py_list(py, vec![op.clone()])?)) ?;
                    }
                    let is_number = op.is_instance(&int_type)? || op.is_instance(&float_type)?;
                    if is_number {
                        let value = float_type.call1((op,))?;
                        crate::text_extraction_mod::extractor::text_extractor_process_tj_offset(py, &extractor, &(value), &(confirm_space_width.clone())) ?;
                    }
                }
            }
        } else if operator_bytes == b"TD" && py_len(&operands)? >= 2 {
            // a non-numeric ty leaves the leading unchanged, like a non-numeric TL
            if is_instance_int_or_float(py, &operands.get_item(1)?)? {
                crate::text_extraction_mod::extractor::text_extractor_process_operation(py, &extractor, &(common::py_bytes(py, b"TL")), &(py_list(py, vec![op_neg(py, &operands.get_item(1)?)?])?)) ?;
            }
            crate::text_extraction_mod::extractor::text_extractor_process_operation(py, &extractor, &(common::py_bytes(py, b"Td")), &(operands.clone())) ?;
        } else if operator_bytes == b"Do" && operands.is_truthy()? {
            add_assign(py, &extractor, "output", &extractor.getattr("text")?)?;
            if !visitor_text.is_none() {
                visitor_text.call1((
                    extractor.getattr("text")?,
                    extractor.getattr("memo_cm")?,
                    extractor.getattr("memo_tm")?,
                    extractor.getattr("font_resource")?,
                    extractor.getattr("font_size")?,
                ))?;
            }
            let last_char: PyResult<Bound<'py, PyAny>> = (|| {
                let output = extractor.getattr("output")?;
                output.get_item(-1)
            })();
            match last_char {
                Ok(last) => {
                    if !last.eq("\n")? {
                        add_assign(py, &extractor, "output", &common::py_str_type(py, "\n"))?;
                        if !visitor_text.is_none() {
                            visitor_text.call1((
                                "\n",
                                extractor.getattr("memo_cm")?,
                                extractor.getattr("memo_tm")?,
                                extractor.getattr("font_resource")?,
                                extractor.getattr("font_size")?,
                            ))?;
                        }
                    }
                }
                Err(error) if error.is_instance_of::<pyo3::exceptions::PyIndexError>(py) => {}
                Err(error) => return Err(error),
            }
            let xform_attempt: PyResult<Option<String>> = (|| {
                let xform_kwargs = PyDict::new(py);
                xform_kwargs.set_item("resources_dict", resources_dict.clone())?;
                xform_kwargs.set_item("operands", operands.clone())?;
                xform_kwargs.set_item("orientations", orientations.clone())?;
                xform_kwargs.set_item("space_width", space_width.clone())?;
                xform_kwargs.set_item("visitor_operand_before", visitor_operand_before.clone())?;
                xform_kwargs.set_item("visitor_operand_after", visitor_operand_after.clone())?;
                xform_kwargs.set_item("visitor_text", visitor_text.clone())?;
                xform_kwargs.set_item("known_ids", known_ids.clone())?;
                xform_kwargs.set_item("traversal_state", traversal_state.clone())?;
                let result = slf.call_method("_extract_text__xform", (), Some(&xform_kwargs))?;
                if result.is_none() {
                    Ok(None)
                } else {
                    Ok(Some(result.extract()?))
                }
            })();
            match xform_attempt {
                Ok(Some(text)) => {
                    add_assign(py, &extractor, "output", &common::py_str_type(py, &text))?;
                }
                Ok(None) => {}
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(exception) => {
                    let operand = operands.get_item(0)?;
                    common::logger_warning(
                        py,
                        "Impossible to decode XFormObject %(operand)s: %(exception)s",
                        &[
                            ("operand", operand),
                            (
                                "exception",
                                exception.into_value(py).into_bound(py).into_any(),
                            ),
                        ],
                    )?;
                }
            }
            // finally:
            extractor.setattr("text", "")?;
            extractor.setattr(
                "memo_cm",
                extractor.getattr("cm_matrix")?.call_method0("copy")?,
            )?;
            extractor.setattr(
                "memo_tm",
                extractor.getattr("tm_matrix")?.call_method0("copy")?,
            )?;
        } else if fast_dispatch && !HANDLED_OPERATORS.contains(&operator_bytes) {
            // No handler for this operator: `process_operation` would do nothing.
        } else {
            crate::text_extraction_mod::extractor::text_extractor_process_operation(py, &extractor, &(operator.clone()), &(operands.clone())) ?;
        }
        if !visitor_operand_after.is_none() {
            visitor_operand_after.call1((
                operator,
                operands,
                extractor.getattr("cm_matrix")?,
                extractor.getattr("tm_matrix")?,
            ))?;
        }
    }
    // just in case
    add_assign(py, &extractor, "output", &extractor.getattr("text")?)?;
    let text = extractor.getattr("text")?;
    if !text.eq("")? && !visitor_text.is_none() {
        visitor_text.call1((
            text,
            extractor.getattr("memo_cm")?,
            extractor.getattr("memo_tm")?,
            extractor.getattr("font_resource")?,
            extractor.getattr("font_size")?,
        ))?;
    }
    extractor.getattr("output")?.extract()
}

// @pypdf: pypdf._page.PageObject._extract_text__xform
#[pyfunction]
#[pyo3(signature = (slf, resources_dict, operands, orientations, space_width, visitor_operand_before, visitor_operand_after, visitor_text, known_ids, traversal_state))]
#[allow(clippy::too_many_arguments)]
pub fn page_extract_text_xform<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    resources_dict: &Bound<'py, PyAny>,
    operands: &Bound<'py, PyAny>,
    orientations: &Bound<'py, PyAny>,
    space_width: &Bound<'py, PyAny>,
    visitor_operand_before: &Bound<'py, PyAny>,
    visitor_operand_after: &Bound<'py, PyAny>,
    visitor_text: &Bound<'py, PyAny>,
    known_ids: &Bound<'py, PyAny>,
    traversal_state: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let xobj = resources_dict.get_item("/XObject")?;
    let xform = xobj.get_item(operands.get_item(0)?)?;
    if xform
        .get_item("/Subtype")?
        .eq(&name_object(py, "/Image")?)?
    {
        return Ok(py.None());
    }
    let xform_id = builtins(py)?.getattr("id")?.call1((xform.clone(),))?;
    if known_ids.contains(&xform_id)? {
        common::logger_warning(
            py,
            "Detected cyclic form XObject reference, skipping %(operand)s.",
            &[("operand", operands.get_item(0)?)],
        )?;
        return Ok(common::py_str_type(py, "").unbind());
    }
    let configuration = py
        .import("pypdf._configuration")?
        .getattr("get_configuration")?
        .call0()?;
    let limit = configuration.getattr("xform_maximum_invocations_per_extraction")?;
    let entry_count = traversal_state.getattr("entry_count")?;
    if entry_count.ge(&limit)? {
        let has_logged = traversal_state.getattr("has_logged")?;
        if !has_logged.is_truthy()? {
            traversal_state.setattr("has_logged", true)?;
            common::logger_warning(
                py,
                "Exceeded %(limit)d form XObject invocations while extracting text; further form content is skipped.",
                &[("limit", limit)],
            )?;
        }
        return Ok(common::py_str_type(py, "").unbind());
    }
    traversal_state.setattr("entry_count", op_add(py, &entry_count, &py_int(py, 1))?)?;
    known_ids.call_method1("add", (xform_id.clone(),))?;
    let call_kwargs = PyDict::new(py);
    call_kwargs.set_item("known_ids", known_ids.clone())?;
    call_kwargs.set_item("traversal_state", traversal_state.clone())?;
    let extracted = slf.call_method(
        "extract_xform_text",
        (
            xform,
            orientations.clone(),
            space_width.clone(),
            visitor_operand_before.clone(),
            visitor_operand_after.clone(),
            visitor_text.clone(),
        ),
        Some(&call_kwargs),
    );
    // finally: known_ids.discard(xform_id)
    known_ids.call_method1("discard", (xform_id,))?;
    Ok(extracted?.unbind())
}

// @pypdf: pypdf._page.PageObject._layout_mode_fonts
#[pyfunction]
pub fn page_layout_mode_fonts<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let fonts = PyDict::new(py);
    let visited = builtins(py)?.getattr("set")?.call0()?;
    let mut obj = slf.clone();
    let resources_key = constants_attr(py, "PageAttributes", "RESOURCES")?;
    let dictionary_class = generic_class(py, "DictionaryObject")?;
    loop {
        let obj_id = builtins(py)?.getattr("id")?.call1((obj.clone(),))?;
        if visited.contains(&obj_id)? {
            common::logger_warning(
                py,
                "Detected cycle in /Parent hierarchy when retrieving fonts.",
                &[],
            )?;
            break;
        }
        visited.call_method1("add", (obj_id,))?;
        let resources_dict = obj
            .call_method1("get", (resources_key.clone(), dict_object(py)?))?
            .call_method0("get_object")?;
        let mut resources_dict = resources_dict;
        if !resources_dict.is_instance(&dictionary_class)? {
            common::logger_warning(
                py,
                "Page resources are not a dictionary: %(resources)s",
                &[("resources", resources_dict.clone())],
            )?;
            resources_dict = dict_object(py)?;
        }
        let pdf = slf.getattr("pdf")?;
        if !pdf.is_none() {
            let font_resources = globals(py, "_get_font_resources")?.call1((resources_dict,))?;
            if font_resources.is_truthy()? {
                for font_name in font_resources.try_iter()? {
                    let font_name = font_name?;
                    let font_object = font_resources
                        .get_item(&font_name)?
                        .call_method0("get_object")?;
                    if font_object.is_instance(&dictionary_class)? {
                        let font = globals(py, "Font")?
                            .getattr("from_font_resource")?
                            .call1((font_object,))?;
                        fonts.set_item(&font_name, font)?;
                    }
                }
            }
        }
        let parent = obj
            .call_method1("get", ("/Parent", null_object(py)?))?
            .call_method0("get_object")?;
        if !parent.is_instance(&dictionary_class)? {
            break;
        }
        obj = parent;
    }
    Ok(fonts.into_any().unbind())
}

// @pypdf: pypdf._page.PageObject._layout_mode_text
#[pyfunction]
#[pyo3(signature = (slf, space_vertically, scale_weight=None, strip_rotated=true, debug_path=None, font_height_weight=None))]
pub fn page_layout_mode_text<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    space_vertically: bool,
    scale_weight: Option<&Bound<'py, PyAny>>,
    strip_rotated: bool,
    debug_path: Option<&Bound<'py, PyAny>>,
    font_height_weight: Option<&Bound<'py, PyAny>>,
) -> PyResult<String> {
    let scale_weight = match scale_weight {
        Some(value) => value.clone(),
        None => common::py_float(py, 1.25),
    };
    let font_height_weight = match font_height_weight {
        Some(value) => value.clone(),
        None => py_int(py, 1),
    };
    let fonts = slf.call_method0("_layout_mode_fonts")?;
    if let Some(debug_path) = debug_path {
        if !debug_path.is_none() {
            let json = py.import("json")?;
            let asdict = globals(py, "asdict")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("indent", 2)?;
            kwargs.set_item("default", asdict)?;
            let dumped = json.call_method("dumps", (fonts.clone(),), Some(&kwargs))?;
            debug_path
                .call_method1("joinpath", ("fonts.json",))?
                .call_method1("write_text", (dumped, "utf-8"))?;
        }
    }
    let contents = slf.call_method1("get", ("/Contents",))?;
    let contents = if contents.is_none() {
        contents
    } else {
        contents.call_method0("get_object")?
    };
    let array_class = generic_class(py, "ArrayObject")?;
    let stream_class = generic_class(py, "StreamObject")?;
    if !(contents.is_instance(&array_class)? || contents.is_instance(&stream_class)?) {
        // No content can be extracted (certainly an empty page).
        return Ok(String::new());
    }
    let pdf = slf.getattr("pdf")?;
    let content_stream = globals(py, "ContentStream")?.call1((contents, pdf, "bytes"))?;
    let ops = builtins(py)?
        .getattr("iter")?
        .call1((content_stream.getattr("operations")?,))?;
    let layout_mode = globals(py, "_layout_mode")?;
    let debug_path_value = match debug_path {
        Some(value) => value.clone(),
        None => py.None().into_bound(py),
    };
    let bt_groups = layout_mode.getattr("text_show_operations")?.call1((
        ops,
        fonts,
        strip_rotated,
        debug_path_value.clone(),
    ))?;
    if !bt_groups.is_truthy()? {
        return Ok(String::new());
    }
    let ty_groups = layout_mode
        .getattr("y_coordinate_groups")?
        .call1((bt_groups.clone(), debug_path_value))?;
    let char_width = layout_mode
        .getattr("fixed_char_width")?
        .call1((bt_groups, scale_weight.clone()))?;
    layout_mode
        .getattr("fixed_width_page")?
        .call1((
            ty_groups,
            char_width,
            space_vertically,
            font_height_weight.clone(),
        ))?
        .extract()
}

// @pypdf: pypdf._page.PageObject.extract_xform_text
#[pyfunction]
#[pyo3(signature = (slf, xform, orientations, space_width, visitor_operand_before, visitor_operand_after, visitor_text, known_ids=None, traversal_state=None))]
#[allow(clippy::too_many_arguments)]
pub fn page_extract_xform_text<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    xform: &Bound<'py, PyAny>,
    orientations: &Bound<'py, PyAny>,
    space_width: &Bound<'py, PyAny>,
    visitor_operand_before: &Bound<'py, PyAny>,
    visitor_operand_after: &Bound<'py, PyAny>,
    visitor_text: &Bound<'py, PyAny>,
    known_ids: Option<&Bound<'py, PyAny>>,
    traversal_state: Option<&Bound<'py, PyAny>>,
) -> PyResult<String> {
    // The type hint would have to use an internal type otherwise, which is not desired.
    if let Some(state) = traversal_state {
        let traversal_class = globals(py, "_TraversalState")?;
        if !state.is_none() && !state.is_instance(&traversal_class)? {
            return Err(pyo3::exceptions::PyAssertionError::new_err(()));
        }
    }
    let pdf = slf.getattr("pdf")?;
    let call_kwargs = PyDict::new(py);
    call_kwargs.set_item(
        "known_ids",
        invocation(known_ids).unwrap_or_else(|| py.None().into_bound(py)),
    )?;
    call_kwargs.set_item(
        "traversal_state",
        invocation(traversal_state).unwrap_or_else(|| py.None().into_bound(py)),
    )?;
    slf.call_method(
        "_extract_text",
        (
            xform.clone(),
            pdf,
            orientations.clone(),
            space_width.clone(),
            py.None(),
            visitor_operand_before.clone(),
            visitor_operand_after.clone(),
            visitor_text.clone(),
        ),
        Some(&call_kwargs),
    )?
    .extract()
}

const _: fn() = || {
    let _ = is_null_or_none;
    let _ = py_str;
};

pub fn register(module: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    module.add_function(pyo3::wrap_pyfunction!(page_extract_text, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_extract_text_inner, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_extract_text_xform, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_layout_mode_fonts, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_layout_mode_text, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_extract_xform_text, module)?)?;
    Ok(())
}
