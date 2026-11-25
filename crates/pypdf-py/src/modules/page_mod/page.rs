//! `pypdf._page.PageObject` (upstream pypdf 7.1.1).
//!
//! Every method body delegates here from the Python shim; the Rust code calls
//! back into Python for anything upstream dispatches dynamically (`self.foo()`,
//! `PdfObject.get_object()`, `ContentStream(...)`, ...) so subclass overrides and
//! monkeypatching behave exactly like upstream.

use pyo3::exceptions::{PyAttributeError, PyKeyError, PyMemoryError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyTuple};

use super::callables;
use super::common::{
    self, constants_attr, constants_str, dict_object, float_object, generic_class, globals,
    is_null_or_none, name_object, null_object, number_object, op_add, op_mul, op_neg, op_sub,
    op_truediv, py_bytes, py_int, py_len, py_list, py_str, py_str_type, py_tuple, rectangle_object,
    slice_object,
};
use crate::errors;

// ---------------------------------------------------------------------------
// Local helpers around Python semantics
// ---------------------------------------------------------------------------

fn get<'py>(
    obj: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    default: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    obj.call_method1("get", (key.clone(), default.clone()))
}

fn get_two<'py>(
    obj: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    default: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    obj.call_method1("get", (key.clone(), default))
}

fn getattr_object<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    obj.call_method0("get_object")
}

fn empty_list<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    py_list(py, Vec::new())
}

fn empty_dict<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyDict::new(py).into_any())
}

fn list_append<'py>(list: &Bound<'py, PyAny>, value: Bound<'py, PyAny>) -> PyResult<()> {
    list.call_method1("append", (value,))?;
    Ok(())
}

fn list_extend<'py>(list: &Bound<'py, PyAny>, values: &Bound<'py, PyAny>) -> PyResult<()> {
    list.call_method1("extend", (values.clone(),))?;
    Ok(())
}

fn py_none<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    py.None().into_bound(py)
}

fn option_or_none<'py>(py: Python<'py>, value: Option<&Bound<'py, PyAny>>) -> Bound<'py, PyAny> {
    match value {
        Some(value) => value.clone(),
        None => py_none(py),
    }
}

fn page_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    constants_attr(py, "PageAttributes", name)
}

fn page_attr_str(py: Python<'_>, name: &str) -> PyResult<String> {
    constants_str(py, "PageAttributes", name)
}

fn resource_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    constants_attr(py, "Resources", name)
}

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    common::builtins(py)
}

fn integer(py: Python<'_>, value: i64) -> Bound<'_, PyAny> {
    py_int(py, value)
}

fn eq_bytes(value: &Bound<'_, PyAny>, expected: &[u8]) -> PyResult<bool> {
    value.eq(py_bytes(value.py(), expected))
}

fn eq_str(value: &Bound<'_, PyAny>, expected: &str) -> PyResult<bool> {
    value.eq(expected)
}

fn truthy(value: &Bound<'_, PyAny>) -> PyResult<bool> {
    value.is_truthy()
}

/// `NameObject(key)` from a Python object already holding the name.
fn name_of<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    name_object(py, &py_str(value)?)
}

// ---------------------------------------------------------------------------
// PageObject.__init__ / hashing / simple properties
// ---------------------------------------------------------------------------

// @pypdf: pypdf._page.PageObject.__init__
#[pyfunction]
pub fn page_init<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
    indirect_reference: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let dictionary_class = generic_class(py, "DictionaryObject")?;
    dictionary_class.call_method1("__init__", (slf.clone(),))?;
    slf.setattr("pdf", pdf)?;
    slf.setattr("_content_stream_images", py_none(py))?;
    slf.setattr("indirect_reference", indirect_reference)?;
    if !is_null_or_none(indirect_reference)? {
        let target = getattr_object(indirect_reference)?;
        slf.call_method1("update", (target,))?;
    }
    Ok(())
}

// @pypdf: pypdf._page.PageObject.hash_bin
#[pyfunction]
pub fn page_hash_bin<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<isize> {
    let items = slf.call_method0("items")?;
    let mut pairs: Vec<Bound<'py, PyAny>> = Vec::new();
    for item in items.try_iter()? {
        let item = item?;
        let key = item.get_item(0)?;
        let value = item.get_item(1)?;
        let value_hash = value.call_method0("hash_bin")?;
        pairs.push(py_tuple(py, vec![key, value_hash])?);
    }
    let inner = py_tuple(py, pairs)?;
    let outer = py_tuple(py, vec![generic_class(py, "DictionaryObject")?, inner])?;
    common::py_hash(&outer)
}

// @pypdf: pypdf._page.PageObject.hash_value_data
#[pyfunction]
pub fn page_hash_value_data<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let dictionary_class = generic_class(py, "DictionaryObject")?;
    let data = dictionary_class
        .getattr("hash_value_data")?
        .call1((slf.clone(),))?;
    let id = builtins(py)?.getattr("id")?.call1((slf.clone(),))?;
    let encoded = py_str_type(py, &py_str(&id)?).call_method0("encode")?;
    Ok(op_add(py, &data, &encoded)?.unbind())
}

// @pypdf: pypdf._page.PageObject.user_unit
#[pyfunction]
pub fn page_user_unit<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let key = page_attr(py, "USER_UNIT")?;
    Ok(get_two(slf, &key, integer(py, 1))?.unbind())
}

// @pypdf: pypdf._page.PageObject.create_blank_page
#[pyfunction]
#[pyo3(signature = (pdf=None, width=None, height=None))]
pub fn page_create_blank_page<'py>(
    py: Python<'py>,
    pdf: Option<&Bound<'py, PyAny>>,
    width: Option<&Bound<'py, PyAny>>,
    height: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let page_class = globals(py, "PageObject")?;
    let pdf_object = option_or_none(py, pdf);
    let page = page_class.call1((pdf_object.clone(),))?;
    page.set_item(
        name_object(py, &page_attr_str(py, "TYPE")?)?,
        name_object(py, "/Page")?,
    )?;
    page.set_item(
        name_object(py, &page_attr_str(py, "PARENT")?)?,
        null_object(py)?,
    )?;
    page.set_item(
        name_object(py, &page_attr_str(py, "RESOURCES")?)?,
        dict_object(py)?,
    )?;
    let mut width = width.cloned();
    let mut height = height.cloned();
    if width.is_none() || height.is_none() {
        if !pdf_object.is_none() {
            let pages = pdf_object.getattr("pages")?;
            let count = py_len(&pages)?;
            if count > 0 {
                let lastpage = pages.get_item(count - 1)?;
                let mediabox = lastpage.getattr("mediabox")?;
                width = Some(mediabox.getattr("width")?);
                height = Some(mediabox.getattr("height")?);
            } else {
                return Err(errors::raise(
                    py,
                    "PageSizeNotDefinedError",
                    PyTuple::empty(py),
                ));
            }
        } else {
            return Err(errors::raise(
                py,
                "PageSizeNotDefinedError",
                PyTuple::empty(py),
            ));
        }
    }
    // Both values are resolved by the branch above (which raises
    // PageSizeNotDefinedError otherwise); keep a graceful fallback so that an
    // internal invariant violation can never panic the interpreter.
    let (width, height) = match (width, height) {
        (Some(width), Some(height)) => (width, height),
        _ => {
            return Err(pyo3::exceptions::PyAssertionError::new_err(
                "page size resolved",
            ))
        }
    };
    let rectangle = py_tuple(py, vec![integer(py, 0), integer(py, 0), width, height])?;
    page.set_item(
        name_object(py, &page_attr_str(py, "MEDIABOX")?)?,
        rectangle_object(py, rectangle)?,
    )?;
    Ok(page.unbind())
}

// ---------------------------------------------------------------------------
// Resource helpers
// ---------------------------------------------------------------------------

// @pypdf: pypdf._page._get_page_resources
#[pyfunction]
#[pyo3(signature = (obj, inherit=true))]
pub fn page_get_page_resources<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    inherit: bool,
) -> PyResult<Py<PyAny>> {
    let resources_key = page_attr(py, "RESOURCES")?;
    let resources = if inherit {
        let kwargs = PyDict::new(py);
        kwargs.set_item("key", resources_key)?;
        kwargs.set_item("default", dict_object(py)?)?;
        obj.call_method("get_inherited", (), Some(&kwargs))?
    } else {
        obj.call_method1("get", (resources_key,))?
    };
    if is_null_or_none(&resources)? {
        return Ok(dict_object(py)?.unbind());
    }
    let resources = getattr_object(&resources)?;
    if !resources.is_instance(&generic_class(py, "DictionaryObject")?)? {
        common::logger_warning(
            py,
            "Page resources are not a dictionary: %(resources)s",
            &[("resources", resources.clone())],
        )?;
        return Ok(dict_object(py)?.unbind());
    }
    Ok(resources.unbind())
}

// @pypdf: pypdf._page._get_font_resources
#[pyfunction]
pub fn page_get_font_resources<'py>(
    py: Python<'py>,
    resources: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let font_key = resource_attr(py, "FONT")?;
    let fonts = resources.call_method1("get", (font_key,))?;
    if is_null_or_none(&fonts)? {
        return Ok(dict_object(py)?.unbind());
    }
    let fonts = getattr_object(&fonts)?;
    if !fonts.is_instance(&generic_class(py, "DictionaryObject")?)? {
        common::logger_warning(
            py,
            "Font resources are not a dictionary: %(fonts)s",
            &[("fonts", fonts.clone())],
        )?;
        return Ok(dict_object(py)?.unbind());
    }
    Ok(fonts.unbind())
}

// ---------------------------------------------------------------------------
// Page boxes
// ---------------------------------------------------------------------------

// @pypdf: pypdf._page._get_rectangle
#[pyfunction]
pub fn page_get_rectangle<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    name: &Bound<'py, PyAny>,
    defaults: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let rectangle_class = globals(py, "RectangleObject")?;
    let array_class = generic_class(py, "ArrayObject")?;
    let mut retval = getattr_object(&get(slf, name, &null_object(py)?)?)?;
    if retval.is_instance(&rectangle_class)? {
        return Ok(retval.unbind());
    }
    if is_null_or_none(&retval)? {
        for default in defaults.try_iter()? {
            let default = default?;
            retval = getattr_object(&get(slf, &default, &null_object(py)?)?)?;
            if !is_null_or_none(&retval)? {
                break;
            }
        }
    }
    if !retval.is_instance(&array_class)? {
        let message = format!(
            "Expected an array of four values for {}, got {}",
            py_str(name)?,
            py_str(&retval)?
        );
        return Err(PyValueError::new_err(message));
    }
    let length = py_len(&retval)?;
    if length != 4 {
        if length > 4 {
            common::logger_warning(
                py,
                "Expected four values, got %(length)d: %(retval)s",
                &[
                    ("length", py_int(py, length as i64)),
                    ("retval", retval.clone()),
                ],
            )?;
            let first_four =
                retval.get_item(slice_object(py, None, Some(integer(py, 4)), None)?)?;
            let first_four = builtins(py)?.getattr("tuple")?.call1((first_four,))?;
            retval = rectangle_object(py, first_four)?;
        } else {
            let message = format!(
                "Expected four values for {}, got {}: {}",
                py_str(name)?,
                length,
                py_str(&retval)?
            );
            return Err(PyValueError::new_err(message));
        }
    } else {
        retval = rectangle_object(py, retval)?;
    }
    globals(py, "_set_rectangle")?.call1((slf.clone(), name.clone(), retval.clone()))?;
    Ok(retval.unbind())
}

