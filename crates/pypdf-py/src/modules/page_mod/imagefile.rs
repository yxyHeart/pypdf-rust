//! `pypdf._page.ImageFile`, `VirtualListImages` and `_VirtualList`
//! (upstream pypdf 7.1.1).

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyTuple};

use super::callables;
use super::common::{
    self, index_error, op_add, op_sub, py_hash, py_int, py_len, py_list, py_str, py_str_type,
    py_tuple, slice_object, type_error,
};
use crate::errors;

/// `assert condition` (no message).
fn plain_assert(_py: Python<'_>) -> PyErr {
    pyo3::exceptions::PyAssertionError::new_err(())
}

fn pil_not_imported(py: Python<'_>) -> PyResult<bool> {
    common::shim(py)?.getattr("pil_not_imported")?.extract()
}

// @pypdf: pypdf._page.ImageFile.replace
#[pyfunction]
#[pyo3(signature = (slf, new_image, kwargs))]
pub fn page_imagefile_replace<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    new_image: &Bound<'py, PyAny>,
    kwargs: &Bound<'py, PyDict>,
) -> PyResult<()> {
    if pil_not_imported(py)? {
        return Err(pyo3::exceptions::PyImportError::new_err(
            "pillow is required to do image extraction. It can be installed via 'pip install pypdf[image]'",
        ));
    }
    // Upstream late imports: `PdfReader`, `_xobj_to_image`.
    let reader_class = crate::pyutil::cached_module(py, "pypdf._reader")?.getattr("PdfReader")?;
    let xobj_to_image = py
        .import("pypdf.generic._image_xobject")?
        .getattr("_xobj_to_image")?;

    let indirect = slf.getattr("indirect_reference")?;
    if indirect.is_none() {
        return Err(common::type_error(py, "Cannot update an inline image."));
    }
    let pdf = indirect.getattr("pdf")?;
    if !pdf.hasattr("_id_translated")? {
        return Err(common::type_error(
            py,
            "Cannot update an image not belonging to a PdfWriter.",
        ));
    }
    let image_class = common::shim(py)?.getattr("Image")?;
    if !new_image.is_instance(&image_class)? {
        return Err(common::type_error(py, "new_image shall be a PIL Image"));
    }

    let buffer = py.import("io")?.getattr("BytesIO")?.call0()?;
    new_image.call_method("save", (buffer.clone(), "PDF"), Some(kwargs))?;
    let reader = reader_class.call1((buffer,))?;
    let page_image = reader
        .getattr("pages")?
        .get_item(0)?
        .getattr("images")?
        .get_item(0)?;
    let page_image_reference = page_image.getattr("indirect_reference")?;
    if page_image_reference.is_none() {
        return Err(plain_assert(py));
    }
    let objects = pdf.getattr("_objects")?;
    let idnum = indirect.getattr("idnum")?;
    let index = op_sub(py, &idnum, &py_int(py, 1))?;
    objects.set_item(&index, page_image_reference.call_method0("get_object")?)?;

    let target = indirect.call_method0("get_object")?;
    target.setattr("indirect_reference", indirect.clone())?;
    let call_kwargs = PyDict::new(py);
    call_kwargs.set_item("pillow_parameters", kwargs)?;
    let converted = xobj_to_image.call((target,), Some(&call_kwargs))?;
    let converted = converted.cast::<PyTuple>()?;
    let extension = converted.get_item(0)?;
    let byte_stream = converted.get_item(1)?;
    let image = converted.get_item(2)?;
    if extension.is_none() {
        return Err(plain_assert(py));
    }
    // self.name = self.name[: self.name.rfind(".")] + extension
    let name = slf.getattr("name")?;
    let rfind = name.call_method1("rfind", (".",))?;
    let prefix = name.get_item(slice_object(py, None, Some(rfind), None)?)?;
    slf.setattr("name", op_add(py, &prefix, &extension)?)?;
    slf.setattr("data", byte_stream)?;
    slf.setattr("image", image)?;
    Ok(())
}

// @pypdf: pypdf._page.ImageFile.__str__
#[pyfunction]
pub fn page_imagefile_str<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<String> {
    let class_name: String = slf.get_type().getattr("__name__")?.extract()?;
    let name = py_str(&slf.getattr("name")?)?;
    let data = slf.getattr("data")?;
    let length = py_len(&data)?;
    let human = common::globals(py, "_human_readable_bytes")?.call1((length,))?;
    Ok(format!(
        "{}(name={}, data: {})",
        class_name,
        name,
        py_str(&human)?
    ))
}

// @pypdf: pypdf._page.ImageFile.__repr__
#[pyfunction]
pub fn page_imagefile_repr<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let rendered = slf.call_method0("__str__")?;
    let head = rendered.get_item(slice_object(py, None, Some(py_int(py, -1)), None)?)?;
    let data = slf.getattr("data")?;
    let hash = py_hash(&data)?;
    let tail = format!(", hash: {})", hash);
    Ok(op_add(py, &head, &py_str_type(py, &tail))?.unbind())
}

