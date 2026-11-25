//! Page-tree logic of `pypdf._doc_common` (upstream pypdf 7.1.1):
//! `get_num_pages`, `get_page`, `_get_page_in_node`, `pages`, `page_labels`,
//! `page_layout`, `page_mode`, the explicit-stack `_flatten` traversal and
//! `remove_page`.

use super::*;

// @pypdf: pypdf._doc_common.PdfDocCommon.get_num_pages
#[pyfunction]
fn doccommon_get_num_pages<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // The page tree cannot be walked while the document is still encrypted; the
    // PDF file's page count is used in this case. Once the document has been
    // decrypted (or if it is not encrypted at all), the flattened page tree is
    // used instead, as the /Count value might be wrong.
    let encryption = slf.getattr("_encryption")?;
    if slf.getattr("is_encrypted")?.is_truthy()? {
        let use_declared_count = if encryption.is_none() {
            true
        } else {
            !encryption.call_method0("is_decrypted")?.is_truthy()?
        };
        if use_declared_count {
            let root = slf.getattr("root_object")?;
            return root.get_item("/Pages")?.get_item("/Count");
        }
    }
    if slf.getattr("flattened_pages")?.is_none() {
        slf.call_method1("_flatten", (slf.getattr("_readonly")?,))?;
    }
    let flattened_pages = slf.getattr("flattened_pages")?;
    if flattened_pages.is_none() {
        return Err(assertion(py, None));
    }
    py_len(py, &flattened_pages)
}

// @pypdf: pypdf._doc_common.PdfDocCommon.get_page
#[pyfunction]
fn doccommon_get_page<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    page_number: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    if slf.getattr("flattened_pages")?.is_none() {
        slf.call_method1("_flatten", (slf.getattr("_readonly")?,))?;
    }
    let flattened_pages = slf.getattr("flattened_pages")?;
    if flattened_pages.is_none() {
        return Err(assertion(py, Some("mypy")));
    }
    flattened_pages.get_item(page_number)
}

/// The nested `recursive_call(_node, mi)` of `_get_page_in_node`.
///
/// Returns `(found node or None, mi)` where `mi` is the number of pages already
/// accounted for (kept as a Python object: upstream does arithmetic on whatever
/// `/Count` holds).
fn recursive_page_lookup<'py>(
    py: Python<'py>,
    node: &Bound<'py, PyAny>,
    top: &Bound<'py, PyAny>,
    page_number: &Bound<'py, PyAny>,
    visited: &Bound<'py, PyAny>,
    mi: &Bound<'py, PyAny>,
) -> PyResult<(Option<Bound<'py, PyAny>>, Bound<'py, PyAny>)> {
    let node_id = py_id(py, node)?;
    if visited.contains(&node_id)? {
        return Err(limit_reached(
            py,
            "Detected cycle in /Pages hierarchy when retrieving page.",
        ));
    }
    visited.call_method1("add", (node_id,))?;

    let ma = dict_get(node, &py_str(py, "/Count"), &py_int(py, 1))?;
    // default 1 for /Page types
    if dict_get_one(node, &py_str(py, "/Type"))?.eq("/Page")? {
        if page_number.eq(mi)? {
            return Ok((Some(node.clone()), py_int(py, -1)));
        }
        return Ok((None, py_add(mi, &py_int(py, 1))?));
    }
    if py_sub(page_number, mi)?.ge(&ma)? {
        // not in nodes below
        if node.eq(top)? {
            return Ok((Some(top.clone()), py_int(py, -1)));
        }
        return Ok((None, py_add(mi, &ma)?));
    }
    let kids = node.get_item(py_str(py, "/Kids"))?;
    let mut index: usize = 0;
    let mut current = mi.clone();
    let mut iterator = kids.try_iter()?;
    while let Some(kid) = next_item(&mut iterator)? {
        let kid = kid.call_method0("get_object")?;
        let (found, returned) =
            recursive_page_lookup(py, &kid, top, page_number, visited, &current)?;
        if let Some(found) = found {
            if returned.lt(0_i64)? {
                // page has just been found ... just below!
                return Ok((Some(node.clone()), py_int(py, index as i64)));
            }
            // ... at lower levels
            return Ok((Some(found), returned));
        }
        current = returned;
        index += 1;
    }
    Err(pypdf_error(py, "Unexpectedly cannot find the node."))
}