// @pypdf: pypdf._page._set_rectangle
#[pyfunction]
pub fn page_set_rectangle<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    name: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let builtins = builtins(py)?;
    let is_sequence = value.is_instance(&builtins.getattr("list")?)?
        || value.is_instance(&builtins.getattr("tuple")?)?;
    if is_sequence && py_len(value)? < 4 {
        let message = format!(
            "Expected four values for {}, got {}: {}",
            py_str(name)?,
            py_len(value)?,
            py_str(value)?
        );
        return Err(PyValueError::new_err(message));
    }
    slf.set_item(name_of(py, name)?, value.clone())?;
    Ok(())
}

// @pypdf: pypdf._page._delete_rectangle
#[pyfunction]
pub fn page_delete_rectangle<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    name: &Bound<'py, PyAny>,
) -> PyResult<()> {
    slf.del_item(name_of(py, name)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------

fn inline_tilde<'py>(image_id: &Bound<'py, PyAny>) -> PyResult<bool> {
    let first = image_id.get_item(0)?;
    let last = image_id.get_item(-1)?;
    Ok(first.eq("~")? && last.eq("~")?)
}

fn image_file<'py>(
    py: Python<'py>,
    name: String,
    data: Bound<'py, PyAny>,
    image: Bound<'py, PyAny>,
    indirect_reference: Bound<'py, PyAny>,
    is_inline: bool,
    is_displayed: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("name", name)?;
    kwargs.set_item("data", data)?;
    kwargs.set_item("image", image)?;
    kwargs.set_item("indirect_reference", indirect_reference)?;
    kwargs.set_item("is_inline", is_inline)?;
    kwargs.set_item("is_displayed", is_displayed)?;
    globals(py, "ImageFile")?.call((), Some(&kwargs))
}

fn unpack_three<'py>(
    value: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let tuple = value.cast::<PyTuple>()?;
    Ok((tuple.get_item(0)?, tuple.get_item(1)?, tuple.get_item(2)?))
}

// @pypdf: pypdf._page.PageObject._get_ids_image
#[pyfunction]
#[pyo3(signature = (slf, obj=None, ancest=None, call_stack=None))]
pub fn page_get_ids_image<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    obj: Option<&Bound<'py, PyAny>>,
    ancest: Option<&Bound<'py, PyAny>>,
    call_stack: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let call_stack = match call_stack {
        Some(value) => value.clone(),
        None => empty_list(py)?,
    };
    let obj_is_none = obj.map(|value| value.is_none()).unwrap_or(true);
    let mut obj = match obj {
        Some(value) => value.clone(),
        None => py_none(py),
    };
    let indirect = match common::getattr_opt(&obj, "indirect_reference")? {
        Some(value) => value,
        None => py_none(py),
    };
    if call_stack.contains(&indirect)? {
        return Ok(empty_list(py)?.unbind());
    }
    list_append(&call_stack, indirect)?;
    let content_images = slf.getattr("_content_stream_images")?;
    if content_images.is_none() {
        let parsed = slf.call_method0("_parse_images_from_content_stream")?;
        slf.setattr("_content_stream_images", &parsed)?;
    }
    if obj_is_none {
        obj = slf.clone();
    }
    let ancest = match ancest {
        Some(value) => value.clone(),
        None => empty_list(py)?,
    };
    let content_images = slf.getattr("_content_stream_images")?;
    let dictionary_class = generic_class(py, "DictionaryObject")?;
    let stream_class = generic_class(py, "StreamObject")?;
    let mut items: Vec<Bound<'py, PyAny>> = Vec::new();
    let resources_key = page_attr(py, "RESOURCES")?;
    let resources = getattr_object(&get(&obj, &resources_key, &null_object(py)?)?)?;
    let mut resources = resources;
    if !resources.is_instance(&dictionary_class)? {
        if !is_null_or_none(&resources)? {
            common::logger_warning(
                py,
                "Resources are not a dictionary: %(resources)s",
                &[("resources", resources.clone())],
            )?;
        }
        resources = dict_object(py)?;
    }
    let xobject_key = resource_attr(py, "XOBJECT")?;
    let ancest_len = py_len(&ancest)?;
    if !resources.contains(&xobject_key)? {
        // Forms without XObject resources have no images inside them
        if ancest_len > 0 {
            return Ok(empty_list(py)?.unbind());
        }
        let content_items = content_images.call_method0("items")?;
        for item in content_items.try_iter()? {
            let item = item?;
            let image_name = item.get_item(0)?;
            let image_value = item.get_item(1)?;
            if truthy(&image_value)? {
                items.push(image_name);
            }
        }
        return Ok(py_list(py, items)?.unbind());
    }
    let x_object = getattr_object(&resources.get_item(&xobject_key)?)?;
    if !x_object.is_instance(&dictionary_class)? {
        common::logger_warning(
            py,
            "XObject resources are not a dictionary: %(x_object)s",
            &[("x_object", x_object.clone())],
        )?;
        return Ok(empty_list(py)?.unbind());
    }
    let subtype_key = constants_attr(py, "ImageAttributes", "SUBTYPE")?;
    for o in x_object.try_iter()? {
        let o = o?;
        let entry = x_object.get_item(&o)?;
        if !entry.is_instance(&stream_class)? {
            continue;
        }
        let subtype = entry.call_method1("get", (subtype_key.clone(), ""))?;
        if eq_str(&subtype, "/Image")? {
            if ancest_len == 0 {
                items.push(o);
            } else {
                let combined = py_list(py, ancest.try_iter()?.collect::<PyResult<Vec<_>>>()?)?;
                list_append(&combined, o)?;
                items.push(combined);
            }
        } else {
            let new_ancest = py_list(py, ancest.try_iter()?.collect::<PyResult<Vec<_>>>()?)?;
            list_append(&new_ancest, o)?;
            let nested =
                slf.call_method1("_get_ids_image", (entry, new_ancest, call_stack.clone()))?;
            items.extend(nested.try_iter()?.collect::<PyResult<Vec<_>>>()?);
        }
    }
    // Removes duplicates and preserves order
    let deduplicated = py_list(py, items)?;
    let content_items = content_images.call_method0("items")?;
    for item in content_items.try_iter()? {
        let item = item?;
        let object_name = item.get_item(0)?;
        let object_value = item.get_item(1)?;
        if !deduplicated.contains(&object_name)? && truthy(&object_value)? {
            list_append(&deduplicated, object_name)?;
        }
    }
    Ok(deduplicated.unbind())
}

// @pypdf: pypdf._page.PageObject._get_image
#[pyfunction]
#[pyo3(signature = (slf, image_id, obj=None))]
pub fn page_get_image<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    image_id: &Bound<'py, PyAny>,
    obj: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let builtins = builtins(py)?;
    let mut obj = match obj {
        Some(value) => value.clone(),
        None => slf.clone(),
    };
    let mut image_id = image_id.clone();
    if image_id.is_instance(&builtins.getattr("tuple")?)? {
        image_id = builtins.getattr("list")?.call1((&image_id,))?;
    }
    if image_id.is_instance(&builtins.getattr("list")?)? && py_len(&image_id)? == 1 {
        image_id = image_id.get_item(0)?;
    }
    let mut xobjs: Option<Bound<'py, PyAny>> = None;
    let resources_key = page_attr(py, "RESOURCES")?;
    let resources = getattr_object(&get(&obj, &resources_key, &null_object(py)?)?)?;
    let dictionary_class = generic_class(py, "DictionaryObject")?;
    let xobject_key = resource_attr(py, "XOBJECT")?;
    if resources.is_instance(&dictionary_class)? && resources.contains(&xobject_key)? {
        xobjs = Some(resources.get_item(&xobject_key)?);
    } else if !inline_tilde(&image_id)? {
        let message = format!(
            "Cannot access image object {} without XObject resources",
            py_str(&image_id)?
        );
        return Err(PyKeyError::new_err(message));
    }
    if image_id.is_instance(&builtins.getattr("str")?)? {
        if inline_tilde(&image_id)? {
            let content_images = slf.getattr("_content_stream_images")?;
            if content_images.is_none() {
                let parsed = slf.call_method0("_parse_images_from_content_stream")?;
                slf.setattr("_content_stream_images", &parsed)?;
            }
            let content_images = slf.getattr("_content_stream_images")?;
            if !content_images.contains(&image_id)? {
                let message = format!("Image {} not found", py_str(&image_id)?);
                return Err(PyKeyError::new_err(message));
            }
            let image_file = content_images.get_item(&image_id)?;
            if image_file.is_none() {
                return Err(pyo3::exceptions::PyAssertionError::new_err(()));
            }
            return Ok(image_file.unbind());
        }
        let xobjs = match &xobjs {
            Some(value) => value.clone(),
            None => return Err(pyo3::exceptions::PyAssertionError::new_err(())),
        };
        if !xobjs.contains(&image_id)? {
            let message = format!("Image {} not found", py_str(&image_id)?);
            return Err(PyKeyError::new_err(message));
        }
        let xobj = xobjs.get_item(&image_id)?;
        let subtype_key = constants_attr(py, "ImageAttributes", "SUBTYPE")?;
        let subtype = xobj.call_method1("get", (subtype_key, ""))?;
        if !eq_str(&subtype, "/Image")? {
            let message = format!("XObject {} is not an image", py_str(&image_id)?);
            return Err(PyKeyError::new_err(message));
        }
        let content_images = slf.getattr("_content_stream_images")?;
        let is_displayed = !content_images.is_none() && content_images.contains(&image_id)?;
        let xobj_to_image = py
            .import("pypdf.generic._image_xobject")?
            .getattr("_xobj_to_image")?;
        let (extension, byte_stream, image) = unpack_three(&xobj_to_image.call1((xobj.clone(),))?)?;
        let suffix = image_id.get_item(slice_object(py, Some(integer(py, 1)), None, None)?)?;
        let name = format!("{}{}", py_str(&suffix)?, py_str(&extension)?);
        let reference = xobj.getattr("indirect_reference")?;
        return Ok(
            image_file(py, name, byte_stream, image, reference, false, is_displayed)?.unbind(),
        );
    }
    let xobjs = match &xobjs {
        Some(value) => value.clone(),
        None => return Err(pyo3::exceptions::PyAssertionError::new_err(())),
    };
    let ids = image_id.get_item(slice_object(py, Some(integer(py, 1)), None, None)?)?;
    let first = image_id.get_item(0)?;
    let nested_obj = xobjs.get_item(&first)?;
    let _ = &mut obj;
    Ok(slf.call_method1("_get_image", (ids, nested_obj))?.unbind())
}

// @pypdf: pypdf._page.PageObject.images
#[pyfunction]
pub fn page_images<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let ids_function = slf.getattr("_get_ids_image")?;
    let get_function = slf.getattr("_get_image")?;
    Ok(globals(py, "VirtualListImages")?
        .call1((ids_function, get_function))?
        .unbind())
}

