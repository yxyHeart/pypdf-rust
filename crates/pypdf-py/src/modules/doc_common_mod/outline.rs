//! Outline and destination-building logic of `pypdf._doc_common`
//! (upstream pypdf 7.1.1): `outline`, `_get_outline`, `_build_outline_item`,
//! `_build_destination`, `open_destination` and `threads`.

use pyo3::exceptions::{PyAttributeError, PyKeyError, PyNotImplementedError};

use super::*;

// @pypdf: pypdf._doc_common.PdfDocCommon.outline
#[pyfunction]
fn doccommon_outline<'py>(slf: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    slf.call_method0("_get_outline")
}

// @pypdf: pypdf._doc_common.PdfDocCommon._get_outline
#[pyfunction]
#[pyo3(signature = (slf, node, outline, visited, depth, traversal_state))]
fn doccommon_get_outline<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    node: &Bound<'py, PyAny>,
    outline: &Bound<'py, PyAny>,
    visited: &Bound<'py, PyAny>,
    depth: &Bound<'py, PyAny>,
    traversal_state: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut traversal_state = traversal_state.clone();
    if traversal_state.is_none() {
        traversal_state = utils_attr(py, "_TraversalState")?.call0()?;
    }
    let mut outline = outline.clone();
    let mut node = node.clone();
    let mut visited = visited.clone();

    if outline.is_none() {
        outline = PyList::empty(py).into_any();
        let catalog = slf.getattr("root_object")?;
        let outlines_key = constants_class(py, "Core")?.getattr("OUTLINES")?;
        if catalog.contains(&outlines_key)? {
            let lines = catalog.get_item(&outlines_key)?;
            // A reference to an object which is missing or free in the xref
            // table resolves to None.
            if is_null_or_none(py, &lines)? {
                return Ok(outline);
            }
            if !is_generic_instance(py, &lines, "DictionaryObject")? {
                log_warning_with(
                    py,
                    "Outlines are not a dictionary: %(lines)s",
                    &[("lines", &lines)],
                )?;
                return Ok(outline);
            }
            // §12.3.3 Document outline, entries in the outline dictionary
            let first_key = py_str(py, "/First");
            if lines.contains(&first_key)? {
                node = lines.get_item(&first_key)?;
            }
        }
        let named_destinations = slf.call_method0("_get_named_destinations")?;
        slf.setattr("_named_destinations", named_destinations)?;
    }

    if node.is_none() {
        return Ok(outline);
    }

    let configuration = configuration(py)?;
    let maximum_depth = configuration.getattr("outline_maximum_depth")?;
    if depth.gt(&maximum_depth)? {
        return Err(limit_reached(
            py,
            &format!(
                "Maximum outline depth reached: {} > {}.",
                str_of(depth)?,
                str_of(&maximum_depth)?
            ),
        ));
    }

    if visited.is_none() {
        visited = PySet::empty(py)?.into_any();
    }
    let maximum_entries = configuration.getattr("outline_maximum_entries")?;

    // see if there are any more outline items
    loop {
        let node_id = py_id(py, &node)?;
        if visited.contains(&node_id)? {
            log_warning_with(
                py,
                "Detected cycle in outline structure for %(node)s",
                &[("node", &node)],
            )?;
            break;
        }
        visited.call_method1("add", (node_id,))?;
        let count = py_add(&traversal_state.getattr("entry_count")?, &py_int(py, 1))?;
        traversal_state.setattr("entry_count", count.clone())?;
        if count.gt(&maximum_entries)? {
            return Err(limit_reached(
                py,
                &format!(
                    "Maximum outline entry limit reached: {} > {}.",
                    str_of(&count)?,
                    str_of(&maximum_entries)?
                ),
            ));
        }

        if !is_generic_instance(py, &node, "DictionaryObject")? {
            log_warning_with(
                py,
                "Outline node is not a dictionary: %(node)s",
                &[("node", &node)],
            )?;
            break;
        }

        let outline_item = slf.call_method1("_build_outline_item", (node.clone(),))?;
        if outline_item.is_truthy()? {
            outline.call_method1("append", (outline_item,))?;
        }

        // check for sub-outline
        let first_key = py_str(py, "/First");
        if node.contains(&first_key)? {
            let sub_outline = PyList::empty(py);
            // Pass a copy to allow multiple outer entries to reference the same
            // inner one.
            let inner_visited = visited.call_method0("copy")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("node", node.get_item(&first_key)?)?;
            kwargs.set_item("outline", sub_outline.clone())?;
            kwargs.set_item("visited", inner_visited)?;
            kwargs.set_item("depth", py_add(depth, &py_int(py, 1))?)?;
            kwargs.set_item("traversal_state", &traversal_state)?;
            slf.call_method("_get_outline", (), Some(&kwargs))?;
            if sub_outline.len() > 0 {
                outline.call_method1("append", (sub_outline,))?;
            }
        }

        let next_key = py_str(py, "/Next");
        if !node.contains(&next_key)? {
            break;
        }
        node = node.get_item(&next_key)?;
    }
    Ok(outline)
}