// @pypdf: pypdf._doc_common.PdfDocCommon._get_page_in_node
#[pyfunction]
fn doccommon_get_page_in_node<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    page_number: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let top = slf.getattr("root_object")?.get_item("/Pages")?;
    let visited = PySet::empty(py)?.into_any();
    let (node, index) =
        recursive_page_lookup(py, &top, &top, page_number, &visited, &py_int(py, 0))?;
    let Some(node) = node else {
        return Err(assertion(py, Some("mypy")));
    };
    if !is_generic_instance(py, &node, "DictionaryObject")? {
        return Err(assertion(py, Some("mypy")));
    }
    py_tuple(py, vec![node, index])
}

// @pypdf: pypdf._doc_common.PdfDocCommon.pages
#[pyfunction]
fn doccommon_pages<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let virtual_list = crate::pyutil::cached_module(py, "pypdf._page")?.getattr("_VirtualList")?;
    virtual_list.call1((slf.getattr("get_num_pages")?, slf.getattr("get_page")?))
}

// @pypdf: pypdf._doc_common.PdfDocCommon.page_labels
#[pyfunction]
fn doccommon_page_labels<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let index2labels = crate::pyutil::cached_module(py, "pypdf._page_labels")?.getattr("index2labels")?;
    let pages = slf.getattr("pages")?;
    index2labels.call1((slf, py_len(py, &pages)?))
}

// @pypdf: pypdf._doc_common.PdfDocCommon.page_layout
#[pyfunction]
fn doccommon_page_layout<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let key = constants_class(py, "CatalogAttributes")?.getattr("PAGE_LAYOUT")?;
    match slf.getattr("root_object")?.get_item(&key) {
        Ok(value) => Ok(value),
        Err(error) if error.is_instance_of::<pyo3::exceptions::PyKeyError>(py) => Ok(py_none(py)),
        Err(error) => Err(error),
    }
}

// @pypdf: pypdf._doc_common.PdfDocCommon.page_mode
#[pyfunction]
fn doccommon_page_mode<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    match slf.getattr("root_object")?.get_item("/PageMode") {
        Ok(value) => Ok(value),
        Err(error) if error.is_instance_of::<pyo3::exceptions::PyKeyError>(py) => Ok(py_none(py)),
        Err(error) => Err(error),
    }
}

/// Construct `_PageTreeItem(node=..., inherit=..., indirect_reference=..., depth=...)`.
fn page_tree_item<'py>(
    py: Python<'py>,
    node: &Bound<'py, PyAny>,
    inherit: &Bound<'py, PyAny>,
    indirect_reference: &Bound<'py, PyAny>,
    depth: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("node", node)?;
    kwargs.set_item("inherit", inherit)?;
    kwargs.set_item("indirect_reference", indirect_reference)?;
    kwargs.set_item("depth", depth)?;
    shim_class(py, "_PageTreeItem")?.call((), Some(&kwargs))
}

// @pypdf: pypdf._doc_common.PdfDocCommon._enter_page_tree_node
#[pyfunction]
fn doccommon_enter_page_tree_node<'py>(
    py: Python<'py>,
    _slf: &Bound<'py, PyAny>,
    item: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let pages = item.getattr("node")?;
    // Copied before being modified, as the kids of the enclosing node share it.
    let inherit = item.getattr("inherit")?.call_method0("copy")?;
    let mut attributes = shim_attr(py, "_INHERITABLE_PAGE_ATTRIBUTES")?.try_iter()?;
    while let Some(attribute) = next_item(&mut attributes)? {
        if pages.contains(&attribute)? {
            inherit.set_item(&attribute, pages.get_item(&attribute)?)?;
        }
    }

    let kids =
        dict_get(&pages, &py_str(py, "/Kids"), &array_object(py)?)?.call_method0("get_object")?;
    let kids = if is_generic_instance(py, &kids, "NullObject")? {
        array_object(py)?
    } else if !is_generic_instance(py, &kids, "ArrayObject")? {
        return Err(pdf_read_error(
            py,
            &format!("Expected /Kids to be an array, got {}.", class_name(&kids)?),
        ));
    } else {
        kids
    };

    let replaced = page_tree_item(
        py,
        &pages,
        &inherit,
        &item.getattr("indirect_reference")?,
        &item.getattr("depth")?,
    )?;
    py_tuple(py, vec![replaced, kids.try_iter()?.into_any()])
}

