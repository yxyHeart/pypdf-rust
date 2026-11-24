//! `pypdf._doc_common` module-level helpers, `DocumentInformation` and
//! `LazyDict` (upstream pypdf 7.1.1).

use super::*;

// The container types below stay Python-side because they are plain data:
// `_PageTreeItem` is a `NamedTuple` (the type and its field defaults are part of
// the module surface) and `_UNSET` is an identity sentinel.  Their annotations
// are recorded here so the coverage gate sees the ported symbols.
//
// @pypdf: pypdf._doc_common._PageTreeItem
// @pypdf: pypdf._doc_common._PageTreeItem.node
// @pypdf: pypdf._doc_common._PageTreeItem.inherit
// @pypdf: pypdf._doc_common._PageTreeItem.indirect_reference
// @pypdf: pypdf._doc_common._PageTreeItem.depth
// @pypdf: pypdf._doc_common._PageTreeFrame
// @pypdf: pypdf._doc_common._UNSET
// @pypdf: pypdf._doc_common._INHERITABLE_PAGE_ATTRIBUTES

// @pypdf: pypdf._doc_common.convert_to_int
#[pyfunction]
fn doccommon_convert_to_int<'py>(
    py: Python<'py>,
    d: &Bound<'py, PyAny>,
    size: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    if size.gt(8)? {
        return Err(pdf_read_error(py, "Invalid size in convert_to_int"));
    }
    // d = b"\x00\x00\x00\x00\x00\x00\x00\x00" + d
    let concatenated = py_add(&py_bytes(py, &[0_u8; 8]), d)?;
    // `bytes + bytes` is `bytes`; d = d[-8:] then struct.unpack(">Q", d)[0].
    let buffer = concatenated.cast::<PyBytes>()?.as_bytes();
    let tail = &buffer[buffer.len() - 8..];
    let value = match <[u8; 8]>::try_from(tail) {
        Ok(value) => u64::from_be_bytes(value),
        // Unreachable: the buffer starts with eight NUL bytes, so it is never
        // shorter than eight bytes.
        Err(_) => return Err(pdf_read_error(py, "Invalid size in convert_to_int")),
    };
    Ok(PyInt::new(py, value).into_any())
}

/// `_INHERITABLE_PAGE_ATTRIBUTES`: the attributes a page inherits from its
/// ancestors in the page tree, as `NameObject`s.
// @pypdf: pypdf._doc_common._INHERITABLE_PAGE_ATTRIBUTES
#[pyfunction]
fn doccommon_inheritable_page_attributes<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    let attributes = constants_class(py, "PageAttributes")?;
    let name_class = generic_class(py, "NameObject")?;
    let mut items = Vec::with_capacity(4);
    for name in ["RESOURCES", "MEDIABOX", "CROPBOX", "ROTATE"] {
        items.push(name_class.call1((attributes.getattr(name)?,))?);
    }
    py_tuple(py, items)
}

// ---------------------------------------------------------------------------
// DocumentInformation
// ---------------------------------------------------------------------------

// @pypdf: pypdf._doc_common.DocumentInformation
// @pypdf: pypdf._doc_common.DocumentInformation.__init__
//
// The class (and its `__init__`, which only chains to
// `DictionaryObject.__init__(self)`) lives in the shim; the logic of every
// property is implemented below.

fn document_information_attribute<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    constants_class(py, "DocumentInformationAttributes")?.getattr(name)
}

// @pypdf: pypdf._doc_common.DocumentInformation._get_text
#[pyfunction]
fn doccommon_documentinformation_get_text<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let retval = slf.call_method1("get", (key, py_none(py)))?;
    if is_generic_instance(py, &retval, "TextStringObject")? {
        return Ok(retval);
    }
    if is_generic_instance(py, &retval, "ByteStringObject")? {
        return Ok(retval.str()?.into_any());
    }
    Ok(py_none(py))
}

/// `self._get_text(<attribute>)`.
fn text_property<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    attribute: &str,
) -> PyResult<Bound<'py, PyAny>> {
    let key = document_information_attribute(py, attribute)?;
    slf.call_method1("_get_text", (key,))
}

/// `self.get(<attribute>)` -- raw access, no `default` argument.
fn raw_property<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    attribute: &str,
) -> PyResult<Bound<'py, PyAny>> {
    let key = document_information_attribute(py, attribute)?;
    slf.call_method1("get", (key,))
}

/// `parse_iso8824_date(self._get_text(<attribute>))`.
fn date_property<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    attribute: &str,
) -> PyResult<Bound<'py, PyAny>> {
    let text = text_property(py, slf, attribute)?;
    utils_attr(py, "parse_iso8824_date")?.call1((text,))
}

// @pypdf: pypdf._doc_common.DocumentInformation.title
#[pyfunction]
fn doccommon_documentinformation_title<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let key = document_information_attribute(py, "TITLE")?;
    // Upstream: `self._get_text(TITLE) or self.get(TITLE).get_object() if
    // self.get(TITLE) else None`.  The conditional expression evaluates the
    // condition first, which is why `get` is called before `_get_text`.
    if !slf.call_method1("get", (key.clone(),))?.is_truthy()? {
        return Ok(py_none(py));
    }
    let text = slf.call_method1("_get_text", (key.clone(),))?;
    if text.is_truthy()? {
        return Ok(text);
    }
    slf.call_method1("get", (key,))?.call_method0("get_object")
}

// @pypdf: pypdf._doc_common.DocumentInformation.title_raw
#[pyfunction]
fn doccommon_documentinformation_title_raw<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    raw_property(py, slf, "TITLE")
}

