//! Form-field logic of `pypdf._doc_common` (upstream pypdf 7.1.1):
//! `get_fields`, `_build_field`, `_check_kids`, `_write_field`,
//! `_get_qualified_field_name`, `_normal_appearance`, `get_form_text_fields`
//! and `get_pages_showing_field`.

use pyo3::exceptions::PyKeyError;
use pyo3::types::{PyAny, PyDict, PyList};

use super::*;

/// `_check_field_limit(_increment)` nested helper of `get_fields`.
fn check_field_limit<'py>(
    py: Python<'py>,
    traversal_state: &Bound<'py, PyAny>,
    increment: &Bound<'py, PyAny>,
    maximum: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let current = traversal_state.getattr("entry_count")?;
    let updated = py_add(&current, increment)?;
    traversal_state.setattr("entry_count", updated.clone())?;
    if updated.gt(maximum)? {
        return Err(limit_reached(
            py,
            "Exceeded maximum number of fields allowed.",
        ));
    }
    Ok(())
}

/// `self._build_field(field=..., retval=..., ...)` (all keyword arguments, like
/// upstream, so that `mock.patch` on `PdfDocCommon._build_field` keeps working).
#[allow(clippy::too_many_arguments)]
fn call_build_field<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    field: &Bound<'py, PyAny>,
    retval: &Bound<'py, PyAny>,
    fileobj: &Bound<'py, PyAny>,
    field_attributes: &Bound<'py, PyAny>,
    visited: &Bound<'py, PyAny>,
    depth: &Bound<'py, PyAny>,
    traversal_state: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("field", field)?;
    kwargs.set_item("retval", retval)?;
    kwargs.set_item("fileobj", fileobj)?;
    kwargs.set_item("field_attributes", field_attributes)?;
    kwargs.set_item("visited", visited)?;
    kwargs.set_item("depth", depth)?;
    kwargs.set_item("traversal_state", traversal_state)?;
    slf.call_method("_build_field", (), Some(&kwargs))?;
    Ok(())
}

// @pypdf: pypdf._doc_common.PdfDocCommon.get_fields
#[pyfunction]
#[pyo3(signature = (slf, tree, retval, fileobj, stack, visited, depth, traversal_state))]
#[allow(clippy::too_many_arguments)]
fn doccommon_get_fields<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    tree: &Bound<'py, PyAny>,
    retval: &Bound<'py, PyAny>,
    fileobj: &Bound<'py, PyAny>,
    stack: &Bound<'py, PyAny>,
    visited: &Bound<'py, PyAny>,
    depth: &Bound<'py, PyAny>,
    traversal_state: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    if !is_same(stack, &shim_attr(py, "_UNSET")?) {
        // deprecation_no_replacement(name="stack", removed_in="7.0.0")
        let kwargs = PyDict::new(py);
        kwargs.set_item("name", "stack")?;
        kwargs.set_item("removed_in", "7.0.0")?;
        utils_attr(py, "deprecation_no_replacement")?.call((), Some(&kwargs))?;
    }

    let configuration = configuration(py)?;
    let maximum_depth = configuration.getattr("form_fields_maximum_depth")?;
    if depth.ge(&maximum_depth)? {
        return Err(limit_reached(
            py,
            "Reached maximum allowed form field depth.",
        ));
    }
    let mut traversal_state = traversal_state.clone();
    if traversal_state.is_none() {
        traversal_state = utils_attr(py, "_TraversalState")?.call0()?;
    }

    let field_attributes_class = constants_class(py, "FieldDictionaryAttributes")?;
    let checkbox_attributes_class = constants_class(py, "CheckboxRadioButtonAttributes")?;
    let field_attributes = field_attributes_class.call_method0("attributes_dict")?;
    field_attributes.call_method1(
        "update",
        (checkbox_attributes_class.call_method0("attributes_dict")?,),
    )?;
    let maximum_entries = configuration.getattr("form_fields_maximum_entries")?;

    let mut retval = retval.clone();
    let mut tree = tree.clone();
    let mut visited = visited.clone();
    if retval.is_none() {
        retval = PyDict::new(py).into_any();
        let catalog = slf.getattr("root_object")?;
        visited = PySet::empty(py)?.into_any();
        // get the AcroForm tree
        let acro_form_key = constants_class(py, "CatalogAttributes")?.getattr("ACRO_FORM")?;
        if catalog.contains(&acro_form_key)? {
            let entry = catalog.get_item(&acro_form_key)?;
            let acro_form = if entry.is_none() {
                py_none(py)
            } else {
                entry.call_method0("get_object")?
            };
            if !acro_form.is_none() && !is_generic_instance(py, &acro_form, "DictionaryObject")? {
                log_warning_with(
                    py,
                    "AcroForm is not a dictionary: %(acro_form)s",
                    &[("acro_form", &acro_form)],
                )?;
                return Ok(py_none(py));
            }
            tree = acro_form;
        } else {
            return Ok(py_none(py));
        }
    }

    if tree.is_none() {
        return Ok(retval);
    }
    if visited.is_none() {
        // assert _visited is not None
        return Err(empty_exception(py, "AssertionError"));
    }
    let fields_key = py_str(py, "/Fields");
    if tree.contains(&fields_key)? {
        let fields = tree.get_item(&fields_key)?.call_method0("get_object")?;
        if !is_generic_instance(py, &fields, "ArrayObject")? {
            log_warning_with(
                py,
                "AcroForm /Fields is not an array: %(fields)s",
                &[("fields", &fields)],
            )?;
            return Ok(retval);
        }
        check_field_limit(
            py,
            &traversal_state,
            &py_len(py, &fields)?,
            &maximum_entries,
        )?;
        let mut iterator = fields.try_iter()?;
        while let Some(field_reference) = next_item(&mut iterator)? {
            let field = field_reference.call_method0("get_object")?;
            call_build_field(
                py,
                slf,
                &field,
                &retval,
                fileobj,
                &field_attributes,
                &visited,
                depth,
                &traversal_state,
            )?;
        }
    } else if contains_any_key(&tree, &field_attributes)? {
        // Tree is a field
        check_field_limit(py, &traversal_state, &py_int(py, 1), &maximum_entries)?;
        call_build_field(
            py,
            slf,
            &tree,
            &retval,
            fileobj,
            &field_attributes,
            &visited,
            depth,
            &traversal_state,
        )?;
    }
    Ok(retval)
}

