//! Rust port of the embedded-file and image modules (upstream pypdf 7.1.1):
//!
//! * `pypdf/generic/_files.py`            -> [`files`] section
//! * `pypdf/generic/_image_inline.py`     -> [`image_inline`] section
//! * `pypdf/generic/_image_xobject.py`    -> [`image_xobject`] section
//!
//! # Design
//!
//! Upstream's object model is Python-native, so all three ports manipulate
//! Python objects exactly like upstream does (`DictionaryObject`, streams,
//! `IndirectObject`, `NameObject`, ...).  The class definitions of
//! `EmbeddedFile` live in the hand-written shim
//! `python/pypdf/generic/_files.py`; every function body and method body is a
//! `#[pyfunction]` implemented here and registered on `pypdf._pypdf`.
//!
//! # Fidelity notes
//!
//! * Pillow is *not* reimplemented.  Every image operation goes through PyO3 to
//!   the installed Pillow (`PIL.Image`), with upstream's exact arguments and
//!   argument order, so encoded/decoded bytes match by construction
//!   (docs/FIDELITY_CONTRACT.md §5.1: dependency parity).
//! * Inline-image parsing performs the same `read`/`seek`/`tell` protocol on the
//!   caller's stream as upstream, including its `EI` heuristics, warnings and
//!   error texts; the byte buffers are plain `Vec<u8>` and the Python slicing
//!   semantics (negative indices, out-of-range slices) are reproduced explicitly.
//! * Functions that upstream calls through its module globals are called here
//!   through this project's shim modules (`pypdf.generic._image_inline`,
//!   `pypdf.generic._image_xobject`, ...), so monkeypatching (e.g.
//!   `extract_inline_default` in the official tests) keeps working.
//! * Log records are emitted with `logger_warning(..., source=<upstream module>)`
//!   and the upstream `%(name)s` message templates, so `caplog` output is
//!   identical.
//! * No Rust hash containers influence output; only `Vec`/`Option` and Python
//!   objects (dict/list/set) are used where upstream uses them.

use pyo3::exceptions::{
    PyIndexError, PyKeyError, PyMemoryError, PyOSError, PyTypeError, PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::{
    PyAny, PyBool, PyBytes, PyDict, PyFloat, PyInt, PyIterator, PyList, PyModule, PySet, PyString,
    PyTuple,
};

use crate::errors;
use crate::pyutil;

/// Upstream module name of the embedded-file module (log source).
const FILES_SOURCE: &str = "pypdf.generic._files";
/// Upstream module name of the inline-image module (log source).
const INLINE_SOURCE: &str = "pypdf.generic._image_inline";
/// Upstream module name of the image-XObject module (log source).
const XOBJECT_SOURCE: &str = "pypdf.generic._image_xobject";

// @pypdf: pypdf.generic._image_inline.BUFFER_SIZE
/// `BUFFER_SIZE` (upstream module constant, inline images).
const BUFFER_SIZE: usize = 8192;

// @pypdf: pypdf.generic._image_xobject.MAX_IMAGE_MODE_NESTING_DEPTH
/// `MAX_IMAGE_MODE_NESTING_DEPTH` (upstream module constant).
const MAX_IMAGE_MODE_NESTING_DEPTH: i64 = 10;

// @pypdf: pypdf.generic._image_xobject._PROCESS_COLORANTS
/// `_PROCESS_COLORANTS` (upstream module constant).
const PROCESS_COLORANTS: [&str; 4] = ["/Cyan", "/Magenta", "/Yellow", "/Black"];

/// `WHITESPACES_AS_BYTES` (from `pypdf._utils`): `b"\x00\t\n\f\r "`.
const WHITESPACES_BYTES: [u8; 6] = [0x00, 0x09, 0x0a, 0x0c, 0x0d, 0x20];

fn is_whitespace_byte(byte: u8) -> bool {
    WHITESPACES_BYTES.contains(&byte)
}

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    crate::pyutil::cached_module(py, "builtins")
}

/// `pypdf.get_configuration()` (upstream imports `get_configuration` directly).
fn configuration<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    crate::pyutil::cached_module(py, "pypdf._configuration")?
        .getattr("get_configuration")?
        .call0()
}

fn config_item<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    configuration(py)?.getattr(name)
}

/// Fetch a class from `pypdf.generic` (upstream `from ..generic import ...` /
/// `from ._base import ...` names).
fn generic_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    crate::pyutil::cached_module(py, "pypdf.generic")?.getattr(name)
}

/// Fetch a name from one of this port's Python shim modules (used for the
/// upstream module-global lookups so that monkeypatching keeps working).
fn shim_attr<'py>(py: Python<'py>, module: &str, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import(module)?.getattr(name)
}

fn files_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, "pypdf.generic._files", name)
}

fn inline_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, "pypdf.generic._image_inline", name)
}

fn utils_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, "pypdf._utils", name)
}

/// `pypdf.constants.<class>.<attribute>`.
fn constant<'py>(
    py: Python<'py>,
    class_name: &str,
    attribute: &str,
) -> PyResult<Bound<'py, PyAny>> {
    crate::pyutil::cached_module(py, "pypdf.constants")?
        .getattr(class_name)?
        .getattr(attribute)
}

fn py_int<'py>(py: Python<'py>, value: i64) -> Bound<'py, PyAny> {
    PyInt::new(py, value).into_any()
}

fn py_float<'py>(py: Python<'py>, value: f64) -> Bound<'py, PyAny> {
    PyFloat::new(py, value).into_any()
}

fn py_str<'py>(py: Python<'py>, value: &str) -> Bound<'py, PyAny> {
    PyString::new(py, value).into_any()
}

fn py_bytes<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, data).into_any()
}

fn py_none<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    py.None().into_bound(py)
}

fn py_bool<'py>(py: Python<'py>, value: bool) -> Bound<'py, PyAny> {
    PyBool::new(py, value).to_owned().into_any()
}

fn py_list<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyList::new(py, items)?.into_any())
}

fn py_tuple<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyTuple::new(py, items)?.into_any())
}

/// `set()` (a *Python* set, never a Rust hash container).
fn py_set<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PySet::empty(py)?.into_any())
}

/// `id(obj)` as a Python int (upstream stores `id()` values in a Python set).
fn py_id<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("id")?.call1((obj,))
}

/// `isinstance(obj, <class>)`.
fn is_instance<'py>(obj: &Bound<'py, PyAny>, class: &Bound<'py, PyAny>) -> PyResult<bool> {
    obj.is_instance(class)
}

fn is_instance_of<T: pyo3::PyTypeCheck>(obj: &Bound<'_, PyAny>) -> bool {
    obj.is_instance_of::<T>()
}

/// `is_null_or_none(obj)` (upstream helper from `pypdf.generic`).
fn is_null_or_none(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    generic_class(py, "is_null_or_none")?
        .call1((obj,))?
        .is_truthy()
}

/// `operator.<name>(a, b)` -- the full Python binary-operator protocol.
///
/// Calling `a.__add__(b)` directly is *not* equivalent: for a numeric subclass
/// such as `int` mixed with `float`, the reflected slot is what CPython falls
/// back to, and the dunder call would yield `NotImplemented` instead of a value.
fn operator_call<'py>(
    py: Python<'py>,
    name: &str,
    a: &Bound<'py, PyAny>,
    b: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    py.import("operator")?.getattr(name)?.call1((a, b))
}

/// `repr(obj)` as a `String`.
fn repr_of(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    pyutil::py_repr(obj)
}

/// `str(obj)` as a `String`.
fn str_of(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    pyutil::py_str(obj)
}

/// `bytes`-like object -> `Vec<u8>`.
fn bytes_of(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    obj.extract::<Vec<u8>>()
}

/// `obj == b"..."`.
fn eq_bytes(obj: &Bound<'_, PyAny>, py: Python<'_>, data: &[u8]) -> PyResult<bool> {
    obj.eq(py_bytes(py, data))
}

/// `len(obj)`.
fn py_len(obj: &Bound<'_, PyAny>) -> PyResult<usize> {
    obj.len()
}

/// Build `<module>.<class>(...)` from `pypdf.constants`.
fn null_object<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "NullObject")?.call0()
}

fn name_object<'py>(py: Python<'py>, value: &str) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "NameObject")?.call1((value,))
}

/// `logger_warning(message, source=..., **values)`.
fn log_warning(
    py: Python<'_>,
    source: &str,
    message: &str,
    values: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    pyutil::logger_warning(py, source, message, values)
}

/// `logger_warning(message, source=...)` without format values.
fn log_plain(py: Python<'_>, source: &str, message: &str) -> PyResult<()> {
    log_warning(py, source, message, None)
}

/// `assert <condition>` with no message -> bare `AssertionError`.
fn assertion_error(py: Python<'_>) -> PyErr {
    match builtins(py)
        .and_then(|builtins| builtins.getattr("AssertionError"))
        .and_then(|class| class.call0())
    {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    }
}

/// `iter_obj.__next__()`, with `StopIteration` folded into `None`.
fn next_item<'py>(iterator: &mut Bound<'py, PyIterator>) -> PyResult<Option<Bound<'py, PyAny>>> {
    match iterator.next() {
        Some(Ok(value)) => Ok(Some(value)),
        Some(Err(error)) => Err(error),
        None => Ok(None),
    }
}

/// `raise <Exception>(...) from exc`: sets `__cause__` (and therefore
/// `__suppress_context__`) plus `__context__`, exactly like CPython's
/// `raise ... from ...` inside an `except` block.
fn raise_from(new_error: PyErr, py: Python<'_>, cause: PyErr) -> PyErr {
    new_error.set_cause(py, Some(cause.clone_ref(py)));
    new_error.set_context(py, Some(cause));
    new_error
}

// ---------------------------------------------------------------------------
// Stream helpers (`read` / `seek` / `tell` protocol, exactly as upstream)
// ---------------------------------------------------------------------------

/// `stream.read(n)` -> `Vec<u8>`.
fn stream_read_vec(stream: &Bound<'_, PyAny>, n: usize) -> PyResult<Vec<u8>> {
    bytes_of(&stream.call_method1("read", (n as i64,))?)
}

/// `stream.seek(offset, whence)`.
fn stream_seek(stream: &Bound<'_, PyAny>, offset: i64, whence: i64) -> PyResult<()> {
    stream.call_method1("seek", (offset, whence))?;
    Ok(())
}

/// `stream.seek(position, 0)`.
fn stream_seek_to(stream: &Bound<'_, PyAny>, position: i64) -> PyResult<()> {
    stream.call_method1("seek", (position,))?;
    Ok(())
}

/// `stream.tell()` as an integer.
fn stream_tell(stream: &Bound<'_, PyAny>) -> PyResult<i64> {
    stream.call_method0("tell")?.extract::<i64>()
}

/// `data[a:b]` for Python bytes semantics (negative indices allowed).
fn py_slice(data: &[u8], start: i64, end: i64) -> Vec<u8> {
    let len = data.len() as i64;
    let resolve = |index: i64| -> usize {
        if index < 0 {
            let resolved = len + index;
            if resolved < 0 {
                0
            } else {
                resolved as usize
            }
        } else if index > len {
            len as usize
        } else {
            index as usize
        }
    };
    let start = resolve(start);
    let end = resolve(end);
    if start >= end {
        Vec::new()
    } else {
        data[start..end].to_vec()
    }
}

/// `data.find(needle)` -> `Option<usize>`.
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// ---------------------------------------------------------------------------
// pypdf.generic._files
// ---------------------------------------------------------------------------

/// `EmbeddedFile` factory (upstream constructs the concrete module-level class).
fn embedded_file_class<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    files_attr(py, "EmbeddedFile")
}

/// `EmbeddedFile(name=..., pdf_object=..., parent=...)`.
fn new_embedded_file<'py>(
    py: Python<'py>,
    name: &Bound<'py, PyAny>,
    pdf_object: &Bound<'py, PyAny>,
    parent: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("name", name)?;
    kwargs.set_item("pdf_object", pdf_object)?;
    kwargs.set_item("parent", parent)?;
    embedded_file_class(py)?.call((), Some(&kwargs))
}

// @pypdf: pypdf.generic._files.EmbeddedFile.name
// (the property body is the pure attribute read ``return self._name``; see the
// trailing annotation list of symbols kept in the Python shim)

