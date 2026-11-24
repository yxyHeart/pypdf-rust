//! Document-level accessors of `pypdf._doc_common` (upstream pypdf 7.1.1):
//! `metadata`, `viewer_preferences`, page-number lookups, permissions, `xfa`
//! and the attachment views.

use super::*;

// @pypdf: pypdf._doc_common.PdfDocCommon.metadata
#[pyfunction]
fn doccommon_metadata<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let retval = shim_class(py, "DocumentInformation")?.call0()?;
    if slf.getattr("_info")?.is_none() {
        return Ok(py_none(py));
    }
    retval.call_method1("update", (slf.getattr("_info")?,))?;
    Ok(retval)
}

// @pypdf: pypdf._doc_common.PdfDocCommon.viewer_preferences
#[pyfunction]
fn doccommon_viewer_preferences<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let key = constants_class(py, "CatalogAttributes")?.getattr("VIEWER_PREFERENCES")?;
    let preference = dict_get(&slf.getattr("root_object")?, &key, &py_none(py))?;
    if preference.is_none() {
        return Ok(py_none(py));
    }
    let mut preference = preference.call_method0("get_object")?;
    if !is_generic_instance(py, &preference, "DictionaryObject")? {
        log_warning_with(
            py,
            "Viewer preferences are not a dictionary: %(preferences)s",
            &[("preferences", &preference)],
        )?;
        return Ok(py_none(py));
    }
    if !is_generic_instance(py, &preference, "ViewerPreferences")? {
        preference = generic_class(py, "ViewerPreferences")?.call1((preference,))?;
        if preference.hasattr("indirect_reference")?
            && !preference.getattr("indirect_reference")?.is_none()
        {
            slf.call_method1(
                "_replace_object",
                (
                    preference.getattr("indirect_reference")?,
                    preference.clone(),
                ),
            )?;
        } else {
            slf.getattr("root_object")?
                .set_item(name_object(py, &key)?, preference.clone())?;
        }
    }
    Ok(preference)
}

// @pypdf: pypdf._doc_common.PdfDocCommon.get_page_number
#[pyfunction]
fn doccommon_get_page_number<'py>(
    slf: &Bound<'py, PyAny>,
    page: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    slf.call_method1(
        "_get_page_number_by_indirect",
        (page.getattr("indirect_reference")?,),
    )
}

// @pypdf: pypdf._doc_common.PdfDocCommon.get_destination_page_number
#[pyfunction]
fn doccommon_get_destination_page_number<'py>(
    slf: &Bound<'py, PyAny>,
    destination: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    slf.call_method1(
        "_get_page_number_by_indirect",
        (destination.getattr("page")?,),
    )
}

// @pypdf: pypdf._doc_common.PdfDocCommon._get_indirect_object
#[pyfunction]
fn doccommon_get_indirect_object<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    num: &Bound<'py, PyAny>,
    generation: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "IndirectObject")?
        .call1((num, generation, slf))?
        .call_method0("get_object")
}

// @pypdf: pypdf._doc_common.PdfDocCommon.user_access_permissions
#[pyfunction]
fn doccommon_user_access_permissions<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let encryption = slf.getattr("_encryption")?;
    if encryption.is_none() {
        return Ok(py_none(py));
    }
    let permissions = encryption.getattr("P")?;
    constants_class(py, "UserAccessPermissions")?.call1((permissions,))
}

// @pypdf: pypdf._doc_common.PdfDocCommon.are_permissions_valid
#[pyfunction]
fn doccommon_are_permissions_valid<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let encryption = slf.getattr("_encryption")?;
    if encryption.is_none() {
        return Ok(py_none(py));
    }
    if !encryption.call_method0("is_decrypted")?.is_truthy()? {
        return Ok(py_none(py));
    }
    encryption.getattr("_are_permissions_valid")
}