// @pypdf: pypdf._doc_common.PdfDocCommon._pop_next_page_tree_kid
#[pyfunction]
fn doccommon_pop_next_page_tree_kid<'py>(
    py: Python<'py>,
    _slf: &Bound<'py, PyAny>,
    stack: &Bound<'py, PyAny>,
    ancestor_ids: &Bound<'py, PyAny>,
    traversal_state: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let configuration = configuration(py)?;
    let maximum_entries = configuration.getattr("page_tree_maximum_entries")?;
    loop {
        let length = stack.len()?;
        if length == 0 {
            return Ok(py_none(py));
        }
        let frame = stack.get_item(length - 1)?;
        let parent = frame.get_item(0)?;
        let parent_node = parent.getattr("node")?;
        let pages_reference = object_attr(py, &parent_node, "indirect_reference")?;
        // `kids` is the very iterator stored in the frame, so `iter()` returns it
        // unchanged and the traversal state is preserved.
        let mut kids = frame.get_item(1)?.try_iter()?;
        let mut next_item_value = None;
        while let Some(page) = next_item(&mut kids)? {
            let page_reference = object_attr(py, &page, "indirect_reference")?;
            if page_reference.eq(&pages_reference)? {
                return Err(pdf_read_error(py, "Detected cyclic page references."));
            }

            let obj = page.call_method0("get_object")?;
            if !is_generic_instance(py, &obj, "DictionaryObject")? {
                if !is_null_or_none(py, &obj)? {
                    log_warning_with(
                        py,
                        "Ignoring page tree entry that is not a dictionary: %(entry)s",
                        &[("entry", &obj)],
                    )?;
                }
                // damaged file may have invalid child in /Pages
                continue;
            }
            if obj.len()? == 0 {
                // An empty dictionary would otherwise be read as a blank page.
                continue;
            }
            let obj_id = py_id(py, &obj)?;
            if ancestor_ids.contains(&obj_id)? {
                return Err(pdf_read_error(py, "Detected cyclic page references."));
            }
            let count = py_add(&traversal_state.getattr("entry_count")?, &py_int(py, 1))?;
            traversal_state.setattr("entry_count", count.clone())?;
            if count.gt(&maximum_entries)? {
                return Err(limit_reached(
                    py,
                    &format!(
                        "Maximum page tree entry limit reached: {} > {}.",
                        str_of(&count)?,
                        str_of(&maximum_entries)?
                    ),
                ));
            }
            let indirect_reference = if is_generic_instance(py, &page, "IndirectObject")? {
                page.clone()
            } else {
                py_none(py)
            };
            next_item_value = Some(page_tree_item(
                py,
                &obj,
                &parent.getattr("inherit")?,
                &indirect_reference,
                &py_add(&parent.getattr("depth")?, &py_int(py, 1))?,
            )?);
            break;
        }
        if let Some(value) = next_item_value {
            return Ok(value);
        }
        stack.call_method0("pop")?;
        let parent_id = py_id(py, &parent_node)?;
        ancestor_ids.call_method1("discard", (parent_id,))?;
    }
}