// @pypdf: pypdf.generic._files.EmbeddedFile._create_new
#[pyfunction]
fn files_embeddedfile_create_new<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    writer: &Bound<'py, PyAny>,
    name: &Bound<'py, PyAny>,
    content: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    // Convert string content to bytes if needed
    let content: Bound<'py, PyAny> = if is_instance_of::<PyString>(content) {
        content.call_method1("encode", (py_str(py, "latin-1"),))?
    } else {
        content.clone()
    };

    let decoded_stream_class = generic_class(py, "DecodedStreamObject")?;
    let dict_class = generic_class(py, "DictionaryObject")?;

    // Create the file entry (the actual embedded file stream)
    let file_entry = decoded_stream_class.call0()?;
    file_entry.call_method1("set_data", (&content,))?;
    let file_entry_dict = PyDict::new(py);
    file_entry_dict.set_item(
        name_object(py, &str_of(&constant(py, "PageAttributes", "TYPE")?)?)?,
        name_object(py, "/EmbeddedFile")?,
    )?;
    file_entry.call_method1("update", (file_entry_dict,))?;

    // Create the /EF entry
    let ef_entry = dict_class.call0()?;
    let ef_dict = PyDict::new(py);
    ef_dict.set_item(
        name_object(py, "/F")?,
        writer.call_method1("_add_object", (&file_entry,))?,
    )?;
    ef_entry.call_method1("update", (ef_dict,))?;

    // Create the filespec dictionary
    let create_string_object = generic_class(py, "create_string_object")?;
    let filespec = dict_class.call0()?;
    let filespec_reference = writer.call_method1("_add_object", (&filespec,))?;
    let name_object_value = create_string_object.call1((name,))?;
    let filespec_dict = PyDict::new(py);
    filespec_dict.set_item(
        name_object(py, &str_of(&constant(py, "PageAttributes", "TYPE")?)?)?,
        name_object(py, "/Filespec")?,
    )?;
    filespec_dict.set_item(
        name_object(
            py,
            &str_of(&constant(py, "FileSpecificationDictionaryEntries", "F")?)?,
        )?,
        &name_object_value,
    )?;
    filespec_dict.set_item(
        name_object(
            py,
            &str_of(&constant(py, "FileSpecificationDictionaryEntries", "EF")?)?,
        )?,
        &ef_entry,
    )?;
    filespec.call_method1("update", (filespec_dict,))?;

    // Add the name and filespec to the names array.  We use the inverse order
    // for insertion, as this allows us to re-use the same index.
    let names_array = cls.call_method1("_get_names_array", (writer,))?;
    let insertion_index = cls
        .call_method1("_get_insertion_index", (&names_array, &name_object_value))?
        .extract::<i64>()?;
    names_array.call_method1("insert", (insertion_index, &filespec_reference))?;
    names_array.call_method1("insert", (insertion_index, &name_object_value))?;

    // Return an EmbeddedFile instance
    let kwargs = PyDict::new(py);
    kwargs.set_item("name", name)?;
    kwargs.set_item("pdf_object", &filespec)?;
    kwargs.set_item("parent", &names_array)?;
    Ok(cls.call((), Some(&kwargs))?.unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile._get_names_array
#[pyfunction]
fn files_embeddedfile_get_names_array<'py>(
    py: Python<'py>,
    _cls: &Bound<'py, PyAny>,
    writer: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let names_key = name_object(py, &str_of(&constant(py, "CatalogAttributes", "NAMES")?)?)?;
    let root_object = writer.getattr("root_object")?;
    let dict_class = generic_class(py, "DictionaryObject")?;
    let array_class = generic_class(py, "ArrayObject")?;

    if !root_object.contains(&names_key)? {
        // Add the /Names entry to the catalog.
        root_object.set_item(
            &name_object(py, &str_of(&constant(py, "CatalogAttributes", "NAMES")?)?)?,
            writer.call_method1("_add_object", (dict_class.call0()?,))?,
        )?;
    }

    let names_dict = root_object.get_item(&names_key)?;
    let embedded_files_key = name_object(py, "/EmbeddedFiles")?;
    if !names_dict.contains(&embedded_files_key)? {
        // We do not yet have an entry for embedded files. Create and return it.
        let names = array_class.call0()?;
        let embedded_files_names_dictionary = dict_class.call1(({
            let inner = PyDict::new(py);
            inner.set_item(&names_key, &names)?;
            inner
        },))?;
        names_dict.set_item(
            &embedded_files_key,
            writer.call_method1("_add_object", (&embedded_files_names_dictionary,))?,
        )?;
        return Ok(names.unbind());
    }

    // We have an existing embedded files entry.
    let embedded_files_names_tree = names_dict.get_item(&embedded_files_key)?;
    if embedded_files_names_tree.contains(&names_key)? {
        // Simple case: We already have a flat list.
        return Ok(embedded_files_names_tree.get_item(&names_key)?.unbind());
    }
    let kids_key = name_object(py, "/Kids")?;
    if !embedded_files_names_tree.contains(&kids_key)? {
        // Invalid case: This is no name tree.
        return Err(errors::pdf_read_error(
            py,
            "Got neither Names nor Kids in embedded files tree.",
        ));
    }

    // Complex case: Convert a /Kids-based name tree to a /Names-based one.
    let names = array_class.call0()?;
    let kids = embedded_files_names_tree
        .get_item(&kids_key)?
        .call_method0("get_object")?;
    let embedded_files_names_dictionary = dict_class.call1(({
        let inner = PyDict::new(py);
        inner.set_item(&names_key, &names)?;
        inner
    },))?;
    names_dict.set_item(
        &embedded_files_key,
        writer.call_method1("_add_object", (&embedded_files_names_dictionary,))?,
    )?;
    for kid in kids.try_iter()? {
        let kid = kid?;
        let names_value = kid
            .call_method0("get_object")?
            .call_method1("get", (names_key.clone(), array_class.call0()?))?;
        for name in names_value.try_iter()? {
            names.call_method1("append", (name?,))?;
        }
    }
    Ok(names.unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile._get_insertion_index
#[pyfunction]
fn files_embeddedfile_get_insertion_index(
    py: Python<'_>,
    _cls: &Bound<'_, PyAny>,
    names_array: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
) -> PyResult<i64> {
    let count = py_len(names_array)?;
    let mut keys: Vec<Bound<'_, PyAny>> = Vec::with_capacity(count / 2 + 1);
    let mut index = 0usize;
    while index < count {
        let item = names_array.get_item(index as i64)?;
        keys.push(item.call_method1("encode", (py_str(py, "utf-8"),))?);
        index += 2;
    }
    let keys = py_list(py, keys)?;
    let name_bytes = name.call_method1("encode", (py_str(py, "utf-8"),))?;
    let bisect = py.import("bisect")?;
    let start = bisect
        .call_method1("bisect_left", (&keys, &name_bytes))?
        .extract::<i64>()?;
    let end = bisect
        .call_method1("bisect_right", (&keys, &name_bytes))?
        .extract::<i64>()?;

    if start != end {
        return Ok(end * 2);
    }
    if start == 0 {
        return Ok(0);
    }
    let key_count = py_len(&keys)? as i64;
    if start == key_count {
        return Ok(key_count * 2);
    }
    Ok(end * 2)
}

// @pypdf: pypdf.generic._files.EmbeddedFile.alternative_name
#[pyfunction]
fn files_embeddedfile_alternative_name_get<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let pdf_object = slf.getattr("pdf_object")?;
    let uf_key = name_object(
        py,
        &str_of(&constant(py, "FileSpecificationDictionaryEntries", "UF")?)?,
    )?;
    let f_key = name_object(
        py,
        &str_of(&constant(py, "FileSpecificationDictionaryEntries", "F")?)?,
    )?;
    for key in [uf_key, f_key] {
        // PDF 2.0 reference, table 43:
        //   > A PDF reader shall use the value of the UF key, when present, instead of the F key.
        if pdf_object.contains(&key)? {
            let value = pdf_object.get_item(&key)?.call_method0("get_object")?;
            if !is_null_or_none(py, &value)? {
                return Ok(value.unbind());
            }
        }
    }
    Ok(py_none(py).unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.alternative_name
#[pyfunction]
fn files_embeddedfile_alternative_name_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let pdf_object = slf.getattr("pdf_object")?;
    let uf_name = str_of(&constant(py, "FileSpecificationDictionaryEntries", "UF")?)?;
    let f_name = str_of(&constant(py, "FileSpecificationDictionaryEntries", "F")?)?;
    if value.is_none() {
        if pdf_object.contains(&name_object(py, &uf_name)?)? {
            pdf_object.set_item(&name_object(py, &uf_name)?, null_object(py)?)?;
        }
        if pdf_object.contains(&name_object(py, &f_name)?)? {
            pdf_object.set_item(&name_object(py, &f_name)?, null_object(py)?)?;
        }
    } else {
        pdf_object.set_item(&name_object(py, &uf_name)?, value)?;
        pdf_object.set_item(&name_object(py, &f_name)?, value)?;
    }
    Ok(())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.description
#[pyfunction]
fn files_embeddedfile_description_get<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let key = str_of(&constant(py, "FileSpecificationDictionaryEntries", "DESC")?)?;
    let value = slf
        .getattr("pdf_object")?
        .call_method1("get", (py_str(py, &key),))?;
    if is_null_or_none(py, &value)? {
        return Ok(py_none(py).unbind());
    }
    Ok(value.unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.description
#[pyfunction]
fn files_embeddedfile_description_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let key = name_object(
        py,
        &str_of(&constant(py, "FileSpecificationDictionaryEntries", "DESC")?)?,
    )?;
    let pdf_object = slf.getattr("pdf_object")?;
    if value.is_none() {
        pdf_object.set_item(&key, null_object(py)?)?;
    } else {
        pdf_object.set_item(&key, value)?;
    }
    Ok(())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.associated_file_relationship
#[pyfunction]
fn files_embeddedfile_associated_file_relationship_get<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let key = name_object(py, "/AFRelationship")?;
    let default = name_object(py, "/Unspecified")?;
    Ok(slf
        .getattr("pdf_object")?
        .call_method1("get", (key, default))?
        .unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.associated_file_relationship
#[pyfunction]
fn files_embeddedfile_associated_file_relationship_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    slf.getattr("pdf_object")?
        .set_item(name_object(py, "/AFRelationship")?, value)?;
    Ok(())
}

// @pypdf: pypdf.generic._files.EmbeddedFile._embedded_file
#[pyfunction]
fn files_embeddedfile_embedded_file<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let pdf_object = slf.getattr("pdf_object")?;
    let ef_key = name_object(py, "/EF")?;
    if !pdf_object.contains(&ef_key)? {
        return Err(errors::pdf_read_error(
            py,
            &format!("/EF entry not found: {}", repr_of(&pdf_object)?),
        ));
    }
    let ef = pdf_object.get_item(&ef_key)?;
    let uf_key = name_object(
        py,
        &str_of(&constant(py, "FileSpecificationDictionaryEntries", "UF")?)?,
    )?;
    let f_key = name_object(
        py,
        &str_of(&constant(py, "FileSpecificationDictionaryEntries", "F")?)?,
    )?;
    for key in [uf_key, f_key] {
        if ef.contains(&key)? {
            return Ok(ef.get_item(&key)?.call_method0("get_object")?.unbind());
        }
    }
    Err(errors::pdf_read_error(
        py,
        &format!("No /(U)F key found in file dictionary: {}", repr_of(&ef)?),
    ))
}

// @pypdf: pypdf.generic._files.EmbeddedFile._params
#[pyfunction]
fn files_embeddedfile_params<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let embedded_file = slf.getattr("_embedded_file")?;
    let default = generic_class(py, "DictionaryObject")?.call0()?;
    Ok(embedded_file
        .call_method1("get", (name_object(py, "/Params")?, default))?
        .call_method0("get_object")?
        .unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile._ensure_params
#[pyfunction]
fn files_embeddedfile_ensure_params<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let embedded_file = slf.getattr("_embedded_file")?;
    let params_key = name_object(py, "/Params")?;
    if !embedded_file.contains(&params_key)? {
        embedded_file.set_item(
            name_object(py, "/Params")?,
            generic_class(py, "DictionaryObject")?.call0()?,
        )?;
    }
    Ok(embedded_file.get_item(&params_key)?.unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.subtype
#[pyfunction]
fn files_embeddedfile_subtype_get<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let value = slf
        .getattr("_embedded_file")?
        .call_method1("get", (name_object(py, "/Subtype")?,))?;
    if is_null_or_none(py, &value)? {
        return Ok(py_none(py).unbind());
    }
    Ok(value.unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.subtype
#[pyfunction]
fn files_embeddedfile_subtype_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let embedded_file = slf.getattr("_embedded_file")?;
    let key = name_object(py, "/Subtype")?;
    if value.is_none() {
        embedded_file.set_item(&key, null_object(py)?)?;
    } else {
        embedded_file.set_item(&key, value)?;
    }
    Ok(())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.content
#[pyfunction]
fn files_embeddedfile_content_get<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(slf
        .getattr("_embedded_file")?
        .call_method0("get_data")?
        .unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.content
#[pyfunction]
fn files_embeddedfile_content_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let value: Bound<'_, PyAny> = if is_instance_of::<PyString>(value) {
        value.call_method1("encode", (py_str(py, "latin-1"),))?
    } else {
        value.clone()
    };
    slf.getattr("_embedded_file")?
        .call_method1("set_data", (value,))?;
    Ok(())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.size
#[pyfunction]
fn files_embeddedfile_size_get<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let value = slf
        .getattr("_params")?
        .call_method1("get", (name_object(py, "/Size")?,))?;
    if is_null_or_none(py, &value)? {
        return Ok(py_none(py).unbind());
    }
    Ok(value.unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.size
#[pyfunction]
fn files_embeddedfile_size_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let params = slf.getattr("_ensure_params")?;
    let key = name_object(py, "/Size")?;
    if value.is_none() {
        params.set_item(&key, null_object(py)?)?;
    } else {
        params.set_item(&key, value)?;
    }
    Ok(())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.creation_date
#[pyfunction]
fn files_embeddedfile_creation_date_get<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let value = slf
        .getattr("_params")?
        .call_method1("get", (name_object(py, "/CreationDate")?,))?;
    Ok(utils_attr(py, "parse_iso8824_date")?
        .call1((value,))?
        .unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.creation_date
#[pyfunction]
fn files_embeddedfile_creation_date_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let params = slf.getattr("_ensure_params")?;
    let key = name_object(py, "/CreationDate")?;
    if value.is_none() {
        params.set_item(&key, null_object(py)?)?;
    } else {
        let date_str = utils_attr(py, "format_iso8824_date")?.call1((value,))?;
        params.set_item(
            &key,
            generic_class(py, "TextStringObject")?.call1((date_str,))?,
        )?;
    }
    Ok(())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.modification_date
#[pyfunction]
fn files_embeddedfile_modification_date_get<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let value = slf
        .getattr("_params")?
        .call_method1("get", (name_object(py, "/ModDate")?,))?;
    Ok(utils_attr(py, "parse_iso8824_date")?
        .call1((value,))?
        .unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.modification_date
#[pyfunction]
fn files_embeddedfile_modification_date_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let params = slf.getattr("_ensure_params")?;
    let key = name_object(py, "/ModDate")?;
    if value.is_none() {
        params.set_item(&key, null_object(py)?)?;
    } else {
        let date_str = utils_attr(py, "format_iso8824_date")?.call1((value,))?;
        params.set_item(
            &key,
            generic_class(py, "TextStringObject")?.call1((date_str,))?,
        )?;
    }
    Ok(())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.checksum
#[pyfunction]
fn files_embeddedfile_checksum_get<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let value = slf
        .getattr("_params")?
        .call_method1("get", (name_object(py, "/CheckSum")?,))?;
    if is_null_or_none(py, &value)? {
        return Ok(py_none(py).unbind());
    }
    Ok(value.unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.checksum
#[pyfunction]
fn files_embeddedfile_checksum_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let params = slf.getattr("_ensure_params")?;
    let key = name_object(py, "/CheckSum")?;
    if value.is_none() {
        params.set_item(&key, null_object(py)?)?;
    } else {
        params.set_item(&key, value)?;
    }
    Ok(())
}

// @pypdf: pypdf.generic._files.EmbeddedFile._names
#[pyfunction]
fn files_embeddedfile_names<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let mut items: Vec<Bound<'py, PyAny>> = Vec::with_capacity(2);
    items.push(slf.getattr("name")?);
    let alternative_name = slf.getattr("alternative_name")?;
    if !alternative_name.is_none() && !alternative_name.eq(slf.getattr("name")?)? {
        items.push(alternative_name);
    }
    let names = py_list(py, items)?;
    Ok(names.call_method0("__iter__")?.unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.delete
#[pyfunction]
fn files_embeddedfile_delete(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<()> {
    let parent = slf.getattr("_parent")?;
    if !parent.is_truthy()? {
        return Err(errors::raise_msg(
            py,
            "PyPdfError",
            "Parent required to delete file from document.",
        ));
    }
    let pdf_object = slf.getattr("pdf_object")?;
    let index: Bound<'_, PyAny>;
    if parent.contains(&pdf_object)? {
        index = parent.call_method1("index", (&pdf_object,))?;
    } else {
        let indirect_reference = builtins(py)?.getattr("getattr")?.call1((
            &pdf_object,
            py_str(py, "indirect_reference"),
            py_none(py),
        ))?;
        if !indirect_reference.is_none() && parent.contains(&indirect_reference)? {
            index = parent.call_method1("index", (&indirect_reference,))?;
        } else {
            return Err(errors::raise_msg(
                py,
                "PyPdfError",
                "File not found in parent object.",
            ));
        }
    }
    parent.call_method1("pop", (&index,))?; // Reference.
    let previous = operator_call(py, "sub", &index, &py_int(py, 1))?;
    parent.call_method1("pop", (&previous,))?; // Name.
    slf.setattr(
        "pdf_object",
        generic_class(py, "DictionaryObject")?.call0()?,
    )?; // Invalidate.
    Ok(())
}

// @pypdf: pypdf.generic._files.EmbeddedFile.__repr__
#[pyfunction]
fn files_embeddedfile_repr(_py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<String> {
    let class_name = str_of(&slf.get_type().getattr("__name__")?)?;
    let name_repr = repr_of(&slf.getattr("name")?)?;
    Ok(format!("<{class_name} name={name_repr}>"))
}

// @pypdf: pypdf.generic._files.EmbeddedFile._report_malformed
#[pyfunction]
fn files_embeddedfile_report_malformed(
    py: Python<'_>,
    message: &Bound<'_, PyAny>,
    strict: bool,
) -> PyResult<()> {
    if strict {
        let message = str_of(message)?;
        return Err(errors::pdf_read_error(py, &message));
    }
    log_warning(py, FILES_SOURCE, &str_of(message)?, None)
}

// @pypdf: pypdf.generic._files.EmbeddedFile._load_from_names
#[pyfunction]
#[pyo3(signature = (cls, names, strict=false))]
fn files_embeddedfile_load_from_names<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    names: &Bound<'py, PyAny>,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let str_class = builtins(py)?.getattr("str")?;
    let null_class = generic_class(py, "NullObject")?;
    let dict_class = generic_class(py, "DictionaryObject")?;
    let mut results: Vec<Bound<'py, PyAny>> = Vec::new();
    // This is a name tree of the format [name_1, reference_1, name_2, reference_2, ...]
    let mut position: i64 = 0;
    for name in names.try_iter()? {
        let name = name?;
        if !is_instance(&name, &str_class)? {
            // Skip plain strings and retrieve them as `direct_name` by index.
            let file_dictionary = name.call_method0("get_object")?;
            if is_instance(&file_dictionary, &null_class)? {
                // A null value is equivalent to an omitted entry.
                position += 1;
                continue;
            }
            if !is_instance(&file_dictionary, &dict_class)? {
                cls.call_method1(
                    "_report_malformed",
                    (
                        format!(
                            "Embedded file specification is not a dictionary: {}",
                            str_of(&file_dictionary)?
                        ),
                        py_bool(py, strict),
                    ),
                )?;
                position += 1;
                continue;
            }
            let direct_name = names
                .get_item(py_int(py, position - 1))?
                .call_method0("get_object")?;
            results.push(new_embedded_file(
                py,
                &direct_name,
                &file_dictionary,
                names,
            )?);
        }
        position += 1;
    }
    let list = py_list(py, results)?;
    Ok(list.call_method0("__iter__")?.unbind())
}

// @pypdf: pypdf.generic._files.EmbeddedFile._load
#[pyfunction]
#[pyo3(signature = (cls, catalog, strict=false))]
fn files_embeddedfile_load<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    catalog: &Bound<'py, PyAny>,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let mut results: Vec<Bound<'py, PyAny>> = Vec::new();
    let names_key = name_object(py, &str_of(&constant(py, "CatalogAttributes", "NAMES")?)?)?;
    let null_class = generic_class(py, "NullObject")?;
    let dict_class = generic_class(py, "DictionaryObject")?;
    let array_class = generic_class(py, "ArrayObject")?;

    if !catalog.contains(&names_key)? {
        let list = py_list(py, results)?;
        return Ok(list.call_method0("__iter__")?.unbind());
    }
    let names = catalog.get_item(&names_key)?;
    if is_instance(&names, &null_class)? {
        // A null value is equivalent to an omitted entry.
        let list = py_list(py, results)?;
        return Ok(list.call_method0("__iter__")?.unbind());
    }
    if !is_instance(&names, &dict_class)? {
        cls.call_method1(
            "_report_malformed",
            (
                format!("Names tree is not a dictionary: {}", str_of(&names)?),
                py_bool(py, strict),
            ),
        )?;
        let list = py_list(py, results)?;
        return Ok(list.call_method0("__iter__")?.unbind());
    }
    let embedded_files_key = name_object(py, "/EmbeddedFiles")?;
    if !names.contains(&embedded_files_key)? {
        let list = py_list(py, results)?;
        return Ok(list.call_method0("__iter__")?.unbind());
    }
    let container = names.get_item(&embedded_files_key)?;
    if is_instance(&container, &null_class)? {
        let list = py_list(py, results)?;
        return Ok(list.call_method0("__iter__")?.unbind());
    }
    if !is_instance(&container, &dict_class)? {
        cls.call_method1(
            "_report_malformed",
            (
                format!(
                    "Embedded files entry is not a dictionary: {}",
                    str_of(&container)?
                ),
                py_bool(py, strict),
            ),
        )?;
        let list = py_list(py, results)?;
        return Ok(list.call_method0("__iter__")?.unbind());
    }

    let kids_key = name_object(py, "/Kids")?;
    let names_plural_key = name_object(py, "/Names")?;
    if container.contains(&kids_key)? {
        let mut kids = container.get_item(&kids_key)?.call_method0("get_object")?;
        if is_instance(&kids, &null_class)? {
            kids = array_class.call0()?;
        } else if !is_instance(&kids, &array_class)? {
            cls.call_method1(
                "_report_malformed",
                (
                    format!("Embedded files /Kids is not an array: {}", str_of(&kids)?),
                    py_bool(py, strict),
                ),
            )?;
            let list = py_list(py, results)?;
            return Ok(list.call_method0("__iter__")?.unbind());
        }
        for kid in kids.try_iter()? {
            // There might be further (nested) kids here.
            let kid = kid?.call_method0("get_object")?;
            if !is_instance(&kid, &dict_class)? {
                cls.call_method1(
                    "_report_malformed",
                    (
                        format!("Embedded files kid is not a dictionary: {}", str_of(&kid)?),
                        py_bool(py, strict),
                    ),
                )?;
                continue;
            }
            if !kid.contains(&names_plural_key)? {
                continue;
            }
            let kid_names = kid
                .get_item(&names_plural_key)?
                .call_method0("get_object")?;
            if is_instance(&kid_names, &null_class)? {
                continue;
            }
            if !is_instance(&kid_names, &array_class)? {
                cls.call_method1(
                    "_report_malformed",
                    (
                        format!(
                            "Embedded files name list is not an array: {}",
                            str_of(&kid_names)?
                        ),
                        py_bool(py, strict),
                    ),
                )?;
                continue;
            }
            let kwargs = PyDict::new(py);
            kwargs.set_item("strict", py_bool(py, strict))?;
            let kid_items = cls
                .getattr("_load_from_names")?
                .call((kid_names,), Some(&kwargs))?;
            let mut kid_iterator = kid_items.try_iter()?;
            while let Some(item) = next_item(&mut kid_iterator)? {
                results.push(item);
            }
        }
    }
    if container.contains(&names_plural_key)? {
        let container_names = container
            .get_item(&names_plural_key)?
            .call_method0("get_object")?;
        if is_instance(&container_names, &null_class)? {
            let list = py_list(py, results)?;
            return Ok(list.call_method0("__iter__")?.unbind());
        }
        if !is_instance(&container_names, &array_class)? {
            cls.call_method1(
                "_report_malformed",
                (
                    format!(
                        "Embedded files name list is not an array: {}",
                        str_of(&container_names)?
                    ),
                    py_bool(py, strict),
                ),
            )?;
            let list = py_list(py, results)?;
            return Ok(list.call_method0("__iter__")?.unbind());
        }
        let kwargs = PyDict::new(py);
        kwargs.set_item("strict", py_bool(py, strict))?;
        let loaded = cls
            .getattr("_load_from_names")?
            .call((container_names,), Some(&kwargs))?;
        for item in loaded.try_iter()? {
            results.push(item?);
        }
    }
    let list = py_list(py, results)?;
    Ok(list.call_method0("__iter__")?.unbind())
}

// ---------------------------------------------------------------------------
// pypdf.generic._image_inline
// ---------------------------------------------------------------------------

/// `read_non_whitespace(stream)` from `pypdf._utils`.
fn read_non_whitespace<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    utils_attr(py, "read_non_whitespace")?.call1((stream,))
}

/// `_check_end_image_marker(stream)`.
fn check_end_image_marker(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<bool> {
    let mut ei_tok = bytes_of(&read_non_whitespace(py, stream)?)?;
    ei_tok.extend(stream_read_vec(stream, 2)?);
    stream_seek(stream, -(ei_tok.len() as i64), 1)?;
    Ok(ei_tok.len() >= 2
        && &ei_tok[0..2] == b"EI"
        && (ei_tok.len() == 2 || is_whitespace_byte(ei_tok[2])))
}

// @pypdf: pypdf.generic._image_inline._check_end_image_marker
#[pyfunction]
fn imageinline_check_end_image_marker(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<bool> {
    check_end_image_marker(py, stream)
}

/// `extract_inline__ascii_hex_decode(stream)`.
fn extract_ascii_hex_decode_impl(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    let mut data_out: Vec<u8> = Vec::new();
    // Read data until delimiter > and EI as backup.
    loop {
        let mut data_buffered = bytes_of(&read_non_whitespace(py, stream)?)?;
        let read_bytes = stream_read_vec(stream, BUFFER_SIZE)?;
        data_buffered.extend_from_slice(&read_bytes);
        if data_buffered.is_empty() || read_bytes.is_empty() {
            return Err(errors::pdf_read_error(py, "Unexpected end of stream."));
        }
        if let Some(pos_tok) = find_bytes(&data_buffered, b">") {
            // found >
            data_out.extend_from_slice(&data_buffered[..pos_tok + 1]);
            stream_seek(stream, pos_tok as i64 + 1 - data_buffered.len() as i64, 1)?;
            break;
        }
        if let Some(pos_ei) = find_bytes(&data_buffered, b"EI") {
            // found EI
            let mut pos_ei = pos_ei as i64;
            stream_seek(stream, pos_ei - 1 - data_buffered.len() as i64, 1)?;
            let mut c = stream_read_vec(stream, 1)?;
            while c.len() == 1 && is_whitespace_byte(c[0]) {
                stream_seek(stream, -2, 1)?;
                c = stream_read_vec(stream, 1)?;
                pos_ei -= 1;
            }
            data_out.extend_from_slice(&py_slice(&data_buffered, 0, pos_ei));
            break;
        }
        if data_buffered.len() == 2 {
            data_out.extend_from_slice(&data_buffered);
            return Err(errors::pdf_read_error(py, "Unexpected end of stream."));
        }
        // Neither > nor EI found
        data_out.extend_from_slice(&py_slice(&data_buffered, 0, data_buffered.len() as i64 - 2));
        stream_seek(stream, -2, 1)?;
    }

    if !check_end_image_marker(py, stream)? {
        return Err(errors::pdf_read_error(py, "EI stream not found."));
    }
    Ok(data_out)
}

// @pypdf: pypdf.generic._image_inline.extract_inline__ascii_hex_decode
#[pyfunction]
fn imageinline_extract_ascii_hex_decode<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(py_bytes(py, &extract_ascii_hex_decode_impl(py, stream)?).unbind())
}

/// `extract_inline__ascii85_decode(stream)`.
fn extract_ascii85_decode_impl(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    let mut data_out: Vec<u8> = Vec::new();
    // Read data until delimiter ~>
    loop {
        let mut data_buffered = bytes_of(&read_non_whitespace(py, stream)?)?;
        let read_bytes = stream_read_vec(stream, BUFFER_SIZE)?;
        data_buffered.extend_from_slice(&read_bytes);
        if data_buffered.is_empty() || read_bytes.is_empty() {
            return Err(errors::pdf_read_error(py, "Unexpected end of stream."));
        }
        if let Some(pos_tok) = find_bytes(&data_buffered, b"~>") {
            // found!
            data_out.extend_from_slice(&data_buffered[..pos_tok + 2]);
            stream_seek(stream, pos_tok as i64 + 2 - data_buffered.len() as i64, 1)?;
            break;
        }
        if data_buffered.len() == 2 {
            // end of buffer
            data_out.extend_from_slice(&data_buffered);
            return Err(errors::pdf_read_error(py, "Unexpected end of stream."));
        }
        // back by one char in case of in the middle of ~>
        data_out.extend_from_slice(&py_slice(&data_buffered, 0, data_buffered.len() as i64 - 2));
        stream_seek(stream, -2, 1)?;
    }

    if !check_end_image_marker(py, stream)? {
        return Err(errors::pdf_read_error(py, "EI stream not found."));
    }
    Ok(data_out)
}

// @pypdf: pypdf.generic._image_inline.extract_inline__ascii85_decode
#[pyfunction]
fn imageinline_extract_ascii85_decode<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(py_bytes(py, &extract_ascii85_decode_impl(py, stream)?).unbind())
}

/// `extract_inline__run_length_decode(stream)`.
fn extract_run_length_decode_impl(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    let mut data_out: Vec<u8> = Vec::new();
    // Read data until delimiter 128
    loop {
        let data_buffered = stream_read_vec(stream, BUFFER_SIZE)?;
        if data_buffered.is_empty() {
            return Err(errors::pdf_read_error(py, "Unexpected end of stream."));
        }
        if let Some(pos_tok) = data_buffered.iter().position(|&byte| byte == 0x80) {
            // found
            let after_start = pos_tok + 1;
            let after_end = (pos_tok + 4).min(data_buffered.len());
            let after_token = if after_start < after_end {
                &data_buffered[after_start..after_end]
            } else {
                &[][..]
            };
            if after_token.starts_with(b"EI") || after_token.ends_with(b"EI") {
                data_out.extend_from_slice(&data_buffered[..pos_tok + 1]);
                stream_seek(stream, pos_tok as i64 + 1 - data_buffered.len() as i64, 1)?;
            } else {
                log_plain(
                    py,
                    INLINE_SOURCE,
                    "Early EOD in RunLengthDecode of inline image, using fallback.",
                )?;
                if let Some(ei_marker) = find_bytes(&data_buffered, b"EI") {
                    if ei_marker > 0 {
                        data_out.extend_from_slice(&data_buffered[..ei_marker]);
                        stream_seek(stream, ei_marker as i64 - 1 - data_buffered.len() as i64, 1)?;
                    }
                }
            }
            break;
        }
        data_out.extend_from_slice(&data_buffered);
    }

    if !check_end_image_marker(py, stream)? {
        return Err(errors::pdf_read_error(py, "EI stream not found."));
    }
    Ok(data_out)
}

// @pypdf: pypdf.generic._image_inline.extract_inline__run_length_decode
#[pyfunction]
fn imageinline_extract_run_length_decode<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(py_bytes(py, &extract_run_length_decode_impl(py, stream)?).unbind())
}

/// The DCT marker set: `b"\xc0...\xcf" b"\xda...\xdf" b"\xe0...\xef\xfe"`
/// (upstream tests membership in that concatenated bytes object, i.e. whether
/// the single byte occurs anywhere in it).
fn is_dct_marker(byte: u8) -> bool {
    matches!(
        byte,
        0xc0 | 0xc1
            | 0xc2
            | 0xc3
            | 0xc4
            | 0xc5
            | 0xc6
            | 0xc7
            | 0xc9
            | 0xca
            | 0xcb
            | 0xcc
            | 0xcd
            | 0xce
            | 0xcf
            | 0xda
            | 0xdb
            | 0xdc
            | 0xdd
            | 0xde
            | 0xdf
            | 0xe0
            | 0xe1
            | 0xe2
            | 0xe3
            | 0xe4
            | 0xe5
            | 0xe6
            | 0xe7
            | 0xe8
            | 0xe9
            | 0xea
            | 0xeb
            | 0xec
            | 0xed
            | 0xee
            | 0xef
            | 0xfe
    )
}

/// `extract_inline__dct_decode(stream)`.
fn extract_dct_decode_impl(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    // If 0 bytes are returned, and *size* was not 0, this indicates end of file.
    // If the object is in non-blocking mode and no bytes are available, `None`
    // is returned.  Both cases raise the same error as upstream.
    let read = |py: Python<'_>, length: i64| -> PyResult<Vec<u8>> {
        let result = stream.call_method1("read", (length,))?;
        if result.is_none() {
            return Err(errors::pdf_read_error(py, "Unexpected end of stream."));
        }
        let data = bytes_of(&result)?;
        if data.len() as i64 != length {
            return Err(errors::pdf_read_error(py, "Unexpected end of stream."));
        }
        Ok(data)
    };

    let mut data_out: Vec<u8> = Vec::new();
    // Read Blocks of data (ID/Size/data) up to ID=FF/D9
    // https://www.digicamsoft.com/itu/itu-t81-36.html
    let mut not_first = false;
    loop {
        let c = read(py, 1)?[0];
        if not_first || c == 0xff {
            data_out.push(c);
        }
        if c != 0xff {
            continue;
        }
        not_first = true;
        let c = read(py, 1)?[0];
        data_out.push(c);
        if c == 0xff {
            stream_seek(stream, -1, 1)?;
        } else if c == 0x00 {
            // stuffing
        } else if c == 0xd9 {
            // end
            break;
        } else if is_dct_marker(c) {
            let size_bytes = read(py, 2)?;
            data_out.extend_from_slice(&size_bytes);
            let sz = size_bytes[0] as i64 * 256 + size_bytes[1] as i64;
            if sz < 2 {
                // ``read(-1)`` returns the rest of the stream, whose length can
                // never equal the requested negative length -> upstream raises.
                return Err(errors::pdf_read_error(py, "Unexpected end of stream."));
            }
            data_out.extend_from_slice(&read(py, sz - 2)?);
        }
    }

    if !check_end_image_marker(py, stream)? {
        return Err(errors::pdf_read_error(py, "EI stream not found."));
    }
    Ok(data_out)
}

// @pypdf: pypdf.generic._image_inline.extract_inline__dct_decode
#[pyfunction]
fn imageinline_extract_dct_decode<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(py_bytes(py, &extract_dct_decode_impl(py, stream)?).unbind())
}

/// `_is_preceded_by_whitespace(stream_out, position)`.
fn is_preceded_by_whitespace(stream_out: &Bound<'_, PyAny>, position: i64) -> PyResult<bool> {
    if position <= 0 {
        return Ok(false);
    }
    let saved_position = stream_tell(stream_out)?;
    stream_seek_to(stream_out, position - 1)?;
    let byte = stream_read_vec(stream_out, 1)?;
    stream_seek_to(stream_out, saved_position)?;
    Ok(byte.len() == 1 && is_whitespace_byte(byte[0]))
}

// @pypdf: pypdf.generic._image_inline._is_preceded_by_whitespace
#[pyfunction]
fn imageinline_is_preceded_by_whitespace(
    stream_out: &Bound<'_, PyAny>,
    position: i64,
) -> PyResult<bool> {
    is_preceded_by_whitespace(stream_out, position)
}

/// `_is_followed_by_closing_operator(stream, first_byte)`.
fn is_followed_by_closing_operator(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    first_byte: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let position = stream_tell(stream)?;
    let is_closing_operator = if eq_bytes(first_byte, py, b"Q")? {
        let suffix = stream_read_vec(stream, 1)?;
        suffix.is_empty() || (suffix.len() == 1 && is_whitespace_byte(suffix[0]))
    } else if eq_bytes(first_byte, py, b"E")? {
        let suffix = stream_read_vec(stream, 3)?;
        suffix.len() >= 2
            && &suffix[0..2] == b"MC"
            && (suffix.len() < 3 || is_whitespace_byte(suffix[2]))
    } else {
        false
    };
    stream_seek_to(stream, position)?;
    Ok(is_closing_operator)
}

// @pypdf: pypdf.generic._image_inline._is_followed_by_closing_operator
#[pyfunction]
fn imageinline_is_followed_by_closing_operator(
    py: Python<'_>,
    stream: &Bound<'_, PyAny>,
    first_byte: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    is_followed_by_closing_operator(py, stream, first_byte)
}

/// `is_followed_by_binary_data(stream, length=10)`.
fn is_followed_by_binary_data_impl(stream: &Bound<'_, PyAny>, length: i64) -> PyResult<bool> {
    let position = stream_tell(stream)?;
    let data = if length < 0 {
        bytes_of(&stream.call_method1("read", (length,))?)?
    } else {
        stream_read_vec(stream, length as usize)?
    };
    stream_seek_to(stream, position)?;
    if data.is_empty() {
        return Ok(false);
    }
    let mut operator_start: Option<usize> = None;
    let mut operator_end: Option<usize> = None;

    for (index, &byte) in data.iter().enumerate() {
        if byte < 32 && !is_whitespace_byte(byte) {
            // This covers all characters not being displayable directly, although omitting whitespace
            // to allow for operator detection.
            return Ok(true);
        }
        let is_whitespace = is_whitespace_byte(byte);
        if operator_start.is_none() && !is_whitespace {
            // Interpret all other non-whitespace characters as the start of an operation.
            operator_start = Some(index);
        }
        if operator_start.is_some() && is_whitespace {
            // A whitespace stops an operation.
            operator_end = Some(index);
            break;
        }
    }

    let operator_start = match operator_start {
        // Inline images should not have tons of whitespaces, which would lead to no operator start.
        None => return Ok(false),
        Some(value) => value,
    };
    // We probably are inside an operation.  Upstream uses the *requested*
    // length (not the number of bytes actually read) as the end index, and only
    // the Python slice clamps it; the operator length therefore uses the
    // unclamped end index.
    let operator_end: i64 = match operator_end {
        Some(value) => value as i64,
        None => length,
    };
    let operator_start_i64 = operator_start as i64;
    let operator_length = operator_end - operator_start_i64;
    let operator = py_slice(&data, operator_start_i64, operator_end);
    if operator.starts_with(b"/") && operator_length > 1 {
        // Name object.
        return Ok(false);
    }
    let without_dots: Vec<u8> = operator
        .iter()
        .copied()
        .filter(|&byte| byte != b'.')
        .collect();
    if !without_dots.is_empty() && without_dots.iter().all(u8::is_ascii_digit) {
        // Graphics operator, for example a move. A number (integer or float).
        return Ok(false);
    }
    if operator_length > 3 {
        // Usually, the operators inside a content stream should not have more than three characters,
        // especially after an inline image.
        return Ok(true);
    }
    Ok(false)
}

// @pypdf: pypdf.generic._image_inline.is_followed_by_binary_data
#[pyfunction]
#[pyo3(signature = (stream, length=10))]
fn imageinline_is_followed_by_binary_data(
    stream: &Bound<'_, PyAny>,
    length: i64,
) -> PyResult<bool> {
    is_followed_by_binary_data_impl(stream, length)
}

/// `extract_inline_default(stream)`.
fn extract_inline_default_impl<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let bytes_io_class = py.import("io")?.getattr("BytesIO")?;
    let stream_out = bytes_io_class.call0()?;
    // Read the inline image, while checking for EI (End Image) operator.
    loop {
        let data_buffered = stream_read_vec(stream, BUFFER_SIZE)?;
        if data_buffered.is_empty() {
            return Err(errors::pdf_read_error(py, "Unexpected end of stream."));
        }
        // We can not look straight for "EI" because it may not have been loaded in the buffer
        let pos_ei = data_buffered.iter().position(|&byte| byte == b'E');

        let pos_ei = match pos_ei {
            None => {
                stream_out.call_method1("write", (py_bytes(py, &data_buffered),))?;
                continue;
            }
            Some(pos_ei) => pos_ei,
        };
        // Write out everything including E (the one from EI to be removed)
        stream_out.call_method1("write", (py_bytes(py, &data_buffered[0..pos_ei + 1]),))?;
        let sav_pos_ei = stream_tell(&stream_out)? - 1;
        // Seek back in the stream to read the E next
        stream_seek(stream, pos_ei as i64 + 1 - data_buffered.len() as i64, 1)?;
        let saved_pos = stream_tell(stream)?;
        // Check for End Image
        let tok2 = stream_read_vec(stream, 1)?; // I of "EI"
        if tok2 != b"I" {
            stream_seek_to(stream, saved_pos)?;
            continue;
        }
        let tok3 = stream_read_vec(stream, 1)?; // possible space after "EI"
        if tok3.is_empty() {
            // The `EI` marker is at the very end of the stream. There can be
            // no trailing binary data, so this is unambiguously the end of the
            // inline image (#3468).
            stream_seek_to(stream, saved_pos - 1)?;
            stream_out.call_method1("truncate", (sav_pos_ei,))?;
            break;
        }
        if !is_whitespace_byte(tok3[0]) {
            stream_seek_to(stream, saved_pos)?;
            continue;
        }
        let mut tok3 = tok3;
        while !tok3.is_empty() && is_whitespace_byte(tok3[0]) {
            tok3 = stream_read_vec(stream, 1)?;
        }
        // The `EI` marker should be delimited by whitespace on both sides. Preserve the
        // legacy tolerance for a missing leading delimiter only when it is followed by an
        // exact closing operator, as binary image data would match otherwise.
        if !is_preceded_by_whitespace(&stream_out, sav_pos_ei)?
            && !is_followed_by_closing_operator(py, stream, &py_bytes(py, &tok3))?
        {
            stream_seek_to(stream, saved_pos)?;
            continue;
        }
        // Inline image contains `EI ` sequence usually marking the end of it, but
        // is followed by binary data which does not make sense for the actual end.
        if is_followed_by_binary_data_impl(stream, 10)? {
            stream_seek_to(stream, saved_pos)?;
            continue;
        }
        // Data contains [\s]EI[\s](Q|EMC): 4 chars are sufficient
        // remove E(I) wrongly inserted earlier
        stream_seek_to(stream, saved_pos - 1)?;
        stream_out.call_method1("truncate", (sav_pos_ei,))?;
        break;
    }

    stream_out.call_method0("getvalue")
}

// @pypdf: pypdf.generic._image_inline.extract_inline_default
#[pyfunction]
fn imageinline_extract_default<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(extract_inline_default_impl(py, stream)?.unbind())
}

/// `_read_settings_dictionary(...)`.
fn read_settings_dictionary_impl<'py>(
    py: Python<'py>,
    content_stream: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
    parsing_budget: &Bound<'py, PyAny>,
    remaining_count: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let settings = generic_class(py, "DictionaryObject")?.call0()?;
    let mut remaining_count: Bound<'py, PyAny> = match remaining_count {
        Some(value) => value.clone(),
        None => py_none(py),
    };
    let content_stream_class = generic_class(py, "ContentStream")?;
    let read_object = py
        .import("pypdf.generic._data_structures")?
        .getattr("read_object")?;

    loop {
        let tok = read_non_whitespace(py, stream)?;
        if !tok.is_truthy()? {
            return Err(errors::pdf_read_error(py, "Unexpected end of stream."));
        }
        stream_seek(stream, -1, 1)?;
        if eq_bytes(&tok, py, b"I")? {
            // "ID" - begin of image data
            break;
        }
        let kwargs = PyDict::new(py);
        kwargs.set_item("remaining_elements", &remaining_count)?;
        kwargs.set_item("parsing_budget", parsing_budget)?;
        kwargs.set_item("token_decrement", py_int(py, 2))?;
        remaining_count = content_stream_class
            .getattr("_check_limits")?
            .call((), Some(&kwargs))?;

        let pdf = content_stream.getattr("pdf")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("parsing_budget", parsing_budget)?;
        let key = read_object.call((stream, &pdf), Some(&kwargs))?;
        let _skipped = read_non_whitespace(py, stream)?;
        stream_seek(stream, -1, 1)?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("parsing_budget", parsing_budget)?;
        let value = read_object.call((stream, &pdf), Some(&kwargs))?;
        settings.set_item(&key, &value)?;
    }

    Ok(settings)
}

// @pypdf: pypdf.generic._image_inline._read_settings_dictionary
#[pyfunction]
#[pyo3(signature = (*, content_stream, stream, parsing_budget, remaining_count))]
fn imageinline_read_settings_dictionary<'py>(
    py: Python<'py>,
    content_stream: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
    parsing_budget: &Bound<'py, PyAny>,
    remaining_count: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(read_settings_dictionary_impl(
        py,
        content_stream,
        stream,
        parsing_budget,
        Some(remaining_count),
    )?
    .unbind())
}

/// `extract_inline__not_set(...)`.
fn extract_not_set_impl<'py>(
    py: Python<'py>,
    content_stream: &Bound<'py, PyAny>,
    saved_position: i64,
    settings: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let int_class = builtins(py)?.getattr("int")?;
    let float_class = builtins(py)?.getattr("float")?;
    let str_class = builtins(py)?.getattr("str")?;
    let list_class = builtins(py)?.getattr("list")?;

    let mut color_space = settings.call_method1("get", (py_str(py, "/CS"), py_str(py, "")))?;
    if is_instance(&color_space, &list_class)? {
        color_space = if color_space.is_truthy()? {
            color_space.get_item(0)?
        } else {
            py_str(py, "")
        };
    }
    if !is_instance(&color_space, &str_class)? {
        color_space = py_str(py, "");
    }

    let bytes_per_pixel: Bound<'py, PyAny>;
    if color_space.contains(py_str(py, "RGB"))? {
        bytes_per_pixel = py_int(py, 3);
    } else if color_space.contains(py_str(py, "CMYK"))? {
        bytes_per_pixel = py_int(py, 4);
    } else {
        let color_space_defaults = py_set(py)?;
        for name in ["/I", "/G", "/Indexed", "/DeviceGray"] {
            color_space_defaults.call_method1("add", (py_str(py, name),))?;
        }
        let default = if color_space_defaults.contains(&color_space)? {
            py_int(py, 8)
        } else {
            py_int(py, -1)
        };
        let bits_per_pixel = settings.call_method1("get", (py_str(py, "/BPC"), default))?;
        let is_number = is_instance(&bits_per_pixel, &int_class)?
            || is_instance(&bits_per_pixel, &float_class)?;
        if is_number && bits_per_pixel.gt(py_int(py, 0))? {
            bytes_per_pixel = operator_call(py, "truediv", &bits_per_pixel, &py_float(py, 8.0))?;
        } else {
            let data = inline_attr(py, "extract_inline_default")?.call1((stream,))?;
            // Move to the `EI` if possible.
            let _ei = read_non_whitespace(py, stream)?;
            stream_seek(stream, -1, 1)?;
            return Ok(data);
        }
    }

    let width = settings.call_method1("get", (py_str(py, "/W"),))?;
    let height = settings.call_method1("get", (py_str(py, "/H"),))?;
    let sample_length: Option<Bound<'py, PyAny>> = if !is_instance(&width, &int_class)?
        || !is_instance(&height, &int_class)?
        || !bytes_per_pixel.is_truthy()?
    {
        None
    } else {
        let scaled = operator_call(py, "mul", &width, &bytes_per_pixel)?;
        let ceil = py.import("math")?.getattr("ceil")?.call1((scaled,))?;
        Some(operator_call(py, "mul", &ceil, &height)?)
    };
    let data_attr = content_stream.getattr("_data")?;
    let remaining_bytes = operator_call(
        py,
        "sub",
        &py_int(py, py_len(&data_attr)? as i64),
        &py_int(py, saved_position),
    )?;
    if let Some(length) = &sample_length {
        // TODO: Replace by general inline image limit and employ everywhere.
        let maximum = config_item(py, "image_maximum_buffer_size")?;
        if length.gt(&maximum)? {
            return Err(errors::limit_reached_error(
                py,
                "Requested image size exceeds total limit.",
            ));
        }
    }

    let mut direct_read: Option<usize> = None;
    if let Some(length) = &sample_length {
        if length.gt(py_int(py, 0))? && length.lt(&remaining_bytes)? {
            direct_read = Some(length.extract::<usize>()?);
        }
    }
    let data = match direct_read {
        Some(length) => stream.call_method1("read", (length,))?,
        // Without usable dimensions the raw sample length is unknown;
        // fall back to scanning for the `EI` marker.
        None => inline_attr(py, "extract_inline_default")?.call1((stream,))?,
    };

    // Move to the `EI` if possible.
    let _ei = read_non_whitespace(py, stream)?;
    stream_seek(stream, -1, 1)?;

    Ok(data)
}