// @pypdf: pypdf._doc_common.PdfDocCommon.xfa
#[pyfunction]
fn doccommon_xfa<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let catalog = slf.getattr("root_object")?;
    let acro_form_key = py_str(py, "/AcroForm");
    if !catalog.contains(&acro_form_key)? || !catalog.get_item(&acro_form_key)?.is_truthy()? {
        return Ok(py_none(py));
    }

    let tree = catalog.get_item(&acro_form_key)?;
    let retval = PyDict::new(py);
    let xfa_key = py_str(py, "/XFA");
    if !tree.contains(&xfa_key)? {
        return Ok(retval.into_any());
    }

    let fields = tree.get_item(&xfa_key)?.call_method0("get_object")?;
    if !is_generic_instance(py, &fields, "ArrayObject")? {
        log_warning_with(
            py,
            "XFA entry is not an array: %(fields)s",
            &[("fields", &fields)],
        )?;
        return Ok(retval.into_any());
    }

    let mut accumulated_length: i64 = 0;
    let maximum_length = config_item(py, "xfa_decoding_work_maximum_length")?;

    let mut field_iterator = fields.try_iter()?;
    while let Some(tag) = next_item(&mut field_iterator)? {
        let field = next_or_none(py, &mut field_iterator)?;
        if is_null_or_none(py, &field)? {
            continue;
        }
        let field = field.call_method0("get_object")?;
        if is_null_or_none(py, &field)? {
            continue;
        }
        let decoded_data = field.call_method0("get_data")?;
        accumulated_length += decoded_data.len()? as i64;
        if py_int(py, accumulated_length).gt(&maximum_length)? {
            return Err(limit_reached(
                py,
                &format!(
                    "XFA decoding exceeded the maximum accumulated decoding work of {} bytes.",
                    str_of(&maximum_length)?
                ),
            ));
        }
        retval.set_item(tag, decoded_data)?;
    }
    Ok(retval.into_any())
}

// @pypdf: pypdf._doc_common.PdfDocCommon.attachments
#[pyfunction]
fn doccommon_attachments<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let entries = PyDict::new(py);
    let mut attachment_iterator = slf.getattr("attachment_list")?.try_iter()?;
    while let Some(entry) = next_item(&mut attachment_iterator)? {
        let mut name_iterator = entry.getattr("_names")?.try_iter()?;
        while let Some(name) = next_item(&mut name_iterator)? {
            // entries.setdefault(name, []).append(entry)
            let bucket = match entries.get_item(&name)? {
                Some(bucket) => bucket,
                None => {
                    let bucket = PyList::empty(py).into_any();
                    entries.set_item(&name, &bucket)?;
                    bucket
                }
            };
            bucket.call_method1("append", (entry.clone(),))?;
        }
    }

    let raw = PyDict::new(py);
    let mut entry_iterator = entries.call_method0("items")?.try_iter()?;
    while let Some(pair) = next_item(&mut entry_iterator)? {
        let name = pair.get_item(0)?;
        let bucket = pair.get_item(1)?;
        let contents = slf.getattr("_get_attachment_contents")?;
        raw.set_item(name, py_tuple(py, vec![contents, bucket])?)?;
    }
    shim_class(py, "LazyDict")?.call1((raw,))
}

// @pypdf: pypdf._doc_common.PdfDocCommon._get_attachment_contents
#[pyfunction]
fn doccommon_get_attachment_contents<'py>(
    py: Python<'py>,
    entries: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let contents = PyList::empty(py);
    let mut iterator = entries.try_iter()?;
    while let Some(entry) = next_item(&mut iterator)? {
        contents.append(entry.getattr("content")?)?;
    }
    Ok(contents.into_any())
}

// @pypdf: pypdf._doc_common.PdfDocCommon.attachment_list
#[pyfunction]
fn doccommon_attachment_list<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let embedded_file = py.import("pypdf.generic._files")?.getattr("EmbeddedFile")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("strict", slf.getattr("strict")?)?;
    embedded_file
        .getattr("_load")?
        .call((slf.getattr("root_object")?,), Some(&kwargs))
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(doccommon_metadata, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_viewer_preferences, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_get_page_number, module)?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_get_destination_page_number,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(doccommon_get_indirect_object, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_user_access_permissions, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_are_permissions_valid, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_xfa, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_attachments, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_get_attachment_contents, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_attachment_list, module)?)?;
    Ok(())
}