// @pypdf: pypdf._page.PageObject.inline_images (getter)
#[pyfunction]
pub fn page_inline_images_get<'py>(py: Python<'py>) -> PyResult<Py<PyAny>> {
    globals(py, "deprecation_with_replacement")?.call1((
        "PageObject.inline_images",
        "PageObject.images",
        "7.0.0",
    ))?;
    Ok(py_none(py).unbind())
}

// @pypdf: pypdf._page.PageObject.inline_images (setter)
#[pyfunction]
pub fn page_inline_images_set<'py>(py: Python<'py>) -> PyResult<()> {
    globals(py, "deprecation_no_replacement")?.call1(("PageObject.inline_images", "7.0.0"))?;
    Ok(())
}

// @pypdf: pypdf._page.PageObject._translate_value_inline_image
#[pyfunction]
pub fn page_translate_value_inline_image<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    k: &Bound<'py, PyAny>,
    v: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let mapping = py
        .import("pypdf.constants")?
        .getattr("_INLINE_IMAGE_VALUE_MAPPING")?;
    match mapping.get_item(v) {
        Ok(mapped) => Ok(name_object(py, &py_str(&mapped)?)?.unbind()),
        Err(exception) => {
            if !(exception.is_instance_of::<PyKeyError>(py)
                || exception.is_instance_of::<PyTypeError>(py))
            {
                return Err(exception);
            }
            if v.is_instance(&generic_class(py, "NameObject")?)? {
                let resources_key = page_attr(py, "RESOURCES")?;
                let resources = getattr_object(&get(slf, &resources_key, &null_object(py)?)?)?;
                let dictionary_class = generic_class(py, "DictionaryObject")?;
                let mut resources = resources;
                if !resources.is_instance(&dictionary_class)? {
                    if !is_null_or_none(&resources)? {
                        common::logger_warning(
                            py,
                            "Resources are not a dictionary: %(resources)s",
                            &[("resources", resources.clone())],
                        )?;
                    }
                    resources = dict_object(py)?;
                }
                if resources.contains("/ColorSpace")? {
                    let color_space = resources.get_item("/ColorSpace")?;
                    if color_space.contains(v)? {
                        return Ok(color_space.get_item(v)?.unbind());
                    }
                }
                let message = format!(
                    "Cannot find resource entry {} for {}",
                    py_str(v)?,
                    py_str(k)?
                );
                let raised = errors::pdf_read_error(py, &message);
                raised.set_cause(py, Some(exception));
                return Err(raised);
            }
            Ok(v.clone().unbind())
        }
    }
}

// @pypdf: pypdf._page.PageObject._parse_images_from_content_stream
#[pyfunction]
pub fn page_parse_images_from_content_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let content = slf.call_method0("get_contents")?;
    if content.is_none() || is_null_or_none(&content)? {
        return Ok(empty_dict(py)?.unbind());
    }
    let operations = content.getattr("operations")?;
    let mut images_data: Vec<Bound<'py, PyAny>> = Vec::new();
    let mut do_image_names: Vec<Bound<'py, PyAny>> = Vec::new();
    for pair in operations.try_iter()? {
        let pair = pair?;
        let param = pair.get_item(0)?;
        let ope = pair.get_item(1)?;
        if eq_bytes(&ope, b"INLINE IMAGE")? {
            let entry = PyDict::new(py);
            entry.set_item("settings", param.get_item("settings")?)?;
            entry.set_item("__streamdata__", param.get_item("data")?)?;
            images_data.push(entry.into_any());
        } else if eq_bytes(&ope, b"Do")? && truthy(&param)? {
            do_image_names.push(param.get_item(0)?);
        } else if eq_bytes(&ope, b"BI")? || eq_bytes(&ope, b"EI")? || eq_bytes(&ope, b"ID")? {
            let message = format!(
                "{} operator met whereas not expected, please share use case with pypdf dev team",
                ope.repr()?.extract::<String>()?
            );
            return Err(errors::pdf_read_error(py, &message));
        }
    }
    // Process Do-referenced objects first (images + forms, no subtype check)
    let builtins = builtins(py)?;
    let string_type = builtins.getattr("str")?;
    let mut mapped_names: Vec<Bound<'py, PyAny>> = Vec::new();
    for name in &do_image_names {
        mapped_names.push(string_type.call1((name.clone(),))?);
    }
    let files = builtins
        .getattr("dict")?
        .getattr("fromkeys")?
        .call1((py_list(py, mapped_names)?, py_none(py)))?;
    let key_mapping = py
        .import("pypdf.constants")?
        .getattr("_INLINE_IMAGE_KEY_MAPPING")?;
    let list_type = builtins.getattr("list")?;
    for (num, image_data) in images_data.iter().enumerate() {
        let stream_data = image_data.get_item("__streamdata__")?;
        let init = PyDict::new(py);
        init.set_item("__streamdata__", stream_data.clone())?;
        init.set_item("/Length", py_len(&stream_data)?)?;
        let settings = image_data.get_item("settings")?;
        let settings_items = settings.call_method0("items")?;
        for item in settings_items.try_iter()? {
            let item = item?;
            let k = item.get_item(0)?;
            let v = item.get_item(1)?;
            if eq_str(&k, "/Length")? || eq_str(&k, "/L")? {
                continue;
            }
            let v = if v.is_instance(&list_type)? {
                let mut translated: Vec<Bound<'py, PyAny>> = Vec::new();
                for x in v.try_iter()? {
                    translated
                        .push(slf.call_method1("_translate_value_inline_image", (k.clone(), x?))?);
                }
                generic_class(py, "ArrayObject")?.call1((py_list(py, translated)?,))?
            } else {
                slf.call_method1("_translate_value_inline_image", (k.clone(), v))?
            };
            let k = if key_mapping.contains(&k)? {
                name_object(py, &py_str(&key_mapping.get_item(&k)?)?)?
            } else {
                common::logger_warning(
                    py,
                    "Unknown inline image key %(key)s, keeping it as-is.",
                    &[("key", k.clone())],
                )?;
                name_of(py, &k)?
            };
            if !init.contains(&k)? {
                init.set_item(&k, v)?;
            }
        }
        let init_any = init.clone().into_any();
        let object = generic_class(py, "EncodedStreamObject")?
            .getattr("initialize_from_dictionary")?
            .call1((init_any,))?;
        image_data.set_item("object", object.clone())?;
        let xobj_to_image = py
            .import("pypdf.generic._image_xobject")?
            .getattr("_xobj_to_image")?;
        let (extension, byte_stream, image) = unpack_three(&xobj_to_image.call1((object,))?)?;
        let name = format!("~{}~{}", num, py_str(&extension)?);
        let entry = image_file(py, name, byte_stream, image, py_none(py), true, true)?;
        files.set_item(format!("~{}~", num), entry)?;
    }
    Ok(files.unbind())
}

// ---------------------------------------------------------------------------
// Rotation
// ---------------------------------------------------------------------------

// @pypdf: pypdf._page.PageObject.rotation (getter)
#[pyfunction]
pub fn page_rotation_get<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let rotate_key = page_attr(py, "ROTATE")?;
    let rotate_obj = get_two(slf, &rotate_key, integer(py, 0))?;
    if rotate_obj.is_instance(&builtins(py)?.getattr("int")?)? {
        return Ok(rotate_obj.unbind());
    }
    Ok(getattr_object(&rotate_obj)?.unbind())
}

// @pypdf: pypdf._page.PageObject.rotation (setter)
#[pyfunction]
pub fn page_rotation_set<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    r: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let builtins = builtins(py)?;
    let integer_value = builtins.getattr("int")?.call1((r,))?;
    let plus = op_add(py, &integer_value, &integer(py, 45))?;
    let floored = common::operator(py, "floordiv")?.call1((&plus, &integer(py, 90)))?;
    let multiplied = op_mul(py, &floored, &integer(py, 90))?;
    let modulo = common::operator(py, "mod")?.call1((&multiplied, &integer(py, 360)))?;
    let rotate_key = page_attr(py, "ROTATE")?;
    slf.set_item(name_of(py, &rotate_key)?, number_object(py, &modulo)?)?;
    Ok(())
}

// @pypdf: pypdf._page.PageObject.transfer_rotation_to_content
#[pyfunction]
pub fn page_transfer_rotation_to_content<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let rotation = slf.getattr("rotation")?;
    let r = op_neg(py, &rotation)?;
    slf.setattr("rotation", integer(py, 0))?;
    let mediabox = slf.getattr("mediabox")?;
    let mb = rectangle_object(py, mediabox)?;
    let builtins = builtins(py)?;
    let float_type = builtins.getattr("float")?;
    let half_width = op_truediv(py, &mb.getattr("width")?, &integer(py, 2))?;
    let centered_x = op_add(py, &mb.getattr("left")?, &half_width)?;
    let tx = op_neg(py, &float_type.call1((centered_x,))?)?;
    let half_height = op_truediv(py, &mb.getattr("height")?, &integer(py, 2))?;
    let centered_y = op_add(py, &mb.getattr("bottom")?, &half_height)?;
    let ty = op_neg(py, &float_type.call1((centered_y,))?)?;
    let transformation = globals(py, "Transformation")?
        .call0()?
        .call_method1("translate", (tx, ty))?
        .call_method1("rotate", (r,))?;
    let pt1 = transformation.call_method1("apply_on", (mb.getattr("lower_left")?,))?;
    let pt2 = transformation.call_method1("apply_on", (mb.getattr("upper_right")?,))?;
    let min_x = builtins
        .getattr("min")?
        .call1((pt1.get_item(0)?, pt2.get_item(0)?))?;
    let min_y = builtins
        .getattr("min")?
        .call1((pt1.get_item(1)?, pt2.get_item(1)?))?;
    let transformation =
        transformation.call_method1("translate", (op_neg(py, &min_x)?, op_neg(py, &min_y)?))?;
    slf.call_method1("add_transformation", (transformation.clone(), false))?;
    for box_name in ["/MediaBox", "/CropBox", "/BleedBox", "/TrimBox", "/ArtBox"] {
        if slf.contains(box_name)? {
            let rectangle = rectangle_object(py, slf.get_item(box_name)?)?;
            let first =
                transformation.call_method1("apply_on", (rectangle.getattr("lower_left")?,))?;
            let second =
                transformation.call_method1("apply_on", (rectangle.getattr("upper_right")?,))?;
            let values = py_tuple(
                py,
                vec![
                    builtins
                        .getattr("min")?
                        .call1((first.get_item(0)?, second.get_item(0)?))?,
                    builtins
                        .getattr("min")?
                        .call1((first.get_item(1)?, second.get_item(1)?))?,
                    builtins
                        .getattr("max")?
                        .call1((first.get_item(0)?, second.get_item(0)?))?,
                    builtins
                        .getattr("max")?
                        .call1((first.get_item(1)?, second.get_item(1)?))?,
                ],
            )?;
            slf.set_item(name_object(py, box_name)?, rectangle_object(py, values)?)?;
        }
    }
    Ok(())
}