// @pypdf: pypdf._doc_common.PdfDocCommon._flatten_leaf_page
#[pyfunction]
#[pyo3(signature = (slf, item, list_only, remaining_page_entries))]
fn doccommon_flatten_leaf_page<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    item: &Bound<'py, PyAny>,
    list_only: &Bound<'py, PyAny>,
    remaining_page_entries: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let node = item.getattr("node")?;
    let indirect_reference = item.getattr("indirect_reference")?;
    let list_only = list_only.is_truthy()?;
    let inherit = item.getattr("inherit")?;

    // Indirect PageObjects are populated by the constructor even when
    // list_only=True. Inline dictionaries are copied only when list_only=False.
    let mut entry_count: i64 = if !indirect_reference.is_none() || !list_only {
        node.len()? as i64
    } else {
        0
    };
    // Inherited attributes are inserted only when missing from the page.
    let mut inherited = inherit.call_method0("keys")?.try_iter()?;
    while let Some(attribute) = next_item(&mut inherited)? {
        if !node.contains(&attribute)? {
            entry_count += 1;
        }
    }
    let remaining = py_sub(
        &remaining_page_entries.getattr("entry_count")?,
        &py_int(py, entry_count),
    )?;
    remaining_page_entries.setattr("entry_count", remaining.clone())?;
    if remaining.lt(0_i64)? {
        return Err(limit_reached(
            py,
            "Maximum page tree page entry limit exceeded!",
        ));
    }

    let page_class = crate::pyutil::cached_module(py, "pypdf._page")?.getattr("PageObject")?;
    let page = page_class.call1((slf, indirect_reference))?;
    if !list_only {
        page.call_method1("update", (node.clone(),))?;
    }
    let mut inherited = inherit.call_method0("items")?.try_iter()?;
    while let Some(pair) = next_item(&mut inherited)? {
        let attribute = pair.get_item(0)?;
        let value = pair.get_item(1)?;
        // If the page has its own value, it does not inherit the parent's value.
        if !page.contains(&attribute)? {
            page.set_item(attribute, value)?;
        }
    }
    Ok(page)
}

// @pypdf: pypdf._doc_common.PdfDocCommon._flatten
#[pyfunction]
fn doccommon_flatten<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    list_only: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let configuration = configuration(py)?;
    let maximum_depth = configuration.getattr("page_tree_maximum_depth")?;
    let maximum_page_entries = configuration.getattr("page_tree_maximum_page_entries")?;

    slf.setattr("flattened_pages", py_none(py))?;
    let root = slf.getattr("root_object")?;
    let pages =
        dict_get(&root, &py_str(py, "/Pages"), &null_object(py)?)?.call_method0("get_object")?;
    if !is_generic_instance(py, &pages, "DictionaryObject")? {
        return Err(pdf_read_error(py, "Invalid object in /Pages"));
    }

    let flattened_pages = PyList::empty(py);
    let traversal_state = utils_attr(py, "_TraversalState")?.call0()?;
    let keyword_arguments = PyDict::new(py);
    keyword_arguments.set_item("entry_count", &maximum_page_entries)?;
    let remaining_page_entries =
        utils_attr(py, "_TraversalState")?.call((), Some(&keyword_arguments))?;
    // id() values of the nodes on the path from the root to the current node.
    // Detects multi-hop cycles such as A→B→C→A that the single-parent check
    // misses.
    let ancestor_ids = PySet::empty(py)?.into_any();
    // The /Pages nodes of that path whose /Kids are not exhausted yet.
    let stack = PyList::empty(py).into_any();
    let mut item = page_tree_item(
        py,
        &pages,
        &PyDict::new(py).into_any(),
        &py_none(py),
        &py_int(py, 0),
    )?;

    loop {
        let depth = item.getattr("depth")?;
        if depth.gt(&maximum_depth)? {
            return Err(limit_reached(
                py,
                &format!(
                    "Maximum page tree depth reached: {} > {}.",
                    str_of(&depth)?,
                    str_of(&maximum_depth)?
                ),
            ));
        }

        // Get the current node_type
        let node = item.getattr("node")?;
        let pages_attributes = constants_class(py, "PagesAttributes")?;
        let type_key = pages_attributes.getattr("TYPE")?;
        let kids_key = pages_attributes.getattr("KIDS")?;
        let node_type = if node.contains(&type_key)? {
            node.get_item(&type_key)?
        } else if !node.contains(&kids_key)? {
            // Without /Type, only accept it as a page if it carries a structural
            // page key.
            if slf.getattr("strict")?.is_truthy()? {
                let page_attributes = constants_class(py, "PageAttributes")?;
                let mut structural = false;
                for name in ["CONTENTS", "MEDIABOX", "PARENT"] {
                    if node.contains(&page_attributes.getattr(name)?)? {
                        structural = true;
                        break;
                    }
                }
                if !structural {
                    return Err(pdf_read_error(
                        py,
                        &format!("Non-page object reached through /Kids: {}", repr_of(&node)?),
                    ));
                }
            }
            py_str(py, "/Page")
        } else {
            py_str(py, "/Pages")
        };

        // Flatten that type of node
        if node_type.eq("/Pages")? {
            ancestor_ids.call_method1("add", (py_id(py, &node)?,))?;
            let frame = slf.call_method1("_enter_page_tree_node", (item.clone(),))?;
            stack.call_method1("append", (frame,))?;
        } else if node_type.eq("/Page")? {
            let kwargs = PyDict::new(py);
            kwargs.set_item("item", item.clone())?;
            kwargs.set_item("list_only", list_only)?;
            kwargs.set_item("remaining_page_entries", remaining_page_entries.clone())?;
            let page = slf.call_method("_flatten_leaf_page", (), Some(&kwargs))?;
            flattened_pages.append(page)?;
        }

        item = slf.call_method1(
            "_pop_next_page_tree_kid",
            (stack.clone(), ancestor_ids.clone(), traversal_state.clone()),
        )?;
        if item.is_none() {
            break;
        }
    }

    // Assigned only here, so that a failed traversal does not leave a partial
    // page list behind.
    slf.setattr("flattened_pages", flattened_pages)
}