// @pypdf: pypdf.generic._image_inline.extract_inline__not_set
#[pyfunction]
#[pyo3(signature = (*, content_stream, saved_position, settings, stream))]
fn imageinline_extract_not_set<'py>(
    py: Python<'py>,
    content_stream: &Bound<'py, PyAny>,
    saved_position: i64,
    settings: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(extract_not_set_impl(py, content_stream, saved_position, settings, stream)?.unbind())
}

/// `_handle_filters(...)`.
fn handle_filters_impl<'py>(
    py: Python<'py>,
    filters: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
    settings: &Bound<'py, PyAny>,
    content_stream: &Bound<'py, PyAny>,
    saved_position: i64,
) -> PyResult<Bound<'py, PyAny>> {
    let list_class = builtins(py)?.getattr("list")?;
    let str_class = builtins(py)?.getattr("str")?;

    let mut filter_to_use = filters.clone();
    if is_instance(filters, &list_class)? {
        filter_to_use = if filter_to_use.is_truthy()? {
            filter_to_use.get_item(0)?
        } else {
            py_str(py, "not set")
        };
    }
    if !is_instance(&filter_to_use, &str_class)? {
        // A valid filter is a name or an array of names; anything else
        // cannot match the abbreviations below, so treat it as unfiltered.
        filter_to_use = py_str(py, "not set");
    }

    if filter_to_use.contains(py_str(py, "AHx"))?
        || filter_to_use.contains(py_str(py, "ASCIIHexDecode"))?
    {
        return inline_attr(py, "extract_inline__ascii_hex_decode")?.call1((stream,));
    }
    if filter_to_use.contains(py_str(py, "A85"))?
        || filter_to_use.contains(py_str(py, "ASCII85Decode"))?
    {
        return inline_attr(py, "extract_inline__ascii85_decode")?.call1((stream,));
    }
    if filter_to_use.contains(py_str(py, "RL"))?
        || filter_to_use.contains(py_str(py, "RunLengthDecode"))?
    {
        return inline_attr(py, "extract_inline__run_length_decode")?.call1((stream,));
    }
    if filter_to_use.contains(py_str(py, "DCT"))?
        || filter_to_use.contains(py_str(py, "DCTDecode"))?
    {
        return inline_attr(py, "extract_inline__dct_decode")?.call1((stream,));
    }
    if filter_to_use.eq(py_str(py, "not set"))? {
        let kwargs = PyDict::new(py);
        kwargs.set_item("settings", settings)?;
        kwargs.set_item("stream", stream)?;
        kwargs.set_item("content_stream", content_stream)?;
        kwargs.set_item("saved_position", py_int(py, saved_position))?;
        return inline_attr(py, "extract_inline__not_set")?.call((), Some(&kwargs));
    }
    inline_attr(py, "extract_inline_default")?.call1((stream,))
}