// @pypdf: pypdf._page.PageObject.rotate
#[pyfunction]
pub fn page_rotate<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    angle: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let modulo = common::operator(py, "mod")?.call1((angle, integer(py, 90)))?;
    if !modulo.eq(integer(py, 0))? {
        return Err(PyValueError::new_err(
            "Rotation angle must be a multiple of 90",
        ));
    }
    let rotation = slf.getattr("rotation")?;
    let sum = op_add(py, &rotation, angle)?;
    let rotate_key = page_attr(py, "ROTATE")?;
    slf.set_item(name_of(py, &rotate_key)?, number_object(py, &sum)?)?;
    Ok(slf.clone().unbind())
}

// ---------------------------------------------------------------------------
// Resource renaming / content stream helpers
// ---------------------------------------------------------------------------

fn compute_unique_key<'py>(
    py: Python<'py>,
    page2res: &Bound<'py, PyAny>,
    new_res: &Bound<'py, PyAny>,
    base_key: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyAny>, bool)> {
    let value = page2res.call_method1("raw_get", (base_key.clone(),))?;
    let mut computed_key = base_key.clone();
    let mut idx: i64 = 0;
    loop {
        if new_res.contains(&computed_key)? {
            if new_res
                .call_method1("raw_get", (computed_key.clone(),))?
                .eq(&value)?
            {
                return Ok((computed_key, true));
            }
            computed_key = py_str_type(py, &format!("{}-{}", py_str(base_key)?, idx));
            idx += 1;
        } else {
            return Ok((computed_key, false));
        }
    }
}

// @pypdf: pypdf._page.PageObject._merge_resources
#[pyfunction]
#[pyo3(signature = (slf, res1, res2, resource, new_res1=true))]
pub fn page_merge_resources<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    res1: &Bound<'py, PyAny>,
    res2: &Bound<'py, PyAny>,
    resource: &Bound<'py, PyAny>,
    new_res1: bool,
) -> PyResult<Py<PyAny>> {
    let mut pdf: Option<Bound<'py, PyAny>> = None;
    let mut is_pdf_writer = false;
    let indirect_class = generic_class(py, "IndirectObject")?;
    if let Some(indirect) = common::getattr_opt(slf, "indirect_reference")? {
        if indirect.is_instance(&indirect_class)? {
            if let Some(pdf_object) = common::getattr_opt(&indirect, "pdf")? {
                is_pdf_writer = pdf_object.hasattr("_add_object")?;
                pdf = Some(pdf_object);
            }
        }
    }
    let new_res = if new_res1 {
        let source = get_two(res1, resource, dict_object(py)?)?;
        let source = getattr_object(&source)?;
        let new_res = dict_object(py)?;
        new_res.call_method1("update", (source,))?;
        new_res
    } else {
        res1.get_item(resource)?
    };
    let page2res_source = get_two(res2, resource, dict_object(py)?)?;
    let page2res = getattr_object(&page2res_source)?;
    let rename_res = PyDict::new(py);
    let keys: Vec<Bound<'py, PyAny>> = page2res.try_iter()?.collect::<PyResult<_>>()?;
    for key in keys {
        let (unique_key, same_value) = compute_unique_key(py, &page2res, &new_res, &key)?;
        let newname = name_of(py, &unique_key)?;
        if !key.eq(&unique_key)? {
            // we have to use a different name for this
            rename_res.set_item(&key, &newname)?;
        }
        if !same_value {
            if is_pdf_writer {
                let raw = page2res.call_method1("raw_get", (key.clone(),))?;
                let Some(target) = pdf.as_ref().cloned() else {
                    return Err(pyo3::exceptions::PyAssertionError::new_err(
                        "is_pdf_writer implies pdf",
                    ));
                };
                let cloned = raw.call_method1("clone", (target,))?;
                new_res.set_item(&newname, cloned)?;
                match new_res
                    .get_item(&newname)
                    .and_then(|value| value.getattr("indirect_reference"))
                {
                    Ok(reference) => {
                        new_res.set_item(&newname, reference)?;
                    }
                    Err(error) if error.is_instance_of::<PyAttributeError>(py) => {}
                    Err(error) => return Err(error),
                }
            } else {
                let raw = page2res.call_method1("raw_get", (key.clone(),))?;
                new_res.set_item(&newname, raw)?;
            }
        }
        let items = new_res.call_method0("items")?;
        let lst = builtins(py)?
            .getattr("sorted")?
            .call1((common::list_of(py, &items)?,))?;
        new_res.call_method0("clear")?;
        for element in lst.try_iter()? {
            let element = element?;
            new_res.set_item(element.get_item(0)?, element.get_item(1)?)?;
        }
    }
    Ok(py_tuple(py, vec![new_res, rename_res.into_any()])?.unbind())
}

// @pypdf: pypdf._page.PageObject._content_stream_rename
#[pyfunction]
pub fn page_content_stream_rename<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
    rename: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if !truthy(rename)? {
        return Ok(stream.clone().unbind());
    }
    let content_stream = globals(py, "ContentStream")?.call1((stream.clone(), pdf.clone()))?;
    let operations = content_stream.getattr("operations")?;
    let builtins = builtins(py)?;
    let list_type = builtins.getattr("list")?;
    let dict_type = builtins.getattr("dict")?;
    let name_type = generic_class(py, "NameObject")?;
    for pair in operations.try_iter()? {
        let pair = pair?;
        let operands = pair.get_item(0)?;
        if operands.is_instance(&list_type)? {
            let length = py_len(&operands)?;
            for index in 0..length {
                let op = operands.get_item(index)?;
                if op.is_instance(&name_type)? {
                    let renamed = rename.call_method1("get", (op.clone(), op.clone()))?;
                    operands.set_item(index, renamed)?;
                }
            }
        } else if operands.is_instance(&dict_type)? {
            let items = operands.call_method0("items")?;
            for item in items.try_iter()? {
                let item = item?;
                let key = item.get_item(0)?;
                let op = item.get_item(1)?;
                if op.is_instance(&name_type)? {
                    let renamed = rename.call_method1("get", (op.clone(), op.clone()))?;
                    operands.set_item(&key, renamed)?;
                }
            }
        } else {
            let message = format!("Type of operands is {}", py_str(&operands.get_type())?);
            return Err(PyKeyError::new_err(message));
        }
    }
    Ok(content_stream.unbind())
}

// @pypdf: pypdf._page.PageObject._add_transformation_matrix
#[pyfunction]
pub fn page_add_transformation_matrix<'py>(
    py: Python<'py>,
    contents: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
    ctm: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let content_stream = globals(py, "ContentStream")?.call1((contents.clone(), pdf.clone()))?;
    let float_type = generic_class(py, "FloatObject")?;
    let mut values: Vec<Bound<'py, PyAny>> = Vec::new();
    for value in ctm.try_iter()? {
        values.push(float_type.call1((value?,))?);
    }
    let item = py_tuple(py, vec![py_list(py, values)?, py_bytes(py, b"cm")])?;
    content_stream
        .getattr("operations")?
        .call_method1("insert", (0, item))?;
    Ok(content_stream.unbind())
}

// @pypdf: pypdf._page.PageObject._get_contents_as_bytes
#[pyfunction]
pub fn page_get_contents_as_bytes<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let contents_key = page_attr(py, "CONTENTS")?;
    if slf.contains(&contents_key)? {
        let obj = getattr_object(&slf.get_item(&contents_key)?)?;
        if obj.is_instance(&builtins(py)?.getattr("list")?)? {
            let mut parts: Vec<Bound<'py, PyAny>> = Vec::new();
            for entry in obj.try_iter()? {
                let entry = entry?;
                parts.push(getattr_object(&entry)?.call_method0("get_data")?);
            }
            return Ok(py_bytes(py, b"")
                .call_method1("join", (py_list(py, parts)?,))?
                .unbind());
        }
        return Ok(obj.call_method0("get_data")?.unbind());
    }
    Ok(py_none(py).unbind())
}

// @pypdf: pypdf._page.PageObject.get_contents
#[pyfunction]
pub fn page_get_contents<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let contents_key = page_attr(py, "CONTENTS")?;
    if !slf.contains(&contents_key)? {
        return Ok(py_none(py).unbind());
    }
    let pdf = match common::getattr_opt(slf, "indirect_reference")? {
        Some(indirect) => match common::getattr_opt(&indirect, "pdf")? {
            Some(pdf) => pdf,
            None => py_none(py),
        },
        None => py_none(py),
    };
    let obj = slf.get_item(&contents_key)?;
    if is_null_or_none(&obj)? {
        return Ok(py_none(py).unbind());
    }
    let resolved_object = getattr_object(&obj)?;
    let array_class = generic_class(py, "ArrayObject")?;
    let stream_class = generic_class(py, "StreamObject")?;
    if !(resolved_object.is_instance(&array_class)?
        || resolved_object.is_instance(&stream_class)?)
    {
        common::logger_warning(
            py,
            "Page contents are not a stream or an array: %(contents)s",
            &[("contents", resolved_object.clone())],
        )?;
        return Ok(py_none(py).unbind());
    }
    Ok(globals(py, "ContentStream")?
        .call1((resolved_object, pdf))?
        .unbind())
}

fn replace_object<'py>(
    py: Python<'py>,
    writer: &Bound<'py, PyAny>,
    reference: &Bound<'py, PyAny>,
    object: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("indirect_reference", reference.clone())?;
    kwargs.set_item("obj", object.clone())?;
    writer.call_method("_replace_object", (), Some(&kwargs))?;
    Ok(())
}

// @pypdf: pypdf._page.PageObject.replace_contents
#[pyfunction]
pub fn page_replace_contents<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    content: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let contents_key = page_attr(py, "CONTENTS")?;
    let indirect = common::getattr_opt(slf, "indirect_reference")?;
    let can_replace_reference = match &indirect {
        Some(value) => !value.is_none(),
        None => false,
    };
    if !can_replace_reference {
        // the page is not attached : the content is directly attached.
        slf.set_item(name_of(py, &contents_key)?, content.clone())?;
        return Ok(());
    }
    let Some(indirect) = indirect else {
        return Err(pyo3::exceptions::PyAssertionError::new_err("checked above"));
    };
    let writer_class = crate::pyutil::cached_module(py, "pypdf._writer")?.getattr("PdfWriter")?;
    let pdf = indirect.getattr("pdf")?;
    if !pdf.is_instance(&writer_class)? {
        globals(py, "deprecation")?.call1((
            "Calling `PageObject.replace_contents()` for pages not assigned to a writer is deprecated \
             and has been removed in pypdf 7.0.0. Attach the page to the writer first or use \
             `PdfWriter(clone_from=...)` directly. The existing approach has proved being unreliable.",
        ))?;
    }
    let writer = pdf;
    // Resolve /Contents because it may be an indirect reference to an
    // ArrayObject. Without resolving it, an indirect contents array is not
    // recognized and its stream objects are left in the writer's object list.
    let old_contents = slf.call_method1("get", (contents_key.clone(), py_none(py)))?;
    let old_contents = if old_contents.is_none() {
        old_contents
    } else {
        getattr_object(&old_contents)?
    };
    let array_class = generic_class(py, "ArrayObject")?;
    let indirect_class = generic_class(py, "IndirectObject")?;
    if old_contents.is_instance(&array_class)? {
        for reference in old_contents.try_iter()? {
            let reference = reference?;
            if !reference.is_instance(&indirect_class)? {
                // Direct objects are not part of the writer's object list.
                continue;
            }
            replace_object(py, &writer, &reference, &null_object(py)?)?;
        }
    }
    let mut content = content.clone();
    if content.is_instance(&array_class)? {
        let mut converted: Vec<Bound<'py, PyAny>> = Vec::new();
        for entry in content.try_iter()? {
            let entry = entry?;
            converted.push(writer.call_method1("_add_object", (entry,))?);
        }
        content = array_class.call1((py_list(py, converted)?,))?;
    }
    if is_null_or_none(&content)? {
        if !slf.contains(&contents_key)? {
            return Ok(());
        }
        let contents_reference = slf.get_item(&contents_key)?.getattr("indirect_reference")?;
        if contents_reference.is_none() {
            return Err(common::assertion_error(py, "mypy"));
        }
        replace_object(py, &writer, &contents_reference, &null_object(py)?)?;
        slf.del_item(name_of(py, &contents_key)?)?;
    } else {
        let current = slf.call_method1("get", (contents_key.clone(), py_none(py)))?;
        let has_reference = current.hasattr("indirect_reference")?;
        if !has_reference {
            let replaced = writer.call_method1("_add_object", (content.clone(),))?;
            slf.set_item(name_of(py, &contents_key)?, replaced)?;
        } else {
            content.setattr(
                "indirect_reference",
                slf.get_item(&contents_key)?.getattr("indirect_reference")?,
            )?;
            let reference = content.getattr("indirect_reference")?;
            if reference.is_none() {
                return Err(common::assertion_error(py, "mypy"));
            }
            replace_object(py, &writer, &reference, &content)?;
        }
    }
    // forces recalculation of images
    slf.setattr("_content_stream_images", py_none(py))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Page merging