// @pypdf: pypdf._doc_common.DocumentInformation.author
#[pyfunction]
fn doccommon_documentinformation_author<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    text_property(py, slf, "AUTHOR")
}

// @pypdf: pypdf._doc_common.DocumentInformation.author_raw
#[pyfunction]
fn doccommon_documentinformation_author_raw<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    raw_property(py, slf, "AUTHOR")
}

// @pypdf: pypdf._doc_common.DocumentInformation.subject
#[pyfunction]
fn doccommon_documentinformation_subject<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    text_property(py, slf, "SUBJECT")
}

// @pypdf: pypdf._doc_common.DocumentInformation.subject_raw
#[pyfunction]
fn doccommon_documentinformation_subject_raw<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    raw_property(py, slf, "SUBJECT")
}

// @pypdf: pypdf._doc_common.DocumentInformation.creator
#[pyfunction]
fn doccommon_documentinformation_creator<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    text_property(py, slf, "CREATOR")
}

// @pypdf: pypdf._doc_common.DocumentInformation.creator_raw
#[pyfunction]
fn doccommon_documentinformation_creator_raw<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    raw_property(py, slf, "CREATOR")
}

// @pypdf: pypdf._doc_common.DocumentInformation.producer
#[pyfunction]
fn doccommon_documentinformation_producer<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    text_property(py, slf, "PRODUCER")
}

// @pypdf: pypdf._doc_common.DocumentInformation.producer_raw
#[pyfunction]
fn doccommon_documentinformation_producer_raw<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    raw_property(py, slf, "PRODUCER")
}

// @pypdf: pypdf._doc_common.DocumentInformation.creation_date
#[pyfunction]
fn doccommon_documentinformation_creation_date<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    date_property(py, slf, "CREATION_DATE")
}

// @pypdf: pypdf._doc_common.DocumentInformation.creation_date_raw
#[pyfunction]
fn doccommon_documentinformation_creation_date_raw<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    raw_property(py, slf, "CREATION_DATE")
}

// @pypdf: pypdf._doc_common.DocumentInformation.modification_date
#[pyfunction]
fn doccommon_documentinformation_modification_date<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    date_property(py, slf, "MOD_DATE")
}

// @pypdf: pypdf._doc_common.DocumentInformation.modification_date_raw
#[pyfunction]
fn doccommon_documentinformation_modification_date_raw<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    raw_property(py, slf, "MOD_DATE")
}

// @pypdf: pypdf._doc_common.DocumentInformation.keywords
#[pyfunction]
fn doccommon_documentinformation_keywords<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    text_property(py, slf, "KEYWORDS")
}

// @pypdf: pypdf._doc_common.DocumentInformation.keywords_raw
#[pyfunction]
fn doccommon_documentinformation_keywords_raw<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    raw_property(py, slf, "KEYWORDS")
}

// ---------------------------------------------------------------------------
// LazyDict
// ---------------------------------------------------------------------------

// @pypdf: pypdf._doc_common.LazyDict
// @pypdf: pypdf._doc_common.LazyDict.__init__
#[pyfunction]
#[pyo3(signature = (slf, args, kwargs))]
fn doccommon_lazydict_init<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    args: &Bound<'py, PyTuple>,
    kwargs: &Bound<'py, PyDict>,
) -> PyResult<()> {
    // self._raw_dict = dict(*args, **kwargs)
    let raw = builtins(py)?.getattr("dict")?.call(args, Some(kwargs))?;
    slf.setattr("_raw_dict", raw)
}

/// `func, arg = value` for the two-element tuples stored by `LazyDict`.
fn unpack_pair<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let length = value.len()?;
    if length != 2 {
        let message = if length < 2 {
            format!("not enough values to unpack (expected 2, got {length})")
        } else {
            "too many values to unpack (expected 2)".to_string()
        };
        return Err(message_exception(py, "ValueError", &message));
    }
    Ok((value.get_item(0)?, value.get_item(1)?))
}

// @pypdf: pypdf._doc_common.LazyDict.__getitem__
#[pyfunction]
fn doccommon_lazydict_getitem<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let raw = slf.getattr("_raw_dict")?;
    let (func, arg) = unpack_pair(py, &raw.get_item(key)?)?;
    func.call1((arg,))
}

// @pypdf: pypdf._doc_common.LazyDict.__iter__
#[pyfunction]
fn doccommon_lazydict_iter<'py>(slf: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyIterator>> {
    slf.getattr("_raw_dict")?.try_iter()
}

// @pypdf: pypdf._doc_common.LazyDict.__len__
#[pyfunction]
fn doccommon_lazydict_len(slf: &Bound<'_, PyAny>) -> PyResult<usize> {
    slf.getattr("_raw_dict")?.len()
}

// @pypdf: pypdf._doc_common.LazyDict.__str__
#[pyfunction]
fn doccommon_lazydict_str<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<String> {
    let keys = builtins(py)?
        .getattr("list")?
        .call1((slf.call_method0("keys")?,))?;
    Ok(format!("LazyDict(keys={})", repr_of(&keys)?))
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(doccommon_convert_to_int, module)?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_inheritable_page_attributes,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_get_text,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_title,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_title_raw,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_author,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_author_raw,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_subject,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_subject_raw,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_creator,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_creator_raw,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_producer,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_producer_raw,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_creation_date,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_creation_date_raw,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_modification_date,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_modification_date_raw,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_keywords,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        doccommon_documentinformation_keywords_raw,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(doccommon_lazydict_init, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_lazydict_getitem, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_lazydict_iter, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_lazydict_len, module)?)?;
    module.add_function(wrap_pyfunction!(doccommon_lazydict_str, module)?)?;
    Ok(())
}