// @pypdf: pypdf.generic._image_inline._handle_filters
#[pyfunction]
#[pyo3(signature = (*, filters, stream, settings, content_stream, saved_position))]
fn imageinline_handle_filters<'py>(
    py: Python<'py>,
    filters: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
    settings: &Bound<'py, PyAny>,
    content_stream: &Bound<'py, PyAny>,
    saved_position: i64,
) -> PyResult<Py<PyAny>> {
    Ok(handle_filters_impl(
        py,
        filters,
        stream,
        settings,
        content_stream,
        saved_position,
    )?
    .unbind())
}

/// `read_inline_image(...)`.
fn read_inline_image_impl<'py>(
    py: Python<'py>,
    content_stream: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
    parsing_budget: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let remaining_count = config_item(py, "dictionary_maximum_element_count")?;

    // First read the dictionary of settings.
    let kwargs = PyDict::new(py);
    kwargs.set_item("content_stream", content_stream)?;
    kwargs.set_item("stream", stream)?;
    kwargs.set_item("parsing_budget", parsing_budget)?;
    kwargs.set_item("remaining_count", &remaining_count)?;
    let settings = inline_attr(py, "_read_settings_dictionary")?.call((), Some(&kwargs))?;

    // We are at the beginning of ID now.
    let read = stream_read_vec(stream, 3)?;
    if !(read.len() >= 2 && &read[0..2] == b"ID") {
        return Err(assertion_error(py));
    }

    let filters = settings.call_method1(
        "get",
        (
            py_str(py, "/F"),
            settings.call_method1("get", (py_str(py, "/Filter"), py_str(py, "not set")))?,
        ),
    )?;
    let saved_position = stream_tell(stream)?;

    let kwargs = PyDict::new(py);
    kwargs.set_item("filters", &filters)?;
    kwargs.set_item("stream", stream)?;
    kwargs.set_item("settings", &settings)?;
    kwargs.set_item("content_stream", content_stream)?;
    kwargs.set_item("saved_position", py_int(py, saved_position))?;
    let mut data = inline_attr(py, "_handle_filters")?.call((), Some(&kwargs))?;

    let mut ei = stream_read_vec(stream, 3)?;
    // An `EI` at the very end of the stream yields only two bytes; rewinding
    // unconditionally would step back into the marker (#3468).
    if ei.len() == 3 {
        stream_seek(stream, -1, 1)?;
    }
    let is_end_marker = |ei: &[u8]| -> bool {
        ei.len() >= 2 && &ei[0..2] == b"EI" && (ei.len() < 3 || is_whitespace_byte(ei[2]))
    };
    if !is_end_marker(&ei) {
        // Deal with wrong/missing `EI` tags. Example: Wrong dimensions.
        stream_seek_to(stream, saved_position)?;
        data = inline_attr(py, "extract_inline_default")?.call1((stream,))?;
        ei = stream_read_vec(stream, 3)?;
        if ei.len() == 3 {
            stream_seek(stream, -1, 1)?;
        }
        if !is_end_marker(&ei) {
            // Check the same condition again. This should never fail as
            // edge cases are covered by `extract_inline_default` above,
            // but check this to make sure that we are behind the `EI` afterwards.
            return Err(errors::pdf_stream_error(
                py,
                &format!(
                    "Could not extract inline image, even using fallback. Expected 'EI', got {}",
                    repr_of(&py_bytes(py, &ei))?
                ),
            ));
        }
    }

    let result = PyDict::new(py);
    result.set_item("settings", settings)?;
    result.set_item("data", data)?;
    Ok(result.into_any())
}