/// `any(attr in tree for attr in field_attributes)`.
fn contains_any_key(tree: &Bound<'_, PyAny>, keys: &Bound<'_, PyAny>) -> PyResult<bool> {
    let mut iterator = keys.try_iter()?;
    while let Some(key) = next_item(&mut iterator)? {
        if tree.contains(&key)? {
            return Ok(true);
        }
    }
    Ok(false)
}

// @pypdf: pypdf._doc_common.PdfDocCommon._get_qualified_field_name
#[pyfunction]
#[pyo3(signature = (slf, parent, visited))]
fn doccommon_get_qualified_field_name<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    parent: &Bound<'py, PyAny>,
    visited: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut visited = visited.clone();
    if visited.is_none() {
        visited = PySet::empty(py)?.into_any();
    }
    let parent_id = py_id(py, parent)?;
    if visited.contains(&parent_id)? {
        return Err(limit_reached(
            py,
            "Detected cycle in /Parent hierarchy when retrieving qualified field name.",
        ));
    }
    visited.call_method1("add", (parent_id,))?;

    let tm_key = py_str(py, "/TM");
    if parent.contains(&tm_key)? {
        return parent.get_item(&tm_key);
    }
    let parent_key = py_str(py, "/Parent");
    if parent.contains(&parent_key)? {
        // self._get_qualified_field_name(parent=parent["/Parent"], visited=visited) + "." + parent.get("/T", "")
        let kwargs = PyDict::new(py);
        kwargs.set_item("parent", parent.get_item(&parent_key)?)?;
        kwargs.set_item("visited", &visited)?;
        let prefix = slf.call_method("_get_qualified_field_name", (), Some(&kwargs))?;
        let separator = py_str(py, ".");
        let suffix = dict_get(parent, &py_str(py, "/T"), &py_str(py, ""))?;
        return py_add(&py_add(&prefix, &separator)?, &suffix);
    }
    dict_get(parent, &py_str(py, "/T"), &py_str(py, ""))
}