// @pypdf: pypdf._doc_common.PdfDocCommon.threads
#[pyfunction]
fn doccommon_threads<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let catalog = slf.getattr("root_object")?;
    let threads_key = constants_class(py, "Core")?.getattr("THREADS")?;
    if catalog.contains(&threads_key)? {
        return catalog.get_item(&threads_key);
    }
    Ok(py_none(py))
}

// @pypdf: pypdf._doc_common.PdfDocCommon._build_destination
#[pyfunction]
fn doccommon_build_destination<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    title: &Bound<'py, PyAny>,
    array: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let destination_class = generic_class(py, "Destination")?;
    let fit_class = generic_class(py, "Fit")?;
    // A valid destination is an array of at least a page and a fit type.
    // Anything else (a name, a bare number, a NullObject, None, ...) cannot be
    // unpacked below, so treat it as a missing destination.
    if !array.is_instance(&generic_class(py, "ArrayObject")?)? || array.len()? < 2 {
        return destination_class.call1((title, null_object(py)?, fit_class.call_method0("fit")?));
    }
    let page = array.get_item(0)?;
    let typ = array.get_item(1)?;
    let mut fit_args = Vec::new();
    for index in 2..array.len()? {
        fit_args.push(array.get_item(index)?);
    }
    let fit_args = py_list(py, fit_args)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("fit_type", typ)?;
    kwargs.set_item("fit_args", &fit_args)?;
    let built = fit_class
        .call((), Some(&kwargs))
        .and_then(|fit| destination_class.call1((title, page.clone(), fit)));
    match built {
        Ok(destination) => Ok(destination),
        Err(error) if err_is(py, &error, "pypdf.errors", "PdfReadError") => {
            log_warning_with(
                py,
                "Unknown destination: %(title)r %(fit_args)s",
                &[("title", title), ("fit_args", &fit_args)],
            )?;
            if slf.getattr("strict")?.is_truthy()? {
                return Err(error);
            }
            // create a link to the first page if the document has one, and to a
            // null target otherwise
            let pages = slf.getattr("pages")?;
            let tmp = if pages.is_truthy()? {
                pages.get_item(0)?.getattr("indirect_reference")?
            } else {
                py_none(py)
            };
            let indirect_reference = if tmp.is_none() { null_object(py)? } else { tmp };
            destination_class.call1((title, indirect_reference, fit_class.call_method0("fit")?))
        }
        Err(error) => Err(error),
    }
}

