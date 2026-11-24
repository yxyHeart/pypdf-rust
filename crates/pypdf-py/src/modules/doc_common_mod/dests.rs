//! Named-destination logic of `pypdf._doc_common` (upstream pypdf 7.1.1):
//! `named_destinations`, `get_named_dest_root` and `_get_named_destinations`.

use pyo3::exceptions::PyIndexError;

use super::*;

// @pypdf: pypdf._doc_common.PdfDocCommon.named_destinations
#[pyfunction]
fn doccommon_named_destinations<'py>(slf: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    slf.call_method0("_get_named_destinations")
}

// @pypdf: pypdf._doc_common.PdfDocCommon.get_named_dest_root
#[pyfunction]
fn doccommon_get_named_dest_root<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let catalog_attributes = constants_class(py, "CatalogAttributes")?;
    let names_key = catalog_attributes.getattr("NAMES")?;
    let dests_key = catalog_attributes.getattr("DESTS")?;
    let root = slf.getattr("root_object")?;

    let mut named_dest = array_object(py)?;
    if root.contains(&names_key)?
        && is_generic_instance(py, &root.get_item(&names_key)?, "DictionaryObject")?
    {
        let names = root.get_item(&names_key)?;
        if names.contains(&dests_key)?
            && is_generic_instance(py, &names.get_item(&dests_key)?, "DictionaryObject")?
        {
            // §3.6.3 Name Dictionary (PDF spec 1.7)
            let dests = names.get_item(&dests_key)?;
            if dests.contains(&names_key)? {
                // §7.9.6, entries in a name tree node dictionary
                named_dest = dests.get_item(&names_key)?;
            } else {
                named_dest = array_object(py)?;
                dests.set_item(&name_object(py, &names_key)?, named_dest.clone())?;
            }
        } else if slf.hasattr("_add_object")? {
            let dests = dictionary_object(py)?;
            let dests_reference = slf.call_method1("_add_object", (dests.clone(),))?;
            names.set_item(&name_object(py, &dests_key)?, dests_reference)?;
            dests.set_item(&name_object(py, &names_key)?, named_dest.clone())?;
        }
    } else if slf.hasattr("_add_object")? {
        let names = dictionary_object(py)?;
        let names_reference = slf.call_method1("_add_object", (names.clone(),))?;
        root.set_item(&name_object(py, &names_key)?, names_reference)?;
        let dests = dictionary_object(py)?;
        let dests_reference = slf.call_method1("_add_object", (dests.clone(),))?;
        names.set_item(&name_object(py, &dests_key)?, dests_reference)?;
        dests.set_item(&name_object(py, &names_key)?, named_dest.clone())?;
    }
    Ok(named_dest)
}

/// `self._get_named_destinations(**kwargs)` used by the recursive visit.
fn call_named_destinations<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    tree: &Bound<'py, PyAny>,
    retval: &Bound<'py, PyAny>,
    visited: &Bound<'py, PyAny>,
    depth: &Bound<'py, PyAny>,
    traversal_state: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("tree", tree)?;
    kwargs.set_item("retval", retval)?;
    kwargs.set_item("visited", visited)?;
    kwargs.set_item("depth", depth)?;
    kwargs.set_item("traversal_state", traversal_state)?;
    slf.call_method("_get_named_destinations", (), Some(&kwargs))?;
    Ok(())
}