// @pypdf: pypdf.generic._image_inline.read_inline_image
#[pyfunction]
#[pyo3(signature = (*, content_stream, stream, parsing_budget))]
fn imageinline_read_inline_image<'py>(
    py: Python<'py>,
    content_stream: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
    parsing_budget: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(read_inline_image_impl(py, content_stream, stream, parsing_budget)?.unbind())
}

// ---------------------------------------------------------------------------
// pypdf.generic._image_xobject
// ---------------------------------------------------------------------------

/// `from PIL import Image` (the *module* `PIL.Image`, exactly like upstream).
fn pil_image_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("PIL.Image")
}

fn pil_image_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    pil_image_module(py)?.getattr(name)
}

/// `PIL.UnidentifiedImageError`.
fn unidentified_image_error<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    py.import("PIL")?.getattr("UnidentifiedImageError")
}

/// `io.BytesIO()`.
fn bytes_io<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    py.import("io")?.getattr("BytesIO")?.call0()
}

/// `io.BytesIO(data)`.
fn bytes_io_with<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    py.import("io")?
        .getattr("BytesIO")?
        .call1((py_bytes(py, data),))
}

/// `mode` attribute of a Pillow image as `String`.
fn image_mode(image: &Bound<'_, PyAny>) -> PyResult<String> {
    str_of(&image.getattr("mode")?)
}

/// `size` attribute of a Pillow image as `(width, height)`.
fn image_size(image: &Bound<'_, PyAny>) -> PyResult<(i64, i64)> {
    image.getattr("size")?.extract::<(i64, i64)>()
}

/// `_PROCESS_COLORANTS.index(colorant)` / `colorant in _PROCESS_COLORANTS`.
fn process_colorant_index(py: Python<'_>, colorant: &Bound<'_, PyAny>) -> PyResult<Option<i64>> {
    for (index, name) in PROCESS_COLORANTS.iter().enumerate() {
        if colorant.eq(py_str(py, name))? {
            return Ok(Some(index as i64));
        }
    }
    Ok(None)
}

/// `_get_process_colorant_channels(color_space)`.
fn get_process_colorant_channels_impl<'py>(
    py: Python<'py>,
    color_space: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let list_class = builtins(py)?.getattr("list")?;
    if !is_instance(color_space, &list_class)? || py_len(color_space)? < 2 {
        return Ok(py_none(py));
    }
    if !color_space.get_item(0)?.eq(py_str(py, "/DeviceN"))? {
        return Ok(py_none(py));
    }
    let colorants = color_space.get_item(1)?.call_method0("get_object")?;
    if !is_instance(&colorants, &list_class)? || py_len(&colorants)? == 0 {
        return Ok(py_none(py));
    }
    let mut channels: Vec<Bound<'py, PyAny>> = Vec::new();
    for colorant in colorants.try_iter()? {
        let colorant = colorant?;
        match process_colorant_index(py, &colorant)? {
            Some(index) => channels.push(py_int(py, index)),
            None => return Ok(py_none(py)),
        }
    }
    py_list(py, channels)
}

// @pypdf: pypdf.generic._image_xobject._get_process_colorant_channels
#[pyfunction]
fn imagexobject_get_process_colorant_channels<'py>(
    py: Python<'py>,
    color_space: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(get_process_colorant_channels_impl(py, color_space)?.unbind())
}

/// `_get_image_mode(color_space, color_components, prev_mode, depth=0)`.
fn get_image_mode_impl(
    py: Python<'_>,
    color_space: &Bound<'_, PyAny>,
    color_components: &Bound<'_, PyAny>,
    prev_mode: &str,
    depth: i64,
) -> PyResult<(String, bool)> {
    if depth > MAX_IMAGE_MODE_NESTING_DEPTH {
        return Err(errors::pdf_read_error(
            py,
            "Color spaces nested too deeply. If required, consider increasing MAX_IMAGE_MODE_NESTING_DEPTH.",
        ));
    }
    if is_null_or_none(py, color_space)? {
        return Ok((String::new(), false));
    }
    let str_class = builtins(py)?.getattr("str")?;
    let list_class = builtins(py)?.getattr("list")?;
    let mut color_components: Bound<'_, PyAny> = color_components.clone();
    let mut color_space_str: Bound<'_, PyAny> = py_str(py, "");
    if is_instance(color_space, &str_class)? {
        color_space_str = color_space.clone();
    } else if !is_instance(color_space, &list_class)? {
        // raise PdfReadError("Cannot interpret color space", color_space)
        return Err(errors::raise(
            py,
            "PdfReadError",
            PyTuple::new(
                py,
                [
                    py_str(py, "Cannot interpret color space"),
                    color_space.clone(),
                ],
            )?,
        ));
    } else if py_len(color_space)? == 0 {
        return Ok((String::new(), false));
    } else {
        let first = color_space.get_item(0)?;
        if first
            .call_method1("startswith", (py_str(py, "/Cal"),))?
            .is_truthy()?
        {
            // /CalRGB or /CalGray
            let rest = first.get_item(pyutil::py_slice(py, Some(&py_int(py, 4)), None, None)?)?;
            color_space_str = operator_call(py, "add", &py_str(py, "/Device"), &rest)?;
        } else if first.eq(py_str(py, "/ICCBased"))? {
            let icc_profile = color_space.get_item(1)?.call_method0("get_object")?;
            color_components = icc_profile.get_item(py_str(py, "/N"))?;
            color_space_str =
                icc_profile.call_method1("get", (py_str(py, "/Alternate"), py_str(py, "")))?;
        } else if first.eq(py_str(py, "/Indexed"))? {
            color_space_str = color_space.get_item(1)?.call_method0("get_object")?;
            if !get_process_colorant_channels_impl(py, &color_space_str)?.is_none() {
                color_space_str = py_str(py, "/DeviceCMYK");
            }
            let (mut mode, invert_color) = get_image_mode_impl(
                py,
                &color_space_str,
                &color_components,
                prev_mode,
                depth + 1,
            )?;
            if mode == "RGB" || mode == "CMYK" {
                mode = "P".to_string();
            }
            return Ok((mode, invert_color));
        } else if first.eq(py_str(py, "/Separation"))? {
            color_space_str = color_space.get_item(2)?.call_method0("get_object")?;
            let (mode, _invert_color) = get_image_mode_impl(
                py,
                &color_space_str,
                &color_components,
                prev_mode,
                depth + 1,
            )?;
            return Ok((mode, true));
        } else if first.eq(py_str(py, "/DeviceN"))? {
            let original_color_space = color_space.clone();
            let component_count = py_len(&color_space.get_item(1)?)?;
            color_components = py_int(py, component_count as i64);
            color_space_str = color_space.get_item(2)?.call_method0("get_object")?;
            if color_space_str.eq(py_str(py, "/DeviceCMYK"))? && component_count == 1 {
                let first_colorant = original_color_space.get_item(1)?.get_item(0)?;
                if !first_colorant.eq(py_str(py, "/Black"))? {
                    let values = PyDict::new(py);
                    values.set_item("color", &first_colorant)?;
                    log_warning(
                        py,
                        XOBJECT_SOURCE,
                        "Color %(color)s converted to Gray. Please share PDF with pypdf dev team",
                        Some(&values),
                    )?;
                }
                return Ok(("L".to_string(), true));
            }
            return get_image_mode_impl(
                py,
                &color_space_str,
                &color_components,
                prev_mode,
                depth + 1,
            );
        }
    }

    let mode_map = PyDict::new(py);
    mode_map.set_item("1bit", "1")?;
    mode_map.set_item("/DeviceGray", "L")?;
    mode_map.set_item("palette", "P")?;
    mode_map.set_item("/DeviceRGB", "RGB")?;
    mode_map.set_item("/DeviceCMYK", "CMYK")?;
    mode_map.set_item("2bit", "2bits")?;
    mode_map.set_item("4bit", "4bits")?;
    let mode_values = py_list(
        py,
        vec![
            py_str(py, "1"),
            py_str(py, "L"),
            py_str(py, "P"),
            py_str(py, "RGB"),
            py_str(py, "CMYK"),
            py_str(py, "2bits"),
            py_str(py, "4bits"),
        ],
    )?;

    let mut mode: Option<Bound<'_, PyAny>> = mode_map.get_item(&color_space_str)?;
    if mode.is_none() {
        let zero = py_int(py, 0);
        let in_range = zero.le(&color_components)? && color_components.lt(py_int(py, 7))?;
        if in_range {
            mode = Some(mode_values.get_item(&color_components)?);
        }
    }
    let mode = match mode {
        Some(value) => value,
        None => py_str(py, prev_mode),
    };
    let invert_color = mode.eq(py_str(py, "CMYK"))?;
    Ok((str_of(&mode)?, invert_color))
}

// @pypdf: pypdf.generic._image_xobject._get_image_mode
#[pyfunction]
#[pyo3(signature = (color_space, color_components, prev_mode, depth=0))]
fn imagexobject_get_image_mode(
    py: Python<'_>,
    color_space: &Bound<'_, PyAny>,
    color_components: &Bound<'_, PyAny>,
    prev_mode: String,
    depth: i64,
) -> PyResult<(String, bool)> {
    get_image_mode_impl(py, color_space, color_components, &prev_mode, depth)
}

/// `bits2byte(data, size, bits, colors=1, scale=False)`.
fn bits2byte_impl(
    py: Python<'_>,
    data: &[u8],
    size: (i64, i64),
    bits: i64,
    colors: i64,
    scale: bool,
) -> PyResult<Vec<u8>> {
    // Number of samples per row = pixels per row * components per pixel.
    let samples_per_row = size.0 * colors;
    let buffer_size = samples_per_row * size.1;
    let maximum = config_item(py, "image_maximum_buffer_size")?;
    if py_int(py, buffer_size).gt(&maximum)? {
        return Err(errors::limit_reached_error(
            py,
            &format!(
                "Requested buffer size {} exceeds limit of {}.",
                buffer_size,
                str_of(&maximum)?
            ),
        ));
    }

    let mut byte_buffer = vec![0u8; buffer_size as usize];
    let mask = (1i64 << bits) - 1;
    // Scale a b-bit sample up to the full 0-255 range (e.g. 4-bit 15 -> 255)
    // when the output is consumed directly as color (RGB) rather than as a
    // palette index.
    let factor = if scale && mask != 0 { 255 / mask } else { 1 };

    let required = size.1 * ((samples_per_row * bits + 7) / 8);
    let mut data: Vec<u8> = data.to_vec();
    if (data.len() as i64) < required {
        log_plain(
            py,
            XOBJECT_SOURCE,
            "Image data is not rectangular. Adding padding.",
        )?;
        data.resize(required as usize, 0);
    }

    let mut data_index: i64 = 0;
    let mut bit: i64 = 8 - bits;
    for y in 0..size.1 {
        if bit != 8 - bits {
            data_index += 1;
            bit = 8 - bits;
        }
        for x in 0..samples_per_row {
            let index = (x + y * samples_per_row) as usize;
            let byte = *data
                .get(data_index as usize)
                .ok_or_else(|| PyIndexError::new_err("index out of range"))?;
            byte_buffer[index] = ((((byte as i64) >> bit) & mask) * factor) as u8;
            bit -= bits;
            if bit < 0 {
                data_index += 1;
                bit = 8 - bits;
            }
        }
    }
    Ok(byte_buffer)
}

// @pypdf: pypdf.generic._image_xobject.bits2byte
#[pyfunction]
#[pyo3(signature = (data, size, bits, colors=1, scale=false))]
fn imagexobject_bits2byte<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
    size: (i64, i64),
    bits: i64,
    colors: i64,
    scale: bool,
) -> PyResult<Py<PyAny>> {
    Ok(py_bytes(
        py,
        &bits2byte_impl(py, &bytes_of(data)?, size, bits, colors, scale)?,
    )
    .unbind())
}

/// `_expand_low_bit_samples(mode, size, data, color_space)`.
fn expand_low_bit_samples_impl(
    py: Python<'_>,
    mode: &str,
    size: (i64, i64),
    data: &[u8],
    color_space: &Bound<'_, PyAny>,
) -> PyResult<(String, Vec<u8>)> {
    if mode != "2bits" && mode != "4bits" {
        return Ok((mode.to_string(), data.to_vec()));
    }
    let bits = mode
        .chars()
        .next()
        .map_or(0, |value| value as i64 - '0' as i64);
    if color_space.eq(py_str(py, "/DeviceRGB"))? {
        return Ok((
            "RGB".to_string(),
            bits2byte_impl(py, data, size, bits, 3, true)?,
        ));
    }
    if color_space.eq(py_str(py, "/DeviceGray"))? {
        return Ok((
            "L".to_string(),
            bits2byte_impl(py, data, size, bits, 1, true)?,
        ));
    }
    Ok((
        "P".to_string(),
        bits2byte_impl(py, data, size, bits, 1, false)?,
    ))
}

// @pypdf: pypdf.generic._image_xobject._expand_low_bit_samples
#[pyfunction]
fn imagexobject_expand_low_bit_samples<'py>(
    py: Python<'py>,
    mode: String,
    size: (i64, i64),
    data: &Bound<'py, PyAny>,
    color_space: &Bound<'py, PyAny>,
) -> PyResult<(String, Py<PyAny>)> {
    let (mode, data) = expand_low_bit_samples_impl(py, &mode, size, &bytes_of(data)?, color_space)?;
    Ok((mode, py_bytes(py, &data).unbind()))
}

/// `_SINGLE_BAND_BYTES_PER_PIXEL[mode]`.
fn single_band_bytes_per_pixel(mode: &str) -> i64 {
    match mode {
        "1" | "L" | "P" => 1,
        "I;16" | "I;16B" | "I;16L" | "I;16N" => 2,
        _ => 4,
    }
}

/// `_get_allocated_bytes_per_pixel(mode)`.
fn get_allocated_bytes_per_pixel_impl(py: Python<'_>, mode: &str) -> PyResult<i64> {
    match pil_image_attr(py, "getmodebands")?.call1((mode,)) {
        Ok(bands) => {
            let bands = bands.extract::<i64>()?;
            if bands > 1 {
                Ok(4)
            } else {
                Ok(single_band_bytes_per_pixel(mode))
            }
        }
        // Not a mode Pillow knows: keep the name length so that such data is
        // still reported by Pillow instead of failing here.
        Err(error) if error.is_instance_of::<PyKeyError>(py) => Ok(mode.chars().count() as i64),
        Err(error) => Err(error),
    }
}