// @pypdf: pypdf._doc_common.PdfDocCommon._normal_appearance
#[pyfunction]
fn doccommon_normal_appearance<'py>(
    py: Python<'py>,
    appearance: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let appearance = appearance.call_method0("get_object")?;
    if !is_generic_instance(py, &appearance, "DictionaryObject")? {
        return Err(pdf_read_error(
            py,
            &format!(
                "Expected appearance dictionary, got {}",
                repr_of(&appearance)?
            ),
        ));
    }
    let normal = dict_get_one(&appearance, &py_str(py, "/N"))?;
    let normal = if normal.is_none() {
        normal
    } else {
        normal.call_method0("get_object")?
    };
    if !is_generic_instance(py, &normal, "DictionaryObject")? {
        return Err(pdf_read_error(
            py,
            &format!(
                "Expected /N appearance dictionary, got {}",
                repr_of(&normal)?
            ),
        ));
    }
    Ok(normal)
}

// @pypdf: pypdf._doc_common.PdfDocCommon._build_field
#[pyfunction]
#[pyo3(signature = (slf, field, retval, fileobj, field_attributes, visited, depth, traversal_state))]
#[allow(clippy::too_many_arguments)]
fn doccommon_build_field<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    field: &Bound<'py, PyAny>,
    retval: &Bound<'py, PyAny>,
    fileobj: &Bound<'py, PyAny>,
    field_attributes: &Bound<'py, PyAny>,
    visited: &Bound<'py, PyAny>,
    depth: &Bound<'py, PyAny>,
    traversal_state: &Bound<'py, PyAny>,
) -> PyResult<()> {
    if !is_generic_instance(py, field, "DictionaryObject")? {
        log_warning_with(
            py,
            "Form field is not a dictionary: %(field)s",
            &[("field", field)],
        )?;
        return Ok(());
    }
    let has_t = field.contains(py_str(py, "/T"))?;
    let has_tm = field.contains(py_str(py, "/TM"))?;
    if !has_t && !has_tm {
        return Ok(());
    }

    // key = self._get_qualified_field_name(parent=field)
    let kwargs = PyDict::new(py);
    kwargs.set_item("parent", field)?;
    let key = slf.call_method("_get_qualified_field_name", (), Some(&kwargs))?;

    if fileobj.is_truthy()? {
        slf.call_method1("_write_field", (fileobj, field, field_attributes))?;
        fileobj.call_method1("write", ("\n",))?;
    }

    let field_class = generic_class(py, "Field")?;
    retval.set_item(&key, field_class.call1((field,))?)?;

    let field_attributes_class = constants_class(py, "FieldDictionaryAttributes")?;
    let ft_key = field_attributes_class.getattr("FT")?;
    let obj = retval
        .get_item(&key)?
        .getattr("indirect_reference")?
        .call_method0("get_object")?;
    let states_key = name_object(py, &py_str(py, "/_States_"))?;

    if dict_get(&obj, &ft_key, &py_str(py, ""))?.eq("/Ch")?
        && dict_get_one(
            &obj,
            &name_object(py, &field_attributes_class.getattr("Opt")?)?,
        )?
        .is_truthy()?
    {
        let opt_key = name_object(py, &field_attributes_class.getattr("Opt")?)?;
        retval
            .get_item(&key)?
            .set_item(&states_key, obj.get_item(&opt_key)?)?;
    }

    let ap_key = py_str(py, "/AP");
    let ff_key = field_attributes_class.getattr("Ff")?;
    let is_button = dict_get(&obj, &ft_key, &py_str(py, ""))?.eq("/Btn")?;
    if is_button && obj.contains(&ap_key)? {
        // Checkbox
        let normal = slf.call_method1("_normal_appearance", (obj.get_item(&ap_key)?,))?;
        let normal_keys = list_items(&normal.call_method0("keys")?)?;
        let states = array_from(py, normal_keys)?;
        retval.get_item(&key)?.set_item(&states_key, &states)?;
        let current_states = retval.get_item(&key)?.get_item(&states_key)?;
        if !current_states.contains("/Off")? {
            current_states.call_method1("append", (name_object(py, &py_str(py, "/Off"))?,))?;
        }
    } else if is_button
        && py_bitand(
            py,
            &dict_get(&obj, &ff_key, &py_int(py, 0))?,
            &field_attributes_class.getattr("FfBits")?.getattr("Radio")?,
        )?
        .ne(0_i64)?
    {
        // Radio buttons
        let states = PyList::empty(py);
        retval
            .get_item(&key)?
            .set_item(&states_key, array_from(py, vec![])?)?;
        let kids = dict_get(&obj, &py_str(py, "/Kids"), &dictionary_object(py)?)?;
        let mut iterator = kids.try_iter()?;
        while let Some(kid) = next_item(&mut iterator)? {
            let kid = kid.call_method0("get_object")?;
            if !kid.contains(&ap_key)? {
                return Err(pdf_read_error(
                    py,
                    &format!("Button field kid missing /AP: {}", repr_of(&kid)?),
                ));
            }
            let normal = slf.call_method1("_normal_appearance", (kid.get_item(&ap_key)?,))?;
            let mut normal_keys = normal.call_method0("keys")?.try_iter()?;
            while let Some(state) = next_item(&mut normal_keys)? {
                if !states.contains(&state)? {
                    states.append(state)?;
                }
            }
            let states_array = array_from(py, list_items(&states)?)?;
            retval
                .get_item(&key)?
                .set_item(&states_key, &states_array)?;
        }
        if py_bitand(
            py,
            &dict_get(&obj, &ff_key, &py_int(py, 0))?,
            &field_attributes_class
                .getattr("FfBits")?
                .getattr("NoToggleToOff")?,
        )?
        .ne(0_i64)?
        {
            let states_array = retval.get_item(&key)?.get_item(&states_key)?;
            if states_array.contains("/Off")? {
                let index = states_array.call_method1("index", ("/Off",))?;
                states_array.del_item(&index)?;
            }
        }
    }

    // at last for order
    let kwargs = PyDict::new(py);
    kwargs.set_item("tree", field)?;
    kwargs.set_item("retval", retval)?;
    kwargs.set_item("fileobj", fileobj)?;
    kwargs.set_item("visited", visited)?;
    kwargs.set_item("depth", depth)?;
    kwargs.set_item("traversal_state", traversal_state)?;
    slf.call_method("_check_kids", (), Some(&kwargs))?;
    Ok(())
}