// @pypdf: pypdf._doc_common.PdfDocCommon._build_outline_item
#[pyfunction]
fn doccommon_build_outline_item<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    node: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut dest = py_none(py);
    let title;

    // title required for valid outline
    // §12.3.3, entries in an outline item dictionary
    let title_key = py_str(py, "/Title");
    if node.contains(&title_key)? {
        title = node.get_item(&title_key)?;
    } else {
        if slf.getattr("strict")?.is_truthy()? {
            return Err(pdf_read_error(
                py,
                &format!("Outline Entry Missing /Title attribute: {}", repr_of(node)?),
            ));
        }
        title = py_str(py, "");
    }

    let action_key = py_str(py, "/A");
    let dest_key = py_str(py, "/Dest");
    if node.contains(&action_key)? {
        // Action, PDF 1.7 and PDF 2.0 §12.6 (only type GoTo supported)
        let action = node.get_item(&action_key)?;
        let goto_arguments = constants_class(py, "GoToActionArguments")?;
        let s_key = goto_arguments.getattr("S")?;
        if action.contains(&s_key)? {
            let action_type = action.get_item(&s_key)?;
            if action_type.eq("/GoTo")? {
                let d_key = goto_arguments.getattr("D")?;
                if action.contains(&d_key)? {
                    dest = action.get_item(&d_key)?;
                } else if slf.getattr("strict")?.is_truthy()? {
                    return Err(pdf_read_error(
                        py,
                        &format!("Outline Action Missing /D attribute: {}", repr_of(node)?),
                    ));
                }
            }
        } else if slf.getattr("strict")?.is_truthy()? {
            return Err(pdf_read_error(
                py,
                &format!("Outline Action Missing /S attribute: {}", repr_of(node)?),
            ));
        }
    } else if node.contains(&dest_key)? {
        // Destination, PDF 1.7 and PDF 2.0 §12.3.2
        dest = node.get_item(&dest_key)?;
        // if array was referenced in another object, will be a dict w/ key "/D"
        let inner_dest_key = py_str(py, "/D");
        if is_generic_instance(py, &dest, "DictionaryObject")? && dest.contains(&inner_dest_key)? {
            dest = dest.get_item(&inner_dest_key)?;
        }
    }

    let outline_item;
    if dest.is_instance(&generic_class(py, "ArrayObject")?)? {
        outline_item = slf.call_method1("_build_destination", (title.clone(), dest.clone()))?;
    } else if dest.is_instance(&builtins(py)?.getattr("str")?)? {
        // named destination, addresses NameObject Issue #193
        let named_destinations = slf.getattr("_named_destinations")?;
        match named_destinations.get_item(&dest) {
            Ok(named) => {
                outline_item = slf.call_method1(
                    "_build_destination",
                    (title.clone(), named.getattr("dest_array")?),
                )?;
            }
            Err(error) if error.is_instance_of::<PyKeyError>(py) => {
                // named destination not found in Name Dict
                outline_item =
                    slf.call_method1("_build_destination", (title.clone(), py_none(py)))?;
            }
            Err(error) => return Err(error),
        }
    } else if dest.is_none() {
        // outline item not required to have destination or action
        // PDFv1.7 Table 153
        outline_item = slf.call_method1("_build_destination", (title.clone(), dest.clone()))?;
    } else {
        if slf.getattr("strict")?.is_truthy()? {
            return Err(pdf_read_error(
                py,
                &format!("Unexpected destination {}", repr_of(&dest)?),
            ));
        }
        log_warning_with(
            py,
            "Removed unexpected destination %(dest)r from destination",
            &[("dest", &dest)],
        )?;
        outline_item = slf.call_method1("_build_destination", (title.clone(), py_none(py)))?;
    }

    // if outline item created, add color, format, and child count if present
    if outline_item.is_truthy()? {
        let color_key = py_str(py, "/C");
        if node.contains(&color_key)? {
            // Color of outline item font in (R, G, B) with values ranging 0.0-1.0
            let color = node.get_item(&color_key)?;
            if color.is_instance(&builtins(py)?.getattr("list")?)? {
                let float_class = generic_class(py, "FloatObject")?;
                let mut components = Vec::new();
                let mut iterator = color.try_iter()?;
                while let Some(component) = next_item(&mut iterator)? {
                    components.push(float_class.call1((component,))?);
                }
                outline_item.set_item(
                    &name_object(py, &py_str(py, "/C"))?,
                    array_from(py, components)?,
                )?;
            } else {
                log_warning_with(
                    py,
                    "Ignoring non-array outline color %(color)r",
                    &[("color", &color)],
                )?;
            }
        }
        let style_key = py_str(py, "/F");
        if node.contains(&style_key)? {
            // specifies style characteristics bold and/or italic
            // with 1=italic, 2=bold, 3=both
            outline_item.set_item(&name_object(py, &style_key)?, node.get_item(&style_key)?)?;
        }
        let count_key = py_str(py, "/Count");
        if node.contains(&count_key)? {
            // absolute value = num. visible children
            // with positive = open/unfolded, negative = closed/folded
            outline_item.set_item(&name_object(py, &count_key)?, node.get_item(&count_key)?)?;
        }
        // if count is 0 we will consider it as open (to have available is_open)
        let is_open = dict_get(node, &count_key, &py_int(py, 0))?.ge(0_i64)?;
        outline_item.set_item(
            &name_object(py, &py_str(py, "/%is_open%"))?,
            boolean_object(py, &py_bool(py, is_open))?,
        )?;
    }
    outline_item.setattr("node", node)?;
    match node.getattr("indirect_reference") {
        Ok(reference) => outline_item.setattr("indirect_reference", reference)?,
        Err(error) if error.is_instance_of::<PyAttributeError>(py) => {}
        Err(error) => return Err(error),
    }
    Ok(outline_item)
}