// @pypdf: pypdf.generic._image_xobject._get_allocated_bytes_per_pixel
#[pyfunction]
fn imagexobject_get_allocated_bytes_per_pixel(py: Python<'_>, mode: String) -> PyResult<i64> {
    get_allocated_bytes_per_pixel_impl(py, &mode)
}

/// `_validate_image_size(mode=..., size=...)`.
fn validate_image_size_impl(py: Python<'_>, mode: &str, size: (i64, i64)) -> PyResult<(i64, i64)> {
    let pixel_count = size.0 * size.1;
    let required_byte_count = pixel_count * get_allocated_bytes_per_pixel_impl(py, mode)?;

    let configuration = configuration(py)?;
    let maximum = configuration.getattr("image_maximum_buffer_size")?;
    if py_int(py, required_byte_count).gt(&maximum)? {
        return Err(errors::limit_reached_error(
            py,
            &format!(
                "Requested image buffer size {} exceeds limit {}.",
                required_byte_count,
                str_of(&maximum)?
            ),
        ));
    }
    Ok((pixel_count, required_byte_count))
}

// @pypdf: pypdf.generic._image_xobject._validate_image_size
#[pyfunction]
#[pyo3(signature = (*, mode, size))]
fn imagexobject_validate_image_size(
    py: Python<'_>,
    mode: String,
    size: (i64, i64),
) -> PyResult<(i64, i64)> {
    validate_image_size_impl(py, &mode, size)
}

/// `_create_image_from_bytes(mode, size, data)`.
fn create_image_from_bytes_impl<'py>(
    py: Python<'py>,
    mode: &str,
    size: (i64, i64),
    data: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let (pixel_count, _) = validate_image_size_impl(py, mode, size)?;
    let size_tuple = py_tuple(py, vec![py_int(py, size.0), py_int(py, size.1)])?;

    match pil_image_attr(py, "frombytes")?.call1((mode, &size_tuple, data)) {
        Ok(image) => Ok(image),
        Err(error) if error.is_instance_of::<PyValueError>(py) => {
            let data_length = bytes_of(data)?.len() as i64;
            if data_length == 0 {
                return Err(raise_from(
                    errors::raise_msg(
                        py,
                        "EmptyImageDataError",
                        "Data is 0 bytes, cannot process an image from empty data.",
                    ),
                    py,
                    error,
                ));
            }
            if data_length % pixel_count != 0 {
                return Err(error);
            }
            // The expansion factor counts raw samples per pixel, not the bytes
            // Pillow allocates: the modes that reach this function are "1", "L",
            // "P", "RGB", "RGBA" and "CMYK", whose names spell out their samples,
            // so len(mode) is the sample count.
            let data_bytes = bytes_of(data)?;
            let raw_byte_count = pixel_count * mode.chars().count() as i64;
            let k = raw_byte_count as f64 / data_length as f64;
            let repeats = k as i64;
            let mut expanded: Vec<u8> = Vec::new();
            for &value in &data_bytes {
                for _ in 0..repeats {
                    expanded.push(value);
                }
            }
            pil_image_attr(py, "frombytes")?.call1((mode, &size_tuple, py_bytes(py, &expanded)))
        }
        Err(error) => Err(error),
    }
}

// @pypdf: pypdf.generic._image_xobject._create_image_from_bytes
#[pyfunction]
fn imagexobject_create_image_from_bytes<'py>(
    py: Python<'py>,
    mode: String,
    size: (i64, i64),
    data: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(create_image_from_bytes_impl(py, &mode, size, data)?.unbind())
}

/// `_create_image_from_buffer(buf=..., formats=None)`.
fn create_image_from_buffer_impl<'py>(
    py: Python<'py>,
    buf: &Bound<'py, PyAny>,
    formats: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("fp", buf)?;
    kwargs.set_item(
        "formats",
        match formats {
            Some(value) => value.clone(),
            None => py_none(py),
        },
    )?;
    let image = pil_image_attr(py, "open")?.call((), Some(&kwargs))?;
    let mode = image_mode(&image)?;
    let size = image_size(&image)?;
    validate_image_size_impl(py, &mode, size)?;
    Ok(image)
}

// @pypdf: pypdf.generic._image_xobject._create_image_from_buffer
#[pyfunction]
#[pyo3(signature = (*, buf, formats=None))]
fn imagexobject_create_image_from_buffer<'py>(
    py: Python<'py>,
    buf: &Bound<'py, PyAny>,
    formats: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    Ok(create_image_from_buffer_impl(py, buf, formats)?.unbind())
}

/// `_convert_image(image=..., mode=...)`.
fn convert_image_impl<'py>(
    py: Python<'py>,
    image: &Bound<'py, PyAny>,
    mode: &str,
) -> PyResult<Bound<'py, PyAny>> {
    let size = image_size(image)?;
    validate_image_size_impl(py, mode, size)?;
    image.call_method1("convert", (mode,))
}

// @pypdf: pypdf.generic._image_xobject._convert_image
#[pyfunction]
#[pyo3(signature = (*, image, mode))]
fn imagexobject_convert_image<'py>(
    py: Python<'py>,
    image: &Bound<'py, PyAny>,
    mode: String,
) -> PyResult<Py<PyAny>> {
    Ok(convert_image_impl(py, image, &mode)?.unbind())
}

/// `_resize_image(image=..., size=...)`.
fn resize_image_impl<'py>(
    py: Python<'py>,
    image: &Bound<'py, PyAny>,
    size: (i64, i64),
) -> PyResult<Bound<'py, PyAny>> {
    let mode = image_mode(image)?;
    validate_image_size_impl(py, &mode, size)?;
    image.call_method1(
        "resize",
        (py_tuple(py, vec![py_int(py, size.0), py_int(py, size.1)])?,),
    )
}

// @pypdf: pypdf.generic._image_xobject._resize_image
#[pyfunction]
#[pyo3(signature = (*, image, size))]
fn imagexobject_resize_image<'py>(
    py: Python<'py>,
    image: &Bound<'py, PyAny>,
    size: (i64, i64),
) -> PyResult<Py<PyAny>> {
    Ok(resize_image_impl(py, image, size)?.unbind())
}

/// `_merge_image(mode=..., bands=...)`.
fn merge_image_impl<'py>(
    py: Python<'py>,
    mode: &str,
    bands: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("mode", mode)?;
    kwargs.set_item("bands", bands)?;
    if py_len(bands)? == 0 {
        return pil_image_attr(py, "merge")?.call((), Some(&kwargs));
    }
    let first = bands.get_item(0)?;
    let size = image_size(&first)?;
    validate_image_size_impl(py, mode, size)?;
    pil_image_attr(py, "merge")?.call((), Some(&kwargs))
}

// @pypdf: pypdf.generic._image_xobject._merge_image
#[pyfunction]
#[pyo3(signature = (*, mode, bands))]
fn imagexobject_merge_image<'py>(
    py: Python<'py>,
    mode: String,
    bands: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(merge_image_impl(py, &mode, bands)?.unbind())
}

/// `(color_space, base, hival, lookup)` returned by `__handle_flate__indexed`.
type FlateIndexedParts<'py> = (
    Bound<'py, PyAny>,
    Bound<'py, PyAny>,
    Bound<'py, PyAny>,
    Bound<'py, PyAny>,
);

/// `__handle_flate__indexed(color_space)`.
fn handle_flate_indexed_impl<'py>(
    py: Python<'py>,
    color_space: &Bound<'py, PyAny>,
) -> PyResult<FlateIndexedParts<'py>> {
    let count = py_len(color_space)?;
    if count == 4 {
        let mut values: Vec<Bound<'py, PyAny>> = Vec::with_capacity(4);
        for index in 0..4 {
            values.push(color_space.get_item(index)?.call_method0("get_object")?);
        }
        let mut iterator = values.into_iter();
        let first = iterator.next().expect("4 unpacked color space values");
        let second = iterator.next().expect("4 unpacked color space values");
        let third = iterator.next().expect("4 unpacked color space values");
        let fourth = iterator.next().expect("4 unpacked color space values");
        return Ok((first, second, third, fourth));
    }

    // Deal with strange AutoDesk files where `base` and `hival` look like this:
    //   /DeviceRGB\x00255
    let element1 = color_space.get_item(1)?;
    let str_class = builtins(py)?.getattr("str")?;
    let element1 = if is_instance(&element1, &str_class)? {
        element1
    } else {
        element1.call_method0("get_object")?
    };
    if count == 3 && element1.contains(py_str(py, "\x00"))? {
        let color_space_ret = color_space.get_item(0)?.call_method0("get_object")?;
        let lookup = color_space.get_item(2)?.call_method0("get_object")?;
        let split = element1.call_method1("split", (py_str(py, "\x00"),))?;
        let base = split.get_item(0)?;
        let hival = builtins(py)?.getattr("int")?.call1((split.get_item(1)?,))?;
        return Ok((color_space_ret, base, hival, lookup));
    }
    Err(errors::pdf_read_error(
        py,
        &format!(
            "Expected color space with 4 values, got {}: {}",
            count,
            repr_of(color_space)?
        ),
    ))
}

// @pypdf: pypdf.generic._image_xobject.__handle_flate__indexed
#[pyfunction(name = "imagexobject_handle_flate__indexed")]
fn imagexobject_handle_flate_indexed<'py>(
    py: Python<'py>,
    color_space: &Bound<'py, PyAny>,
) -> PyResult<FlateIndexedParts<'py>> {
    handle_flate_indexed_impl(py, color_space)
}

/// `_handle_flate(size, data, mode, color_space, colors, obj_as_text)`.
fn handle_flate_impl<'py>(
    py: Python<'py>,
    size: (i64, i64),
    data: &[u8],
    mode: &str,
    color_space: &Bound<'py, PyAny>,
    _colors: &Bound<'py, PyAny>,
    obj_as_text: &str,
) -> PyResult<(Bound<'py, PyAny>, String, String, bool)> {
    let array_class = generic_class(py, "ArrayObject")?;
    let encoded_stream_class = generic_class(py, "EncodedStreamObject")?;
    let decoded_stream_class = generic_class(py, "DecodedStreamObject")?;
    let text_string_class = generic_class(py, "TextStringObject")?;
    let str_class = builtins(py)?.getattr("str")?;

    let mut color_space: Bound<'py, PyAny> = color_space.clone();
    let mut mode: String = mode.to_string();
    let mut base: Option<Bound<'py, PyAny>> = None;
    let mut hival: i64 = 0;
    let mut lookup: Option<Bound<'py, PyAny>> = None;
    if is_instance(&color_space, &array_class)?
        && color_space.get_item(0)?.eq(py_str(py, "/Indexed"))?
    {
        let (indexed_color_space, indexed_base, indexed_hival, indexed_lookup) =
            handle_flate_indexed_impl(py, &color_space)?;
        color_space = indexed_color_space;
        base = Some(indexed_base);
        hival = indexed_hival.extract::<i64>()?;
        lookup = Some(indexed_lookup);
    }
    let data = data.to_vec();
    let (expanded_mode, expanded_data) =
        expand_low_bit_samples_impl(py, &mode, size, &data, &color_space)?;
    mode = expanded_mode;
    let mut img = create_image_from_bytes_impl(py, &mode, size, &py_bytes(py, &expanded_data))?;

    if color_space.eq(py_str(py, "/Indexed"))? {
        let mut lookup = match lookup {
            Some(value) => value,
            None => py_none(py),
        };
        if is_instance(&lookup, &encoded_stream_class)?
            || is_instance(&lookup, &decoded_stream_class)?
        {
            lookup = lookup.call_method0("get_data")?;
        }
        if is_instance(&lookup, &text_string_class)? {
            lookup = lookup.getattr("original_bytes")?;
        }
        if is_instance(&lookup, &str_class)? {
            lookup = lookup.call_method0("encode")?;
        }
        let base = base.expect("indexed color space has a base color space");
        let channels = get_process_colorant_channels_impl(py, &base)?;
        let mut base = base;
        if !channels.is_none() {
            // Put each tint into its own CMYK channel instead of evaluating the tint transform.
            let channels_vec: Vec<i64> = channels.extract()?;
            let colorant_count = channels_vec.len() as i64;
            let lookup_bytes = bytes_of(&lookup)?;
            let mut cmyk_lookup: Vec<u8> = Vec::new();
            let mut index: i64 = 0;
            while index < lookup_bytes.len() as i64 - colorant_count + 1 {
                let mut cmyk = [0u8; 4];
                for (offset, tint) in lookup_bytes
                    [index as usize..(index + colorant_count) as usize]
                    .iter()
                    .enumerate()
                {
                    cmyk[channels_vec[offset] as usize] = *tint;
                }
                cmyk_lookup.extend_from_slice(&cmyk);
                index += colorant_count;
            }
            lookup = py_bytes(py, &cmyk_lookup);
            base = py_str(py, "/DeviceCMYK");
        }
        let base_mode = get_image_mode_impl(py, &base, &py_int(py, 0), "", 0)?.0;
        let table_entry: Option<(i64, &str, &str)> = match base_mode.as_str() {
            "1" => Some((0, "", "")),
            "L" => Some((1, "P", "L")),
            "P" => Some((0, "", "")),
            "RGB" => Some((3, "P", "RGB")),
            "CMYK" => Some((4, "P", "CMYK")),
            _ => None,
        };
        let table_entry = match table_entry {
            Some(entry) => entry,
            None => {
                let values = PyDict::new(py);
                values.set_item("base", &base)?;
                log_warning(
                    py,
                    XOBJECT_SOURCE,
                    "Base %(base)s not coded. Please share PDF with pypdf dev team",
                    Some(&values),
                )?;
                lookup = py_none(py);
                (0, "", "")
            }
        };
        let (nb, conv, table_mode) = table_entry;
        mode = table_mode.to_string();
        let mut did_lookup = false;
        if !lookup.is_none() {
            did_lookup = true;
        }
        let img_mode = image_mode(&img)?;
        if did_lookup {
            if img_mode == "1" && nb == 0 {
                // A two-color lookup needs at least one byte per color, but the
                // base color space resolves to a 1-bit or palette image.
                let values = PyDict::new(py);
                values.set_item("base", &base)?;
                log_warning(
                    py,
                    XOBJECT_SOURCE,
                    "Cannot apply lookup for base %(base)s to image with mode 1. Please share PDF with pypdf dev team",
                    Some(&values),
                )?;
            } else if img_mode == "1" {
                // Two values ("high" and "low").
                let expected_count = 2 * nb;
                let mut lookup_bytes = bytes_of(&lookup)?;
                let actual_count = lookup_bytes.len() as i64;
                if actual_count != expected_count {
                    if actual_count < expected_count {
                        let values = PyDict::new(py);
                        values.set_item("expected_count", py_int(py, expected_count))?;
                        values.set_item("actual_count", py_int(py, actual_count))?;
                        log_warning(
                            py,
                            XOBJECT_SOURCE,
                            "Not enough lookup values: Expected %(expected_count)d, got %(actual_count)d.",
                            Some(&values),
                        )?;
                        lookup_bytes.resize(expected_count as usize, 0);
                    } else {
                        let check = utils_attr(py, "check_if_whitespace_only")?
                            .call1((py_bytes(py, &lookup_bytes[expected_count as usize..]),))?;
                        if !check.is_truthy()? {
                            let values = PyDict::new(py);
                            values.set_item("expected_count", py_int(py, expected_count))?;
                            values.set_item("actual_count", py_int(py, actual_count))?;
                            log_warning(
                                py,
                                XOBJECT_SOURCE,
                                "Too many lookup values: Expected %(expected_count)d, got %(actual_count)d.",
                                Some(&values),
                            )?;
                        }
                    }
                    lookup_bytes.truncate(expected_count.max(0) as usize);
                }
                let colors_arr = (
                    py_bytes(py, &lookup_bytes[0..nb.max(0) as usize]),
                    py_bytes(py, &lookup_bytes[nb as usize..]),
                );
                let source = convert_image_impl(py, &img, "L")?;
                let mut bands: Vec<Bound<'py, PyAny>> = Vec::new();
                let low_values = bytes_of(&colors_arr.0)?;
                let high_values = bytes_of(&colors_arr.1)?;
                for (low_value, high_value) in low_values.iter().zip(high_values.iter()) {
                    let mut lut: Vec<Bound<'py, PyAny>> = Vec::with_capacity(256);
                    for _ in 0..128 {
                        lut.push(py_int(py, *low_value as i64));
                    }
                    for _ in 0..128 {
                        lut.push(py_int(py, *high_value as i64));
                    }
                    let lut = py_list(py, lut)?;
                    bands.push(source.call_method1("point", (lut,))?);
                }
                img = if nb == 1 {
                    bands.remove(0)
                } else {
                    merge_image_impl(py, table_mode, &py_list(py, bands)?)?
                };
            } else {
                img = convert_image_impl(py, &img, conv)?;
                let lookup_bytes = bytes_of(&lookup)?;
                let expected = (hival + 1) * nb;
                if lookup_bytes.len() as i64 != expected {
                    let values = PyDict::new(py);
                    values.set_item("obj_as_text", obj_as_text)?;
                    log_warning(
                        py,
                        XOBJECT_SOURCE,
                        "Invalid Lookup Table in %(obj_as_text)s",
                        Some(&values),
                    )?;
                } else {
                    let mut final_mode = table_mode.to_string();
                    let mut final_lookup = lookup_bytes;
                    if final_mode == "L" {
                        // gray lookup does not work: it is converted to a similar RGB lookup
                        let mut joined: Vec<u8> = Vec::with_capacity(final_lookup.len() * 3);
                        for value in &final_lookup {
                            joined.extend_from_slice(&[*value, *value, *value]);
                        }
                        final_lookup = joined;
                        final_mode = "RGB".to_string();
                    } else if final_mode == "CMYK" {
                        // Convert the CMYK palette to RGB manually.
                        let mut rgb: Vec<u8> = Vec::new();
                        let mut index = 0usize;
                        while index + 4 <= final_lookup.len() {
                            let c = final_lookup[index] as f64;
                            let m = final_lookup[index + 1] as f64;
                            let y = final_lookup[index + 2] as f64;
                            let k = final_lookup[index + 3] as f64;
                            let r = (255.0 * (1.0 - c / 255.0) * (1.0 - k / 255.0)) as i64;
                            let g = (255.0 * (1.0 - m / 255.0) * (1.0 - k / 255.0)) as i64;
                            let b = (255.0 * (1.0 - y / 255.0) * (1.0 - k / 255.0)) as i64;
                            rgb.extend_from_slice(&[r as u8, g as u8, b as u8]);
                            index += 4;
                        }
                        final_lookup = rgb;
                        final_mode = "RGB".to_string();
                    }
                    mode = final_mode.clone();
                    let kwargs = PyDict::new(py);
                    kwargs.set_item("rawmode", &final_mode)?;
                    img.call_method("putpalette", (py_bytes(py, &final_lookup),), Some(&kwargs))?;
                }
            }
        }
        let convert_mode = if base.eq(py_str(py, "/DeviceGray"))? {
            "L"
        } else {
            "RGB"
        };
        img = convert_image_impl(py, &img, convert_mode)?;
    } else if !is_null_or_none(py, &color_space)?
        && color_space.get_item(0)?.eq(py_str(py, "/ICCBased"))?
    {
        // Exclude pure black-and-white images.
        if mode != "1" {
            // Table 65 - Additional Entries Specific to an ICC Profile Stream Dictionary
            let mode2 = get_image_mode_impl(py, &color_space, _colors, &mode, 0)?.0;
            if mode != mode2 {
                // reloaded as mode may have changed
                img = create_image_from_bytes_impl(py, &mode, size, &py_bytes(py, &expanded_data))?;
            }
        }
    }

    let (image_format, extension) = if mode == "CMYK" {
        ("TIFF".to_string(), ".tif".to_string())
    } else {
        ("PNG".to_string(), ".png".to_string())
    };
    Ok((img, image_format, extension, false))
}