// ---------------------------------------------------------------------------
// VirtualListImages
// ---------------------------------------------------------------------------

// @pypdf: pypdf._page.VirtualListImages.__len__
#[pyfunction]
pub fn page_virtual_listimages_len(slf: &Bound<'_, PyAny>) -> PyResult<usize> {
    let ids = slf.getattr("ids_function")?.call0()?;
    py_len(&ids)
}

// @pypdf: pypdf._page.VirtualListImages.keys
#[pyfunction]
pub fn page_virtual_listimages_keys<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let _ = py;
    Ok(slf.getattr("ids_function")?.call0()?.unbind())
}

// @pypdf: pypdf._page.VirtualListImages.items
#[pyfunction]
pub fn page_virtual_listimages_items<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let ids = slf.getattr("ids_function")?.call0()?;
    let get_function = slf.getattr("get_function")?;
    let mut items: Vec<Bound<'py, PyAny>> = Vec::new();
    for id in ids.try_iter()? {
        let id = id?;
        let value = get_function.call1((id.clone(),))?;
        items.push(py_tuple(py, vec![id, value])?);
    }
    Ok(py_list(py, items)?.unbind())
}

// @pypdf: pypdf._page.VirtualListImages.__getitem__
#[pyfunction]
pub fn page_virtual_listimages_getitem<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    index: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let ids = slf.getattr("ids_function")?.call0()?;
    let builtins = common::builtins(py)?;
    if index.is_instance(&builtins.getattr("slice")?)? {
        let len_self = common::py_len(&slf.getattr("ids_function")?.call0()?)?;
        let indices = index.call_method1("indices", (len_self,))?;
        let range = common::range_from_tuple(py, &indices)?;
        let mut selected: Vec<Bound<'py, PyAny>> = Vec::new();
        for position in range.try_iter()? {
            let position = position?;
            selected.push(ids.get_item(&position)?);
        }
        let sliced = py_list(py, selected)?;
        let class = slf.get_type();
        let ids_function = callables::constant_callable(py, sliced)?;
        let get_function = slf.getattr("get_function")?;
        return Ok(class.call1((ids_function, get_function))?.unbind());
    }
    if index.is_instance(&builtins.getattr("str")?)?
        || index.is_instance(&builtins.getattr("list")?)?
        || index.is_instance(&builtins.getattr("tuple")?)?
    {
        return Ok(slf.getattr("get_function")?.call1((index,))?.unbind());
    }
    if !index.is_instance(&builtins.getattr("int")?)? {
        return Err(type_error(py, "Invalid sequence indices type"));
    }
    let len_object = builtins.getattr("len")?.call1((&ids,))?;
    let zero = py_int(py, 0);
    let mut position = index.clone();
    if position.lt(&zero)? {
        // support negative indexes
        position = op_add(py, &position, &len_object)?;
    }
    if !(!position.lt(&zero)? && position.lt(&len_object)?) {
        return Err(index_error(py, "Sequence index out of range"));
    }
    let target = ids.get_item(&position)?;
    Ok(slf.getattr("get_function")?.call1((target,))?.unbind())
}

// @pypdf: pypdf._page.VirtualListImages.__str__
#[pyfunction]
pub fn page_virtual_listimages_str<'py>(slf: &Bound<'py, PyAny>) -> PyResult<String> {
    let ids = slf.getattr("ids_function")?.call0()?;
    let mut parts: Vec<String> = Vec::new();
    for (index, name) in ids.try_iter()?.enumerate() {
        let name = name?;
        parts.push(format!("Image_{}={}", index, py_str(&name)?));
    }
    Ok(format!("[{}]", parts.join(", ")))
}

// ---------------------------------------------------------------------------
// _VirtualList
// ---------------------------------------------------------------------------

// @pypdf: pypdf._page._VirtualList.__len__
#[pyfunction]
pub fn page_virtual_list_len(slf: &Bound<'_, PyAny>) -> PyResult<usize> {
    // Upstream returns `self.length_function()` unchanged; Python's len() only
    // requires an integer, so the value is converted, not re-measured.
    let length = slf.getattr("length_function")?.call0()?;
    length.extract::<usize>()
}

// @pypdf: pypdf._page._VirtualList.__getitem__
#[pyfunction]
pub fn page_virtual_list_getitem<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    index: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let builtins = common::builtins(py)?;
    if index.is_instance(&builtins.getattr("slice")?)? {
        let len_self = py_len(slf)?;
        let indices = index.call_method1("indices", (len_self,))?;
        let range = common::range_from_tuple(py, &indices)?;
        let class = slf.get_type();
        let length_function = range.getattr("__len__")?;
        // Upstream closes over the `range` object, not over `index.indices()`:
        // `lambda idx: self[indices[idx]]`.
        let get_function = Py::new(py, callables::IndexCallable::new(slf.clone(), range))?;
        return Ok(class.call1((length_function, get_function))?.unbind());
    }
    if !index.is_instance(&builtins.getattr("int")?)? {
        return Err(type_error(py, "Sequence indices must be integers"));
    }
    let len_object = builtins.getattr("len")?.call1((slf,))?;
    let zero = py_int(py, 0);
    let mut position = index.clone();
    if position.lt(&zero)? {
        position = op_add(py, &position, &len_object)?;
    }
    if !(!position.lt(&zero)? && position.lt(&len_object)?) {
        return Err(index_error(py, "Sequence index out of range"));
    }
    Ok(slf.getattr("get_function")?.call1((position,))?.unbind())
}