// ---------------------------------------------------------------------------

// @pypdf: pypdf._page.PageObject.merge_page
#[pyfunction]
#[pyo3(signature = (slf, page2, expand=false, over=true))]
pub fn page_merge_page<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    page2: &Bound<'py, PyAny>,
    expand: bool,
    over: bool,
) -> PyResult<()> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("over", over)?;
    kwargs.set_item("expand", expand)?;
    slf.call_method("_merge_page", (page2.clone(),), Some(&kwargs))?;
    Ok(())
}

fn merge_page_content<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    page2: &Bound<'py, PyAny>,
    rename: &Bound<'py, PyAny>,
    new_content_array: &Bound<'py, PyAny>,
    page2_transformation: Option<&Bound<'py, PyAny>>,
    over: bool,
) -> PyResult<()> {
    let page2_content = page2.call_method0("get_contents")?;
    if page2_content.is_none() {
        return Ok(());
    }
    let configuration = globals(py, "get_configuration")?.call0()?;
    let merge_box = configuration.getattr("page_merge_box")?;
    let rect = page2.getattr(py_str(&merge_box)?)?;
    let float_type = generic_class(py, "FloatObject")?;
    let rect_values = py_list(
        py,
        vec![
            rect.getattr("left")?,
            rect.getattr("bottom")?,
            rect.getattr("width")?,
            rect.getattr("height")?,
        ],
    )?;
    let mapped = builtins(py)?
        .getattr("map")?
        .call1((float_type, rect_values))?;
    let operations = page2_content.getattr("operations")?;
    operations.call_method1(
        "insert",
        (0, py_tuple(py, vec![mapped, py_bytes(py, b"re")])?),
    )?;
    operations.call_method1(
        "insert",
        (
            1,
            py_tuple(py, vec![py_list(py, vec![])?, py_bytes(py, b"W")])?,
        ),
    )?;
    operations.call_method1(
        "insert",
        (
            2,
            py_tuple(py, vec![py_list(py, vec![])?, py_bytes(py, b"n")])?,
        ),
    )?;
    let mut page2_content = page2_content;
    if let Some(transformation) = page2_transformation {
        page2_content = transformation.call1((page2_content,))?;
    }
    let pdf = slf.getattr("pdf")?;
    page2_content = globals(py, "PageObject")?
        .getattr("_content_stream_rename")?
        .call1((page2_content, rename.clone(), pdf))?;
    page2_content.call_method0("isolate_graphics_state")?;
    if over {
        list_append(new_content_array, page2_content)?;
    } else {
        new_content_array.call_method1("insert", (0, page2_content))?;
    }
    Ok(())
}

// @pypdf: pypdf._page.PageObject._merge_page
#[pyfunction]
#[pyo3(signature = (slf, page2, page2_transformation=None, ctm=None, over=true, expand=false))]
pub fn page_merge_page_inner<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    page2: &Bound<'py, PyAny>,
    page2_transformation: Option<&Bound<'py, PyAny>>,
    ctm: Option<&Bound<'py, PyAny>>,
    over: bool,
    expand: bool,
) -> PyResult<()> {
    // First we work on merging the resource dictionaries. This allows us
    // to find out what symbols in the content streams we might need to
    // rename.
    //
    // Upstream wraps this dispatch in `try: ... except (AssertionError,
    // AttributeError): pass`, so an `AttributeError` raised *inside*
    // `_merge_page_writer` is swallowed and the page falls back to the legacy
    // merge below (bug-compatible on purpose).
    let indirect_class = generic_class(py, "IndirectObject")?;
    let dispatch: PyResult<Option<()>> = (|| {
        let Some(indirect) = common::getattr_opt(slf, "indirect_reference")? else {
            return Ok(None);
        };
        if !indirect.is_instance(&indirect_class)? {
            return Ok(None);
        }
        let Some(pdf) = common::getattr_opt(&indirect, "pdf")? else {
            return Ok(None);
        };
        if !pdf.hasattr("_add_object")? {
            return Ok(None);
        }
        slf.call_method1(
            "_merge_page_writer",
            (
                page2.clone(),
                option_or_none(py, page2_transformation),
                option_or_none(py, ctm),
                over,
                expand,
            ),
        )?;
        Ok(Some(()))
    })();
    match dispatch {
        Ok(Some(())) => return Ok(()),
        Ok(None) => {}
        Err(error)
            if error.is_instance_of::<PyAttributeError>(py)
                || error.is_instance_of::<pyo3::exceptions::PyAssertionError>(py) => {}
        Err(error) => return Err(error),
    }

    let new_resources = dict_object(py)?;
    let rename = PyDict::new(py);
    let resources_key = page_attr(py, "RESOURCES")?;
    let original_resources = getattr_object(&get_two(slf, &resources_key, dict_object(py)?)?)?;
    let page2_resources = getattr_object(&get_two(page2, &resources_key, dict_object(py)?)?)?;
    let new_annots = common::array_object(py)?;
    let annots_key = page_attr(py, "ANNOTS")?;
    let array_class = generic_class(py, "ArrayObject")?;
    for page in [slf.clone(), page2.clone()] {
        if page.contains(&annots_key)? {
            let annots = page.get_item(&annots_key)?;
            if annots.is_instance(&array_class)? {
                list_extend(&new_annots, &annots)?;
            }
        }
    }
    slf.set_item(name_of(py, &annots_key)?, new_annots)?;

    let resources_list = [
        resource_attr(py, "EXT_G_STATE")?,
        resource_attr(py, "COLOR_SPACE")?,
        resource_attr(py, "PATTERN")?,
        resource_attr(py, "SHADING")?,
        resource_attr(py, "XOBJECT")?,
        resource_attr(py, "FONT")?,
        resource_attr(py, "PROPERTIES")?,
    ];
    for resource in resources_list {
        let merged = slf.call_method1(
            "_merge_resources",
            (
                original_resources.clone(),
                page2_resources.clone(),
                resource.clone(),
            ),
        )?;
        let new = merged.get_item(0)?;
        let new_resource_name = merged.get_item(1)?;
        if truthy(&new)? {
            new_resources.set_item(name_of(py, &resource)?, new)?;
            rename.call_method1("update", (new_resource_name,))?;
        }
    }
    // Combine /ProcSet sets, making sure there is a consistent order
    let proc_set = resource_attr(py, "PROC_SET")?;
    let original_procset = getattr_object(&get_two(
        &original_resources,
        &proc_set,
        common::array_object(py)?,
    )?)?;
    let page2_procset = getattr_object(&get_two(
        &page2_resources,
        &proc_set,
        common::array_object(py)?,
    )?)?;
    let builtins = builtins(py)?;
    let set_type = builtins.getattr("set")?;
    let left = set_type.call1((original_procset,))?;
    let right = set_type.call1((page2_procset,))?;
    let union = left.call_method1("union", (right,))?;
    let sorted = builtins.getattr("sorted")?.call1((union,))?;
    new_resources.set_item(
        name_of(py, &proc_set)?,
        generic_class(py, "ArrayObject")?.call1((sorted,))?,
    )?;

    let new_content_array = common::array_object(py)?;
    let original_content = slf.call_method0("get_contents")?;
    if !original_content.is_none() {
        original_content.call_method0("isolate_graphics_state")?;
        list_append(&new_content_array, original_content)?;
    }
    merge_page_content(
        py,
        slf,
        page2,
        rename.as_any(),
        &new_content_array,
        page2_transformation,
        over,
    )?;

    // if expanding the page to fit a new page, calculate the new media box size
    if expand {
        slf.call_method1("_expand_mediabox", (page2.clone(), option_or_none(py, ctm)))?;
    }

    let pdf = slf.getattr("pdf")?;
    let content_stream = globals(py, "ContentStream")?.call1((new_content_array, pdf))?;
    slf.call_method1("replace_contents", (content_stream,))?;
    slf.set_item(name_of(py, &resources_key)?, new_resources)?;
    Ok(())
}