// @pypdf: pypdf.generic._image_xobject._handle_flate
#[pyfunction]
fn imagexobject_handle_flate<'py>(
    py: Python<'py>,
    size: (i64, i64),
    data: &Bound<'py, PyAny>,
    mode: String,
    color_space: &Bound<'py, PyAny>,
    colors: &Bound<'py, PyAny>,
    obj_as_text: String,
) -> PyResult<(Py<PyAny>, String, String, bool)> {
    let (img, image_format, extension, invert_color) = handle_flate_impl(
        py,
        size,
        &bytes_of(data)?,
        &mode,
        color_space,
        colors,
        &obj_as_text,
    )?;
    Ok((img.unbind(), image_format, extension, invert_color))
}

/// `_handle_jpx(size, data, mode, color_space, colors)`.
fn handle_jpx_impl<'py>(
    py: Python<'py>,
    _size: (i64, i64),
    data: &[u8],
    mode: &str,
    color_space: &Bound<'py, PyAny>,
    colors: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyAny>, String, String, bool)> {
    let buf = bytes_io_with(py, data)?;
    let formats = py_tuple(py, vec![py_str(py, "JPEG2000")])?;
    let img1 = create_image_from_buffer_impl(py, &buf, Some(&formats))?;
    let (mut mode, mut invert_color) = get_image_mode_impl(py, color_space, colors, mode, 0)?;
    if mode.is_empty() {
        mode = image_mode(&img1)?;
        invert_color = mode == "CMYK";
    }
    let img1_mode = image_mode(&img1)?;
    if img1_mode == "RGBA" && mode == "RGB" {
        mode = "RGBA".to_string();
    }
    // we need to convert to the good mode
    let img = if img1_mode == mode
        || ((img1_mode == "L" && mode == "P") || (img1_mode == "P" && mode == "L"))
    {
        // L and P are indexed modes which should not be changed.
        img1
    } else if (img1_mode == "RGBA" && mode == "CMYK") || (img1_mode == "CMYK" && mode == "RGBA") {
        // RGBA / CMYK are 4bytes encoding where the encoding should be corrected
        create_image_from_bytes_impl(
            py,
            &mode,
            image_size(&img1)?,
            &img1.call_method0("tobytes")?,
        )?
    } else {
        convert_image_impl(py, &img1, &mode)?
    };
    // CMYK conversion
    let mut img = img;
    if image_mode(&img)? == "CMYK" && color_space.eq(py_str(py, "/ICCBased"))? {
        img = convert_image_impl(py, &img, "RGB")?;
    }
    Ok((
        img,
        "JPEG2000".to_string(),
        ".jp2".to_string(),
        invert_color,
    ))
}

// @pypdf: pypdf.generic._image_xobject._handle_jpx
#[pyfunction]
fn imagexobject_handle_jpx<'py>(
    py: Python<'py>,
    size: (i64, i64),
    data: &Bound<'py, PyAny>,
    mode: String,
    color_space: &Bound<'py, PyAny>,
    colors: &Bound<'py, PyAny>,
) -> PyResult<(Py<PyAny>, String, String, bool)> {
    let (img, image_format, extension, invert_color) =
        handle_jpx_impl(py, size, &bytes_of(data)?, &mode, color_space, colors)?;
    Ok((img.unbind(), image_format, extension, invert_color))
}

/// `[float] * bands` as a flat Python list.
fn repeat_decode_pairs<'py>(
    py: Python<'py>,
    first: f64,
    second: f64,
    bands: usize,
) -> PyResult<Bound<'py, PyAny>> {
    let mut items: Vec<Bound<'py, PyAny>> = Vec::with_capacity(bands * 2);
    for _ in 0..bands {
        items.push(py_float(py, first));
        items.push(py_float(py, second));
    }
    py_list(py, items)
}

/// `img.getbands()` -> number of bands.
fn band_count(image: &Bound<'_, PyAny>) -> PyResult<usize> {
    py_len(&image.call_method0("getbands")?)
}

/// `_apply_decode(...)`.
fn apply_decode_impl<'py>(
    py: Python<'py>,
    img: &Bound<'py, PyAny>,
    x_object: &Bound<'py, PyAny>,
    lfilters: &Bound<'py, PyAny>,
    color_space: &Bound<'py, PyAny>,
    invert_color: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let decode_key = py_str(py, "/Decode");
    let dct = py_str(py, "/DCTDecode");
    let jpx = py_str(py, "/JPXDecode");
    let mode = image_mode(img)?;
    let mut decode: Option<Bound<'py, PyAny>> = None;

    if x_object.contains(&decode_key)? {
        let raw = x_object.get_item(&decode_key)?;
        if mode == "CMYK" && lfilters.eq(&dct)? {
            // Adobe writes CMYK JPEGs with inverted component values. An explicit
            // /Decode array is expressed in terms of the true values, so it has to
            // be combined with that inversion rather than replacing it.
            // A malformed odd-length array is left untouched here and dropped with
            // a warning further down.
            let length = py_len(&raw)? as i64;
            let mut items: Vec<Bound<'py, PyAny>> = Vec::new();
            let mut index = 0i64;
            while index < length - 1 {
                items.push(raw.get_item(py_int(py, index + 1))?);
                items.push(raw.get_item(py_int(py, index))?);
                index += 2;
            }
            decode = Some(py_list(py, items)?);
        } else {
            decode = Some(raw);
        }
    } else if mode == "CMYK" && lfilters.eq(&jpx)? {
        let bands = band_count(img)?;
        decode = Some(if invert_color {
            repeat_decode_pairs(py, 0.0, 1.0, bands)?
        } else {
            repeat_decode_pairs(py, 1.0, 0.0, bands)?
        });
    } else if (mode == "CMYK" && lfilters.eq(&dct)?) || (invert_color && mode == "L") {
        let bands = band_count(img)?;
        decode = Some(repeat_decode_pairs(py, 1.0, 0.0, bands)?);
    }

    let array_class = generic_class(py, "ArrayObject")?;
    if is_instance(color_space, &array_class)? {
        let first = color_space.get_item(0)?.call_method0("get_object")?;
        if first.eq(py_str(py, "/Indexed"))? {
            // decode is meaningless if Indexed
            decode = None;
        }
        if first.eq(py_str(py, "/Separation"))? {
            decode = Some(repeat_decode_pairs(py, 1.0, 0.0, band_count(img)?)?);
        }
    }

    let list_class = builtins(py)?.getattr("list")?;
    if let Some(value) = &decode {
        if !is_instance(value, &list_class)? || py_len(value)? % 2 != 0 {
            // /Decode is read straight from the image dictionary and must hold an
            // even number of values (a [min max] pair per component). A malformed
            // array would otherwise read past its end at decode[i + 1]; drop it and
            // render the image without the remap.
            let values = PyDict::new(py);
            values.set_item("decode", value)?;
            log_warning(
                py,
                XOBJECT_SOURCE,
                "Ignoring malformed /Decode array %(decode)s; expected an even number of values.",
                Some(&values),
            )?;
            decode = None;
        }
    }

    if let Some(value) = &decode {
        let length = py_len(value)?;
        let mut all_identity = true;
        for index in 0..length {
            if !value
                .get_item(index as i64)?
                .eq(py_int(py, (index % 2) as i64))?
            {
                all_identity = false;
                break;
            }
        }
        if !all_identity {
            let mut lut: Vec<Bound<'py, PyAny>> = Vec::with_capacity(length / 2 * 256);
            let mut index = 0usize;
            while index < length {
                let dmin: f64 = value.get_item(index as i64)?.extract()?;
                let dmax: f64 = value.get_item((index + 1) as i64)?.extract()?;
                for j in 0..256 {
                    let scaled = 255.0 * ((j as f64) / 255.0 * (dmax - dmin) + dmin);
                    // Python's ``round()`` on floats: round half to even.
                    lut.push(py_int(py, scaled.round_ties_even() as i64));
                }
                index += 2;
            }
            let lut = py_list(py, lut)?;
            return img.call_method1("point", (lut,));
        }
    }
    Ok(img.clone())
}

// @pypdf: pypdf.generic._image_xobject._apply_decode
#[pyfunction]
fn imagexobject_apply_decode<'py>(
    py: Python<'py>,
    img: &Bound<'py, PyAny>,
    x_object: &Bound<'py, PyAny>,
    lfilters: &Bound<'py, PyAny>,
    color_space: &Bound<'py, PyAny>,
    invert_color: bool,
) -> PyResult<Py<PyAny>> {
    Ok(apply_decode_impl(py, img, x_object, lfilters, color_space, invert_color)?.unbind())
}

/// `_get_mode_and_invert_color(x_object, colors, color_space)`.
fn get_mode_and_invert_color_impl(
    py: Python<'_>,
    x_object: &Bound<'_, PyAny>,
    colors: &Bound<'_, PyAny>,
    color_space: &Bound<'_, PyAny>,
) -> PyResult<(String, bool)> {
    let bits = x_object.call_method1("get", (py_str(py, "/BitsPerComponent"), py_int(py, 8)))?;
    let color_space_key = py_str(py, "/ColorSpace");
    if x_object.contains(&color_space_key)?
        && x_object
            .get_item(&color_space_key)?
            .eq(py_str(py, "/DeviceRGB"))?
        && bits.ge(py_int(py, 8))?
    {
        // Full-width /DeviceRGB samples are RGB. Narrower samples stay on
        // the low-bit path so they can be expanded (#3367).
        return Ok(("RGB".to_string(), false));
    }
    if bits.lt(py_int(py, 8))? {
        let bits_mode = py_str(py, &format!("{}bit", str_of(&bits)?));
        get_image_mode_impl(py, &bits_mode, &py_int(py, 0), "", 0)
    } else {
        let components: Bound<'_, PyAny> = if colors.eq(py_int(py, 1))?
            && !is_null_or_none(py, color_space)?
            && !color_space.contains(py_str(py, "Gray"))?
        {
            py_int(py, 2)
        } else {
            colors.clone()
        };
        get_image_mode_impl(py, color_space, &components, "", 0)
    }
}

// @pypdf: pypdf.generic._image_xobject._get_mode_and_invert_color
#[pyfunction]
fn imagexobject_get_mode_and_invert_color(
    py: Python<'_>,
    x_object: &Bound<'_, PyAny>,
    colors: &Bound<'_, PyAny>,
    color_space: &Bound<'_, PyAny>,
) -> PyResult<(String, bool)> {
    get_mode_and_invert_color_impl(py, x_object, colors, color_space)
}

/// `_apply_alpha(...)`.
fn apply_alpha_impl<'py>(
    py: Python<'py>,
    img: &Bound<'py, PyAny>,
    x_object: &Bound<'py, PyAny>,
    obj_as_text: &str,
    image_format: &str,
    extension: &str,
    visited: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyAny>, String, String)> {
    let smask_key = py_str(py, "/SMask");
    if !x_object.contains(&smask_key)? {
        return Ok((img.clone(), extension.to_string(), image_format.to_string()));
    }

    let s_mask = x_object.get_item(&smask_key)?;
    let s_mask_id = py_id(py, &s_mask)?;
    if visited.contains(&s_mask_id)? {
        // A soft mask that refers back to an image already being
        // converted would recurse until the interpreter runs out of
        // stack. Such a chain cannot describe a real alpha channel, so
        // drop the mask and keep the image we have.
        let values = PyDict::new(py);
        values.set_item("obj_as_text", obj_as_text)?;
        log_warning(
            py,
            XOBJECT_SOURCE,
            "Ignoring cyclic /SMask reference in %(obj_as_text)s",
            Some(&values),
        )?;
        return Ok((img.clone(), extension.to_string(), image_format.to_string()));
    }

    let (_, _, mut alpha) = xobj_to_image_impl(py, &s_mask, None, Some(visited))?;
    // TODO: implement mask
    if image_mode(&alpha)? != "L" {
        alpha = convert_image_impl(py, &alpha, "L")?;
    }
    let img_size = img.getattr("size")?;
    if !img_size.eq(alpha.getattr("size")?)? {
        // Both are mapped onto the unit square, thus a soft mask may use other dimensions than its image.
        let size = image_size(img)?;
        alpha = resize_image_impl(py, &alpha, size)?;
    }
    let mut img = img.clone();
    let img_mode = image_mode(&img)?;
    if img_mode == "P" {
        img = convert_image_impl(py, &img, "RGB")?;
    } else if img_mode == "1" {
        img = convert_image_impl(py, &img, "L")?;
    }
    img.call_method1("putalpha", (&alpha,))?;

    let (image_format, extension) = if image_format.contains("JPEG") {
        ("JPEG2000".to_string(), ".jp2".to_string())
    } else {
        ("PNG".to_string(), ".png".to_string())
    };

    Ok((img, extension, image_format))
}