// @pypdf: pypdf._page._VirtualList.__delitem__
#[pyfunction]
pub fn page_virtual_list_delitem<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    index: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let builtins = common::builtins(py)?;
    if index.is_instance(&builtins.getattr("slice")?)? {
        let len_self = py_len(slf)?;
        let indices = index.call_method1("indices", (len_self,))?;
        let range = common::range_from_tuple(py, &indices)?;
        let list = builtins.getattr("list")?.call1((range,))?;
        list.call_method0("sort")?;
        list.call_method0("reverse")?;
        for item in list.try_iter()? {
            let item = item?;
            slf.call_method1("__delitem__", (item,))?;
        }
        return Ok(());
    }
    if !index.is_instance(&builtins.getattr("int")?)? {
        return Err(type_error(py, "Index must be integers"));
    }
    let len_object = builtins.getattr("len")?.call1((slf,))?;
    let zero = py_int(py, 0);
    let mut position = index.clone();
    if position.lt(&zero)? {
        position = op_add(py, &position, &len_object)?;
    }
    if !(!position.lt(&zero)? && position.lt(&len_object)?) {
        return Err(index_error(py, "Index out of range"));
    }

    let mut indirect = slf
        .call_method1("__getitem__", (&position,))?
        .getattr("indirect_reference")?;
    if indirect.is_none() {
        return Err(plain_assert(py));
    }
    let mut parent = Some(
        indirect
            .call_method0("get_object")?
            .call_method1("get", ("/Parent", py.None()))?,
    );
    let mut first = true;
    while let Some(current) = parent.clone() {
        if current.is_none() {
            break;
        }
        let current = current.call_method0("get_object")?;
        let mut advance_parent = true;
        match current
            .get_item("/Kids")?
            .call_method1("index", (&indirect,))
        {
            Ok(found) => {
                let kids = current.get_item("/Kids")?;
                kids.del_item(&found)?;
                first = false;
                // `del ind.pdf.flattened_pages[index]`: failures other than
                // MemoryError are swallowed by upstream.
                let deletion: PyResult<()> = (|| {
                    let pdf = indirect.getattr("pdf")?;
                    pdf.getattr("flattened_pages")?.del_item(&position)?;
                    Ok(())
                })();
                match deletion {
                    Ok(()) => {}
                    Err(error) => {
                        if error.is_instance_of::<pyo3::exceptions::PyMemoryError>(py) {
                            return Err(error);
                        }
                    }
                }
                if current.contains("/Count")? {
                    let count = current.get_item("/Count")?;
                    let decremented = op_sub(
                        py,
                        &builtins.getattr("int")?.call1((count,))?,
                        &py_int(py, 1),
                    )?;
                    current.set_item(
                        common::name_object(py, "/Count")?,
                        common::number_object(py, &decremented)?,
                    )?;
                }
                if py_len(&current.get_item("/Kids")?)? == 0 {
                    // No more objects in this part of this subtree
                    indirect = current.getattr("indirect_reference")?;
                }
                parent = Some(current.call_method1("get", ("/Parent", py.None()))?);
            }
            Err(error) => {
                if !error.is_instance_of::<PyValueError>(py) {
                    return Err(error);
                }
                if first {
                    let message = format!("Page not found in page tree: {}", py_str(&indirect)?);
                    let raised = errors::pdf_read_error(py, &message);
                    raised.set_cause(py, Some(error));
                    return Err(raised);
                }
                advance_parent = false;
            }
        }
        if !advance_parent {
            break;
        }
    }
    Ok(())
}

// @pypdf: pypdf._page._VirtualList.__str__
#[pyfunction]
pub fn page_virtual_list_str<'py>(slf: &Bound<'py, PyAny>) -> PyResult<String> {
    let length: usize = slf.getattr("length_function")?.call0()?.extract()?;
    let mut parts: Vec<String> = Vec::with_capacity(length);
    for index in 0..length {
        parts.push(format!("PageObject({})", index));
    }
    Ok(format!("[{}]", parts.join(", ")))
}

pub fn register(module: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    module.add_function(pyo3::wrap_pyfunction!(page_imagefile_replace, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_imagefile_str, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_imagefile_repr, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_virtual_listimages_len, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(
        page_virtual_listimages_keys,
        module
    )?)?;
    module.add_function(pyo3::wrap_pyfunction!(
        page_virtual_listimages_items,
        module
    )?)?;
    module.add_function(pyo3::wrap_pyfunction!(
        page_virtual_listimages_getitem,
        module
    )?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_virtual_listimages_str, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_virtual_list_len, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_virtual_list_getitem, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_virtual_list_delitem, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_virtual_list_str, module)?)?;
    Ok(())
}