// @pypdf: pypdf._page.PageObject._merge_page_writer
#[pyfunction]
#[pyo3(signature = (slf, page2, page2transformation=None, ctm=None, over=true, expand=false))]
pub fn page_merge_page_writer<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    page2: &Bound<'py, PyAny>,
    page2transformation: Option<&Bound<'py, PyAny>>,
    ctm: Option<&Bound<'py, PyAny>>,
    over: bool,
    expand: bool,
) -> PyResult<()> {
    let indirect = slf.getattr("indirect_reference")?;
    if !indirect.is_instance(&generic_class(py, "IndirectObject")?)? {
        return Err(pyo3::exceptions::PyAssertionError::new_err(()));
    }
    let pdf = indirect.getattr("pdf")?;
    let resources_key = page_attr(py, "RESOURCES")?;
    let original_resources = globals(py, "_get_page_resources")?.call1((slf.clone(), false))?;
    let current_resources = getattr_object(&get(slf, &resources_key, &null_object(py)?)?)?;
    if !current_resources.is(&original_resources) {
        // missing, null or malformed: the merged resources are stored in a new dictionary
        slf.set_item(name_of(py, &resources_key)?, original_resources.clone())?;
    }
    let page2resources = globals(py, "_get_page_resources")?.call1((page2.clone(), false))?;

    let rename = PyDict::new(py);
    let resources_list = [
        resource_attr(py, "EXT_G_STATE")?,
        resource_attr(py, "COLOR_SPACE")?,
        resource_attr(py, "PATTERN")?,
        resource_attr(py, "SHADING")?,
        resource_attr(py, "XOBJECT")?,
        resource_attr(py, "FONT")?,
        resource_attr(py, "PROPERTIES")?,
    ];
    for resource in resources_list {
        if page2resources.contains(&resource)? {
            if !original_resources.contains(&resource)? {
                original_resources.set_item(name_of(py, &resource)?, dict_object(py)?)?;
            }
            let merged = slf.call_method1(
                "_merge_resources",
                (
                    original_resources.clone(),
                    page2resources.clone(),
                    resource.clone(),
                    false,
                ),
            )?;
            let new_resource_name = merged.get_item(1)?;
            rename.call_method1("update", (new_resource_name,))?;
        }
    }
    // Combine /ProcSet sets
    let proc_set = resource_attr(py, "PROC_SET")?;
    if page2resources.contains(&proc_set)? {
        if !original_resources.contains(&proc_set)? {
            original_resources.set_item(name_of(py, &proc_set)?, common::array_object(py)?)?;
        }
        let array = original_resources.get_item(&proc_set)?;
        for value in page2resources.get_item(&proc_set)?.try_iter()? {
            let value = value?;
            if !array.contains(&value)? {
                list_append(&array, value)?;
            }
        }
        array.call_method0("sort")?;
    }

    if !is_null_or_none(&get(page2, &page_attr(py, "ANNOTS")?, &py_none(py))?)? {
        let annots_key = page_attr(py, "ANNOTS")?;
        let array_class = generic_class(py, "ArrayObject")?;
        let mut annots = getattr_object(&get(slf, &annots_key, &null_object(py)?)?)?;
        if !annots.is_instance(&array_class)? {
            if !is_null_or_none(&annots)? {
                common::logger_warning(
                    py,
                    "Annotations are not an array: %(annotations)s",
                    &[("annotations", annots.clone())],
                )?;
            }
            annots = common::array_object(py)?;
            slf.set_item(name_of(py, &annots_key)?, annots.clone())?;
        }
        let transformation = match ctm {
            None => globals(py, "Transformation")?.call0()?,
            Some(ctm) => globals(py, "Transformation")?.call1((ctm.clone(),))?,
        };
        // Ensure we are working on a copy of the list. Otherwise, if both pages
        // are the same object, we might run into an infinite loop.
        let copied = py
            .import("copy")?
            .getattr("deepcopy")?
            .call1((page2.get_item(&annots_key)?,))?;
        let builtins = builtins(py)?;
        for annotation in copied.try_iter()? {
            let annotation = getattr_object(&annotation?)?;
            let kwargs = PyDict::new(py);
            kwargs.set_item(
                "ignore_fields",
                py_tuple(
                    py,
                    vec![
                        py_str_type(py, "/P"),
                        py_str_type(py, "/StructParent"),
                        py_str_type(py, "/Parent"),
                    ],
                )?,
            )?;
            kwargs.set_item("force_duplicate", true)?;
            let cloned = annotation.call_method("clone", (pdf.clone(),), Some(&kwargs))?;
            let rect = annotation.get_item("/Rect")?;
            let pt1 = transformation.call_method1(
                "apply_on",
                (
                    py_tuple(py, vec![rect.get_item(0)?, rect.get_item(1)?])?,
                    true,
                ),
            )?;
            let pt2 = transformation.call_method1(
                "apply_on",
                (
                    py_tuple(py, vec![rect.get_item(2)?, rect.get_item(3)?])?,
                    true,
                ),
            )?;
            let min_fn = builtins.getattr("min")?;
            let max_fn = builtins.getattr("max")?;
            cloned.set_item(
                name_object(py, "/Rect")?,
                generic_class(py, "ArrayObject")?.call1((py_tuple(
                    py,
                    vec![
                        min_fn.call1((pt1.get_item(0)?, pt2.get_item(0)?))?,
                        min_fn.call1((pt1.get_item(1)?, pt2.get_item(1)?))?,
                        max_fn.call1((pt1.get_item(0)?, pt2.get_item(0)?))?,
                        max_fn.call1((pt1.get_item(1)?, pt2.get_item(1)?))?,
                    ],
                )?,))?,
            )?;
            if annotation.contains("/QuadPoints")? {
                let quad = annotation.get_item("/QuadPoints")?;
                let mut values: Vec<Bound<'py, PyAny>> = Vec::new();
                let length = py_len(&quad)?;
                let mut index = 0usize;
                while index < length {
                    let point = transformation.call_method1(
                        "apply_on",
                        (
                            py_tuple(py, vec![quad.get_item(index)?, quad.get_item(index + 1)?])?,
                            true,
                        ),
                    )?;
                    values.extend(point.try_iter()?.collect::<PyResult<Vec<_>>>()?);
                    index += 2;
                }
                cloned.set_item(
                    name_object(py, "/QuadPoints")?,
                    generic_class(py, "ArrayObject")?.call1((py_list(py, values)?,))?,
                )?;
            }
            // The /Rect update above only repositions and resizes the
            // annotation's bounding box; it does not touch the appearance
            // stream's own coordinate system.
            let transform_appearance = py
                .import("pypdf.generic._appearance_stream")?
                .getattr("transform_annotation_appearance")?;
            transform_appearance.call1((cloned.clone(), transformation.clone()))?;
            // try: aa["/Popup"][NameObject("/Parent")] = aa.indirect_reference
            // except KeyError: pass
            let popup_attempt: PyResult<()> = (|| {
                let popup = cloned.get_item("/Popup")?;
                popup.set_item(
                    name_object(py, "/Parent")?,
                    cloned.getattr("indirect_reference")?,
                )?;
                Ok(())
            })();
            match popup_attempt {
                Ok(()) => {}
                Err(error) if error.is_instance_of::<PyKeyError>(py) => {}
                Err(error) => return Err(error),
            }
            cloned.set_item(name_object(py, "/P")?, slf.getattr("indirect_reference")?)?;
            list_append(&annots, cloned.getattr("indirect_reference")?)?;
        }
    }

    let new_content_array = common::array_object(py)?;
    let original_content = slf.call_method0("get_contents")?;
    if !original_content.is_none() {
        original_content.call_method0("isolate_graphics_state")?;
        list_append(&new_content_array, original_content)?;
    }
    merge_page_content(
        py,
        slf,
        page2,
        rename.as_any(),
        &new_content_array,
        page2transformation,
        over,
    )?;
    // if expanding the page to fit a new page, calculate the new media box size
    if expand {
        slf.call_method1("_expand_mediabox", (page2.clone(), option_or_none(py, ctm)))?;
    }
    slf.call_method1("replace_contents", (new_content_array,))?;
    Ok(())
}

// @pypdf: pypdf._page.PageObject._expand_mediabox
#[pyfunction]
pub fn page_expand_mediabox<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    page2: &Bound<'py, PyAny>,
    ctm: Option<&Bound<'py, PyAny>>,
) -> PyResult<()> {
    let builtins = builtins(py)?;
    let mediabox = slf.getattr("mediabox")?;
    let corners1 = [
        mediabox.getattr("left")?.call_method0("as_numeric")?,
        mediabox.getattr("bottom")?.call_method0("as_numeric")?,
        mediabox.getattr("right")?.call_method0("as_numeric")?,
        mediabox.getattr("top")?.call_method0("as_numeric")?,
    ];
    let other = page2.getattr("mediabox")?;
    let left = other.getattr("left")?;
    let bottom = other.getattr("bottom")?;
    let right = other.getattr("right")?;
    let top = other.getattr("top")?;
    let corners2 = [
        left.call_method0("as_numeric")?,
        bottom.call_method0("as_numeric")?,
        left.call_method0("as_numeric")?,
        top.call_method0("as_numeric")?,
        right.call_method0("as_numeric")?,
        top.call_method0("as_numeric")?,
        right.call_method0("as_numeric")?,
        bottom.call_method0("as_numeric")?,
    ];
    let (new_x, new_y): (Vec<Bound<'py, PyAny>>, Vec<Bound<'py, PyAny>>) = match ctm {
        Some(ctm) => {
            let float_type = builtins.getattr("float")?;
            let mut values: Vec<Bound<'py, PyAny>> = Vec::new();
            for value in ctm.try_iter()? {
                values.push(float_type.call1((value?,))?);
            }
            let component = |index: usize| -> PyResult<Bound<'_, PyAny>> {
                values
                    .get(index)
                    .cloned()
                    .ok_or_else(|| common::index_error(py, "tuple index out of range"))
            };
            let mut new_x = Vec::new();
            let mut new_y = Vec::new();
            let mut index = 0;
            while index < 8 {
                let x = op_add(
                    py,
                    &op_add(
                        py,
                        &op_mul(py, &component(0)?, &corners2[index])?,
                        &op_mul(py, &component(2)?, &corners2[index + 1])?,
                    )?,
                    &component(4)?,
                )?;
                let y = op_add(
                    py,
                    &op_add(
                        py,
                        &op_mul(py, &component(1)?, &corners2[index])?,
                        &op_mul(py, &component(3)?, &corners2[index + 1])?,
                    )?,
                    &component(5)?,
                )?;
                new_x.push(x);
                new_y.push(y);
                index += 2;
            }
            (new_x, new_y)
        }
        None => (
            vec![
                corners2[0].clone(),
                corners2[2].clone(),
                corners2[4].clone(),
                corners2[6].clone(),
            ],
            vec![
                corners2[1].clone(),
                corners2[3].clone(),
                corners2[5].clone(),
                corners2[7].clone(),
            ],
        ),
    };
    let min_fn = builtins.getattr("min")?;
    let max_fn = builtins.getattr("max")?;
    let lower_x = min_fn.call1((py_list(py, new_x.clone())?,))?;
    let lower_y = min_fn.call1((py_list(py, new_y.clone())?,))?;
    let upper_x = max_fn.call1((py_list(py, new_x)?,))?;
    let upper_y = max_fn.call1((py_list(py, new_y)?,))?;
    let lowerleft = py_tuple(
        py,
        vec![
            min_fn.call1((corners1[0].clone(), lower_x))?,
            min_fn.call1((corners1[1].clone(), lower_y))?,
        ],
    )?;
    let upperright = py_tuple(
        py,
        vec![
            max_fn.call1((corners1[2].clone(), upper_x))?,
            max_fn.call1((corners1[3].clone(), upper_y))?,
        ],
    )?;
    mediabox.setattr("lower_left", lowerleft)?;
    mediabox.setattr("upper_right", upperright)?;
    Ok(())
}

// @pypdf: pypdf._page.PageObject.merge_transformed_page
#[pyfunction]
#[pyo3(signature = (slf, page2, ctm, over=true, expand=false))]
pub fn page_merge_transformed_page<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    page2: &Bound<'py, PyAny>,
    ctm: &Bound<'py, PyAny>,
    over: bool,
    expand: bool,
) -> PyResult<()> {
    let mut ctm = ctm.clone();
    if ctm.is_instance(&globals(py, "Transformation")?)? {
        ctm = ctm.getattr("ctm")?;
    }
    let transformation = Py::new(
        py,
        callables::TransformationCallable::new(page2.clone(), ctm.clone()),
    )?;
    slf.call_method1(
        "_merge_page",
        (page2.clone(), transformation, ctm, over, expand),
    )?;
    Ok(())
}