// @pypdf: pypdf._doc_common.PdfDocCommon._get_named_destinations
#[pyfunction]
#[pyo3(signature = (slf, tree, retval, visited, depth, traversal_state))]
fn doccommon_get_named_destinations<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    tree: &Bound<'py, PyAny>,
    retval: &Bound<'py, PyAny>,
    visited: &Bound<'py, PyAny>,
    depth: &Bound<'py, PyAny>,
    traversal_state: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut traversal_state = traversal_state.clone();
    if traversal_state.is_none() {
        traversal_state = utils_attr(py, "_TraversalState")?.call0()?;
    }
    let mut visited = visited.clone();
    if visited.is_none() {
        visited = PySet::empty(py)?.into_any();
    }
    let mut retval = retval.clone();
    let mut tree = tree.clone();

    if retval.is_none() {
        retval = PyDict::new(py).into_any();
        let catalog = slf.getattr("root_object")?;
        let catalog_attributes = constants_class(py, "CatalogAttributes")?;
        let dests_key = catalog_attributes.getattr("DESTS")?;
        let names_key = catalog_attributes.getattr("NAMES")?;

        // get the name tree
        let mut candidate = py_none(py);
        if catalog.contains(&dests_key)? {
            candidate = catalog.get_item(&dests_key)?.call_method0("get_object")?;
        } else if catalog.contains(&names_key)? {
            let names = catalog.get_item(&names_key)?.call_method0("get_object")?;
            if is_generic_instance(py, &names, "DictionaryObject")? && names.contains(&dests_key)? {
                candidate = names.get_item(&dests_key)?.call_method0("get_object")?;
            }
        }
        if !candidate.is_none() && !is_generic_instance(py, &candidate, "DictionaryObject")? {
            log_warning_with(
                py,
                "Destination tree is not a dictionary: %(tree)s",
                &[("tree", &candidate)],
            )?;
            return Ok(retval);
        }
        if !candidate.is_none() {
            tree = candidate;
        }
    }

    if is_null_or_none(py, &tree)? {
        return Ok(retval);
    }
    if tree.is_none() {
        // assert tree is not None, "mypy"
        return Err(empty_exception(py, "AssertionError"));
    }

    let configuration = configuration(py)?;
    let maximum_depth = configuration.getattr("named_destinations_maximum_depth")?;
    if depth.gt(&maximum_depth)? {
        return Err(limit_reached(
            py,
            &format!(
                "Maximum named destination depth exceeded: {} > {}.",
                str_of(depth)?,
                str_of(&maximum_depth)?
            ),
        ));
    }

    let tree_id = py_id(py, &tree)?;
    if visited.contains(&tree_id)? {
        log_warning(py, "Detected cycle in destination tree.", None)?;
        return Ok(retval);
    }
    visited.call_method1("add", (tree_id,))?;

    let count = py_add(&traversal_state.getattr("entry_count")?, &py_int(py, 1))?;
    traversal_state.setattr("entry_count", count.clone())?;
    let maximum_entries = configuration.getattr("named_destinations_maximum_entries")?;
    if count.gt(&maximum_entries)? {
        return Err(limit_reached(
            py,
            &format!(
                "Maximum named destination entry limit exceeded: {} > {}.",
                str_of(&count)?,
                str_of(&maximum_entries)?
            ),
        ));
    }

    let kids_key = constants_class(py, "PagesAttributes")?.getattr("KIDS")?;
    let names_key = constants_class(py, "CatalogAttributes")?.getattr("NAMES")?;
    let dests_key = py_str(py, "/D");

    if tree.contains(&kids_key)? {
        let kids = tree.get_item(&kids_key)?.call_method0("get_object")?;
        if !is_generic_instance(py, &kids, "ArrayObject")? {
            log_warning_with(
                py,
                "Destination tree kids are not an array: %(kids)s",
                &[("kids", &kids)],
            )?;
            return Ok(retval);
        }
        // recurse down the tree
        let mut iterator = kids.try_iter()?;
        while let Some(kid) = next_item(&mut iterator)? {
            let kid = kid.call_method0("get_object")?;
            call_named_destinations(
                py,
                slf,
                &kid,
                &retval,
                &visited,
                &py_add(depth, &py_int(py, 1))?,
                &traversal_state,
            )?;
        }
    } else if tree.contains(&names_key)? {
        // §7.9.6, entries in a name tree node dictionary
        // (/Kids and /Names are exclusives)
        let names = tree.get_item(&names_key)?.call_method0("get_object")?;
        if !is_generic_instance(py, &names, "ArrayObject")? {
            log_warning_with(
                py,
                "Destination tree names are not an array: %(names)s",
                &[("names", &names)],
            )?;
            return Ok(retval);
        }
        let mut index: usize = 0;
        while index < names.len()? {
            let key = names.get_item(index)?.call_method0("get_object")?;
            index += 1;
            if !(key.is_instance(&builtins(py)?.getattr("bytes")?)?
                || key.is_instance(&builtins(py)?.getattr("str")?)?)
            {
                continue;
            }
            let value = match names.get_item(index) {
                Ok(value) => value.call_method0("get_object")?,
                Err(error) if error.is_instance_of::<PyIndexError>(py) => break,
                Err(error) => return Err(error),
            };
            index += 1;
            let value = if is_generic_instance(py, &value, "DictionaryObject")? {
                if value.contains(&dests_key)? {
                    value.get_item(&dests_key)?
                } else {
                    continue;
                }
            } else {
                value
            };
            let destination = slf.call_method1("_build_destination", (key.clone(), value))?;
            if !destination.is_none() {
                retval.set_item(
                    destination.get_item(py_str(py, "/Title"))?,
                    destination.clone(),
                )?;
                // Remain backwards-compatible.
                let key_string = builtins(py)?.getattr("str")?.call1((key,))?;
                retval.set_item(key_string, destination)?;
            }
        }
    } else {
        // case where /Dests is in the document's catalog dictionary
        // (PDF 1.7 specs, §2 about PDF 1.1)
        let mut iterator = tree.call_method0("items")?.try_iter()?;
        while let Some(pair) = next_item(&mut iterator)? {
            let key = pair.get_item(0)?;
            let value = pair.get_item(1)?;
            let mut value = value.call_method0("get_object")?;
            if is_generic_instance(py, &value, "DictionaryObject")? {
                if value.contains(&dests_key)? {
                    value = value.get_item(&dests_key)?.call_method0("get_object")?;
                } else {
                    continue;
                }
            }
            let destination = slf.call_method1("_build_destination", (key.clone(), value))?;
            if !destination.is_none() {
                retval.set_item(key, destination)?;
            }
        }
    }
    Ok(retval)
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(doccommon_named_destinations, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_get_named_dest_root, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_get_named_destinations, module)?)?;
    Ok(())
}