// @pypdf: pypdf._doc_common.PdfDocCommon._check_kids
#[pyfunction]
#[pyo3(signature = (slf, tree, retval, fileobj, visited, depth, traversal_state))]
#[allow(clippy::too_many_arguments)]
fn doccommon_check_kids<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    tree: &Bound<'py, PyAny>,
    retval: &Bound<'py, PyAny>,
    fileobj: &Bound<'py, PyAny>,
    visited: &Bound<'py, PyAny>,
    depth: &Bound<'py, PyAny>,
    traversal_state: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let tree_id = py_id(py, tree)?;
    if visited.contains(&tree_id)? {
        let kwargs = PyDict::new(py);
        kwargs.set_item("parent", tree)?;
        let field_name = slf.call_method("_get_qualified_field_name", (), Some(&kwargs))?;
        log_warning_with(
            py,
            "%(field_name)s already parsed",
            &[("field_name", &field_name)],
        )?;
        return Ok(());
    }
    visited.call_method1("add", (tree_id,))?;
    let kids_key = constants_class(py, "PagesAttributes")?.getattr("KIDS")?;
    if tree.contains(&kids_key)? {
        // recurse down the tree
        let mut iterator = tree.get_item(&kids_key)?.try_iter()?;
        while let Some(kid) = next_item(&mut iterator)? {
            let kid = kid.call_method0("get_object")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("tree", &kid)?;
            kwargs.set_item("retval", retval)?;
            kwargs.set_item("fileobj", fileobj)?;
            kwargs.set_item("_visited", visited)?;
            kwargs.set_item("_depth", py_add(depth, &py_int(py, 1))?)?;
            kwargs.set_item("_traversal_state", traversal_state)?;
            slf.call_method("get_fields", (), Some(&kwargs))?;
        }
    }
    Ok(())
}