// @pypdf: pypdf._page.PageObject.merge_scaled_page
#[pyfunction]
#[pyo3(signature = (slf, page2, scale, over=true, expand=false))]
pub fn page_merge_scaled_page<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    page2: &Bound<'py, PyAny>,
    scale: &Bound<'py, PyAny>,
    over: bool,
    expand: bool,
) -> PyResult<()> {
    let transformation = globals(py, "Transformation")?
        .call0()?
        .call_method1("scale", (scale.clone(), scale.clone()))?;
    slf.call_method1(
        "merge_transformed_page",
        (page2.clone(), transformation, over, expand),
    )?;
    Ok(())
}

// @pypdf: pypdf._page.PageObject.merge_rotated_page
#[pyfunction]
#[pyo3(signature = (slf, page2, rotation, over=true, expand=false))]
pub fn page_merge_rotated_page<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    page2: &Bound<'py, PyAny>,
    rotation: &Bound<'py, PyAny>,
    over: bool,
    expand: bool,
) -> PyResult<()> {
    let transformation = globals(py, "Transformation")?
        .call0()?
        .call_method1("rotate", (rotation.clone(),))?;
    slf.call_method1(
        "merge_transformed_page",
        (page2.clone(), transformation, over, expand),
    )?;
    Ok(())
}

// @pypdf: pypdf._page.PageObject.merge_translated_page
#[pyfunction]
#[pyo3(signature = (slf, page2, tx, ty, over=true, expand=false))]
pub fn page_merge_translated_page<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    page2: &Bound<'py, PyAny>,
    tx: &Bound<'py, PyAny>,
    ty: &Bound<'py, PyAny>,
    over: bool,
    expand: bool,
) -> PyResult<()> {
    let transformation = globals(py, "Transformation")?
        .call0()?
        .call_method1("translate", (tx.clone(), ty.clone()))?;
    slf.call_method1(
        "merge_transformed_page",
        (page2.clone(), transformation, over, expand),
    )?;
    Ok(())
}

// @pypdf: pypdf._page.PageObject.add_transformation
#[pyfunction]
#[pyo3(signature = (slf, ctm, expand=false))]
pub fn page_add_transformation<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    ctm: &Bound<'py, PyAny>,
    expand: bool,
) -> PyResult<()> {
    let mut ctm = ctm.clone();
    if ctm.is_instance(&globals(py, "Transformation")?)? {
        ctm = ctm.getattr("ctm")?;
    }
    let content = slf.call_method0("get_contents")?;
    if !content.is_none() {
        let pdf = slf.getattr("pdf")?;
        let content = globals(py, "PageObject")?
            .getattr("_add_transformation_matrix")?
            .call1((content, pdf, ctm.clone()))?;
        content.call_method0("isolate_graphics_state")?;
        slf.call_method1("replace_contents", (content,))?;
    }
    // if expanding the page to fit a new page, calculate the new media box size
    if expand {
        let mediabox = slf.getattr("mediabox")?;
        let corners: Vec<Bound<'py, PyAny>> = vec![
            mediabox.getattr("left")?.call_method0("as_numeric")?,
            mediabox.getattr("bottom")?.call_method0("as_numeric")?,
            mediabox.getattr("left")?.call_method0("as_numeric")?,
            mediabox.getattr("top")?.call_method0("as_numeric")?,
            mediabox.getattr("right")?.call_method0("as_numeric")?,
            mediabox.getattr("top")?.call_method0("as_numeric")?,
            mediabox.getattr("right")?.call_method0("as_numeric")?,
            mediabox.getattr("bottom")?.call_method0("as_numeric")?,
        ];
        let builtins = builtins(py)?;
        let float_type = builtins.getattr("float")?;
        let mut values: Vec<Bound<'py, PyAny>> = Vec::new();
        for value in ctm.try_iter()? {
            values.push(float_type.call1((value?,))?);
        }
        let component = |index: usize| -> PyResult<Bound<'_, PyAny>> {
            values
                .get(index)
                .cloned()
                .ok_or_else(|| common::index_error(py, "tuple index out of range"))
        };
        let mut new_x = Vec::new();
        let mut new_y = Vec::new();
        let mut index = 0;
        while index < 8 {
            new_x.push(op_add(
                py,
                &op_add(
                    py,
                    &op_mul(py, &component(0)?, &corners[index])?,
                    &op_mul(py, &component(2)?, &corners[index + 1])?,
                )?,
                &component(4)?,
            )?);
            new_y.push(op_add(
                py,
                &op_add(
                    py,
                    &op_mul(py, &component(1)?, &corners[index])?,
                    &op_mul(py, &component(3)?, &corners[index + 1])?,
                )?,
                &component(5)?,
            )?);
            index += 2;
        }
        let min_fn = builtins.getattr("min")?;
        let max_fn = builtins.getattr("max")?;
        mediabox.setattr(
            "lower_left",
            py_tuple(
                py,
                vec![
                    min_fn.call1((py_list(py, new_x.clone())?,))?,
                    min_fn.call1((py_list(py, new_y.clone())?,))?,
                ],
            )?,
        )?;
        mediabox.setattr(
            "upper_right",
            py_tuple(
                py,
                vec![
                    max_fn.call1((py_list(py, new_x)?,))?,
                    max_fn.call1((py_list(py, new_y)?,))?,
                ],
            )?,
        )?;
    }
    Ok(())
}

// @pypdf: pypdf._page.PageObject.scale
#[pyfunction]
pub fn page_scale<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    sx: &Bound<'py, PyAny>,
    sy: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let transformation = py_tuple(
        py,
        vec![
            sx.clone(),
            integer(py, 0),
            integer(py, 0),
            sy.clone(),
            integer(py, 0),
            integer(py, 0),
        ],
    )?;
    slf.call_method1("add_transformation", (transformation,))?;
    for box_name in ["bleedbox", "trimbox", "artbox", "cropbox", "mediabox"] {
        let rectangle = slf.getattr(box_name)?;
        let scaled = rectangle.call_method1("scale", (sx.clone(), sy.clone()))?;
        slf.setattr(box_name, scaled)?;
    }

    let annots_key = page_attr(py, "ANNOTS")?;
    let array_class = generic_class(py, "ArrayObject")?;
    if slf.contains(&annots_key)? {
        let annotations = slf.get_item(&annots_key)?;
        if annotations.is_instance(&array_class)? {
            let rect_key = constants_attr(py, "AnnotationDictionaryAttributes", "Rect")?;
            for annotation in annotations.try_iter()? {
                let annotation_obj = getattr_object(&annotation?)?;
                if annotation_obj.contains(&rect_key)? {
                    let rectangle = annotation_obj.get_item(&rect_key)?;
                    if rectangle.is_instance(&array_class)? {
                        let mut index = 0;
                        while index < 4 {
                            let factor = if index % 2 == 0 { &sx } else { &sy };
                            let scaled = op_mul(
                                py,
                                &builtins(py)?
                                    .getattr("float")?
                                    .call1((rectangle.get_item(index)?,))?,
                                factor,
                            )?;
                            rectangle.set_item(index, float_object(py, &scaled)?)?;
                            index += 1;
                        }
                    }
                }
            }
        }
    }

    let viewport_key = page_attr(py, "VP")?;
    if slf.contains(&viewport_key)? {
        let viewport = slf.get_item(&viewport_key)?;
        let bbox = if viewport.is_instance(&array_class)? {
            viewport.get_item(0)?.get_item("/BBox")?
        } else {
            viewport.get_item("/BBox")?
        };
        let scaled_bbox = rectangle_object(
            py,
            py_tuple(
                py,
                vec![
                    op_mul(
                        py,
                        &builtins(py)?
                            .getattr("float")?
                            .call1((bbox.get_item(0)?,))?,
                        sx,
                    )?,
                    op_mul(
                        py,
                        &builtins(py)?
                            .getattr("float")?
                            .call1((bbox.get_item(1)?,))?,
                        sy,
                    )?,
                    op_mul(
                        py,
                        &builtins(py)?
                            .getattr("float")?
                            .call1((bbox.get_item(2)?,))?,
                        sx,
                    )?,
                    op_mul(
                        py,
                        &builtins(py)?
                            .getattr("float")?
                            .call1((bbox.get_item(3)?,))?,
                        sy,
                    )?,
                ],
            )?,
        )?;
        if viewport.is_instance(&array_class)? {
            slf.get_item(&viewport_key)?
                .get_item(number_object(py, &integer(py, 0))?)?
                .set_item(name_object(py, "/BBox")?, scaled_bbox)?;
        } else {
            slf.get_item(&viewport_key)?
                .set_item(name_object(py, "/BBox")?, scaled_bbox)?;
        }
    }
    Ok(())
}

// @pypdf: pypdf._page.PageObject.scale_by
#[pyfunction]
pub fn page_scale_by<'py>(slf: &Bound<'py, PyAny>, factor: &Bound<'py, PyAny>) -> PyResult<()> {
    slf.call_method1("scale", (factor.clone(), factor.clone()))?;
    Ok(())
}

// @pypdf: pypdf._page.PageObject.scale_to
#[pyfunction]
pub fn page_scale_to<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    width: &Bound<'py, PyAny>,
    height: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let mediabox = slf.getattr("mediabox")?;
    let float_type = builtins(py)?.getattr("float")?;
    let sx = op_truediv(py, width, &float_type.call1((mediabox.getattr("width")?,))?)?;
    let sy = op_truediv(
        py,
        height,
        &float_type.call1((mediabox.getattr("height")?,))?,
    )?;
    slf.call_method1("scale", (sx, sy))?;
    Ok(())
}

// @pypdf: pypdf._page.PageObject.compress_content_streams
#[pyfunction]
#[pyo3(signature = (slf, level=-1))]
pub fn page_compress_content_streams<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    level: i64,
) -> PyResult<()> {
    let content = slf.call_method0("get_contents")?;
    if content.is_none() {
        return Ok(());
    }
    let content_obj = content.call_method1("flate_encode", (level,))?;
    let update: PyResult<()> = (|| {
        let reference = content.getattr("indirect_reference")?;
        let pdf = reference.getattr("pdf")?;
        let objects = pdf.getattr("_objects")?;
        let idnum = reference.getattr("idnum")?;
        let index = op_sub(py, &idnum, &integer(py, 1))?;
        objects.set_item(index, content_obj.clone())?;
        Ok(())
    })();
    match update {
        Ok(()) => {}
        Err(exception) => {
            if !exception.is_instance_of::<PyAttributeError>(py) {
                return Err(exception);
            }
            let indirect = slf.getattr("indirect_reference")?;
            let mut replace = false;
            if !indirect.is_none() {
                let pdf = indirect.getattr("pdf")?;
                replace = pdf.hasattr("_add_object")?;
            }
            if replace {
                slf.call_method1("replace_contents", (content_obj,))?;
            } else {
                let error = PyValueError::new_err("Page must be part of a PdfWriter");
                error.set_cause(py, Some(exception));
                return Err(error);
            }
        }
    }
    Ok(())
}