// @pypdf: pypdf._doc_common.PdfDocCommon.remove_page
#[pyfunction]
fn doccommon_remove_page<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    page: &Bound<'py, PyAny>,
    clean: &Bound<'py, PyAny>,
) -> PyResult<()> {
    if slf.getattr("flattened_pages")?.is_none() {
        slf.call_method1("_flatten", (slf.getattr("_readonly")?,))?;
    }
    let flattened_pages = slf.getattr("flattened_pages")?;
    if flattened_pages.is_none() {
        return Err(assertion(py, None));
    }
    let mut page = page.clone();
    if is_generic_instance(py, &page, "IndirectObject")? {
        let resolved = page.call_method0("get_object")?;
        if !resolved.is_instance(&crate::pyutil::cached_module(py, "pypdf._page")?.getattr("PageObject")?)? {
            log_warning(py, "IndirectObject is not referencing a page", None)?;
            return Ok(());
        }
        page = resolved;
    }
    if !page.is_instance(&builtins(py)?.getattr("int")?)? {
        match flattened_pages.call_method1("index", (page.clone(),)) {
            Ok(index) => page = index,
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyValueError>(py) => {
                log_warning(py, "Cannot find page in pages", None)?;
                return Ok(());
            }
            Err(error) => return Err(error),
        }
    }
    let length = py_len(py, &flattened_pages)?;
    if !(py_int(py, 0).le(&page)? && page.lt(&length)?) {
        log_warning(py, "Page number is out of range", None)?;
        return Ok(());
    }

    let indirect_reference = slf
        .getattr("pages")?
        .get_item(&page)?
        .getattr("indirect_reference")?;
    // `pages` is typed as a Sequence because it is not a list, but the concrete
    // _VirtualList does implement deletion.
    slf.getattr("pages")?.del_item(&page)?;
    if clean.is_truthy()? && !indirect_reference.is_none() {
        slf.call_method1("_replace_object", (indirect_reference, null_object(py)?))?;
    }
    Ok(())
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(doccommon_get_num_pages, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_get_page, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_get_page_in_node, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_pages, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_page_labels, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_page_layout, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_page_mode, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_enter_page_tree_node, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_pop_next_page_tree_kid, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_flatten_leaf_page, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_flatten, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_remove_page, module)?)?;
    Ok(())
}