/// One `for attr in field_attributes_tuple:` iteration of `_write_field`.
///
/// Returns the line to write, or `None` when upstream would skip the attribute
/// (field attribute is N/A or unknown: the enclosing `except KeyError: pass`).
fn field_report_line<'py>(
    py: Python<'py>,
    field: &Bound<'py, PyAny>,
    attribute: &Bound<'py, PyAny>,
    attribute_name: &Bound<'py, PyAny>,
    field_attributes_class: &Bound<'py, PyAny>,
) -> PyResult<Option<String>> {
    // `field[attr]`, with upstream's enclosing `except KeyError: pass`.
    let field_value = match field.get_item(attribute) {
        Ok(value) => Some(value),
        Err(error) if error.is_instance_of::<PyKeyError>(py) => None,
        Err(error) => return Err(error),
    };

    if attribute.eq(&field_attributes_class.getattr("FT")?)? {
        // Make the field type value clearer
        let Some(value) = field_value else {
            return Ok(None);
        };
        let types = PyDict::new(py);
        types.set_item("/Btn", "Button")?;
        types.set_item("/Tx", "Text")?;
        types.set_item("/Ch", "Choice")?;
        types.set_item("/Sig", "Signature")?;
        if let Some(type_name) = types.get_item(&value)? {
            return Ok(Some(format!(
                "{}: {}\n",
                str_of(attribute_name)?,
                str_of(&type_name)?
            )));
        }
        return Ok(None);
    }
    if attribute.eq(&field_attributes_class.getattr("Parent")?)? {
        // Let's just write the name of the parent
        let Some(parent) = field_value else {
            return Ok(None);
        };
        let name = match parent.get_item(&field_attributes_class.getattr("TM")?) {
            Ok(name) => name,
            Err(error) if error.is_instance_of::<PyKeyError>(py) => {
                match parent.get_item(&field_attributes_class.getattr("T")?) {
                    Ok(name) => name,
                    Err(error) if error.is_instance_of::<PyKeyError>(py) => return Ok(None),
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };
        return Ok(Some(format!(
            "{}: {}\n",
            str_of(attribute_name)?,
            str_of(&name)?
        )));
    }
    let Some(value) = field_value else {
        return Ok(None);
    };
    Ok(Some(format!(
        "{}: {}\n",
        str_of(attribute_name)?,
        str_of(&value)?
    )))
}

// @pypdf: pypdf._doc_common.PdfDocCommon._write_field
#[pyfunction]
fn doccommon_write_field<'py>(
    py: Python<'py>,
    fileobj: &Bound<'py, PyAny>,
    field: &Bound<'py, PyAny>,
    field_attributes: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let field_attributes_class = constants_class(py, "FieldDictionaryAttributes")?;
    let checkbox_attributes_class = constants_class(py, "CheckboxRadioButtonAttributes")?;
    let mut field_attributes_tuple = field_attributes_class.call_method0("attributes")?;
    field_attributes_tuple = py_add(
        &field_attributes_tuple,
        &checkbox_attributes_class.call_method0("attributes")?,
    )?;

    let kids = field_attributes_class.getattr("Kids")?;
    let additional_actions = field_attributes_class.getattr("AA")?;
    let mut iterator = field_attributes_tuple.try_iter()?;
    while let Some(attribute) = next_item(&mut iterator)? {
        if attribute.eq(&kids)? || attribute.eq(&additional_actions)? {
            continue;
        }
        // `attr_name = field_attributes[attr]` is *outside* the `try` upstream,
        // so a missing entry propagates.
        let attribute_name = field_attributes.get_item(&attribute)?;
        if let Some(line) = field_report_line(
            py,
            field,
            &attribute,
            &attribute_name,
            &field_attributes_class,
        )? {
            fileobj.call_method1("write", (line,))?;
        }
    }
    Ok(())
}

/// `indexed_key(k, fields)` nested helper of `get_form_text_fields`.
fn indexed_key<'py>(
    py: Python<'py>,
    key: &Bound<'py, PyAny>,
    fields: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    if !fields.contains(key)? {
        return Ok(key.clone());
    }
    let prefix = py_add(key, &py_str(py, "."))?;
    let mut count: i64 = 0;
    let mut iterator = fields.call_method0("keys")?.try_iter()?;
    while let Some(other) = next_item(&mut iterator)? {
        if other
            .call_method1("startswith", (prefix.clone(),))?
            .is_truthy()?
        {
            count += 1;
        }
    }
    py_add(&prefix, &py_str(py, &(count + 2).to_string()))
}