// @pypdf: pypdf.generic._image_xobject._apply_alpha
#[pyfunction]
#[pyo3(signature = (*, img, x_object, obj_as_text, image_format, extension, visited))]
fn imagexobject_apply_alpha<'py>(
    py: Python<'py>,
    img: &Bound<'py, PyAny>,
    x_object: &Bound<'py, PyAny>,
    obj_as_text: String,
    image_format: String,
    extension: String,
    visited: &Bound<'py, PyAny>,
) -> PyResult<(Py<PyAny>, String, String)> {
    let (img, extension, image_format) = apply_alpha_impl(
        py,
        img,
        x_object,
        &obj_as_text,
        &image_format,
        &extension,
        visited,
    )?;
    Ok((img.unbind(), extension, image_format))
}

/// `_xobj_to_image(x_object, pillow_parameters=None, visited=None)`.
fn xobj_to_image_impl<'py>(
    py: Python<'py>,
    x_object: &Bound<'py, PyAny>,
    pillow_parameters: Option<&Bound<'py, PyAny>>,
    visited: Option<&Bound<'py, PyAny>>,
) -> PyResult<(String, Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let visited: Bound<'py, PyAny> = match visited {
        Some(value) => value.clone(),
        None => py_set(py)?,
    };
    let x_object_id = py_id(py, x_object)?;
    visited.call_method1("add", (&x_object_id,))?;

    // For error reporting
    let obj_as_text = repr_of(x_object)?;

    let list_class = builtins(py)?.getattr("list")?;

    // Get size and data
    let size = (
        x_object.get_item(py_str(py, "/Width"))?,
        x_object.get_item(py_str(py, "/Height"))?,
    );
    let data_value = x_object.call_method0("get_data")?;
    let data_value = if is_instance_of::<PyString>(&data_value) {
        data_value.call_method0("encode")?
    } else {
        data_value
    };
    let mut data = bytes_of(&data_value)?;
    let size_product = operator_call(py, "mul", &size.0, &size.1)?;
    let modulo = operator_call(py, "mod", &py_int(py, data.len() as i64), &size_product)?;
    if modulo.eq(py_int(py, 1))? && data.last() == Some(&0x0a) {
        // i.e. '\n'
        data.pop();
    }

    // Get color properties
    let colors = x_object.call_method1("get", (py_str(py, "/Colors"), py_int(py, 1)))?;
    let mut color_space = x_object
        .call_method1("get", (py_str(py, "/ColorSpace"), null_object(py)?))?
        .call_method0("get_object")?;
    if is_instance(&color_space, &list_class)? && py_len(&color_space)? == 1 {
        color_space = color_space.get_item(0)?.call_method0("get_object")?;
    }

    let (mode, invert_color) = get_mode_and_invert_color_impl(py, x_object, &colors, &color_space)?;

    // Get filters
    let filters = x_object
        .call_method1("get", (py_str(py, "/Filter"), null_object(py)?))?
        .call_method0("get_object")?;
    // An empty array is a valid way of saying that no filter is applied: treat it
    // like a missing /Filter entry rather than raising IndexError on the lookup.
    let last_filter = if filters.is_truthy()? && is_instance(&filters, &list_class)? {
        filters.get_item(py_int(py, -1))?
    } else {
        filters.clone()
    };
    let decode_parms = x_object.call_method1("get", (py_str(py, "/DecodeParms"),))?;
    if decode_parms.is_truthy()? {
        let tuple_class = builtins(py)?.getattr("tuple")?;
        if is_instance(&decode_parms, &tuple_class)? || is_instance(&decode_parms, &list_class)? {
            let _first_decode_parms = decode_parms.get_item(0)?;
        }
    }

    let size_tuple = image_size_tuple(py, &size.0, &size.1)?;
    let mut image_format: String;
    let mut extension: String;
    let mut invert_color = invert_color;
    let mut img: Bound<'py, PyAny>;

    let flate = py_str(py, "/FlateDecode");
    let run_length = py_str(py, "/RunLengthDecode");
    let lzw = py_str(py, "/LZWDecode");
    let ascii85 = py_str(py, "/ASCII85Decode");
    let dct = py_str(py, "/DCTDecode");
    let jpx = py_str(py, "/JPXDecode");
    let ccitt = py_str(py, "/CCITTFaxDecode");
    let jbig2 = py_str(py, "/JBIG2Decode");

    if last_filter.eq(&flate)? || last_filter.eq(&run_length)? {
        let (flate_img, flate_format, flate_extension, _) = handle_flate_impl(
            py,
            size_tuple,
            &data,
            &mode,
            &color_space,
            &colors,
            &obj_as_text,
        )?;
        img = flate_img;
        image_format = flate_format;
        extension = flate_extension;
    } else if last_filter.eq(&lzw)? || last_filter.eq(&ascii85)? {
        // The filter does not tell which image format the decoded data has, so
        // let Pillow detect it.
        let buf = bytes_io_with(py, &data)?;
        let formats = py_tuple(py, vec![py_str(py, "TIFF"), py_str(py, "PNG")])?;
        match create_image_from_buffer_impl(py, &buf, Some(&formats)) {
            Ok(buffer_img) => {
                let detected = buffer_img.getattr("format")?;
                image_format = str_of(&detected)?;
                extension = if image_format == "TIFF" {
                    ".tiff".to_string()
                } else {
                    ".png".to_string()
                };
                img = buffer_img;
            }
            Err(error) => {
                let unidentified = unidentified_image_error(py)?;
                if !error.value(py).is_instance(&unidentified)? {
                    return Err(error);
                }
                // Not a TIFF or PNG container: interpret the data as raw pixel samples.
                let (fallback_mode, fallback_data) =
                    expand_low_bit_samples_impl(py, &mode, size_tuple, &data, &color_space)?;
                img = create_image_from_bytes_impl(
                    py,
                    &fallback_mode,
                    size_tuple,
                    &py_bytes(py, &fallback_data),
                )?;
                // There is no format information, thus fall back to the usual one
                // for the filter.
                image_format = if last_filter.eq(&lzw)? {
                    "TIFF".to_string()
                } else {
                    "PNG".to_string()
                };
                extension = if image_format == "TIFF" {
                    ".tiff".to_string()
                } else {
                    ".png".to_string()
                };
            }
        }
    } else if last_filter.eq(&dct)? {
        let buf = bytes_io_with(py, &data)?;
        let formats = py_tuple(py, vec![py_str(py, "JPEG")])?;
        img = create_image_from_buffer_impl(py, &buf, Some(&formats))?;
        image_format = "JPEG".to_string();
        extension = ".jpg".to_string();
        // invert_color kept unchanged
    } else if last_filter.eq(&jpx)? {
        let (jpx_img, jpx_format, jpx_extension, jpx_invert) =
            handle_jpx_impl(py, size_tuple, &data, &mode, &color_space, &colors)?;
        img = jpx_img;
        image_format = jpx_format;
        extension = jpx_extension;
        invert_color = jpx_invert;
    } else if last_filter.eq(&ccitt)? {
        let buf = bytes_io_with(py, &data)?;
        let formats = py_tuple(py, vec![py_str(py, "TIFF")])?;
        img = create_image_from_buffer_impl(py, &buf, Some(&formats))?;
        image_format = "TIFF".to_string();
        extension = ".tiff".to_string();
        invert_color = false;
    } else if last_filter.eq(&jbig2)? {
        let buf = bytes_io_with(py, &data)?;
        let formats = py_tuple(py, vec![py_str(py, "PNG"), py_str(py, "PPM")])?;
        img = create_image_from_buffer_impl(py, &buf, Some(&formats))?;
        image_format = "PNG".to_string();
        extension = ".png".to_string();
        invert_color = false;
    } else if mode == "CMYK" {
        img = create_image_from_bytes_impl(py, &mode, size_tuple, &py_bytes(py, &data))?;
        image_format = "TIFF".to_string();
        extension = ".tif".to_string();
        invert_color = false;
    } else if mode.is_empty() {
        return Err(errors::pdf_read_error(
            py,
            &format!("ColorSpace field not found in {}", repr_of(x_object)?),
        ));
    } else {
        // Images without a filter (for example inline images) reach Pillow
        // directly, so low-bit samples have to be expanded here as well.
        let (expanded_mode, expanded_data) =
            expand_low_bit_samples_impl(py, &mode, size_tuple, &data, &color_space)?;
        img = create_image_from_bytes_impl(
            py,
            &expanded_mode,
            size_tuple,
            &py_bytes(py, &expanded_data),
        )?;
        image_format = "PNG".to_string();
        extension = ".png".to_string();
        invert_color = false;
    }

    img = apply_decode_impl(py, &img, x_object, &last_filter, &color_space, invert_color)?;
    let (alpha_img, alpha_extension, alpha_format) = apply_alpha_impl(
        py,
        &img,
        x_object,
        &obj_as_text,
        &image_format,
        &extension,
        &visited,
    )?;
    img = alpha_img;
    extension = alpha_extension;
    image_format = alpha_format;

    let pillow_parameters: Bound<'py, PyAny> = match pillow_parameters {
        Some(value) => value.clone(),
        None => PyDict::new(py).into_any(),
    };
    // Preserve JPEG image quality - see issue #3515.
    if image_format == "JPEG" {
        // This prevents: Cannot use 'keep' when original image is not a JPEG:
        // "JPEG" is the value of PIL.JpegImagePlugin.JpegImageFile.format
        img.setattr("format", "JPEG")?;
        if !pillow_parameters.contains(py_str(py, "quality"))? {
            pillow_parameters.set_item(py_str(py, "quality"), py_str(py, "keep"))?;
        }
    }

    // Save image to bytes
    let img_byte_arr = bytes_io(py)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("format", &image_format)?;
    let mut duplicate_format = false;
    for key in pillow_parameters.try_iter()? {
        let key = key?;
        let value = pillow_parameters.get_item(&key)?;
        if key.eq(py_str(py, "format"))? {
            duplicate_format = true;
        }
        kwargs.set_item(&key, &value)?;
    }
    if duplicate_format {
        return Err(PyTypeError::new_err(
            "save() got multiple values for keyword argument 'format'",
        ));
    }
    let saved = img.call_method("save", (&img_byte_arr,), Some(&kwargs));
    let data = match saved {
        Ok(_) => bytes_of(&img_byte_arr.call_method0("getvalue")?)?,
        Err(error) if error.is_instance_of::<PyOSError>(py) => {
            // in case of we convert to RGBA and then to PNG
            let img1 = convert_image_impl(py, &img, "RGBA")?;
            let image_format = "PNG";
            let img_byte_arr = bytes_io(py)?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("format", image_format)?;
            img1.call_method("save", (&img_byte_arr,), Some(&kwargs))?;
            let data = bytes_of(&img_byte_arr.call_method0("getvalue")?)?;
            let reloaded = match create_image_from_buffer_impl(py, &bytes_io_with(py, &data)?, None)
            {
                Ok(image) => image,
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(error) => {
                    let values = PyDict::new(py);
                    values.set_item("exception", error.value(py))?;
                    log_warning(
                        py,
                        XOBJECT_SOURCE,
                        "Failed loading image: %(exception)s",
                        Some(&values),
                    )?;
                    py_none(py)
                }
            };
            return Ok((".png".to_string(), py_bytes(py, &data), reloaded));
        }
        Err(error) => return Err(error),
    };

    let reloaded = match create_image_from_buffer_impl(py, &bytes_io_with(py, &data)?, None) {
        Ok(image) => image,
        Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
        Err(error) => {
            let values = PyDict::new(py);
            values.set_item("exception", error.value(py))?;
            log_warning(
                py,
                XOBJECT_SOURCE,
                "Failed loading image: %(exception)s",
                Some(&values),
            )?;
            py_none(py)
        }
    };
    Ok((extension, py_bytes(py, &data), reloaded))
}

/// `(size[0], size[1])` as a Python tuple of ints (the values are already ints).
fn image_size_tuple<'py>(
    py: Python<'py>,
    width: &Bound<'py, PyAny>,
    height: &Bound<'py, PyAny>,
) -> PyResult<(i64, i64)> {
    let _ = py;
    let width = width.extract::<i64>()?;
    let height = height.extract::<i64>()?;
    Ok((width, height))
}

// @pypdf: pypdf.generic._image_xobject._xobj_to_image
#[pyfunction]
#[pyo3(signature = (x_object, pillow_parameters=None, visited=None))]
fn imagexobject_xobj_to_image<'py>(
    py: Python<'py>,
    x_object: &Bound<'py, PyAny>,
    pillow_parameters: Option<&Bound<'py, PyAny>>,
    visited: Option<&Bound<'py, PyAny>>,
) -> PyResult<(String, Py<PyAny>, Py<PyAny>)> {
    let (extension, data, image) = xobj_to_image_impl(py, x_object, pillow_parameters, visited)?;
    Ok((extension, data.unbind(), image.unbind()))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    // pypdf.generic._files
    module.add_function(wrap_pyfunction!(files_embeddedfile_create_new, module)?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_get_names_array,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_get_insertion_index,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_alternative_name_get,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_alternative_name_set,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_description_get,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_description_set,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_associated_file_relationship_get,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_associated_file_relationship_set,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_embedded_file, module)?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_params, module)?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_ensure_params, module)?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_subtype_get, module)?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_subtype_set, module)?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_content_get, module)?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_content_set, module)?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_size_get, module)?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_size_set, module)?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_creation_date_get,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_creation_date_set,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_modification_date_get,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_modification_date_set,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_checksum_get, module)?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_checksum_set, module)?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_names, module)?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_delete, module)?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_repr, module)?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_report_malformed,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        files_embeddedfile_load_from_names,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(files_embeddedfile_load, module)?)?;

    // pypdf.generic._image_inline
    module.add_function(wrap_pyfunction!(
        imageinline_check_end_image_marker,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        imageinline_extract_ascii_hex_decode,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        imageinline_extract_ascii85_decode,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        imageinline_extract_run_length_decode,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(imageinline_extract_dct_decode, module)?)?;
    module.add_function(wrap_pyfunction!(
        imageinline_is_preceded_by_whitespace,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        imageinline_is_followed_by_closing_operator,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(imageinline_extract_default, module)?)?;
    module.add_function(wrap_pyfunction!(
        imageinline_is_followed_by_binary_data,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        imageinline_read_settings_dictionary,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(imageinline_extract_not_set, module)?)?;
    module.add_function(wrap_pyfunction!(imageinline_handle_filters, module)?)?;
    module.add_function(wrap_pyfunction!(imageinline_read_inline_image, module)?)?;

    // pypdf.generic._image_xobject
    module.add_function(wrap_pyfunction!(
        imagexobject_get_process_colorant_channels,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(imagexobject_get_image_mode, module)?)?;
    module.add_function(wrap_pyfunction!(imagexobject_bits2byte, module)?)?;
    module.add_function(wrap_pyfunction!(
        imagexobject_expand_low_bit_samples,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        imagexobject_get_allocated_bytes_per_pixel,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(imagexobject_validate_image_size, module)?)?;
    module.add_function(wrap_pyfunction!(
        imagexobject_create_image_from_bytes,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        imagexobject_create_image_from_buffer,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(imagexobject_convert_image, module)?)?;
    module.add_function(wrap_pyfunction!(imagexobject_resize_image, module)?)?;
    module.add_function(wrap_pyfunction!(imagexobject_merge_image, module)?)?;
    module.add_function(wrap_pyfunction!(imagexobject_handle_flate_indexed, module)?)?;
    module.add_function(wrap_pyfunction!(imagexobject_handle_flate, module)?)?;
    module.add_function(wrap_pyfunction!(imagexobject_handle_jpx, module)?)?;
    module.add_function(wrap_pyfunction!(imagexobject_apply_decode, module)?)?;
    module.add_function(wrap_pyfunction!(
        imagexobject_get_mode_and_invert_color,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(imagexobject_apply_alpha, module)?)?;
    module.add_function(wrap_pyfunction!(imagexobject_xobj_to_image, module)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Symbols implemented in the hand-written Python shims
// (`python/pypdf/generic/_files.py`, `_image_inline.py`, `_image_xobject.py`).
//
// The class object, its attribute-assignment ``__init__`` and the pure attribute
// read of ``name`` are *definitions*, not logic: they must exist on the Python
// side so that ``isinstance``/subclassing/``repr`` behave exactly like upstream.
// Module-level type aliases and constant tables are Python data, not logic.
//
// @pypdf: pypdf.generic._files.EmbeddedFile
// @pypdf: pypdf.generic._files.EmbeddedFile.__init__
// @pypdf: pypdf.generic._files.EmbeddedFile.name
// @pypdf: pypdf.generic._image_inline.logger
// @pypdf: pypdf.generic._image_xobject.ImageMode
// @pypdf: pypdf.generic._image_xobject._SINGLE_BAND_BYTES_PER_PIXEL