// @pypdf: pypdf._page.PageObject.page_number
#[pyfunction]
pub fn page_number<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let indirect = slf.getattr("indirect_reference")?;
    if indirect.is_none() {
        return Ok(py_none(py).unbind());
    }
    let pages = indirect.getattr("pdf")?.getattr("pages")?;
    for (number, page) in (0_i64..).zip(pages.try_iter()?) {
        let page = page?;
        if page.getattr("indirect_reference")?.eq(&indirect)? {
            return Ok(integer(py, number).unbind());
        }
    }
    Ok(py_none(py).unbind())
}

// @pypdf: pypdf._page.PageObject._debug_for_extract
#[pyfunction]
pub fn page_debug_for_extract<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<String> {
    let mut out = String::new();
    let contents = slf.get_item("/Contents")?.call_method0("get_object")?;
    let pdf = slf.getattr("pdf")?;
    let content_stream = globals(py, "ContentStream")?.call1((contents, pdf, "bytes"))?;
    for pair in content_stream.getattr("operations")?.try_iter()? {
        let pair = pair?;
        let operands = pair.get_item(0)?;
        let operator = pair.get_item(1)?;
        let strings: Vec<Bound<'py, PyAny>> = if eq_bytes(&operator, b"TJ")? {
            let first = operands.get_item(0)?;
            let mut strings = Vec::new();
            for value in first.try_iter()? {
                let value = value?;
                if value.is_instance(&builtins(py)?.getattr("str")?)? {
                    strings.push(value);
                }
            }
            strings
        } else {
            Vec::new()
        };
        out.push_str(
            &operator
                .call_method1("decode", ("utf-8",))?
                .extract::<String>()?,
        );
        out.push(' ');
        out.push_str(&py_str(
            &py_str_type(py, "").call_method1("join", (py_list(py, strings)?,))?,
        )?);
        out.push_str(&operands.repr()?.extract::<String>()?);
        out.push('\n');
    }
    out.push_str("\n=============================\n");
    let resources_key = page_attr(py, "RESOURCES")?;
    let font_result: PyResult<Vec<String>> = (|| {
        let mut lines = Vec::new();
        let resources = slf.get_item(&resources_key)?;
        let fonts = resources.get_item("/Font")?;
        for font_name in fonts.try_iter()? {
            let font_name = font_name?;
            lines.push(format!("{}\n", py_str(&font_name)?));
            let font = fonts.get_item(&font_name)?;
            lines.push(format!("{}\n", font.repr()?.extract::<String>()?));
            let encode_attempt: PyResult<String> = (|| {
                Ok(format!(
                    "{}\n",
                    font.get_item("/Encoding")?.repr()?.extract::<String>()?
                ))
            })();
            match encode_attempt {
                Ok(line) => lines.push(line),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(_) => {}
            }
            let unicode_attempt: PyResult<String> = (|| {
                Ok(format!(
                    "{}\n",
                    font.get_item("/ToUnicode")?
                        .call_method0("get_data")?
                        .call_method0("decode")?
                        .extract::<String>()?
                ))
            })();
            match unicode_attempt {
                Ok(line) => lines.push(line),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(_) => {}
            }
        }
        Ok(lines)
    })();
    match font_result {
        Ok(lines) => out.push_str(&lines.concat()),
        Err(error) if error.is_instance_of::<PyKeyError>(py) => out.push_str("No Font\n"),
        Err(error) => return Err(error),
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Annotations and actions
// ---------------------------------------------------------------------------

// @pypdf: pypdf._page.PageObject.annotations (getter)
#[pyfunction]
pub fn page_annotations_get<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    if !slf.contains("/Annots")? {
        return Ok(py_none(py).unbind());
    }
    let annotations = slf.get_item("/Annots")?.call_method0("get_object")?;
    if !annotations.is_instance(&generic_class(py, "ArrayObject")?)? {
        common::logger_warning(
            py,
            "Annotations are not an array: %(annotations)s",
            &[("annotations", annotations.clone())],
        )?;
        return Ok(py_none(py).unbind());
    }
    Ok(annotations.unbind())
}

// @pypdf: pypdf._page.PageObject.annotations (setter)
#[pyfunction]
pub fn page_annotations_set<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    if value.is_none() {
        if !slf.contains("/Annots")? {
            return Ok(());
        }
        slf.del_item(name_object(py, "/Annots")?)?;
    } else {
        slf.set_item(name_object(py, "/Annots")?, value.clone())?;
    }
    Ok(())
}

// @pypdf: pypdf._page.PageObject.add_action
#[pyfunction]
pub fn page_add_action<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    trigger: &Bound<'py, PyAny>,
    action: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(globals(py, "Action")?
        .getattr("_create_new")?
        .call1((slf.clone(), trigger.clone(), action.clone()))?
        .unbind())
}

// @pypdf: pypdf._page.PageObject.delete_action
#[pyfunction]
pub fn page_delete_action<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    trigger: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(globals(py, "Action")?
        .getattr("_delete")?
        .call1((slf.clone(), trigger.clone()))?
        .unbind())
}

// ---------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------

// @pypdf: pypdf._page.PageObject._get_fonts
#[pyfunction]
pub fn page_get_fonts<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let obj = slf.call_method0("get_object")?;
    if !obj.is_instance(&generic_class(py, "DictionaryObject")?)? {
        return Err(pyo3::exceptions::PyAssertionError::new_err(()));
    }
    let fonts = crate::pyutil::cached_module(py, "builtins")?.getattr("set")?.call0()?;
    let embedded = crate::pyutil::cached_module(py, "builtins")?.getattr("set")?.call0()?;
    let result = globals(py, "_get_fonts_walk")?.call1((obj, fonts.clone(), embedded.clone()))?;
    let fonts = result.get_item(0)?;
    let embedded = result.get_item(1)?;
    let unembedded = common::operator(py, "sub")?.call1((&fonts, &embedded))?;
    Ok(py_tuple(py, vec![embedded, unembedded])?.unbind())
}

// @pypdf: pypdf._page._get_fonts_walk
#[pyfunction]
pub fn page_get_fonts_walk<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    fnt: &Bound<'py, PyAny>,
    emb: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let fontkeys = ["/FontFile", "/FontFile2", "/FontFile3"];
    let process_font = |font: &Bound<'py, PyAny>| -> PyResult<()> {
        let font = getattr_object(font)?;
        if font.contains("/BaseFont")? {
            fnt.call_method1("add", (font.get_item("/BaseFont")?,))?;
        }
        let has_char_procs = font.contains("/CharProcs")?;
        let has_font_descriptor = font.contains("/FontDescriptor")?;
        let mut embedded_font = has_char_procs;
        if !embedded_font && has_font_descriptor {
            let descriptor = font.get_item("/FontDescriptor")?;
            for key in fontkeys {
                if descriptor.contains(key)? {
                    embedded_font = true;
                    break;
                }
            }
        }
        if !embedded_font && font.contains("/DescendantFonts")? {
            let descendant = font
                .get_item("/DescendantFonts")?
                .get_item(0)?
                .call_method0("get_object")?;
            if descendant.contains("/FontDescriptor")? {
                let descriptor = descendant.get_item("/FontDescriptor")?;
                for key in fontkeys {
                    if descriptor.contains(key)? {
                        embedded_font = true;
                        break;
                    }
                }
            }
        }
        if embedded_font {
            match font.get_item("/BaseFont") {
                Ok(base_font) => {
                    emb.call_method1("add", (base_font,))?;
                }
                Err(error) if error.is_instance_of::<PyKeyError>(py) => {
                    let subtype = font.get_item("/Subtype")?;
                    let value = format!("({})", py_str(&subtype)?);
                    emb.call_method1("add", (value,))?;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    };
    if obj.contains("/DR")? {
        let dr = obj.get_item("/DR")?;
        if dr.contains("/Font")? {
            for font in dr.get_item("/Font")?.call_method0("values")?.try_iter()? {
                process_font(&font?)?;
            }
        }
    }
    if obj.contains("/Resources")? {
        let mut resources = obj.get_item("/Resources")?.call_method0("get_object")?;
        if !resources.is_instance(&generic_class(py, "DictionaryObject")?)? {
            if !is_null_or_none(&resources)? {
                common::logger_warning(
                    py,
                    "Resources are not a dictionary: %(resources)s",
                    &[("resources", resources.clone())],
                )?;
            }
            resources = dict_object(py)?;
        }
        if resources.contains("/Font")? {
            for font in resources
                .get_item("/Font")?
                .call_method0("values")?
                .try_iter()?
            {
                process_font(&font?)?;
            }
        }
        if resources.contains("/XObject")? {
            for xobject in resources
                .get_item("/XObject")?
                .call_method0("values")?
                .try_iter()?
            {
                let xobject = getattr_object(&xobject?)?;
                globals(py, "_get_fonts_walk")?.call1((xobject, fnt.clone(), emb.clone()))?;
            }
        }
    }
    if obj.contains("/Annots")? {
        for annot in obj.get_item("/Annots")?.try_iter()? {
            let annot = getattr_object(&annot?)?;
            globals(py, "_get_fonts_walk")?.call1((annot, fnt.clone(), emb.clone()))?;
        }
    }
    if obj.contains("/AP")? {
        let normal = obj
            .get_item("/AP")?
            .get_item("/N")?
            .call_method0("get_object")?;
        if normal.is_instance(&generic_class(py, "StreamObject")?)? {
            // /Type is optional in an appearance stream
            globals(py, "_get_fonts_walk")?.call1((normal, fnt.clone(), emb.clone()))?;
        } else {
            // One appearance stream per state, e.g. /Off and /Yes of a check box
            for stream in normal.call_method0("values")?.try_iter()? {
                let stream = getattr_object(&stream?)?;
                globals(py, "_get_fonts_walk")?.call1((stream, fnt.clone(), emb.clone()))?;
            }
        }
    }
    Ok(py_tuple(py, vec![fnt.clone(), emb.clone()])?.unbind())
}

pub fn register(module: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    module.add_function(pyo3::wrap_pyfunction!(page_init, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_hash_bin, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_hash_value_data, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_user_unit, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_create_blank_page, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_get_page_resources, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_get_font_resources, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_get_rectangle, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_set_rectangle, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_delete_rectangle, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_get_ids_image, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_get_image, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_images, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_inline_images_get, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_inline_images_set, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(
        page_translate_value_inline_image,
        module
    )?)?;
    module.add_function(pyo3::wrap_pyfunction!(
        page_parse_images_from_content_stream,
        module
    )?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_rotation_get, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_rotation_set, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(
        page_transfer_rotation_to_content,
        module
    )?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_rotate, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_merge_resources, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_content_stream_rename, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(
        page_add_transformation_matrix,
        module
    )?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_get_contents_as_bytes, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_get_contents, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_replace_contents, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_merge_page, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_merge_page_inner, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_merge_page_writer, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_expand_mediabox, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_merge_transformed_page, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_merge_scaled_page, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_merge_rotated_page, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_merge_translated_page, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_add_transformation, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_scale, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_scale_by, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_scale_to, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(
        page_compress_content_streams,
        module
    )?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_number, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_debug_for_extract, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_annotations_get, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_annotations_set, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_add_action, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_delete_action, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_get_fonts, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_get_fonts_walk, module)?)?;
    Ok(())
}