// @pypdf: pypdf._doc_common.PdfDocCommon.get_form_text_fields
#[pyfunction]
fn doccommon_get_form_text_fields<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    full_qualified_name: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // Retrieve document form fields
    let form_fields = slf.call_method0("get_fields")?;
    if form_fields.is_none() {
        return Ok(PyDict::new(py).into_any());
    }
    let result = PyDict::new(py);
    let mut iterator = form_fields.call_method0("items")?.try_iter()?;
    while let Some(pair) = next_item(&mut iterator)? {
        let field = pair.get_item(0)?;
        let value = pair.get_item(1)?;
        if dict_get_one(&value, &py_str(py, "/FT"))?.eq("/Tx")? {
            if full_qualified_name.is_truthy()? {
                result.set_item(&field, dict_get_one(&value, &py_str(py, "/V"))?)?;
            } else {
                let key = dict_get(&value, &py_str(py, "/T"), &field)?;
                let indexed = indexed_key(py, &key, result.as_any())?;
                result.set_item(indexed, dict_get_one(&value, &py_str(py, "/V"))?)?;
            }
        }
    }
    Ok(result.into_any())
}

// @pypdf: pypdf._doc_common.PdfDocCommon.get_pages_showing_field
#[pyfunction]
fn doccommon_get_pages_showing_field<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    field: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // to cope with all types
    let field = match field
        .getattr("indirect_reference")
        .and_then(|reference| reference.call_method0("get_object"))
    {
        Ok(field) => field,
        Err(error) => return Err(value_error_from(py, "Field type is invalid", error)),
    };
    let kwargs = PyDict::new(py);
    kwargs.set_item("key", py_str(py, "/FT"))?;
    kwargs.set_item("default", py_none(py))?;
    if is_null_or_none(py, &field.call_method("get_inherited", (), Some(&kwargs))?)? {
        return Err(message_exception(py, "ValueError", "Field is not valid"));
    }

    let subtype_key = py_str(py, "/Subtype");
    let annotation_key = py_str(py, "/Annots");
    let parent_key = py_str(py, "/P");
    let ret = PyList::empty(py);
    if dict_get(&field, &subtype_key, &py_str(py, ""))?.eq("/Widget")? {
        if field.contains(&parent_key)? {
            ret.append(field.get_item(&parent_key)?.call_method0("get_object")?)?;
        } else {
            let field_reference = field.getattr("indirect_reference")?;
            let pages = slf.getattr("pages")?;
            let mut iterator = pages.try_iter()?;
            while let Some(page) = next_item(&mut iterator)? {
                if dict_get(&page, &annotation_key, &py_str(py, ""))?.contains(&field_reference)? {
                    ret.append(page)?;
                }
            }
        }
    } else {
        let kids = dict_get(&field, &py_str(py, "/Kids"), &py_tuple(py, vec![])?)?;
        let mut iterator = kids.try_iter()?;
        while let Some(kid) = next_item(&mut iterator)? {
            let kid = kid.call_method0("get_object")?;
            // Kid that is just a widget, not a field
            if dict_get(&kid, &subtype_key, &py_str(py, ""))?.eq("/Widget")?
                && !kid.contains(py_str(py, "/T"))?
            {
                if kid.contains(&parent_key)? {
                    ret.append(kid.get_item(&parent_key)?.call_method0("get_object")?)?;
                } else {
                    let kid_reference = kid.getattr("indirect_reference")?;
                    let pages = slf.getattr("pages")?;
                    let mut page_iterator = pages.try_iter()?;
                    while let Some(page) = next_item(&mut page_iterator)? {
                        if dict_get(&page, &annotation_key, &py_str(py, ""))?
                            .contains(&kid_reference)?
                        {
                            ret.append(page)?;
                        }
                    }
                }
            }
        }
    }

    let page_class = py.import("pypdf._page")?.getattr("PageObject")?;
    let result = PyList::empty(py);
    let mut iterator = ret.try_iter()?;
    while let Some(item) = next_item(&mut iterator)? {
        if item.is_instance(&page_class)? {
            result.append(item)?;
        } else {
            let reference = item.getattr("indirect_reference")?;
            let page_number = slf.call_method1("_get_page_number_by_indirect", (reference,))?;
            result.append(slf.getattr("pages")?.get_item(page_number)?)?;
        }
    }
    Ok(result.into_any())
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(doccommon_get_fields, module)?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_get_qualified_field_name,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(doccommon_normal_appearance, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_build_field, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_check_kids, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_write_field, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_get_form_text_fields, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_get_pages_showing_field, module)?)?;
    Ok(())
}