/// `page, typ, *array = oa` then `Destination("OpenAction", page, Fit(typ, tuple(array)))`.
fn build_open_destination<'py>(
    py: Python<'py>,
    handle: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let length = handle.len()?;
    if length < 2 {
        return Err(message_exception(
            py,
            "ValueError",
            &format!("not enough values to unpack (expected at least 2, got {length})"),
        ));
    }
    let page = handle.get_item(0)?;
    let typ = handle.get_item(1)?;
    let mut fit_args = Vec::new();
    for index in 2..length {
        fit_args.push(handle.get_item(index)?);
    }
    let fit = generic_class(py, "Fit")?.call1((typ, py_tuple(py, fit_args)?))?;
    generic_class(py, "Destination")?.call1((py_str(py, "OpenAction"), page, fit))
}

// @pypdf: pypdf._doc_common.PdfDocCommon.open_destination
#[pyfunction]
fn doccommon_open_destination<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let root = slf.getattr("root_object")?;
    let open_action_key = py_str(py, "/OpenAction");
    if !root.contains(&open_action_key)? {
        return Ok(py_none(py));
    }
    let mut open_action = root.get_item(&open_action_key)?;
    if open_action.is_instance(&builtins(py)?.getattr("bytes")?)? {
        open_action = open_action.call_method0("decode")?;
    }
    if open_action.is_instance(&builtins(py)?.getattr("str")?)? {
        return generic_class(py, "create_string_object")?.call1((open_action,));
    }
    if open_action.is_instance(&generic_class(py, "ArrayObject")?)? {
        return match build_open_destination(py, &open_action) {
            Ok(destination) => Ok(destination),
            Err(error) => {
                let cause = str_of(error.value(py).as_any())?;
                Err(exception_from(
                    py,
                    &format!("Invalid Destination {}: {}", str_of(&open_action)?, cause),
                    error,
                ))
            }
        };
    }
    Ok(py_none(py))
}

// @pypdf: pypdf._doc_common.PdfDocCommon.open_destination
#[pyfunction]
fn doccommon_set_open_destination<'py>(
    _slf: &Bound<'py, PyAny>,
    _dest: &Bound<'py, PyAny>,
) -> PyResult<()> {
    Err(PyNotImplementedError::new_err(
        "No setter for open_destination",
    ))
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(doccommon_outline, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_get_outline, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_threads, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_build_destination, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_build_outline_item, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_open_destination, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_set_open_destination, module)?)?;
    Ok(())
}
