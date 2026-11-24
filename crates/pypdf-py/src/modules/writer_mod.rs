//! Symbol registry for gate A6 (constants / enum members / class attributes that
//! live in the `python/pypdf/_writer.py` shim as upstream-verbatim data or
//! declared class members; all behaviour is implemented in this file):
// @pypdf: pypdf._writer.ALL_DOCUMENT_PERMISSIONS
// @pypdf: pypdf._writer.ObjectDeletionFlag
// @pypdf: pypdf._writer.ObjectDeletionFlag.NONE
// @pypdf: pypdf._writer.ObjectDeletionFlag.TEXT
// @pypdf: pypdf._writer.ObjectDeletionFlag.LINKS
// @pypdf: pypdf._writer.ObjectDeletionFlag.ATTACHMENTS
// @pypdf: pypdf._writer.ObjectDeletionFlag.OBJECTS_3D
// @pypdf: pypdf._writer.ObjectDeletionFlag.ALL_ANNOTATIONS
// @pypdf: pypdf._writer.ObjectDeletionFlag.XOBJECT_IMAGES
// @pypdf: pypdf._writer.ObjectDeletionFlag.INLINE_IMAGES
// @pypdf: pypdf._writer.ObjectDeletionFlag.DRAWING_IMAGES
// @pypdf: pypdf._writer.ObjectDeletionFlag.IMAGES
// @pypdf: pypdf._writer._NAMED_DESTINATION_INDEX_MINIMUM
// @pypdf: pypdf._writer._NamedDestinationIndex
// @pypdf: pypdf._writer.PdfWriter
// @pypdf: pypdf._writer.PdfWriter.incremental
// @pypdf: pypdf._writer.PdfWriter._objects
// @pypdf: pypdf._writer.PdfWriter._id_translated
// @pypdf: pypdf._writer.PdfWriter._reader
// @pypdf: pypdf._writer.PdfWriter.FFBITS_NUL
// @pypdf: pypdf._writer.PdfWriter._UNSET
// @pypdf: pypdf._writer.PdfWriter._valid_layouts
// @pypdf: pypdf._writer.PdfWriter._valid_modes

//! Rust port of `pypdf/_writer.py` (upstream pypdf 7.1.1).
//!
//! # Design
//!
//! The Python shim (`python/pypdf/_writer.py`) keeps upstream's class surface
//! (`PdfWriter`, `ObjectDeletionFlag`, `_NamedDestinationIndex`) verbatim so
//! `isinstance`/subclassing/MRO/`sys.settrace` behaviour stays identical; every
//! method body delegates to a `#[pyfunction]` here.  All algorithms, parsing and
//! byte emission live in this file.
//!
//! Writers manipulate pypdf's *Python* object graph (see PORTING_GUIDE §1): the
//! Rust code calls the same Python classes/methods upstream calls, so cloning,
//! indirect-object translation and dictionary ordering are inherited from the
//! very same code paths.  Byte-identical output (gate A2) requires:
//!
//! * xref/trailer formatting copied character for character;
//! * `/ID` values from the interpreter's `secrets.token_bytes` (the oracle test
//!   harness monkeypatches it with the shared splitmix64 stream, so the port and
//!   the reference consume the identical stream -- dependency parity, exactly
//!   like the `zlib` requirement in FIDELITY_CONTRACT §5.3);
//! * deterministic iteration: Python `dict`/`list` order everywhere, never a
//!   Rust `HashMap`.

use std::collections::HashSet;

use pyo3::exceptions::{
    PyAttributeError, PyIndexError, PyKeyError, PyMemoryError, PyNotImplementedError, PyTypeError,
    PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::{
    PyAny, PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyModule, PySet, PyString, PyTuple,
};

use crate::errors;
use crate::pyutil;

/// Upstream module name; used as `logger_warning(..., source=__name__)`.
const SOURCE: &str = "pypdf._writer";

// ---------------------------------------------------------------------------
// Python-interop helpers
// ---------------------------------------------------------------------------

fn none<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    py.None().into_bound(py)
}

fn arg<'py>(args: &Bound<'py, PyTuple>, index: usize) -> PyResult<Bound<'py, PyAny>> {
    args.get_item(index)
}

fn get<'py>(obj: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    obj.getattr(name)
}

/// `getattr(obj, name, default)` (default only for missing attributes).
fn get_def<'py>(
    obj: &Bound<'py, PyAny>,
    name: &str,
    default: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    match obj.getattr(name) {
        Ok(value) => Ok(value),
        Err(error) if error.is_instance_of::<PyAttributeError>(obj.py()) => Ok(default.clone()),
        Err(error) => Err(error),
    }
}

fn has(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<bool> {
    obj.hasattr(name)
}

fn call0<'py>(obj: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    obj.call_method0(name)
}

fn callv<'py>(
    obj: &Bound<'py, PyAny>,
    name: &str,
    args: Vec<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let py = obj.py();
    obj.call_method1(name, PyTuple::new(py, args)?)
}

fn call1<'py>(
    obj: &Bound<'py, PyAny>,
    name: &str,
    value: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    obj.call_method1(name, (value,))
}

fn callkw<'py>(
    obj: &Bound<'py, PyAny>,
    name: &str,
    args: Vec<Bound<'py, PyAny>>,
    kwargs: Vec<(&str, Bound<'py, PyAny>)>,
) -> PyResult<Bound<'py, PyAny>> {
    let py = obj.py();
    let args = PyTuple::new(py, args)?;
    let kwargs_dict = PyDict::new(py);
    for (key, value) in kwargs {
        kwargs_dict.set_item(key, value)?;
    }
    obj.call_method(name, args, Some(&kwargs_dict))
}

fn items<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let mut out = Vec::new();
    for item in obj.try_iter()? {
        out.push(item?);
    }
    Ok(out)
}

/// `list(obj)` through Python.
fn py_list<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    py.import("builtins")?.getattr("list")?.call1((obj,))
}

fn py_len(obj: &Bound<'_, PyAny>) -> PyResult<usize> {
    obj.len()
}

fn eq(a: &Bound<'_, PyAny>, b: &Bound<'_, PyAny>) -> PyResult<bool> {
    a.eq(b)
}

fn lt(a: &Bound<'_, PyAny>, b: &Bound<'_, PyAny>) -> PyResult<bool> {
    a.lt(b)
}

fn is_none(obj: &Bound<'_, PyAny>) -> bool {
    obj.is_none()
}

fn truthy(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    obj.is_truthy()
}

fn generic<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("pypdf.generic")
}

fn gclass<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    generic(py)?.getattr(name)
}

fn new_g<'py>(
    py: Python<'py>,
    class: &str,
    args: Vec<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    gclass(py, class)?.call1(PyTuple::new(py, args)?)
}

/// `isinstance(obj, pypdf._page.PageObject)` (PageObject is not re-exported by `pypdf.generic`).
fn is_page_object(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    let class = py.import("pypdf._page")?.getattr("PageObject")?;
    obj.is_instance(&class)
}

/// `isinstance(obj, pypdf.generic.<class>)`.
fn is_inst(obj: &Bound<'_, PyAny>, class: &str) -> PyResult<bool> {
    let py = obj.py();
    obj.is_instance(&gclass(py, class)?)
}

/// `isinstance(obj, cls)` for an arbitrary Python class object.
fn is_instance_of(obj: &Bound<'_, PyAny>, class: &Bound<'_, PyAny>) -> PyResult<bool> {
    obj.is_instance(class)
}

fn py_str<'py>(py: Python<'py>, value: &str) -> Bound<'py, PyAny> {
    PyString::new(py, value).into_any()
}

fn py_bytes<'py>(py: Python<'py>, value: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, value).into_any()
}

fn py_int<'py>(py: Python<'py>, value: i64) -> Bound<'py, PyAny> {
    PyInt::new(py, value).into_any()
}

fn py_float<'py>(py: Python<'py>, value: f64) -> Bound<'py, PyAny> {
    PyFloat::new(py, value).into_any()
}

fn py_bool<'py>(py: Python<'py>, value: bool) -> Bound<'py, PyAny> {
    PyBool::new(py, value).to_owned().into_any()
}

fn to_usize(obj: &Bound<'_, PyAny>) -> PyResult<usize> {
    obj.extract::<usize>()
}

fn to_i64(obj: &Bound<'_, PyAny>) -> PyResult<i64> {
    obj.extract::<i64>()
}

fn to_f64(obj: &Bound<'_, PyAny>) -> PyResult<f64> {
    obj.extract::<f64>()
}

fn to_string(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    obj.str()?.extract::<String>()
}

fn to_bytes(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    obj.extract::<Vec<u8>>()
}

fn name_obj<'py>(py: Python<'py>, value: &str) -> PyResult<Bound<'py, PyAny>> {
    new_g(py, "NameObject", vec![py_str(py, value)])
}

fn text_obj<'py>(py: Python<'py>, value: &str) -> PyResult<Bound<'py, PyAny>> {
    new_g(py, "TextStringObject", vec![py_str(py, value)])
}

fn byte_string_obj<'py>(py: Python<'py>, value: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    new_g(py, "ByteStringObject", vec![py_bytes(py, value)])
}

fn number_obj<'py>(py: Python<'py>, value: i64) -> PyResult<Bound<'py, PyAny>> {
    new_g(py, "NumberObject", vec![py_int(py, value)])
}

fn boolean_obj<'py>(py: Python<'py>, value: bool) -> PyResult<Bound<'py, PyAny>> {
    new_g(
        py,
        "BooleanObject",
        vec![PyBool::new(py, value).to_owned().into_any()],
    )
}

fn null_obj<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    new_g(py, "NullObject", vec![])
}

fn array_obj<'py>(py: Python<'py>, values: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    let list = PyList::new(py, values)?;
    new_g(py, "ArrayObject", vec![list.into_any()])
}

fn dict_obj<'py>(
    py: Python<'py>,
    entries: Vec<(Bound<'py, PyAny>, Bound<'py, PyAny>)>,
) -> PyResult<Bound<'py, PyAny>> {
    let dict = PyDict::new(py);
    for (key, value) in entries {
        dict.set_item(key, value)?;
    }
    new_g(py, "DictionaryObject", vec![dict.into_any()])
}

/// Plain Python `dict` (upstream builds several of these by hand, e.g. the
/// incremental-xref ``init_data`` handed to ``StreamObject.initialize_from_dictionary``).
fn pydict_obj<'py>(
    py: Python<'py>,
    entries: Vec<(Bound<'py, PyAny>, Bound<'py, PyAny>)>,
) -> PyResult<Bound<'py, PyAny>> {
    let dict = PyDict::new(py);
    for (key, value) in entries {
        dict.set_item(key, value)?;
    }
    Ok(dict.into_any())
}

fn stream_obj<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    new_g(py, "StreamObject", vec![])
}

fn indirect_obj<'py>(
    py: Python<'py>,
    idnum: usize,
    generation: usize,
    pdf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    new_g(
        py,
        "IndirectObject",
        vec![
            py_int(py, idnum as i64),
            py_int(py, generation as i64),
            pdf.clone(),
        ],
    )
}

fn get_item<'py>(obj: &Bound<'py, PyAny>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    obj.get_item(key)
}

fn get_item_str<'py>(obj: &Bound<'py, PyAny>, key: &str) -> PyResult<Bound<'py, PyAny>> {
    obj.get_item(key)
}

fn set_item<'py>(
    obj: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    obj.set_item(key, value)
}

/// Write raw bytes to a Python stream (`stream.write(bytes)`).
fn write_bytes(stream: &Bound<'_, PyAny>, data: &[u8]) -> PyResult<()> {
    let py = stream.py();
    stream.call_method1("write", (PyBytes::new(py, data),))?;
    Ok(())
}

fn logger_warning(
    py: Python<'_>,
    message: &str,
    values: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    pyutil::logger_warning(py, SOURCE, message, values)
}

fn pdf_read_error(py: Python<'_>, message: &str) -> PyErr {
    errors::pdf_read_error(py, message)
}

fn value_error(_py: Python<'_>, message: &str) -> PyErr {
    PyValueError::new_err(message.to_string())
}

fn type_error(py: Python<'_>, message: &str) -> PyErr {
    let _ = py;
    PyTypeError::new_err(message.to_string())
}

fn index_error(py: Python<'_>, message: &str) -> PyErr {
    let _ = py;
    PyIndexError::new_err(message.to_string())
}

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("builtins")
}

fn builtin_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr(name)
}

/// `isinstance(obj, <builtin>)`.
fn is_inst_builtin(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<bool> {
    obj.is_instance(&builtin_class(obj.py(), name)?)
}

/// `id(obj)` as a Python int (used as a dict key, exactly like upstream).
fn py_id<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("id")?.call1((obj,))
}

/// Fetch a constant, e.g. `("PageAttributes", "CONTENTS")` from `pypdf.constants`.
fn const_str<'py>(py: Python<'py>, class: &str, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf.constants")?.getattr(class)?.getattr(name)
}

fn del_item<'py>(obj: &Bound<'py, PyAny>, key: &Bound<'py, PyAny>) -> PyResult<()> {
    obj.del_item(key)
}

/// `obj.get(key, default)` where `key` may be a str.
fn gget<'py>(
    obj: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    default: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    obj.call_method1("get", (key, default))
}

// ---------------------------------------------------------------------------
// Ported symbols
// ---------------------------------------------------------------------------

// ===========================================================================
// _NamedDestinationIndex
// ===========================================================================

// @pypdf: pypdf._writer._NamedDestinationIndex.__init__
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_nameddestinationindex_init<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let is_enabled = arg(args, 1)?;
    slf.setattr("_is_enabled", is_enabled)?;
    slf.setattr("_names", none(py))?;
    slf.setattr("_length", 0)?;
    slf.setattr("_strings", PySet::empty(py)?)?;
    slf.setattr("_is_sorted", false)?;
    slf.setattr("_pending", PyList::empty(py))?;
    Ok(none(py))
}

// @pypdf: pypdf._writer._NamedDestinationIndex._update
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_nameddestinationindex_private_update<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let names = arg(args, 1)?;
    let current = get(&slf, "_names")?;
    let length = get(&slf, "_length")?;
    if names.is(&current) && to_usize(&length)? == py_len(&names)? {
        return Ok(none(py));
    }
    call0(&slf, "flush")?;
    slf.setattr("_names", names.clone())?;
    let length = py_len(&names)?;
    slf.setattr("_length", length)?;
    let strings = PySet::empty(py)?;
    for entry in items(&names)? {
        if is_inst_builtin(&entry, "str")? {
            strings.add(entry)?;
        }
    }
    slf.setattr("_strings", strings)?;
    let step = py_int(py, 2);
    let slice = pyutil::py_slice(py, None, None, Some(&step))?;
    let keys = items(&names.get_item(&slice)?)?;
    let mut is_sorted = length % 2 == 0;
    if is_sorted {
        for key in &keys {
            if !is_inst_builtin(key, "str")? {
                is_sorted = false;
                break;
            }
        }
    }
    if is_sorted {
        for window in keys.windows(2) {
            if !window[0].le(&window[1])? {
                is_sorted = false;
                break;
            }
        }
    }
    slf.setattr("_is_sorted", is_sorted)?;
    Ok(none(py))
}

// @pypdf: pypdf._writer._NamedDestinationIndex.contains
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_nameddestinationindex_contains<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut container = arg(args, 1)?;
    let title = arg(args, 2)?;
    if is_inst(&container, "IndirectObject")? {
        container = call0(&container, "get_object")?;
    }
    if !truthy(&get(&slf, "_is_enabled")?)?
        || !is_inst_builtin(&container, "list")?
        || !is_inst_builtin(&title, "str")?
    {
        return Ok(py_bool(py, container.contains(&title)?));
    }
    call1(&slf, "_update", container.clone())?;
    let strings = get(&slf, "_strings")?;
    Ok(py_bool(py, strings.contains(&title)?))
}

// @pypdf: pypdf._writer._NamedDestinationIndex.insert
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_nameddestinationindex_insert<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let names = arg(args, 1)?;
    let title = arg(args, 2)?;
    let destination = arg(args, 3)?;
    if !truthy(&get(&slf, "_is_enabled")?)? {
        return Ok(py_bool(py, false));
    }
    call1(&slf, "_update", names.clone())?;
    if !truthy(&get(&slf, "_is_sorted")?)? || !is_inst_builtin(&title, "str")? {
        call0(&slf, "flush")?;
        return Ok(py_bool(py, false));
    }
    let pending = get(&slf, "_pending")?;
    call1(
        &pending,
        "append",
        PyTuple::new(py, [title.clone(), destination])?.into_any(),
    )?;
    let strings = get(&slf, "_strings")?;
    call1(&strings, "add", title)?;
    Ok(py_bool(py, true))
}

// @pypdf: pypdf._writer._NamedDestinationIndex.record_insertion
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_nameddestinationindex_record_insertion<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let names = arg(args, 1)?;
    let title = arg(args, 2)?;
    if names.is(&get(&slf, "_names")?) && is_inst_builtin(&title, "str")? {
        let strings = get(&slf, "_strings")?;
        call1(&strings, "add", title)?;
        let length = py_len(&names)?;
        slf.setattr("_length", length)?;
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer._NamedDestinationIndex.flush
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_nameddestinationindex_flush<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let pending = get(&slf, "_pending")?;
    if py_len(&pending)? == 0 {
        return Ok(none(py));
    }
    let names = get(&slf, "_names")?;
    if is_none(&names) {
        return Err(pyo3::exceptions::PyAssertionError::new_err("mypy"));
    }
    let mut pairs: Vec<Bound<'py, PyAny>> = Vec::new();
    let names_len = py_len(&names)?;
    let mut index = 0usize;
    while index < names_len {
        let pair = PyTuple::new(py, [names.get_item(index)?, names.get_item(index + 1)?])?;
        pairs.push(pair.into_any());
        index += 2;
    }
    for entry in items(&pending)? {
        let title = entry.get_item(0)?;
        let destination = entry.get_item(1)?;
        let text = text_obj(py, &to_string(&title)?)?;
        pairs.push(PyTuple::new(py, [text, destination])?.into_any());
    }
    let pylist = PyList::new(py, pairs)?;
    let key = py.import("operator")?.getattr("itemgetter")?.call1((0,))?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("key", key)?;
    pylist.call_method("sort", (), Some(&kwargs))?;
    let mut flattened: Vec<Bound<'py, PyAny>> = Vec::new();
    for pair in pylist.iter() {
        flattened.push(pair.get_item(0)?);
        flattened.push(pair.get_item(1)?);
    }
    let slice = pyutil::py_slice(py, None, None, None)?;
    names.set_item(&slice, PyList::new(py, flattened)?)?;
    slf.setattr("_pending", PyList::empty(py))?;
    let length = py_len(&names)?;
    slf.setattr("_length", length)?;
    Ok(none(py))
}

// ===========================================================================
// PdfWriter
// ===========================================================================

/// Upstream `PdfWriter.__init__._get_clone_from` (nested helper, no captured state).
fn writer_get_clone_from<'py>(
    py: Python<'py>,
    fileobj: &Bound<'py, PyAny>,
    clone_from: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let io_module = py.import("io")?;
    let is_str_or_path = is_inst_builtin(fileobj, "str")?
        || is_instance_of(fileobj, &py.import("pathlib")?.getattr("Path")?)?;
    let is_file_like = is_instance_of(fileobj, &io_module.getattr("IOBase")?)?
        || is_instance_of(fileobj, &io_module.getattr("BytesIO")?)?;
    let is_typing_io = match py.import("typing")?.getattr("IO") {
        Ok(class) => is_instance_of(fileobj, &class).unwrap_or(false),
        Err(_) => false,
    };
    let is_any = is_str_or_path || is_file_like || is_typing_io;
    if is_any && (eq(fileobj, &py_str(py, ""))? || !is_none(clone_from)) {
        return Ok(clone_from.clone());
    }
    let mut cloning = true;
    if is_str_or_path {
        let path = py.import("pathlib")?.getattr("Path")?.call1((fileobj,))?;
        if !truthy(&call0(&path, "exists")?)?
            || to_usize(&call0(&path, "stat")?.getattr("st_size")?)? == 0
        {
            cloning = false;
        }
    } else if is_file_like {
        let tell = call0(fileobj, "tell")?;
        let seek = fileobj.call_method1("seek", (py_int(py, 0), py_int(py, 2)))?;
        if to_i64(&seek)? == 0 {
            cloning = false;
        }
        fileobj.call_method1("seek", (tell, py_int(py, 0)))?;
        let _ = tell;
    }
    if cloning {
        return Ok(fileobj.clone());
    }
    Ok(clone_from.clone())
}

// @pypdf: pypdf._writer.PdfWriter.__init__
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_init<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut fileobj = arg(args, 1)?;
    let clone_from = arg(args, 2)?;
    let incremental = arg(args, 3)?;
    let full = arg(args, 4)?;
    let strict = arg(args, 5)?;
    let keep_initial_header = arg(args, 6)?;
    let count_limit = arg(args, 7)?;
    let id_limit = arg(args, 8)?;

    slf.setattr("strict", strict.clone())?;
    let incremental_flag = truthy(&incremental)? || truthy(&full)?;
    slf.setattr("incremental", incremental_flag)?;
    slf.setattr("_objects", PyList::empty(py))?;
    slf.setattr("_original_hash", PyList::empty(py))?;
    slf.setattr("_idnum_hash", PyDict::new(py))?;
    slf.setattr("_id_translated", PyDict::new(py))?;
    slf.setattr("_reader", none(py))?;
    slf.setattr("_ID", none(py))?;
    slf.setattr("_unresolved_links", PyList::empty(py))?;
    slf.setattr("_merged_in_pages", PyDict::new(py))?;
    let sys_maxsize = py.import("sys")?.getattr("maxsize")?;
    let count_limit = if is_inst_builtin(&count_limit, "int")? {
        count_limit.clone()
    } else {
        sys_maxsize.clone()
    };
    let id_limit = if is_inst_builtin(&id_limit, "int")? {
        id_limit.clone()
    } else {
        sys_maxsize.clone()
    };
    slf.setattr("_incremental_clone_object_count_limit", count_limit)?;
    slf.setattr("_incremental_clone_object_id_limit", id_limit)?;

    if incremental_flag {
        if is_inst_builtin(&fileobj, "str")?
            || is_instance_of(&fileobj, &py.import("pathlib")?.getattr("Path")?)?
        {
            let data = py
                .import("builtins")?
                .getattr("open")?
                .call1((fileobj.clone(), py_str(py, "rb")))?;
            let content = call1(&data, "read", py_int(py, -1))?;
            call0(&data, "close")?;
            fileobj = py.import("io")?.getattr("BytesIO")?.call1((content,))?;
        }
        if is_instance_of(&fileobj, &py.import("io")?.getattr("BytesIO")?)? {
            fileobj = py
                .import("pypdf._reader")?
                .getattr("PdfReader")?
                .call1((fileobj,))?;
        }
        if !is_instance_of(&fileobj, &py.import("pypdf._reader")?.getattr("PdfReader")?)? {
            return Err(errors::raise_msg(
                py,
                "PyPdfError",
                "Invalid type for incremental mode",
            ));
        }
        slf.setattr("_reader", fileobj.clone())?;
        let header = get(&fileobj, "pdf_header")?;
        slf.setattr("_header", call0(&header, "encode")?)?;
        slf.setattr("_readonly", true)?;
    } else {
        slf.setattr("_header", py_bytes(py, b"%PDF-1.3"))?;
        let producer_key = name_obj(py, "/Producer")?;
        let producer = py
            .import("pypdf.generic")?
            .getattr("create_string_object")?
            .call1(("pypdf",))?;
        let info = dict_obj(py, vec![(producer_key, producer)])?;
        let info_ref = call1(&slf, "_add_object", info)?;
        slf.setattr("_info_obj", info_ref)?;
    }

    let clone_from = writer_get_clone_from(py, &fileobj, &clone_from)?;
    slf.setattr("temp_fileobj", fileobj.clone())?;
    slf.setattr("fileobj", "")?;
    slf.setattr("_with_as_usage", false)?;
    slf.setattr("_cloned", false)?;
    let pages_key_type = const_str(py, "PagesAttributes", "TYPE")?;
    let pages_key_count = const_str(py, "PagesAttributes", "COUNT")?;
    let pages_key_kids = const_str(py, "PagesAttributes", "KIDS")?;
    let pages = dict_obj(
        py,
        vec![
            (
                name_obj(py, &to_string(&pages_key_type)?)?,
                name_obj(py, "/Pages")?,
            ),
            (
                name_obj(py, &to_string(&pages_key_count)?)?,
                number_obj(py, 0)?,
            ),
            (
                name_obj(py, &to_string(&pages_key_kids)?)?,
                array_obj(py, vec![])?,
            ),
        ],
    )?;
    slf.setattr("flattened_pages", PyList::empty(py))?;
    slf.setattr("_encryption", none(py))?;
    slf.setattr("_encrypt_entry", none(py))?;

    if !is_none(&clone_from) {
        let mut clone_from = clone_from;
        let reader_class = py.import("pypdf._reader")?.getattr("PdfReader")?;
        if !is_instance_of(&clone_from, &reader_class)? {
            clone_from = reader_class.call1((clone_from,))?;
        }
        call1(&slf, "clone_document_from_reader", clone_from.clone())?;
        slf.setattr("_cloned", true)?;
        if truthy(&keep_initial_header)? && !truthy(&get(&slf, "incremental")?)? {
            let header = get(&clone_from, "pdf_header")?;
            slf.setattr("_header", call0(&header, "encode")?)?;
        }
    } else {
        let pages_ref = call1(&slf, "_add_object", pages)?;
        slf.setattr("_pages", pages_ref.clone())?;
        let catalog_type = name_obj(py, "/Type")?;
        let catalog_core = const_str(py, "Core", "CATALOG")?;
        let catalog_pages = const_str(py, "Core", "PAGES")?;
        let root = dict_obj(
            py,
            vec![
                (catalog_type, name_obj(py, &to_string(&catalog_core)?)?),
                (name_obj(py, &to_string(&catalog_pages)?)?, pages_ref),
            ],
        )?;
        call1(&slf, "_add_object", root.clone())?;
        slf.setattr("_root_object", root)?;
    }
    if truthy(&full)? && !truthy(&incremental)? {
        slf.setattr("incremental", false)?;
    }
    let id = get(&slf, "_ID")?;
    if is_inst_builtin(&id, "list")? {
        for index in 0..2usize {
            let value = id.get_item(index)?;
            if is_inst(&value, "TextStringObject")? {
                let original = get(&value, "original_bytes")?;
                let replacement = byte_string_obj(py, &to_bytes(&original)?)?;
                id.set_item(index, replacement)?;
            }
        }
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.is_encrypted
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_is_encrypted<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let _ = args;
    Ok(py_bool(py, false))
}

// @pypdf: pypdf._writer.PdfWriter.root_object
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_root_object<'py>(
    _py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    get(&slf, "_root_object")
}

// @pypdf: pypdf._writer.PdfWriter._info
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_private_info<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let info_obj = get(&slf, "_info_obj")?;
    if is_none(&info_obj) {
        Ok(none(py))
    } else {
        call0(&info_obj, "get_object")
    }
}

// @pypdf: pypdf._writer.PdfWriter._info
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_property_set_private_info<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let value = arg(args, 1)?;
    if is_none(&value) {
        let attempted: PyResult<()> = (|| {
            let info_obj = get(&slf, "_info_obj")?;
            let reference = get(&info_obj, "indirect_reference")?;
            let idnum = to_usize(&get(&reference, "idnum")?)?;
            let objects = get(&slf, "_objects")?;
            objects.set_item(idnum - 1, py.None())?;
            Ok(())
        })();
        match attempted {
            Ok(()) => {}
            Err(error)
                if error.is_instance_of::<PyKeyError>(py)
                    || error.is_instance_of::<PyAttributeError>(py) => {}
            Err(error) => return Err(error),
        }
        slf.setattr("_info_obj", py.None())?;
    } else {
        let mut info_obj = get(&slf, "_info_obj")?;
        if is_none(&info_obj) {
            let new_info = new_g(py, "DictionaryObject", vec![])?;
            info_obj = call1(&slf, "_add_object", new_info)?;
            slf.setattr("_info_obj", info_obj.clone())?;
        }
        let obj = call0(&info_obj, "get_object")?;
        call0(&obj, "clear")?;
        call1(&obj, "update", call0(&value, "get_object")?)?;
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.xmp_metadata
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_xmp_metadata<'py>(
    _py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let root = get(&slf, "root_object")?;
    get(&root, "xmp_metadata")
}

// @pypdf: pypdf._writer.PdfWriter.xmp_metadata
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_property_set_xmp_metadata<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let value = arg(args, 1)?;
    let root = get(&slf, "root_object")?;
    let metadata_key = name_obj(py, "/Metadata")?;
    if is_none(&value) {
        if root.contains(&metadata_key)? {
            del_item(&root, &metadata_key)?;
        }
        return Ok(none(py));
    }
    let metadata = gget(&root, &metadata_key, &py.None().into_bound(py))?;
    let metadata_stream;
    if !is_inst(&metadata, "IndirectObject")? {
        if !is_none(&metadata) {
            del_item(&root, &metadata_key)?;
        }
        let new_stream = stream_obj(py)?;
        let stream_reference = call1(&slf, "_add_object", new_stream)?;
        set_item(&root, &metadata_key, &stream_reference)?;
        metadata_stream = call0(&stream_reference, "get_object")?;
    } else {
        metadata_stream = call0(&metadata, "get_object")?;
    }
    let xmp_class = py.import("pypdf.xmp")?.getattr("XmpInformation")?;
    let bytes_data = if is_instance_of(&value, &xmp_class)? {
        let stream = get(&value, "stream")?;
        call0(&stream, "get_data")?
    } else {
        value
    };
    call1(&metadata_stream, "set_data", bytes_data)?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.__enter__
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_enter<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let cloned = get(&slf, "_cloned")?;
    let temp_fileobj = get(&slf, "temp_fileobj")?;
    call0(&slf, "__init__")?;
    slf.setattr("_cloned", cloned)?;
    slf.setattr("_with_as_usage", true)?;
    slf.setattr("fileobj", temp_fileobj)?;
    let _ = py;
    Ok(slf)
}

// @pypdf: pypdf._writer.PdfWriter.__exit__
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_exit<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let fileobj = get(&slf, "fileobj")?;
    if truthy(&fileobj)? && !truthy(&get(&slf, "_cloned")?)? {
        call1(&slf, "write", fileobj)?;
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.pdf_header
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_pdf_header<'py>(
    _py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let header = get(&slf, "_header")?;
    call0(&header, "decode")
}

// @pypdf: pypdf._writer.PdfWriter.pdf_header
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_property_set_pdf_header<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut new_header = arg(args, 1)?;
    if is_inst_builtin(&new_header, "str")? {
        new_header = call0(&new_header, "encode")?;
    }
    slf.setattr("_header", new_header)?;
    let _ = py;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._add_object
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_add_object<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let obj = arg(args, 1)?;
    let default = none(py);
    let reference = get_def(&obj, "indirect_reference", &default)?;
    if !is_none(&reference) && eq(&get(&reference, "pdf")?, &slf)? {
        return Ok(reference);
    }
    // check for /Contents in Pages (/Contents in annotations are strings)
    if is_inst(&obj, "DictionaryObject")? {
        let contents_key = const_str(py, "PageAttributes", "CONTENTS")?;
        let contents = gget(&obj, &contents_key, &none(py))?;
        if is_inst(&contents, "ArrayObject")? || is_inst(&contents, "DictionaryObject")? {
            let replacement = call1(&slf, "_add_object", contents)?;
            let key = name_obj(py, &to_string(&contents_key)?)?;
            set_item(&obj, &key, &replacement)?;
        }
    }
    let objects = get(&slf, "_objects")?;
    call1(&objects, "append", obj.clone())?;
    let idnum = py_len(&objects)?;
    let reference = indirect_obj(py, idnum, 0, &slf)?;
    obj.setattr("indirect_reference", reference.clone())?;
    Ok(reference)
}

// @pypdf: pypdf._writer.PdfWriter.get_object
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_object<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let indirect_reference = arg(args, 1)?;
    let objects = get(&slf, "_objects")?;
    let obj;
    if is_inst_builtin(&indirect_reference, "int")? {
        let index = to_i64(&indirect_reference)? - 1;
        obj = objects.get_item(index)?;
    } else if !eq(&get(&indirect_reference, "pdf")?, &slf)? {
        return Err(value_error(py, "PDF must be self"));
    } else {
        let index = to_i64(&get(&indirect_reference, "idnum")?)? - 1;
        obj = objects.get_item(index)?;
    }
    if is_none(&obj) {
        let text = pyutil::py_repr(&indirect_reference)?;
        return Err(pdf_read_error(py, &format!("Object {text} not found!")));
    }
    Ok(obj)
}

// @pypdf: pypdf._writer.PdfWriter._replace_object
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_replace_object<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let indirect_reference = arg(args, 1)?;
    let mut obj = arg(args, 2)?;
    let idnum: usize;
    if is_inst(&indirect_reference, "IndirectObject")? {
        if !eq(&get(&indirect_reference, "pdf")?, &slf)? {
            return Err(value_error(py, "PDF must be self"));
        }
        idnum = to_usize(&get(&indirect_reference, "idnum")?)?;
    } else {
        idnum = to_usize(&indirect_reference)?;
    }
    let objects = get(&slf, "_objects")?;
    let current = objects.get_item(idnum - 1)?;
    let generation = to_usize(&get(&get(&current, "indirect_reference")?, "generation")?)?;
    let default = none(py);
    let existing = get_def(&obj, "indirect_reference", &default)?;
    if !is_none(&existing) && !eq(&get(&existing, "pdf")?, &slf)? {
        obj = call1(&obj, "clone", slf.clone())?;
    }
    objects.set_item(idnum - 1, obj.clone())?;
    obj.setattr(
        "indirect_reference",
        indirect_obj(py, idnum, generation, &slf)?,
    )?;
    Ok(obj)
}

// @pypdf: pypdf._writer.PdfWriter._add_page
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_add_page<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let page = arg(args, 1)?;
    let index = arg(args, 2)?;
    let excluded_keys_arg = arg(args, 3)?;
    let type_key = const_str(py, "PagesAttributes", "TYPE")?;
    let page_type = gget(&page, &type_key, &none(py))?;
    let core_page = const_str(py, "Core", "PAGE")?;
    if !is_page_object(py, &page)? || !eq(&page_type, &core_page)? {
        return Err(value_error(py, "Invalid page object"));
    }
    let page_org = page;
    let mut excluded_keys = items(&excluded_keys_arg)?;
    excluded_keys.push(const_str(py, "PagesAttributes", "PARENT")?);
    excluded_keys.push(py_str(py, "/StructParents"));
    // try: delete an already existing page
    let attempted: PyResult<()> = (|| {
        let reference = get(&page_org, "indirect_reference")?;
        let pdf = get(&reference, "pdf")?;
        let translated = get(&slf, "_id_translated")?;
        let container = get_item(&translated, &py_id(py, &pdf)?)?;
        let idnum = get(&reference, "idnum")?;
        container.del_item(idnum)?;
        Ok(())
    })();
    match attempted {
        Ok(()) => {}
        Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
        Err(_) => {}
    }
    let excluded_list = PyList::new(py, excluded_keys.clone())?;
    let cloned = callv(
        &page_org,
        "clone",
        vec![slf.clone(), py_bool(py, false), excluded_list.into_any()],
    )?;
    let page = call0(&cloned, "get_object")?;
    let org_pdf = get(&page_org, "pdf")?;
    if !is_none(&org_pdf) {
        let other = get(&org_pdf, "pdf_header")?;
        let own = get(&slf, "pdf_header")?;
        let merged = py
            .import("pypdf._utils")?
            .getattr("_get_max_pdf_version_header")?
            .call1((own, other))?;
        slf.setattr("pdf_header", merged)?;
    }
    let node_and_index = call1(&slf, "_get_page_in_node", index.clone())?;
    let node = node_and_index.get_item(0)?;
    let idx = node_and_index.get_item(1)?;
    let parent_key = name_obj(py, "/Parent")?;
    let node_reference = get(&node, "indirect_reference")?;
    set_item(&page, &parent_key, &node_reference)?;
    let kids_key = const_str(py, "PagesAttributes", "KIDS")?;
    let flattened = get(&slf, "flattened_pages")?;
    let page_reference = get(&page, "indirect_reference")?;
    if to_i64(&idx)? >= 0 {
        let kids = get_item(&node, &kids_key)?;
        callv(&kids, "insert", vec![idx.clone(), page_reference])?;
        callv(&flattened, "insert", vec![index.clone(), page.clone()])?;
    } else {
        let kids = get_item(&node, &kids_key)?;
        call1(&kids, "append", page_reference)?;
        call1(&flattened, "append", page.clone())?;
    }
    let count_key = name_obj(py, "/Count")?;
    let parent_lookup = const_str(py, "PagesAttributes", "PARENT")?;
    let is_null_or_none = generic(py)?.getattr("is_null_or_none")?;
    let mut current = node;
    let mut recurse = 0usize;
    while !truthy(&is_null_or_none.call1((current.clone(),))?)? {
        let node_dict = call0(&current, "get_object")?;
        let count = get_item(&node_dict, &count_key)?;
        let incremented = number_obj(py, to_i64(&count)? + 1)?;
        set_item(&node_dict, &count_key, &incremented)?;
        current = gget(&node_dict, &parent_lookup, &none(py))?;
        recurse += 1;
        if recurse > 1000 {
            return Err(errors::raise_msg(
                py,
                "PyPdfError",
                "Too many recursive calls!",
            ));
        }
    }
    if !is_none(&org_pdf) {
        let annots_key = py_str(py, "/Annots");
        let excluded_annots = excluded_keys
            .iter()
            .any(|key| key.eq(&annots_key).unwrap_or(false));
        if !excluded_annots {
            let links = generic(py)?
                .getattr("extract_links")?
                .call1((page.clone(), page_org.clone()))?;
            let unresolved = get(&slf, "_unresolved_links")?;
            call1(&unresolved, "extend", links)?;
        }
        let merged = get(&slf, "_merged_in_pages")?;
        merged.set_item(
            get(&page_org, "indirect_reference")?,
            get(&page, "indirect_reference")?,
        )?;
    }
    Ok(page)
}

// @pypdf: pypdf._writer.PdfWriter.set_need_appearances_writer
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_set_need_appearances_writer<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let state = arg(args, 1)?;
    let attempt: PyResult<()> = (|| {
        let root = get(&slf, "_root_object")?;
        let acro_key = const_str(py, "CatalogAttributes", "ACRO_FORM")?;
        if !root.contains(&acro_key)? {
            let new_form = new_g(py, "DictionaryObject", vec![])?;
            let reference = call1(&slf, "_add_object", new_form)?;
            let key = name_obj(py, &to_string(&acro_key)?)?;
            set_item(&root, &key, &reference)?;
        }
        let need_appearances = name_obj(
            py,
            &to_string(&const_str(
                py,
                "InteractiveFormDictEntries",
                "NeedAppearances",
            )?)?,
        )?;
        let form = get_item(&root, &acro_key)?;
        let value = boolean_obj(py, truthy(&state)?)?;
        set_item(&form, &need_appearances, &value)?;
        Ok(())
    })();
    match attempt {
        Ok(()) => {}
        Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
        Err(error) => {
            let values = PyDict::new(py);
            values.set_item("state", state)?;
            values.set_item("exc", error)?;
            logger_warning(
                py,
                "set_need_appearances_writer(%(state)s) catch : %(exc)s",
                Some(&values),
            )?;
        }
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.create_viewer_preferences
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_create_viewer_preferences<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let preferences = new_g(py, "ViewerPreferences", vec![])?;
    let root = get(&slf, "_root_object")?;
    let key = name_obj(
        py,
        &to_string(&const_str(py, "CatalogAttributes", "VIEWER_PREFERENCES")?)?,
    )?;
    let reference = call1(&slf, "_add_object", preferences.clone())?;
    set_item(&root, &key, &reference)?;
    Ok(preferences)
}

// @pypdf: pypdf._writer.PdfWriter.add_page
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_page<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let page = arg(args, 1)?;
    let excluded_keys = arg(args, 2)?;
    let flattened = get(&slf, "flattened_pages")?;
    if is_none(&flattened) {
        return Err(pyo3::exceptions::PyAssertionError::new_err("mypy"));
    }
    let index = py_len(&flattened)?;
    callv(
        &slf,
        "_add_page",
        vec![page, py_int(py, index as i64), excluded_keys],
    )
}

// @pypdf: pypdf._writer.PdfWriter.insert_page
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_insert_page<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let page = arg(args, 1)?;
    let mut index = to_i64(&arg(args, 2)?)?;
    let excluded_keys = arg(args, 3)?;
    let flattened = get(&slf, "flattened_pages")?;
    if is_none(&flattened) {
        return Err(pyo3::exceptions::PyAssertionError::new_err("mypy"));
    }
    let count = py_len(&flattened)? as i64;
    if index < 0 {
        index += count;
    }
    if index < 0 {
        return Err(value_error(py, "Invalid index value"));
    }
    if index >= count {
        return callv(&slf, "add_page", vec![page, excluded_keys]);
    }
    callv(
        &slf,
        "_add_page",
        vec![page, py_int(py, index), excluded_keys],
    )
}

// @pypdf: pypdf._writer.PdfWriter._get_page_number_by_indirect
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_get_page_number_by_indirect<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut indirect_reference = arg(args, 1)?;
    let generic_module = generic(py)?;
    let is_null_or_none = generic_module.getattr("is_null_or_none")?;
    if truthy(&is_null_or_none.call1((indirect_reference.clone(),))?)? {
        return Ok(none(py));
    }
    if is_inst_builtin(&indirect_reference, "int")? {
        indirect_reference = indirect_obj(py, to_usize(&indirect_reference)?, 0, &slf)?;
    }
    let obj = call0(&indirect_reference, "get_object")?;
    if is_page_object(py, &obj)? {
        return get(&obj, "page_number");
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.add_blank_page
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_blank_page<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let width = arg(args, 1)?;
    let height = arg(args, 2)?;
    let page_class = py.import("pypdf._page")?.getattr("PageObject")?;
    let page = page_class
        .getattr("create_blank_page")?
        .call1((slf.clone(), width, height))?;
    call1(&slf, "add_page", page)
}

// @pypdf: pypdf._writer.PdfWriter.insert_blank_page
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_insert_blank_page<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut width = arg(args, 1)?;
    let mut height = arg(args, 2)?;
    let index = arg(args, 3)?;
    let num_pages = to_i64(&call0(&slf, "get_num_pages")?)?;
    let index_value = to_i64(&index)?;
    if index_value.abs() > num_pages {
        return Err(index_error(
            py,
            &format!("Index should be in range [-{num_pages}, {num_pages}]"),
        ));
    }
    if num_pages != 0 {
        let fixed_index = index_value.min(num_pages - 1);
        let pages = get(&slf, "pages")?;
        let mediabox = get(&get_item(&pages, &py_int(py, fixed_index))?, "mediabox")?;
        if is_none(&width) || to_f64(&width)? <= 0.0 {
            width = get(&mediabox, "width")?;
        }
        if is_none(&height) || to_f64(&height)? <= 0.0 {
            height = get(&mediabox, "height")?;
        }
    }
    let page_class = py.import("pypdf._page")?.getattr("PageObject")?;
    let page = page_class
        .getattr("create_blank_page")?
        .call1((slf.clone(), width, height))?;
    callv(&slf, "insert_page", vec![page.clone(), index])?;
    Ok(page)
}

// @pypdf: pypdf._writer.PdfWriter.open_destination
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_open_destination<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    py.import("pypdf._doc_common")?
        .getattr("PdfDocCommon")?
        .getattr("open_destination")?
        .getattr("fget")?
        .call1((slf,))
}

// @pypdf: pypdf._writer.PdfWriter.open_destination
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_property_set_open_destination<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let dest = arg(args, 1)?;
    let root = get(&slf, "_root_object")?;
    let open_action = name_obj(py, "/OpenAction")?;
    if is_none(&dest) {
        if root.contains(&open_action)? {
            del_item(&root, &open_action)?;
        }
    } else if is_inst_builtin(&dest, "str")? {
        let value = text_obj(py, &to_string(&dest)?)?;
        set_item(&root, &open_action, &value)?;
    } else if is_inst(&dest, "Destination")? {
        let value = get(&dest, "dest_array")?;
        set_item(&root, &open_action, &value)?;
    } else if is_page_object(py, &dest)? {
        let reference = get(&dest, "indirect_reference")?;
        let reference = if is_none(&reference) {
            null_obj(py)?
        } else {
            reference
        };
        let destination = py.import("pypdf.generic")?.getattr("Destination")?.call1((
            "Opening",
            reference,
            py.import("pypdf.generic")?.getattr("PAGE_FIT")?,
        ))?;
        let value = get(&destination, "dest_array")?;
        set_item(&root, &open_action, &value)?;
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.add_js
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_js<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let _javascript = arg(args, 1)?;
    py.import("pypdf._utils")?
        .getattr("deprecation_with_replacement")?
        .call1(("add_js", "add_open_action", "7.0.0"))?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.add_open_action
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_open_action<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let action = arg(args, 1)?;
    py.import("pypdf.actions")?
        .getattr("Action")?
        .getattr("_create_open_action")?
        .call1((slf, action))?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.add_attachment
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_attachment<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let filename = arg(args, 1)?;
    let data = arg(args, 2)?;
    py.import("pypdf.generic")?
        .getattr("EmbeddedFile")?
        .getattr("_create_new")?
        .call1((slf, filename, data))
}

// @pypdf: pypdf._writer.PdfWriter.append_pages_from_reader
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_append_pages_from_reader<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let reader = arg(args, 1)?;
    let after_page_append = arg(args, 2)?;
    let pages = get(&reader, "pages")?;
    let num_pages = py_len(&pages)?;
    let callable_class = builtin_class(py, "callable")?;
    for number in 0..num_pages {
        let reader_page = get_item(&pages, &py_int(py, number as i64))?;
        let writer_page = call1(&slf, "add_page", reader_page)?;
        if truthy(&callable_class.call1((after_page_append.clone(),))?)? {
            after_page_append.call1((writer_page,))?;
        }
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._merge_content_stream_to_page
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_merge_content_stream_to_page<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let page = arg(args, 1)?;
    let new_content_data = arg(args, 2)?;
    let contents_key = name_obj(py, "/Contents")?;
    if page.contains(&contents_key)? {
        let existing_reference = get_item(&page, &contents_key)?;
        let existing_content = call0(&existing_reference, "get_object")?;
        if is_inst(&existing_content, "ArrayObject")? {
            let new_stream = stream_obj(py)?;
            call1(&new_stream, "set_data", new_content_data.clone())?;
            let reference = call1(&slf, "_add_object", new_stream)?;
            call1(&existing_content, "append", reference)?;
            let replacement = call1(&slf, "_add_object", existing_content.clone())?;
            set_item(&page, &contents_key, &replacement)?;
        }
        if is_inst(&existing_content, "StreamObject")? {
            let data = call0(&existing_content, "get_data")?;
            let mut merged = to_bytes(&data)?;
            merged.push(b'\n');
            merged.extend(to_bytes(&new_content_data)?);
            let new_stream = stream_obj(py)?;
            call1(&new_stream, "set_data", py_bytes(py, &merged))?;
            let reference = call1(&slf, "_add_object", new_stream)?;
            set_item(&page, &contents_key, &reference)?;
        }
    } else {
        let new_stream = stream_obj(py)?;
        call1(&new_stream, "set_data", new_content_data)?;
        let reference = call1(&slf, "_add_object", new_stream)?;
        set_item(&page, &contents_key, &reference)?;
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._add_apstream_object
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_add_apstream_object<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let page = arg(args, 1)?;
    let appearance_stream_obj = arg(args, 2)?;
    let object_name = arg(args, 3)?;
    let x_offset = arg(args, 4)?;
    let y_offset = arg(args, 5)?;
    let resources_key = const_str(py, "PageAttributes", "RESOURCES")?;
    let page_resources = get_item(&page, &resources_key)?;
    let xobject_ref = call1(&slf, "_add_object", appearance_stream_obj)?;
    let raw_name = name_obj(py, &format!("/Fm_{}", to_string(&object_name)?))?;
    let xobject_name = call0(&raw_name, "_sanitize")?;
    let xobject_key = name_obj(py, "/XObject")?;
    if !page_resources.contains(&xobject_key)? {
        let new_dict = new_g(py, "DictionaryObject", vec![])?;
        set_item(&page_resources, &xobject_key, &new_dict)?;
    }
    let page_xobjects = get_item(&page_resources, &xobject_key)?;
    if !page_xobjects.contains(&xobject_name)? {
        set_item(&page_xobjects, &xobject_name, &xobject_ref)?;
    } else {
        let values = PyDict::new(py);
        values.set_item("xobject_name", xobject_name.clone())?;
        logger_warning(
            py,
            "XObject %(xobject_name)r already added to page resources. This might be an issue.",
            Some(&values),
        )?;
    }
    let transformation = py
        .import("pypdf._page")?
        .getattr("Transformation")?
        .call0()?;
    let matrix = callv(&transformation, "translate", vec![x_offset, y_offset])?;
    let cm = call0(&matrix, "_to_cm")?;
    let commands = format!(
        "q\n{}\n{} Do\nQ",
        to_string(&cm)?,
        to_string(&xobject_name)?
    );
    callv(
        &slf,
        "_merge_content_stream_to_page",
        vec![page, py_bytes(py, commands.as_bytes())],
    )?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.generate_file_identifiers
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_generate_file_identifiers<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let id = get(&slf, "_ID")?;
    let token_bytes = py.import("secrets")?.getattr("token_bytes")?;
    let (id1, id2) = if truthy(&id)? {
        let first = get_item(&id, &py_int(py, 0))?;
        let second = byte_string_obj(py, &to_bytes(&token_bytes.call1((16,))?)?)?;
        (first, second)
    } else {
        let first = byte_string_obj(py, &to_bytes(&token_bytes.call1((16,))?)?)?;
        (first.clone(), first)
    };
    let array = array_obj(py, vec![id1, id2])?;
    slf.setattr("_ID", array)?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.encrypt
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_encrypt<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let user_password = arg(args, 1)?;
    let mut owner_password = arg(args, 2)?;
    let use_128bit = arg(args, 3)?;
    let permissions_flag = arg(args, 4)?;
    let algorithm = arg(args, 5)?;
    if truthy(&get(&slf, "incremental")?)? {
        return Err(PyNotImplementedError::new_err(
            "Encrypting incremental PDF files is currently not supported.",
        ));
    }
    if is_none(&owner_password) {
        owner_password = user_password.clone();
    }
    let encryption_module = py.import("pypdf._encryption")?;
    let algorithm_enum = encryption_module.getattr("EncryptAlgorithm")?;
    let alg = if !is_none(&algorithm) {
        let name = to_string(&algorithm)?.replace('-', "_");
        match algorithm_enum.getattr(name.as_str()) {
            Ok(value) => value,
            Err(error) if error.is_instance_of::<PyAttributeError>(py) => {
                return Err(value_error(
                    py,
                    &format!("Algorithm '{}' NOT supported", to_string(&algorithm)?),
                ));
            }
            Err(error) => return Err(error),
        }
    } else if truthy(&use_128bit)? {
        algorithm_enum.getattr("RC4_128")?
    } else {
        algorithm_enum.getattr("RC4_40")?
    };
    call0(&slf, "generate_file_identifiers")?;
    let id = get(&slf, "_ID")?;
    let id0 = get_item(&id, &py_int(py, 0))?;
    let encryption = encryption_module
        .getattr("Encryption")?
        .getattr("make")?
        .call1((alg, permissions_flag, id0))?;
    let entry = callkw(
        &encryption,
        "write_entry",
        vec![user_password, owner_password],
        vec![("strict", get(&slf, "strict")?)],
    )?;
    slf.setattr("_encryption", encryption)?;
    let existing = get(&slf, "_encrypt_entry")?;
    if truthy(&existing)? {
        let reference = get(&existing, "indirect_reference")?;
        entry.setattr("indirect_reference", reference.clone())?;
        let idnum = to_usize(&get(&reference, "idnum")?)?;
        let objects = get(&slf, "_objects")?;
        objects.set_item(idnum - 1, entry.clone())?;
    } else {
        call1(&slf, "_add_object", entry.clone())?;
    }
    slf.setattr("_encrypt_entry", entry)?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._resolve_links
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_resolve_links<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut known_names: Option<Bound<'py, PyAny>> = None;
    let unresolved = get(&slf, "_unresolved_links")?;
    let merged = get(&slf, "_merged_in_pages")?;
    let named_reference_link = generic(py)?.getattr("NamedReferenceLink")?;
    for pair in items(&unresolved)? {
        let new_link = pair.get_item(0)?;
        let old_link = pair.get_item(1)?;
        let old_page = call0(&old_link, "find_referenced_page")?;
        if !truthy(&old_page)? {
            continue;
        }
        let new_page = gget(&merged, &old_page, &none(py))?;
        if is_none(&new_page) {
            continue;
        }
        if is_instance_of(&new_link, &named_reference_link)? {
            if known_names.is_none() {
                let destinations = get(&slf, "named_destinations")?;
                known_names = Some(builtin_class(py, "set")?.call1((destinations,))?);
            }
            callkw(
                &new_link,
                "patch_reference",
                vec![slf.clone(), new_page],
                vec![("known_names", known_names.clone().unwrap())],
            )?;
        } else {
            callv(&new_link, "patch_reference", vec![slf.clone(), new_page])?;
        }
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.write_stream
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_write_stream<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let stream = arg(args, 1)?;
    if has(&stream, "mode")? && !stream.getattr("mode")?.contains(py_str(py, "b"))? {
        let values = PyDict::new(py);
        values.set_item("stream_name", get(&stream, "name")?)?;
        logger_warning(
            py,
            "File <%(stream_name)s> to write to is not in binary mode. It may not be written to correctly.",
            Some(&values),
        )?;
    }
    call0(&slf, "_resolve_links")?;
    if truthy(&get(&slf, "incremental")?)? {
        let reader = get(&slf, "_reader")?;
        if is_none(&reader) {
            return Err(pyo3::exceptions::PyAssertionError::new_err("mypy"));
        }
        let increment = call0(&slf, "list_objects_in_increment")?;
        let has_increment = py_len(&increment)? > 0;
        if has_increment && truthy(&get(&reader, "is_encrypted")?)? {
            return Err(PyNotImplementedError::new_err(
                "Writing incremental updates of encrypted PDF files is currently not supported.",
            ));
        }
        let reader_stream = get(&reader, "stream")?;
        callv(&reader_stream, "seek", vec![py_int(py, 0)])?;
        let content = callv(&reader_stream, "read", vec![py_int(py, -1)])?;
        call1(&stream, "write", content)?;
        if has_increment {
            call1(&slf, "_write_increment", stream.clone())?;
        }
    } else {
        let structure = call1(&slf, "_write_pdf_structure", stream.clone())?;
        let object_positions = structure.get_item(0)?;
        let free_objects = structure.get_item(1)?;
        let xref_location = callv(
            &slf,
            "_write_xref_table",
            vec![stream.clone(), object_positions, free_objects],
        )?;
        callv(&slf, "_write_trailer", vec![stream, xref_location])?;
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.write
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_write<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut stream = arg(args, 1)?;
    let mut my_file = false;
    if eq(&stream, &py_str(py, ""))? {
        let text = pyutil::py_repr(&stream)?;
        return Err(value_error(py, &format!("Output(stream={text}) is empty.")));
    }
    if is_inst_builtin(&stream, "str")?
        || is_instance_of(&stream, &py.import("pathlib")?.getattr("Path")?)?
    {
        stream = py
            .import("io")?
            .getattr("FileIO")?
            .call1((stream, py_str(py, "wb")))?;
        my_file = true;
    }
    call1(&slf, "write_stream", stream.clone())?;
    if my_file {
        call0(&stream, "close")?;
    } else {
        call0(&stream, "flush")?;
    }
    Ok(PyTuple::new(py, [py_bool(py, my_file), stream])?.into_any())
}

// @pypdf: pypdf._writer.PdfWriter.list_objects_in_increment
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_list_objects_in_increment<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let original_hash = get(&slf, "_original_hash")?;
    let original_hash_count = py_len(&original_hash)?;
    let objects = get(&slf, "_objects")?;
    let mut result: Vec<Bound<'py, PyAny>> = Vec::new();
    for (index, obj) in items(&objects)?.into_iter().enumerate() {
        if is_none(&obj) {
            continue;
        }
        let modified = if index >= original_hash_count {
            true
        } else {
            let hash = call0(&obj, "hash_bin")?;
            !eq(&hash, &get_item(&original_hash, &py_int(py, index as i64))?)?
        };
        if modified {
            result.push(get(&obj, "indirect_reference")?);
        }
    }
    Ok(array_obj(py, result)?)
}

// @pypdf: pypdf._writer.PdfWriter._write_increment
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_write_increment<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let stream = arg(args, 1)?;
    let reader = get(&slf, "_reader")?;
    if is_none(&reader) {
        return Err(pyo3::exceptions::PyAssertionError::new_err("mypy"));
    }
    if truthy(&get(&reader, "is_encrypted")?)? {
        return Err(PyNotImplementedError::new_err(
            "Writing incremental updates of encrypted PDF files is currently not supported.",
        ));
    }
    let objects = get(&slf, "_objects")?;
    let original_hash = get(&slf, "_original_hash")?;
    let original_hash_count = py_len(&original_hash)?;
    let mut object_positions: Vec<(usize, i64)> = Vec::new();
    let mut object_blocks: Vec<[i64; 2]> = Vec::new();
    let mut current_start: i64 = -1;
    let mut current_stop: i64 = -2;
    for (index, obj) in items(&objects)?.into_iter().enumerate() {
        if is_none(&obj) {
            continue;
        }
        let modified = if index >= original_hash_count {
            true
        } else {
            let hash = call0(&obj, "hash_bin")?;
            !eq(&hash, &get_item(&original_hash, &py_int(py, index as i64))?)?
        };
        if !modified {
            continue;
        }
        let idnum = index + 1;
        let position = to_i64(&call0(&stream, "tell")?)?;
        object_positions.push((idnum, position));
        write_bytes(&stream, format!("{idnum} 0 obj\n").as_bytes())?;
        call1(&obj, "write_to_stream", stream.clone())?;
        write_bytes(&stream, b"\nendobj\n")?;
        if idnum as i64 != current_stop {
            if current_start > 0 {
                object_blocks.push([current_start, current_stop - current_start]);
            }
            current_start = idnum as i64;
        }
        current_stop = idnum as i64 + 1;
    }
    if current_start <= 0 {
        return Err(pyo3::exceptions::PyAssertionError::new_err(
            "for pytest only",
        ));
    }
    object_blocks.push([current_start, current_stop - current_start]);
    // write incremented xref
    let xref_location = call0(&stream, "tell")?;
    let xr_id = py_len(&objects)? + 1;
    write_bytes(&stream, format!("{xr_id} 0 obj").as_bytes())?;
    let mut init_data: Vec<(Bound<'py, PyAny>, Bound<'py, PyAny>)> = Vec::new();
    init_data.push((name_obj(py, "/Type")?, name_obj(py, "/XRef")?));
    init_data.push((name_obj(py, "/Size")?, number_obj(py, xr_id as i64 + 1)?));
    let root = get(&slf, "root_object")?;
    init_data.push((name_obj(py, "/Root")?, get(&root, "indirect_reference")?));
    init_data.push((name_obj(py, "/Filter")?, name_obj(py, "/FlateDecode")?));
    let mut index_values: Vec<Bound<'py, PyAny>> = Vec::new();
    for block in &object_blocks {
        index_values.push(number_obj(py, block[0])?);
        index_values.push(number_obj(py, block[1])?);
    }
    init_data.push((name_obj(py, "/Index")?, array_obj(py, index_values)?));
    init_data.push((
        name_obj(py, "/W")?,
        array_obj(
            py,
            vec![number_obj(py, 1)?, number_obj(py, 4)?, number_obj(py, 1)?],
        )?,
    ));
    init_data.push((py_str(py, "__streamdata__"), py_bytes(py, b"")));
    let info = get(&slf, "_info")?;
    if !is_none(&info) {
        let info_reference = get(&info, "indirect_reference")?;
        let idnum = to_usize(&get(&info_reference, "idnum")?)?;
        let info_hash = call0(&info, "hash_bin")?;
        if idnum - 1 >= original_hash_count
            || !eq(
                &info_hash,
                &get_item(&original_hash, &py_int(py, (idnum - 1) as i64))?,
            )?
        {
            init_data.push((
                name_obj(py, &to_string(&const_str(py, "TrailerKeys", "INFO")?)?)?,
                info_reference,
            ));
        }
    }
    let prev = get(&reader, "_startxref")?;
    init_data.push((
        name_obj(py, &to_string(&const_str(py, "TrailerKeys", "PREV")?)?)?,
        number_obj(py, to_i64(&prev)?)?,
    ));
    let id = get(&slf, "_ID")?;
    if truthy(&id)? {
        init_data.push((
            name_obj(py, &to_string(&const_str(py, "TrailerKeys", "ID")?)?)?,
            id,
        ));
    }
    let dictionary = pydict_obj(py, init_data)?;
    let xr = gclass(py, "StreamObject")?
        .getattr("initialize_from_dictionary")?
        .call1((dictionary,))?;
    let mut stream_data: Vec<u8> = Vec::new();
    for (_, position) in &object_positions {
        stream_data.push(1u8);
        stream_data.extend_from_slice(&(*position as u32).to_be_bytes());
        stream_data.push(0u8);
    }
    call1(&xr, "set_data", py_bytes(py, &stream_data))?;
    call1(&xr, "write_to_stream", stream.clone())?;
    let xref_text = format!("\nendobj\nstartxref\n{}\n%%EOF\n", to_i64(&xref_location)?);
    write_bytes(&stream, xref_text.as_bytes())?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._write_pdf_structure
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_write_pdf_structure<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let stream = arg(args, 1)?;
    let mut object_positions: Vec<i64> = Vec::new();
    let mut free_objects: Vec<i64> = Vec::new();
    let header = get(&slf, "pdf_header")?;
    let mut header_bytes = to_bytes(&call0(&header, "encode")?)?;
    header_bytes.extend_from_slice(b"\n");
    write_bytes(&stream, &header_bytes)?;
    write_bytes(&stream, b"%\xE2\xE3\xCF\xD3\n")?;
    let objects = get(&slf, "_objects")?;
    let encryption = get(&slf, "_encryption")?;
    let encrypt_entry = get(&slf, "_encrypt_entry")?;
    for (idnum, obj) in items(&objects)?.into_iter().enumerate() {
        let idnum = idnum + 1;
        if is_none(&obj) {
            object_positions.push(-1);
            free_objects.push(idnum as i64);
            continue;
        }
        object_positions.push(to_i64(&call0(&stream, "tell")?)?);
        write_bytes(&stream, format!("{idnum} 0 obj\n").as_bytes())?;
        let mut obj = obj;
        if truthy(&encryption)? && !eq(&obj, &encrypt_entry)? {
            obj = callv(
                &encryption,
                "encrypt_object",
                vec![obj, py_int(py, idnum as i64), py_int(py, 0)],
            )?;
        }
        call1(&obj, "write_to_stream", stream.clone())?;
        write_bytes(&stream, b"\nendobj\n")?;
    }
    free_objects.push(0);
    Ok(PyTuple::new(
        py,
        [
            array_obj(
                py,
                object_positions
                    .iter()
                    .map(|value| py_int(py, *value))
                    .collect(),
            )?,
            array_obj(
                py,
                free_objects
                    .iter()
                    .map(|value| py_int(py, *value))
                    .collect(),
            )?,
        ],
    )?
    .into_any())
}

// @pypdf: pypdf._writer.PdfWriter._write_xref_table
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_write_xref_table<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let stream = arg(args, 1)?;
    let object_positions = items(&arg(args, 2)?)?;
    let free_objects = items(&arg(args, 3)?)?;
    let xref_location = call0(&stream, "tell")?;
    write_bytes(&stream, b"xref\n")?;
    let objects = get(&slf, "_objects")?;
    let count = py_len(&objects)? + 1;
    write_bytes(&stream, format!("0 {count}\n").as_bytes())?;
    let first = to_i64(&free_objects[0])?;
    write_bytes(
        &stream,
        format!("{first:0>10} {:0>5} f \n", 65535).as_bytes(),
    )?;
    let mut free_idx = 1usize;
    for offset in object_positions {
        let offset = to_i64(&offset)?;
        if offset > 0 {
            write_bytes(&stream, format!("{offset:0>10} {:0>5} n \n", 0).as_bytes())?;
        } else {
            let value = to_i64(&free_objects[free_idx])?;
            write_bytes(&stream, format!("{value:0>10} {:0>5} f \n", 1).as_bytes())?;
            free_idx += 1;
        }
    }
    Ok(xref_location)
}

// @pypdf: pypdf._writer.PdfWriter._write_trailer
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_write_trailer<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let stream = arg(args, 1)?;
    let xref_location = arg(args, 2)?;
    write_bytes(&stream, b"trailer\n")?;
    let objects = get(&slf, "_objects")?;
    let size = py_len(&objects)? + 1;
    let root = get(&slf, "root_object")?;
    let trailer = dict_obj(
        py,
        vec![
            (
                name_obj(py, &to_string(&const_str(py, "TrailerKeys", "SIZE")?)?)?,
                number_obj(py, size as i64)?,
            ),
            (
                name_obj(py, &to_string(&const_str(py, "TrailerKeys", "ROOT")?)?)?,
                get(&root, "indirect_reference")?,
            ),
        ],
    )?;
    let info = get(&slf, "_info")?;
    if !is_none(&info) {
        set_item(
            &trailer,
            &name_obj(py, &to_string(&const_str(py, "TrailerKeys", "INFO")?)?)?,
            &get(&info, "indirect_reference")?,
        )?;
    }
    let id = get(&slf, "_ID")?;
    if !is_none(&id) {
        set_item(
            &trailer,
            &name_obj(py, &to_string(&const_str(py, "TrailerKeys", "ID")?)?)?,
            &id,
        )?;
    }
    let encrypt_entry = get(&slf, "_encrypt_entry")?;
    if truthy(&encrypt_entry)? {
        set_item(
            &trailer,
            &name_obj(py, &to_string(&const_str(py, "TrailerKeys", "ENCRYPT")?)?)?,
            &get(&encrypt_entry, "indirect_reference")?,
        )?;
    }
    call1(&trailer, "write_to_stream", stream.clone())?;
    let text = format!("\nstartxref\n{}\n%%EOF\n", to_i64(&xref_location)?);
    write_bytes(&stream, text.as_bytes())?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.metadata
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_metadata<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    py.import("pypdf._doc_common")?
        .getattr("PdfDocCommon")?
        .getattr("metadata")?
        .getattr("fget")?
        .call1((slf,))
}

// @pypdf: pypdf._writer.PdfWriter.metadata
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_property_set_metadata<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let value = arg(args, 1)?;
    if is_none(&value) {
        slf.setattr("_info", py.None())?;
    } else {
        let info = get(&slf, "_info")?;
        if !is_none(&info) {
            call0(&info, "clear")?;
        }
        call1(&slf, "add_metadata", value)?;
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.add_metadata
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_metadata<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut infos = arg(args, 1)?;
    if crate::base_mod::is_pdf_object_like(py, &infos)? {
        infos = call0(&infos, "get_object")?;
    }
    let args_dict = PyDict::new(py);
    let items_list = items(&call0(&infos, "items")?)?;
    for pair in items_list {
        let key = pair.get_item(0)?;
        let mut value = pair.get_item(1)?;
        if crate::base_mod::is_pdf_object_like(py, &value)? {
            value = call0(&value, "get_object")?;
        }
        let text = py
            .import("pypdf.generic")?
            .getattr("create_string_object")?
            .call1((to_string(&value)?,))?;
        args_dict.set_item(name_obj(py, &to_string(&key)?)?, text)?;
    }
    if is_none(&get(&slf, "_info")?) {
        let empty = new_g(py, "DictionaryObject", vec![])?;
        slf.setattr("_info", empty)?;
    }
    let info = get(&slf, "_info")?;
    call1(&info, "update", args_dict.into_any())?;
    Ok(none(py))
}

/// Upstream `PdfWriter.compress_identical_objects.replace_in_obj` (nested helper).
fn writer_replace_in_obj<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    crossref: &Bound<'py, PyAny>,
    unreferenced: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let pairs: Vec<(Bound<'py, PyAny>, Bound<'py, PyAny>)>;
    if is_inst(obj, "DictionaryObject")? {
        let mut collected = Vec::new();
        for pair in items(&call0(obj, "items")?)? {
            collected.push((pair.get_item(0)?, pair.get_item(1)?));
        }
        pairs = collected;
    } else if is_inst(obj, "ArrayObject")? {
        let mut collected = Vec::new();
        for (index, value) in items(obj)?.into_iter().enumerate() {
            collected.push((py_int(py, index as i64), value));
        }
        pairs = collected;
    } else {
        return Ok(());
    }
    for (key, value) in pairs {
        if is_inst(&value, "IndirectObject")? {
            let idnum = to_usize(&get(&value, "idnum")?)?;
            unreferenced.set_item(idnum - 1, PyBool::new(py, false).to_owned())?;
            if crossref.contains(&value)? {
                let replacement = get_item(crossref, &value)?;
                set_item(obj, &key, &replacement)?;
            }
        } else {
            writer_replace_in_obj(py, &value, crossref, unreferenced)?;
        }
    }
    Ok(())
}

// @pypdf: pypdf._writer.PdfWriter.compress_identical_objects
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_compress_identical_objects<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let remove_identicals = arg(args, 1)?;
    let remove_orphans = arg(args, 2)?;
    let remove_duplicates = arg(args, 3)?;
    let remove_unreferenced = arg(args, 4)?;
    let unset = get(&slf, "_UNSET")?;
    let utils = py.import("pypdf._utils")?;
    if !eq(&remove_identicals, &unset)? {
        utils.getattr("deprecation_with_replacement")?.call1((
            "remove_identicals",
            "remove_duplicates",
            "7.0.0",
        ))?;
    }
    if !eq(&remove_orphans, &unset)? {
        utils.getattr("deprecation_with_replacement")?.call1((
            "remove_orphans",
            "remove_unreferenced",
            "7.0.0",
        ))?;
    }
    let hash_map = PyDict::new(py);
    let objects = get(&slf, "_objects")?;
    let count = py_len(&objects)?;
    let unreferenced = PyList::new(py, vec![PyBool::new(py, true).to_owned(); count])?;
    let is_null_or_none = generic(py)?.getattr("is_null_or_none")?;
    for index in 0..count {
        let obj = objects.get_item(index)?;
        if truthy(&is_null_or_none.call1((obj.clone(),))?)? {
            continue;
        }
        let reference = get(&obj, "indirect_reference")?;
        let hash = call0(&obj, "hash_value")?;
        if truthy(&remove_duplicates)? && hash_map.contains(&hash)? {
            let entry = get_item(hash_map.as_any(), &hash)?;
            let duplicates = entry.get_item(1)?;
            call1(&duplicates, "append", reference)?;
            objects.set_item(index, py.None())?;
        } else {
            hash_map.set_item(
                &hash,
                PyTuple::new(py, [reference, PyList::empty(py).into_any()])?,
            )?;
        }
    }
    slf.setattr("_idnum_hash", &hash_map)?;
    // generate the dict converting others to 1st
    let crossref = PyDict::new(py);
    let crossref_rev = PyDict::new(py);
    for entry in items(&hash_map.call_method0("values")?)? {
        let first = entry.get_item(0)?;
        let others = entry.get_item(1)?;
        if py_len(&others)? == 0 {
            continue;
        }
        crossref.set_item(&first, &others)?;
        for other in items(&others)? {
            crossref_rev.set_item(&other, &first)?;
        }
    }
    // replace reference to merged objects
    for obj in items(&objects)? {
        if is_inst(&obj, "DictionaryObject")? || is_inst(&obj, "ArrayObject")? {
            writer_replace_in_obj(py, &obj, crossref_rev.as_any(), &unreferenced)?;
        }
    }
    if truthy(&remove_unreferenced)? {
        let root = get(&slf, "root_object")?;
        let idnum = to_usize(&get(&get(&root, "indirect_reference")?, "idnum")?)?;
        unreferenced.set_item(idnum - 1, PyBool::new(py, false).to_owned())?;
        let info = get(&slf, "_info")?;
        if !truthy(&is_null_or_none.call1((info.clone(),))?)? {
            let idnum = to_usize(&get(&get(&info, "indirect_reference")?, "idnum")?)?;
            unreferenced.set_item(idnum - 1, PyBool::new(py, false).to_owned())?;
        }
        let attempted: PyResult<()> = (|| {
            let id = get(&slf, "_ID")?;
            let idnum = to_usize(&get(&get(&id, "indirect_reference")?, "idnum")?)?;
            unreferenced.set_item(idnum - 1, PyBool::new(py, false).to_owned())?;
            Ok(())
        })();
        match attempted {
            Ok(()) => {}
            Err(error) if error.is_instance_of::<PyAttributeError>(py) => {}
            Err(error) => return Err(error),
        }
        let range = builtin_class(py, "range")?.call1((py_int(py, count as i64),))?;
        let compressed = py
            .import("itertools")?
            .getattr("compress")?
            .call1((range, unreferenced))?;
        for index in items(&compressed)? {
            objects.set_item(to_usize(&index)?, py.None())?;
        }
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._create_stream
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_create_stream<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let fileobj = arg(args, 1)?;
    let io_module = py.import("io")?;
    if is_inst_builtin(&fileobj, "str")?
        || is_instance_of(&fileobj, &py.import("pathlib")?.getattr("Path")?)?
    {
        let handle = io_module
            .getattr("FileIO")?
            .call1((fileobj, py_str(py, "rb")))?;
        let content = call0(&handle, "read")?;
        call0(&handle, "close")?;
        return io_module.getattr("BytesIO")?.call1((content,));
    } else if has(&fileobj, "seek")? && has(&fileobj, "read")? {
        callv(&fileobj, "seek", vec![py_int(py, 0)])?;
        let content = call0(&fileobj, "read")?;
        return io_module.getattr("BytesIO")?.call1((content,));
    }
    Err(PyNotImplementedError::new_err(
        "Merging requires an object that PdfReader can parse. Typically, that is a Path or a string representing a Path, a file object, or an object implementing .seek and .read. Passing a PdfReader directly works as well.",
    ))
}

// @pypdf: pypdf._writer.PdfWriter.clone_document_from_reader
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_clone_document_from_reader<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let reader = arg(args, 1)?;
    let after_page_append = arg(args, 2)?;
    call1(&slf, "clone_reader_document_root", reader.clone())?;
    let info = get(&reader, "_info")?;
    if truthy(&get(&slf, "incremental")?)? {
        if !is_none(&info) {
            let cloned_info = call1(&info, "clone", slf.clone())?;
            let info_obj = get(&cloned_info, "indirect_reference")?;
            slf.setattr("_info_obj", info_obj.clone())?;
            let idnum = to_usize(&get(&get(&info_obj, "indirect_reference")?, "idnum")?)?;
            let hash = call0(&get(&slf, "_info")?, "hash_bin")?;
            let original_hash = get(&slf, "_original_hash")?;
            original_hash.set_item(idnum - 1, hash)?;
        }
    } else if !is_none(&info) {
        let cloned_info = call1(&info, "clone", slf.clone())?;
        let info_obj = call1(&slf, "_add_object", cloned_info)?;
        slf.setattr("_info_obj", info_obj)?;
    }
    let attempted: PyResult<Bound<'py, PyAny>> = (|| {
        let id = get(&reader, "_ID")?;
        call1(&id, "clone", slf.clone())
    })();
    match attempted {
        Ok(id) => {
            slf.setattr("_ID", id)?;
        }
        Err(error) if error.is_instance_of::<PyAttributeError>(py) => {}
        Err(error) => return Err(error),
    }
    let callable_class = builtin_class(py, "callable")?;
    if truthy(&callable_class.call1((after_page_append.clone(),))?)? {
        let pages = get(&slf, "_pages")?;
        let pages_object = call0(&pages, "get_object")?;
        let kids = get_item(&pages_object, &py_str(py, "/Kids"))?;
        for page in items(&kids)? {
            after_page_append.call1((call0(&page, "get_object")?,))?;
        }
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._collect_incremental_clone_object_ids
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_collect_incremental_clone_object_ids<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let reader = arg(args, 1)?;
    let mut object_ids: HashSet<i64> = HashSet::new();
    let xref = get(&reader, "xref")?;
    for pair in items(&xref.call_method0("items")?)? {
        let entries = pair.get_item(1)?;
        for entry in items(&entries)? {
            if !is_none(&entry) && truthy(&entry)? {
                object_ids.insert(to_i64(&entry)?);
            }
        }
    }
    let xref_obj_stm = get(&reader, "xref_objStm")?;
    for entry in items(&xref_obj_stm)? {
        if !is_none(&entry) && truthy(&entry)? {
            object_ids.insert(to_i64(&entry)?);
        }
    }
    let count_limit = to_i64(&get(&slf, "_incremental_clone_object_count_limit")?)?;
    if object_ids.len() as i64 > count_limit {
        return Err(errors::limit_reached_error(
            py,
            &format!(
                "Incremental clone object count {} exceeds maximum allowed count {}.",
                object_ids.len(),
                count_limit
            ),
        ));
    }
    let max_object_id = object_ids.iter().copied().max().unwrap_or(0);
    let id_limit = to_i64(&get(&slf, "_incremental_clone_object_id_limit")?)?;
    if max_object_id > id_limit {
        return Err(errors::limit_reached_error(
            py,
            &format!(
                "Incremental clone object ID {max_object_id} exceeds maximum allowed ID {id_limit}."
            ),
        ));
    }
    let mut sorted: Vec<i64> = object_ids.into_iter().collect();
    sorted.sort_unstable();
    Ok(array_obj(
        py,
        sorted.iter().map(|value| py_int(py, *value)).collect(),
    )?)
}

// @pypdf: pypdf._writer.PdfWriter.clone_reader_document_root
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_clone_reader_document_root<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let reader = arg(args, 1)?;
    slf.setattr("_info_obj", py.None())?;
    if truthy(&get(&slf, "incremental")?)? {
        let object_ids = call1(
            &slf,
            "_collect_incremental_clone_object_ids",
            reader.clone(),
        )?;
        let count = if py_len(&object_ids)? == 0 {
            0
        } else {
            to_usize(&object_ids.get_item(py_len(&object_ids)? - 1)?)?
        };
        let objects = PyList::new(
            py,
            (0..count)
                .map(|_| py.None().into_bound(py))
                .collect::<Vec<_>>(),
        )?;
        slf.setattr("_objects", objects.clone())?;
        for object_id in items(&object_ids)? {
            let idnum = to_usize(&object_id)?;
            let reader_object = callv(&reader, "get_object", vec![object_id])?;
            if !is_none(&reader_object) {
                let replicated = call1(&reader_object, "replicate", slf.clone())?;
                objects.set_item(idnum - 1, replicated)?;
            }
        }
    } else {
        call0(&get(&slf, "_objects")?, "clear")?;
    }
    let root = call1(&get(&reader, "root_object")?, "clone", slf.clone())?;
    slf.setattr("_root_object", root.clone())?;
    let pages = callv(&root, "raw_get", vec![py_str(py, "/Pages")])?;
    slf.setattr("_pages", pages)?;
    let trailer = get(&reader, "trailer")?;
    let trailer_size = to_usize(&get_item(&trailer, &py_str(py, "/Size"))?)?;
    let objects = get(&slf, "_objects")?;
    if py_len(&objects)? > trailer_size {
        if truthy(&get(&slf, "strict")?)? {
            return Err(pdf_read_error(
                py,
                &format!(
                    "Object count {} exceeds defined trailer size {trailer_size}",
                    py_len(&objects)?
                ),
            ));
        }
        let values = PyDict::new(py);
        values.set_item("object_count", py_len(&objects)?)?;
        values.set_item("trailer_size", trailer_size)?;
        logger_warning(
            py,
            "Object count %(object_count)d exceeds defined trailer size %(trailer_size)d",
            Some(&values),
        )?;
    }
    if truthy(&get(&slf, "incremental")?)? {
        let mut hashes: Vec<Bound<'py, PyAny>> = Vec::new();
        for obj in items(&objects)? {
            if is_none(&obj) {
                hashes.push(py_int(py, 0));
            } else {
                hashes.push(call0(&obj, "hash_bin")?);
            }
        }
        slf.setattr("_original_hash", PyList::new(py, hashes)?)?;
    }
    let flatten_result = call0(&slf, "_flatten");
    match flatten_result {
        Ok(_) => {}
        Err(error) if error.is_instance_of::<PyIndexError>(py) => {
            return Err(pdf_read_error(py, "Got index error while flattening."));
        }
        Err(error) => return Err(error),
    }
    let flattened = get(&slf, "flattened_pages")?;
    if is_none(&flattened) {
        return Err(pyo3::exceptions::PyAssertionError::new_err("mypy"));
    }
    let pages_ref = get(&slf, "_pages")?;
    let incremental = truthy(&get(&slf, "incremental")?)?;
    for page in items(&flattened)? {
        let reference = get(&page, "indirect_reference")?;
        let idnum = to_usize(&get(&reference, "idnum")?)?;
        callv(
            &slf,
            "_replace_object",
            vec![py_int(py, idnum as i64), page.clone()],
        )?;
        if !incremental {
            let parent_key = name_obj(py, "/Parent")?;
            set_item(&page, &parent_key, &pages_ref)?;
        }
    }
    if !incremental {
        let pages_object = call0(&pages_ref, "get_object")?;
        let mut kids: Vec<Bound<'py, PyAny>> = Vec::new();
        for page in items(&flattened)? {
            kids.push(get(&page, "indirect_reference")?);
        }
        let kids_array = array_obj(py, kids)?;
        set_item(&pages_object, &name_obj(py, "/Kids")?, &kids_array)?;
    }
    Ok(none(py))
}

// ===========================================================================
// Outlines, threads, named destinations
// ===========================================================================

/// `pypdf._writer.ObjectDeletionFlag.<name>`.
fn odf<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf._writer")?
        .getattr("ObjectDeletionFlag")?
        .getattr(name)
}

/// `a & b` through Python (IntFlag/int semantics).
fn py_and<'py>(a: &Bound<'py, PyAny>, b: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    a.call_method1("__and__", (b,))
}

/// `a | b` through Python.
fn py_or<'py>(a: &Bound<'py, PyAny>, b: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    a.call_method1("__or__", (b,))
}

/// `pypdf.constants.ImageType.<name>`.
fn image_type<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf.constants")?
        .getattr("ImageType")?
        .getattr(name)
}

// @pypdf: pypdf._writer.PdfWriter.get_reference
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_reference<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let obj = arg(args, 1)?;
    let objects = get(&slf, "_objects")?;
    let index = to_usize(&call1(&objects, "index", obj.clone())?)?;
    let reference = indirect_obj(py, index + 1, 0, &slf)?;
    let resolved = call0(&reference, "get_object")?;
    if !eq(&resolved, &obj)? {
        return Err(pyo3::exceptions::PyAssertionError::new_err(""));
    }
    Ok(reference)
}

// @pypdf: pypdf._writer.PdfWriter.get_outline_root
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_outline_root<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let root = get(&slf, "_root_object")?;
    let outlines_key = const_str(py, "Core", "OUTLINES")?;
    let mut outline: Option<Bound<'py, PyAny>> = None;
    if root.contains(&outlines_key)? {
        let entry = get_item(&root, &outlines_key)?;
        if is_inst(&entry, "TreeObject")? {
            outline = Some(entry);
        } else if is_inst(&entry, "DictionaryObject")? {
            let tree = gclass(py, "TreeObject")?.call1((entry.clone(),))?;
            let reference = get_def(&entry, "indirect_reference", &none(py))?;
            if is_none(&reference) {
                let key = name_obj(py, &to_string(&outlines_key)?)?;
                let reference = call1(&slf, "_add_object", tree.clone())?;
                set_item(&root, &key, &reference)?;
            } else {
                callv(
                    &slf,
                    "_replace_object",
                    vec![get(&reference, "idnum")?, tree.clone()],
                )?;
            }
            outline = Some(tree);
        } else {
            let is_null_or_none = generic(py)?.getattr("is_null_or_none")?;
            if !truthy(&is_null_or_none.call1((entry.clone(),))?)? {
                let values = PyDict::new(py);
                values.set_item("entry", entry)?;
                logger_warning(
                    py,
                    "Outlines are not a dictionary: %(entry)s",
                    Some(&values),
                )?;
            }
        }
    }
    let outline = match outline {
        Some(outline) => outline,
        None => {
            let tree = gclass(py, "TreeObject")?.call0()?;
            call1(&tree, "update", PyDict::new(py).into_any())?;
            let reference = call1(&slf, "_add_object", tree.clone())?;
            let key = name_obj(py, &to_string(&outlines_key)?)?;
            set_item(&root, &key, &reference)?;
            tree
        }
    };
    Ok(outline)
}

// @pypdf: pypdf._writer.PdfWriter.get_threads_root
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_threads_root<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let root = get(&slf, "_root_object")?;
    let threads_key = const_str(py, "Core", "THREADS")?;
    if root.contains(&threads_key)? {
        return get_item(&root, &threads_key);
    }
    let threads = array_obj(py, vec![])?;
    let key = name_obj(py, &to_string(&threads_key)?)?;
    set_item(&root, &key, &threads)?;
    Ok(threads)
}

// @pypdf: pypdf._writer.PdfWriter.threads
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_threads<'py>(
    _py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    call0(&slf, "get_threads_root")
}

// @pypdf: pypdf._writer.PdfWriter.add_outline_item_destination
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_outline_item_destination<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let page_destination = call0(&arg(args, 1)?, "get_object")?;
    let mut parent = arg(args, 2)?;
    let mut before = arg(args, 3)?;
    let is_open = arg(args, 4)?;
    if is_page_object(py, &page_destination)? {
        let page_number = get(&page_destination, "page_number")?;
        let destination = gclass(py, "Destination")?.call1((
            format!("page #{}", to_string(&page_number)?),
            get(&page_destination, "indirect_reference")?,
            gclass(py, "Fit")?.getattr("fit")?.call0()?,
        ))?;
        return callv(
            &slf,
            "add_outline_item_destination",
            vec![destination, parent, before, is_open],
        );
    }
    if is_none(&parent) {
        parent = call0(&slf, "get_outline_root")?;
    }
    let open_key = name_obj(py, "/%is_open%")?;
    let open_value = boolean_obj(py, truthy(&is_open)?)?;
    set_item(&page_destination, &open_key, &open_value)?;
    parent = call0(&parent, "get_object")?;
    let reference = call1(&slf, "_add_object", page_destination.clone())?;
    if !is_none(&before) {
        before = get(&before, "indirect_reference")?;
    }
    let inc_counter = get(&page_destination, "inc_parent_counter_outline")?;
    callv(
        &parent,
        "insert_child",
        vec![reference.clone(), before, slf.clone(), inc_counter],
    )?;
    let count_key = name_obj(py, "/Count")?;
    if !page_destination.contains(&count_key)? {
        let zero = number_obj(py, 0)?;
        set_item(&page_destination, &count_key, &zero)?;
    }
    Ok(reference)
}

// @pypdf: pypdf._writer.PdfWriter.add_outline_item_dict
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_outline_item_dict<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let outline_item = arg(args, 1)?;
    let parent = arg(args, 2)?;
    let before = arg(args, 3)?;
    let is_open = arg(args, 4)?;
    let tree = gclass(py, "TreeObject")?.call0()?;
    call1(&tree, "update", outline_item)?;
    callv(
        &slf,
        "add_outline_item_destination",
        vec![tree, parent, before, is_open],
    )
}

// @pypdf: pypdf._writer.PdfWriter.add_outline_item
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_outline_item<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let title = arg(args, 1)?;
    let page_number = arg(args, 2)?;
    let parent = arg(args, 3)?;
    let before = arg(args, 4)?;
    let color = arg(args, 5)?;
    let bold = arg(args, 6)?;
    let italic = arg(args, 7)?;
    let fit = arg(args, 8)?;
    let is_open = arg(args, 9)?;
    let action_ref;
    if is_none(&page_number) {
        action_ref = none(py);
    } else {
        let mut page_ref = none(py);
        if is_inst(&page_number, "IndirectObject")? {
            page_ref = page_number.clone();
        } else if is_page_object(py, &page_number)? {
            page_ref = get(&page_number, "indirect_reference")?;
        } else if is_inst_builtin(&page_number, "int")? {
            let index = to_i64(&page_number)?;
            let pages = get(&slf, "pages")?;
            match get_item(&pages, &py_int(py, index)) {
                Ok(page) => page_ref = get(&page, "indirect_reference")?,
                Err(error) if error.is_instance_of::<PyIndexError>(py) => {
                    page_ref = number_obj(py, index)?;
                }
                Err(error) => return Err(error),
            }
        } else {
            let kind = to_string(
                &builtins(py)?
                    .getattr("type")?
                    .call1((page_number.clone(),))?,
            )?;
            return Err(type_error(py, &format!("page_number: invalid type {kind}")));
        }
        if is_none(&page_ref) {
            let values = PyDict::new(py);
            values.set_item("page_number", page_number.clone())?;
            logger_warning(
                py,
                "can not find reference of page %(page_number)s",
                Some(&values),
            )?;
            page_ref = null_obj(py)?;
        }
        let dest = gclass(py, "Destination")?.call1((
            name_obj(py, &format!("/{} outline item", to_string(&title)?))?,
            page_ref,
            fit,
        ))?;
        let action = dict_obj(
            py,
            vec![
                (
                    name_obj(py, &to_string(&const_str(py, "GoToActionArguments", "D")?)?)?,
                    get(&dest, "dest_array")?,
                ),
                (
                    name_obj(py, &to_string(&const_str(py, "GoToActionArguments", "S")?)?)?,
                    name_obj(py, "/GoTo")?,
                ),
            ],
        )?;
        action_ref = call1(&slf, "_add_object", action)?;
    }
    let outline_item = writer_create_outline_item(py, &action_ref, &title, &color, &italic, &bold)?;
    let outline_ref = call1(&slf, "_add_object", outline_item)?;
    let mut parent = parent;
    if is_none(&parent) {
        parent = call0(&slf, "get_outline_root")?;
    }
    callv(
        &slf,
        "add_outline_item_destination",
        vec![outline_ref, parent, before, is_open],
    )
}

// @pypdf: pypdf._writer.PdfWriter.add_outline
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_outline<'py>(
    _py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let _ = args;
    Err(PyNotImplementedError::new_err(
        "This method is not yet implemented. Use :meth:`add_outline_item` instead.",
    ))
}

// @pypdf: pypdf._writer.PdfWriter.add_named_destination_array
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_named_destination_array<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let title = arg(args, 1)?;
    let destination = arg(args, 2)?;
    let named_dest = call0(&slf, "get_named_dest_root")?;
    let mut index = 0usize;
    while index < py_len(&named_dest)? {
        let entry = get_item(&named_dest, &py_int(py, index as i64))?;
        if lt(&title, &entry)? {
            callv(
                &named_dest,
                "insert",
                vec![py_int(py, index as i64), destination.clone()],
            )?;
            let text = text_obj(py, &to_string(&title)?)?;
            callv(&named_dest, "insert", vec![py_int(py, index as i64), text])?;
            return Ok(none(py));
        }
        index += 2;
    }
    let text = text_obj(py, &to_string(&title)?)?;
    call1(
        &named_dest,
        "extend",
        array_obj(py, vec![text, destination])?,
    )?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.add_named_destination_object
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_named_destination_object<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let page_destination = arg(args, 1)?;
    let reference = call1(&slf, "_add_object", get(&page_destination, "dest_array")?)?;
    let title = get_item_str(&page_destination, "/Title")?;
    callv(
        &slf,
        "add_named_destination_array",
        vec![title, reference.clone()],
    )?;
    Ok(reference)
}

// @pypdf: pypdf._writer.PdfWriter._get_page_reference
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_get_page_reference<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let page_number = to_i64(&arg(args, 1)?)?;
    let pages = get(&slf, "pages")?;
    let count = py_len(&pages)? as i64;
    if !(-count <= page_number && page_number < count) {
        return Err(index_error(
            py,
            &format!("Page number {page_number} is out of range"),
        ));
    }
    let page = get_item(&pages, &py_int(py, page_number))?;
    get(&page, "indirect_reference")
}

// @pypdf: pypdf._writer.PdfWriter.add_named_destination
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_named_destination<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut title = arg(args, 1)?;
    let page_number = arg(args, 2)?;
    let mut fit = arg(args, 3)?;
    let page_ref = call1(&slf, "_get_page_reference", page_number)?;
    if is_none(&fit) {
        let page_object = call0(&page_ref, "get_object")?;
        let top = gget(
            &page_object,
            &const_str(py, "PageAttributes", "MEDIABOX")?,
            &none(py),
        )?;
        let top = if is_none(&top) {
            py_int(py, 0)
        } else {
            gclass(py, "RectangleObject")?
                .call1((top,))?
                .getattr("top")?
        };
        let fit_class = gclass(py, "Fit")?;
        fit = callkw(&fit_class, "fit_horizontally", vec![], vec![("top", top)])?;
    }
    if !is_inst(&title, "TextStringObject")? {
        title = text_obj(py, &to_string(&title)?)?;
    }
    let dest = dict_obj(py, vec![])?;
    let destination = gclass(py, "Destination")?.call1((title.clone(), page_ref, fit))?;
    let mut entries = Vec::new();
    entries.push((
        name_obj(py, &to_string(&const_str(py, "GoToActionArguments", "D")?)?)?,
        get(&destination, "dest_array")?,
    ));
    entries.push((
        name_obj(py, &to_string(&const_str(py, "GoToActionArguments", "S")?)?)?,
        name_obj(py, "/GoTo")?,
    ));
    let update = pydict_obj(py, entries)?;
    call1(&dest, "update", update)?;
    let reference = call1(&slf, "_add_object", dest)?;
    callv(
        &slf,
        "add_named_destination_array",
        vec![title, reference.clone()],
    )?;
    Ok(reference)
}

// ===========================================================================
// Removal of objects / annotations
// ===========================================================================

// @pypdf: pypdf._writer.PdfWriter.remove_links
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_remove_links<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let all = odf(py, "ALL_ANNOTATIONS")?;
    for page in items(&get(&slf, "pages")?)? {
        callv(&slf, "remove_objects_from_page", vec![page, all.clone()])?;
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.remove_annotations
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_remove_annotations<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let subtypes = arg(args, 1)?;
    for page in items(&get(&slf, "pages")?)? {
        callv(
            &slf,
            "_remove_annots_from_page",
            vec![page, subtypes.clone()],
        )?;
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._remove_annots_from_page
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_remove_annots_from_page<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let page = call0(&arg(args, 1)?, "get_object")?;
    let subtypes = arg(args, 2)?;
    let annots_key = const_str(py, "PageAttributes", "ANNOTS")?;
    if !page.contains(&annots_key)? {
        return Ok(none(py));
    }
    let annotations = get_item(&page, &annots_key)?;
    if !is_inst(&annotations, "ArrayObject")? {
        let values = PyDict::new(py);
        values.set_item("annotations", annotations)?;
        logger_warning(
            py,
            "Annotations are not an array: %(annotations)s",
            Some(&values),
        )?;
        return Ok(none(py));
    }
    let objects = get(&slf, "_objects")?;
    let mut index = 0usize;
    while index < py_len(&annotations)? {
        let annotation = get_item(&annotations, &py_int(py, index as i64))?;
        let obj = call0(&annotation, "get_object")?;
        let subtype = if is_inst(&obj, "DictionaryObject")? {
            gget(&obj, &py_str(py, "/Subtype"), &none(py))?
        } else {
            none(py)
        };
        let remove = if is_none(&subtypes) {
            true
        } else {
            !is_none(&subtype) && subtypes.contains(&subtype)?
        };
        if remove {
            if is_inst(&annotation, "IndirectObject")? {
                let idnum = to_usize(&get(&annotation, "idnum")?)?;
                let null = null_obj(py)?;
                objects.set_item(idnum - 1, null)?;
            }
            annotations.del_item(py_int(py, index as i64))?;
        } else {
            index += 1;
        }
    }
    Ok(none(py))
}

/// `content.operations` cleaning loop of
/// `PdfWriter._remove_objects_from_page__clean`.
fn writer_clean_content<'py>(
    py: Python<'py>,
    content: &Bound<'py, PyAny>,
    images: &[Bound<'py, PyAny>],
    _forms: &[Bound<'py, PyAny>],
    jump_operators: &[Vec<u8>],
    to_delete: &Bound<'py, PyAny>,
    text_filters: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let mut font_id = none(py);
    let mut font_ids_to_delete: Vec<Bound<'py, PyAny>> = Vec::new();
    let text_flag = truthy(&py_and(to_delete, &odf(py, "TEXT")?)?)?;
    if truthy(text_filters)? && text_flag {
        let ids = gget(
            text_filters,
            &py_str(py, "font_ids"),
            &array_obj(py, vec![])?,
        )?;
        font_ids_to_delete = items(&ids)?;
    }
    let operations = get(content, "operations")?;
    let mut index = 0usize;
    while index < py_len(&operations)? {
        let entry = get_item(&operations, &py_int(py, index as i64))?;
        let operands = entry.get_item(0)?;
        let operator = entry.get_item(1)?;
        if eq(&operator, &py_bytes(py, b"Tf"))? {
            font_id = get_item(&operands, &py_int(py, 0))?;
        }
        let mut matches = false;
        if eq(&operator, &py_bytes(py, b"INLINE IMAGE"))?
            && truthy(&py_and(to_delete, &odf(py, "INLINE_IMAGES")?)?)?
        {
            matches = true;
        }
        if !matches && !jump_operators.is_empty() {
            let operator_bytes = to_bytes(&operator)?;
            if jump_operators.iter().any(|item| *item == operator_bytes) {
                matches = true;
            }
        }
        if !matches
            && eq(&operator, &py_bytes(py, b"Do"))?
            && truthy(&py_and(to_delete, &odf(py, "XOBJECT_IMAGES")?)?)?
        {
            let operand = get_item(&operands, &py_int(py, 0))?;
            if images
                .iter()
                .any(|image| image.eq(&operand).unwrap_or(false))
            {
                matches = true;
            }
        }
        if matches {
            let delete = !text_flag
                || (text_flag && !truthy(text_filters)?)
                || (text_flag
                    && font_ids_to_delete
                        .iter()
                        .any(|item| item.eq(&font_id).unwrap_or(false)));
            if delete {
                operations.del_item(py_int(py, index as i64))?;
            } else {
                index += 1;
            }
        } else {
            index += 1;
        }
    }
    call0(content, "get_data")?;
    Ok(())
}

/// `PdfWriter._remove_objects_from_page__clean_forms` (recursive).
fn writer_clean_forms<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    elt: &Bound<'py, PyAny>,
    stack: &Bound<'py, PyAny>,
    jump_operators: &[Vec<u8>],
    to_delete: &Bound<'py, PyAny>,
    text_filters: &Bound<'py, PyAny>,
) -> PyResult<(Vec<Bound<'py, PyAny>>, Vec<Bound<'py, PyAny>>)> {
    // cycle guard
    let mut cyclic = stack.contains(elt)?;
    if !cyclic && has(elt, "indirect_reference")? {
        let reference = get(elt, "indirect_reference")?;
        for item in items(stack)? {
            let other = get_def(&item, "indirect_reference", &py_int(py, -1))?;
            if eq(&reference, &other)? {
                cyclic = true;
                break;
            }
        }
    }
    if cyclic {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut resources = call0(
        &gget(elt, &py_str(py, "/Resources"), &null_obj(py)?)?,
        "get_object",
    )?;
    if !is_inst(&resources, "DictionaryObject")? {
        let is_null_or_none = generic(py)?.getattr("is_null_or_none")?;
        if !truthy(&is_null_or_none.call1((resources.clone(),))?)? {
            let values = PyDict::new(py);
            values.set_item("resources", resources.clone())?;
            logger_warning(
                py,
                "Resources are not a dictionary: %(resources)s",
                Some(&values),
            )?;
        }
        resources = new_g(py, "DictionaryObject", vec![])?;
    }
    let mut dictionary = call0(
        &gget(
            &resources,
            &py_str(py, "/XObject"),
            &new_g(py, "DictionaryObject", vec![])?,
        )?,
        "get_object",
    )?;
    if !is_inst(&dictionary, "DictionaryObject")? {
        let is_null_or_none = generic(py)?.getattr("is_null_or_none")?;
        if !truthy(&is_null_or_none.call1((dictionary.clone(),))?)? {
            let values = PyDict::new(py);
            values.set_item("x_object", dictionary.clone())?;
            logger_warning(
                py,
                "XObject resources are not a dictionary: %(x_object)s",
                Some(&values),
            )?;
        }
        dictionary = PyDict::new(py).into_any();
    }
    let mut images: Vec<Bound<'py, PyAny>> = Vec::new();
    let mut forms: Vec<Bound<'py, PyAny>> = Vec::new();
    let items_list = items(&call0(&dictionary, "items")?)?;
    for pair in items_list {
        let key = pair.get_item(0)?;
        let value = pair.get_item(1)?;
        let obj = call0(&value, "get_object")?;
        let attempt: PyResult<Option<Bound<'py, PyAny>>> = (|| {
            let mut content: Option<Bound<'py, PyAny>> = None;
            let subtype = get_item_str(&obj, "/Subtype")?;
            if truthy(&py_and(to_delete, &odf(py, "XOBJECT_IMAGES")?)?)?
                && eq(&subtype, &py_str(py, "/Image"))?
            {
                content = Some(null_obj(py)?);
                images.push(key.clone());
            }
            if eq(&subtype, &py_str(py, "/Form"))? {
                forms.push(key.clone());
                let stream_content = if is_inst(&obj, "ContentStream")? {
                    obj.clone()
                } else {
                    let stream = gclass(py, "ContentStream")?.call1((obj.clone(), slf.clone()))?;
                    let mut update: Vec<(Bound<'py, PyAny>, Bound<'py, PyAny>)> = Vec::new();
                    for inner in items(&call0(&obj, "items")?)? {
                        let inner_key = inner.get_item(0)?;
                        let text = to_string(&inner_key)?;
                        if text != "/Length" && text != "/Filter" && text != "/DecodeParms" {
                            update.push((inner_key, inner.get_item(1)?));
                        }
                    }
                    call1(&stream, "update", pydict_obj(py, update)?)?;
                    if let Ok(reference) = get(&obj, "indirect_reference") {
                        stream.setattr("indirect_reference", reference)?;
                    }
                    stream
                };
                call1(stack, "append", elt.clone())?;
                writer_clean_forms(
                    py,
                    slf,
                    &stream_content,
                    stack,
                    jump_operators,
                    to_delete,
                    text_filters,
                )?;
                content = Some(stream_content);
            }
            if let Some(content) = content {
                if is_inst(&value, "IndirectObject")? {
                    let idnum = to_usize(&get(&value, "idnum")?)?;
                    let objects = get(slf, "_objects")?;
                    objects.set_item(idnum - 1, content)?;
                } else {
                    let reference = callv(slf, "_add_object", vec![content])?;
                    set_item(&dictionary, &key, &reference)?;
                }
            }
            Ok(None)
        })();
        match attempt {
            Ok(_) => {}
            Err(error)
                if error.is_instance_of::<PyTypeError>(py)
                    || error.is_instance_of::<PyKeyError>(py) => {}
            Err(error) => return Err(error),
        }
    }
    for image in &images {
        dictionary.del_item(image)?;
    }
    if is_inst(elt, "StreamObject")? {
        let mut target = elt.clone();
        if !is_inst(elt, "ContentStream")? {
            let stream = gclass(py, "ContentStream")?.call1((elt.clone(), slf.clone()))?;
            call1(&stream, "update", call0(elt, "items")?)?;
            target = stream;
        }
        writer_clean_content(
            py,
            &target,
            &images,
            &forms,
            jump_operators,
            to_delete,
            text_filters,
        )?;
    }
    Ok((images, forms))
}

// @pypdf: pypdf._writer.PdfWriter._remove_objects_from_page__clean
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_remove_objects_from_page_clean<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let content = arg(args, 1)?;
    let images = items(&arg(args, 2)?)?;
    let forms = items(&arg(args, 3)?)?;
    let jump_operators = to_bytes_list(&arg(args, 4)?)?;
    let to_delete = arg(args, 5)?;
    let text_filters = arg(args, 6)?;
    writer_clean_content(
        py,
        &content,
        &images,
        &forms,
        &jump_operators,
        &to_delete,
        &text_filters,
    )?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._remove_objects_from_page__clean_forms
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_remove_objects_from_page_clean_forms<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let elt = arg(args, 1)?;
    let stack = arg(args, 2)?;
    let jump_operators = to_bytes_list(&arg(args, 3)?)?;
    let to_delete = arg(args, 4)?;
    let text_filters = arg(args, 5)?;
    let (images, forms) = writer_clean_forms(
        py,
        &slf,
        &elt,
        &stack,
        &jump_operators,
        &to_delete,
        &text_filters,
    )?;
    Ok(PyTuple::new(py, [array_obj(py, images)?, array_obj(py, forms)?])?.into_any())
}

fn to_bytes_list(obj: &Bound<'_, PyAny>) -> PyResult<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    for item in items(obj)? {
        out.push(to_bytes(&item)?);
    }
    Ok(out)
}

// @pypdf: pypdf._writer.PdfWriter.remove_objects_from_page
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_remove_objects_from_page<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut page = arg(args, 1)?;
    let to_delete = arg(args, 2)?;
    let text_filters = arg(args, 3)?;
    if is_inst_builtin(&to_delete, "list")? || is_inst_builtin(&to_delete, "tuple")? {
        for single in items(&to_delete)? {
            callv(&slf, "remove_objects_from_page", vec![page.clone(), single])?;
        }
        return Ok(none(py));
    }
    if truthy(&py_and(&to_delete, &odf(py, "LINKS")?)?)? {
        callv(
            &slf,
            "_remove_annots_from_page",
            vec![page, array_obj(py, vec![py_str(py, "/Link")])?],
        )?;
        return Ok(none(py));
    }
    if truthy(&py_and(&to_delete, &odf(py, "ATTACHMENTS")?)?)? {
        callv(
            &slf,
            "_remove_annots_from_page",
            vec![
                page,
                array_obj(
                    py,
                    vec![
                        py_str(py, "/FileAttachment"),
                        py_str(py, "/Sound"),
                        py_str(py, "/Movie"),
                        py_str(py, "/Screen"),
                    ],
                )?,
            ],
        )?;
        return Ok(none(py));
    }
    if truthy(&py_and(&to_delete, &odf(py, "OBJECTS_3D")?)?)? {
        callv(
            &slf,
            "_remove_annots_from_page",
            vec![page, array_obj(py, vec![py_str(py, "/3D")])?],
        )?;
        return Ok(none(py));
    }
    if truthy(&py_and(&to_delete, &odf(py, "ALL_ANNOTATIONS")?)?)? {
        callv(&slf, "_remove_annots_from_page", vec![page, none(py)])?;
        return Ok(none(py));
    }
    let mut jump_operators: Vec<Vec<u8>> = Vec::new();
    if truthy(&py_and(&to_delete, &odf(py, "DRAWING_IMAGES")?)?)? {
        jump_operators = [
            "w", "J", "j", "M", "d", "i", "W", "W*", "b", "b*", "B", "B*", "S", "s", "f", "f*",
            "F", "n", "m", "l", "c", "v", "y", "h", "re", "sh",
        ]
        .iter()
        .map(|item| item.as_bytes().to_vec())
        .collect();
    }
    if truthy(&py_and(&to_delete, &odf(py, "TEXT")?)?)? {
        jump_operators = vec![
            b"Tj".to_vec(),
            b"TJ".to_vec(),
            b"'".to_vec(),
            b"\"".to_vec(),
        ];
    }
    if !is_page_object(py, &page)? {
        let reference = get(&page, "indirect_reference")?;
        let page_class = py.import("pypdf._page")?.getattr("PageObject")?;
        page = page_class.call1((slf.clone(), reference))?;
    }
    let contents_key = py_str(py, "/Contents");
    if page.contains(&contents_key)? {
        let content = call0(&page, "get_contents")?;
        let cleaned = callv(
            &slf,
            "_remove_objects_from_page__clean_forms",
            vec![
                page.clone(),
                PyList::empty(py).into_any(),
                array_obj(
                    py,
                    jump_operators
                        .iter()
                        .map(|item| py_bytes(py, item))
                        .collect(),
                )?,
                to_delete.clone(),
                text_filters.clone(),
            ],
        )?;
        let images = cleaned.get_item(0)?;
        let forms = cleaned.get_item(1)?;
        callv(
            &slf,
            "_remove_objects_from_page__clean",
            vec![
                content.clone(),
                images,
                forms,
                array_obj(
                    py,
                    jump_operators
                        .iter()
                        .map(|item| py_bytes(py, item))
                        .collect(),
                )?,
                to_delete.clone(),
                text_filters.clone(),
            ],
        )?;
        call1(&page, "replace_contents", content)?;
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.remove_images
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_remove_images<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut to_delete = arg(args, 1)?;
    if is_inst_builtin(&to_delete, "bool")? {
        py.import("pypdf._utils")?
            .getattr("deprecate_with_replacement")?
            .call1((
                "Passing a boolean to remove_images",
                "ImageType.ALL",
                "8.0.0",
            ))?;
        to_delete = image_type(py, "ALL")?;
    }
    let mut flags = odf(py, "NONE")?;
    for name in ["XOBJECT_IMAGES", "INLINE_IMAGES", "DRAWING_IMAGES"] {
        if truthy(&py_and(&to_delete, &image_type(py, name)?)?)? {
            flags = py_or(&flags, &odf(py, name)?)?;
        }
    }
    for page in items(&get(&slf, "pages")?)? {
        callv(&slf, "remove_objects_from_page", vec![page, flags.clone()])?;
    }
    Ok(none(py))
}

/// `PdfWriter.remove_text.get_font_info` (nested helper).
fn writer_get_font_info<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    font_info: Option<Bound<'py, PyAny>>,
    key: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let font_info = match font_info {
        Some(info) => info,
        None => PyDict::new(py).into_any(),
    };
    let mut obj = obj.clone();
    if is_inst(&obj, "IndirectObject")? {
        obj = call0(&obj, "get_object")?;
    }
    if is_inst_builtin(&obj, "dict")? {
        if eq(
            &gget(&obj, &py_str(py, "/Type"), &none(py))?,
            &py_str(py, "/Font"),
        )? {
            let font_name = gget(&obj, &py_str(py, "/BaseFont"), &py_str(py, ""))?;
            let mut normalized = to_string(&font_name)?;
            normalized = normalized.trim_start_matches('/').to_string();
            let normalized = normalized.rsplit('+').next().unwrap_or("").to_string();
            if !font_info.contains(&py_str(py, &normalized))? {
                let entry = dict_obj(
                    py,
                    vec![
                        (py_str(py, "normalized_font_name"), py_str(py, &normalized)),
                        (py_str(py, "resource_ids"), array_obj(py, vec![])?),
                    ],
                )?;
                set_item(&font_info, &py_str(py, &normalized), &entry)?;
            }
            let entry = get_item(&font_info, &py_str(py, &normalized))?;
            let resource_ids = get_item(&entry, &py_str(py, "resource_ids"))?;
            if !resource_ids.contains(key)? {
                call1(&resource_ids, "append", key.clone())?;
            }
        }
        for inner in items(&call0(&obj, "items")?)? {
            let inner_key = inner.get_item(0)?;
            let inner_value = inner.get_item(1)?;
            writer_get_font_info(py, &inner_value, Some(font_info.clone()), &inner_key)?;
        }
    } else if is_inst_builtin(&obj, "list")? || is_inst(&obj, "ArrayObject")? {
        for child in items(&obj)? {
            writer_get_font_info(py, &child, Some(font_info.clone()), &none(py))?;
        }
    }
    Ok(font_info)
}

// @pypdf: pypdf._writer.PdfWriter.remove_text
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_remove_text<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let font_names = arg(args, 1)?;
    let font_names = if !truthy(&font_names)? {
        array_obj(py, vec![])?
    } else {
        font_names
    };
    for page in items(&get(&slf, "pages")?)? {
        let mut resource_ids_to_remove: Vec<Bound<'py, PyAny>> = Vec::new();
        if truthy(&font_names)? {
            let resources = gget(&page, &py_str(py, "/Resources"), &none(py))?;
            let font_info = writer_get_font_info(py, &resources, None, &none(py))?;
            for font_name in items(&font_names)? {
                if font_info.contains(&font_name)? {
                    let entry = get_item(&font_info, &font_name)?;
                    let ids = get_item(&entry, &py_str(py, "resource_ids"))?;
                    for id in items(&ids)? {
                        resource_ids_to_remove.push(id);
                    }
                }
            }
        }
        let text_filters = PyDict::new(py);
        if truthy(&font_names)? {
            text_filters.set_item("font_ids", array_obj(py, resource_ids_to_remove)?)?;
        }
        callv(
            &slf,
            "remove_objects_from_page",
            vec![page, odf(py, "TEXT")?, text_filters.into_any()],
        )?;
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.reattach_fields
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_reattach_fields<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let page = arg(args, 1)?;
    let mut result: Vec<Bound<'py, PyAny>> = Vec::new();
    if is_none(&page) {
        for entry in items(&get(&slf, "pages")?)? {
            let reattached = call1(&slf, "reattach_fields", entry)?;
            for item in items(&reattached)? {
                result.push(item);
            }
        }
        return Ok(array_obj(py, result)?);
    }
    let root = get(&slf, "_root_object")?;
    let acro_key = const_str(py, "CatalogAttributes", "ACRO_FORM")?;
    let acro_form = if root.contains(&acro_key)? {
        get_item(&root, &acro_key)?
    } else {
        let form = new_g(py, "DictionaryObject", vec![])?;
        let key = name_obj(py, &to_string(&acro_key)?)?;
        set_item(&root, &key, &form)?;
        form
    };
    let fields_key = const_str(py, "InteractiveFormDictEntries", "Fields")?;
    let fields = if acro_form.contains(&fields_key)? {
        get_item(&acro_form, &fields_key)?
    } else {
        let array = array_obj(py, vec![])?;
        let key = name_obj(py, &to_string(&fields_key)?)?;
        set_item(&acro_form, &key, &array)?;
        array
    };
    let annots_key = py_str(py, "/Annots");
    if !page.contains(&annots_key)? {
        return Ok(array_obj(py, result)?);
    }
    let annotations = get_item(&page, &annots_key)?;
    let mut index = 0usize;
    let total = py_len(&annotations)?;
    while index < total {
        let annotation = get_item(&annotations, &py_int(py, index as i64))?;
        let is_indirect = is_inst(&annotation, "IndirectObject")?;
        let annotation = call0(&annotation, "get_object")?;
        if eq(
            &gget(&annotation, &py_str(py, "/Subtype"), &py_str(py, ""))?,
            &py_str(py, "/Widget"),
        )? && annotation.contains(&py_str(py, "/FT"))?
        {
            let dict = get(&annotation, "__dict__")?;
            let skip = if dict.contains(&py_str(py, "indirect_reference"))? {
                let reference = get(&annotation, "indirect_reference")?;
                fields.contains(&reference)?
            } else {
                false
            };
            if !skip {
                if !is_indirect {
                    let reference = call1(&slf, "_add_object", annotation.clone())?;
                    annotations.set_item(index, reference)?;
                }
                call1(&fields, "append", get(&annotation, "indirect_reference")?)?;
                result.push(annotation);
            }
        }
        index += 1;
    }
    Ok(array_obj(py, result)?)
}

// ===========================================================================
// add_uri / page layout / page mode / annotations / clean_page
// ===========================================================================

// @pypdf: pypdf._writer.PdfWriter.add_uri
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_uri<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let page_number = arg(args, 1)?;
    let uri = arg(args, 2)?;
    let rect = arg(args, 3)?;
    let border = arg(args, 4)?;
    let page_link = call1(&slf, "_get_page_reference", page_number)?;
    let page_ref = call1(&slf, "get_object", page_link.clone())?;
    let mut border_arr: Vec<Bound<'py, PyAny>> = Vec::new();
    if !is_none(&border) {
        let border_items = items(&border)?;
        let mut count = 0;
        for item in &border_items {
            if count >= 3 {
                break;
            }
            border_arr.push(number_obj(py, to_i64(item)?)?);
            count += 1;
        }
        if border_items.len() == 4 {
            let dash_items = items(&border_items[3])?;
            let mut dash: Vec<Bound<'py, PyAny>> = Vec::new();
            for item in dash_items {
                dash.push(number_obj(py, to_i64(&item)?)?);
            }
            border_arr.push(array_obj(py, dash)?);
        }
    } else {
        border_arr.push(number_obj(py, 2)?);
        border_arr.push(number_obj(py, 2)?);
        border_arr.push(number_obj(py, 2)?);
    }
    let rect = if is_inst_builtin(&rect, "str")? {
        let text = to_string(&rect)?;
        let cleaned = text.trim().trim_start_matches('[').trim_end_matches(']');
        let mut coords: Vec<Bound<'py, PyAny>> = Vec::new();
        for token in cleaned.split_whitespace() {
            let value = token
                .parse::<f64>()
                .map_err(|_| value_error(py, "could not convert string to float"))?;
            coords.push(py_float(py, value));
        }
        gclass(py, "RectangleObject")?.call1((PyList::new(py, coords)?,))?
    } else if is_inst(&rect, "RectangleObject")? {
        rect
    } else {
        gclass(py, "RectangleObject")?.call1((rect,))?
    };
    let lnk2 = dict_obj(
        py,
        vec![
            (name_obj(py, "/S")?, name_obj(py, "/URI")?),
            (name_obj(py, "/URI")?, text_obj(py, &to_string(&uri)?)?),
        ],
    )?;
    let lnk = dict_obj(
        py,
        vec![
            (
                name_obj(
                    py,
                    &to_string(&const_str(py, "AnnotationDictionaryAttributes", "Type")?)?,
                )?,
                name_obj(py, "/Annot")?,
            ),
            (
                name_obj(
                    py,
                    &to_string(&const_str(py, "AnnotationDictionaryAttributes", "Subtype")?)?,
                )?,
                name_obj(py, "/Link")?,
            ),
            (
                name_obj(
                    py,
                    &to_string(&const_str(py, "AnnotationDictionaryAttributes", "P")?)?,
                )?,
                page_link,
            ),
            (
                name_obj(
                    py,
                    &to_string(&const_str(py, "AnnotationDictionaryAttributes", "Rect")?)?,
                )?,
                rect,
            ),
            (name_obj(py, "/H")?, name_obj(py, "/I")?),
            (
                name_obj(
                    py,
                    &to_string(&const_str(py, "AnnotationDictionaryAttributes", "Border")?)?,
                )?,
                array_obj(py, border_arr)?,
            ),
            (name_obj(py, "/A")?, lnk2),
        ],
    )?;
    let lnk_ref = call1(&slf, "_add_object", lnk)?;
    let annots_key = const_str(py, "PageAttributes", "ANNOTS")?;
    if page_ref.contains(&annots_key)? {
        call1(&get_item(&page_ref, &annots_key)?, "append", lnk_ref)?;
    } else {
        let key = name_obj(py, &to_string(&annots_key)?)?;
        let array = array_obj(py, vec![lnk_ref])?;
        set_item(&page_ref, &key, &array)?;
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._get_page_layout
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_get_page_layout<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let root = get(&slf, "_root_object")?;
    let key = py_str(py, "/PageLayout");
    if root.contains(&key)? {
        Ok(get_item(&root, &key)?)
    } else {
        Ok(none(py))
    }
}

// @pypdf: pypdf._writer.PdfWriter._set_page_layout
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_set_page_layout<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut layout = arg(args, 1)?;
    if !is_inst(&layout, "NameObject")? {
        let valid = get(&slf, "_valid_layouts")?;
        if !valid.contains(&layout)? {
            let values = PyDict::new(py);
            values.set_item(
                "layouts",
                items(&valid)?
                    .iter()
                    .map(|v| to_string(v).unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join(", "),
            )?;
            logger_warning(py, "Layout should be one of: %(layouts)s", Some(&values))?;
        }
        layout = name_obj(py, &to_string(&layout)?)?;
    }
    let root = get(&slf, "_root_object")?;
    let update = pydict_obj(py, vec![(name_obj(py, "/PageLayout")?, layout)])?;
    call1(&root, "update", update)?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.set_page_layout
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_set_page_layout<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let layout = arg(args, 1)?;
    call1(&slf, "_set_page_layout", layout)?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.page_layout
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_page_layout<'py>(
    _py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    call0(&slf, "_get_page_layout")
}

// @pypdf: pypdf._writer.PdfWriter.page_layout
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_property_set_page_layout<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let layout = arg(args, 1)?;
    call1(&slf, "_set_page_layout", layout)?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._get_page_mode
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_get_page_mode<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let root = get(&slf, "_root_object")?;
    let key = py_str(py, "/PageMode");
    if root.contains(&key)? {
        Ok(get_item(&root, &key)?)
    } else {
        Ok(none(py))
    }
}

// @pypdf: pypdf._writer.PdfWriter.page_mode
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_get_page_mode<'py>(
    _py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    call0(&slf, "_get_page_mode")
}

// @pypdf: pypdf._writer.PdfWriter.page_mode
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_property_set_page_mode<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut mode = arg(args, 1)?;
    if !is_inst(&mode, "NameObject")? {
        let valid = get(&slf, "_valid_modes")?;
        if !valid.contains(&mode)? {
            let values = PyDict::new(py);
            values.set_item(
                "modes",
                items(&valid)?
                    .iter()
                    .map(|v| to_string(v).unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join(", "),
            )?;
            logger_warning(py, "Mode should be one of: %(modes)s", Some(&values))?;
        }
        mode = name_obj(py, &to_string(&mode)?)?;
    }
    let root = get(&slf, "_root_object")?;
    let update = pydict_obj(py, vec![(name_obj(py, "/PageMode")?, mode)])?;
    call1(&root, "update", update)?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.clean_page
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_clean_page<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let page = call0(&arg(args, 1)?, "get_object")?;
    let annots = gget(&page, &py_str(py, "/Annots"), &array_obj(py, vec![])?)?;
    for annotation in items(&annots)? {
        let annotation = call0(&annotation, "get_object")?;
        let dest = gget(&annotation, &py_str(py, "/Dest"), &none(py))?;
        let action = gget(&annotation, &py_str(py, "/A"), &none(py))?;
        if is_inst(&dest, "NameObject")? {
            let key = name_obj(py, "/Dest")?;
            let value = text_obj(py, &to_string(&dest)?)?;
            set_item(&annotation, &key, &value)?;
        } else if !is_none(&action) {
            let action = call0(&action, "get_object")?;
            let dest = gget(&action, &py_str(py, "/D"), &none(py))?;
            if is_inst(&dest, "NameObject")? {
                let key = name_obj(py, "/D")?;
                let value = text_obj(py, &to_string(&dest)?)?;
                set_item(&action, &key, &value)?;
            }
        }
    }
    Ok(page)
}

// @pypdf: pypdf._writer.PdfWriter.add_annotation
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_annotation<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut page = arg(args, 1)?;
    let annotation = arg(args, 2)?;
    if is_inst_builtin(&page, "int")? {
        let pages = get(&slf, "pages")?;
        page = get_item(&pages, &page)?;
    } else if !is_page_object(py, &page)? {
        return Err(type_error(py, "page: invalid type"));
    }
    let to_add = writer_pdf_objectify(py, &annotation)?;
    let p_key = name_obj(py, "/P")?;
    let page_reference = get(&page, "indirect_reference")?;
    set_item(&to_add, &p_key, &page_reference)?;
    if is_none(&get(&page, "annotations")?) {
        let key = name_obj(py, "/Annots")?;
        let array = array_obj(py, vec![])?;
        set_item(&page, &key, &array)?;
    }
    // Resolve pypdf's intermediate internal-link destination.
    let destination = gget(&to_add, &py_str(py, "/Dest"), &none(py))?;
    if eq(
        &gget(&to_add, &py_str(py, "/Subtype"), &none(py))?,
        &py_str(py, "/Link"),
    )? && is_inst(&destination, "DictionaryObject")?
        && destination.contains(&py_str(py, "target_page_index"))?
    {
        let target_page_index = to_i64(&get_item(&destination, &py_str(py, "target_page_index"))?)?;
        let fit_type = get_item(&destination, &py_str(py, "fit"))?;
        let fit_args = get_item(
            &builtins(py)?
                .getattr("dict")?
                .call1((destination.clone(),))?,
            &py_str(py, "fit_args"),
        )?;
        let pages = get(&slf, "pages")?;
        let page_count = py_len(&pages)? as i64;
        let target_page_reference = if 0 <= target_page_index && target_page_index < page_count {
            let page = get_item(&pages, &py_int(py, target_page_index))?;
            get(&page, "indirect_reference")?
        } else {
            number_obj(py, target_page_index)?
        };
        let kwargs = PyDict::new(py);
        kwargs.set_item("fit_type", fit_type)?;
        kwargs.set_item("fit_args", fit_args)?;
        let fit = gclass(py, "Fit")?.call((), Some(&kwargs))?;
        let destination_object = gclass(py, "Destination")?.call1((
            name_obj(py, "/LinkName")?,
            target_page_reference,
            fit,
        ))?;
        let dest_key = name_obj(py, "/Dest")?;
        let dest_array = get(&destination_object, "dest_array")?;
        set_item(&to_add, &dest_key, &dest_array)?;
    }
    let annotations = get(&page, "annotations")?;
    let reference = call1(&slf, "_add_object", to_add.clone())?;
    call1(&annotations, "append", reference)?;
    if eq(
        &gget(&to_add, &py_str(py, "/Subtype"), &none(py))?,
        &py_str(py, "/Popup"),
    )? && to_add.contains(&py_str(py, "/Parent"))?
    {
        let parent = call0(&get_item_str(&to_add, "/Parent")?, "get_object")?;
        let key = name_obj(py, "/Popup")?;
        let reference = get(&to_add, "indirect_reference")?;
        set_item(&parent, &key, &reference)?;
    }
    Ok(to_add)
}

// @pypdf: pypdf._writer.PdfWriter.close
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_close<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let _ = args;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.find_outline_item
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_find_outline_item<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let outline_item = arg(args, 1)?;
    let root = arg(args, 2)?;
    let visited = arg(args, 3)?;
    writer_find_outline_item(py, &slf, &outline_item, &root, &visited)
}

fn writer_find_outline_item<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    outline_item: &Bound<'py, PyAny>,
    root: &Bound<'py, PyAny>,
    visited: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut outline = if is_none(root) {
        call0(slf, "get_outline_root")?
    } else {
        root.clone()
    };
    let visited = if is_none(visited) {
        PySet::empty(py)?.into_any()
    } else {
        visited.clone()
    };
    let mut index = 0i64;
    while !is_none(&outline) {
        if eq(&get(&outline, "indirect_reference")?, outline_item)?
            || eq(
                &gget(&outline, &py_str(py, "/Title"), &none(py))?,
                outline_item,
            )?
        {
            return Ok(array_obj(py, vec![py_int(py, index)])?);
        }
        let outline_id = py_id(py, &outline)?;
        if visited.contains(&outline_id)? {
            return Err(errors::limit_reached_error(
                py,
                "Detected cyclic outline hierarchy.",
            ));
        }
        call1(&visited, "add", outline_id)?;
        let first_key = py_str(py, "/First");
        if outline.contains(&first_key)? {
            let result = writer_find_outline_item(
                py,
                slf,
                outline_item,
                &get_item(&outline, &first_key)?,
                &visited,
            )?;
            if truthy(&result)? {
                let mut items_result: Vec<Bound<'py, PyAny>> = Vec::new();
                if outline.contains(&py_str(py, "/Title"))? {
                    items_result.push(py_int(py, index));
                }
                for item in items(&result)? {
                    items_result.push(item);
                }
                return Ok(array_obj(py, items_result)?);
            }
        }
        let next_key = py_str(py, "/Next");
        if outline.contains(&next_key)? {
            index += 1;
            outline = get_item(&outline, &next_key)?;
        } else {
            return Ok(none(py));
        }
    }
    Err(errors::raise_msg(
        py,
        "PyPdfError",
        "This line is theoretically unreachable.",
    ))
}

// @pypdf: pypdf._writer.PdfWriter.reset_translation
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_reset_translation<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let reader = arg(args, 1)?;
    let reader_class = py.import("pypdf._reader")?.getattr("PdfReader")?;
    if is_none(&reader) {
        slf.setattr("_id_translated", PyDict::new(py))?;
    } else if is_instance_of(&reader, &reader_class)? {
        let attempted: PyResult<()> = (|| {
            let translated = get(&slf, "_id_translated")?;
            translated.del_item(py_id(py, &reader)?)?;
            Ok(())
        })();
        match attempted {
            Ok(()) => {}
            Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
            Err(_) => {}
        }
    } else if is_inst(&reader, "IndirectObject")? {
        let attempted: PyResult<()> = (|| {
            let translated = get(&slf, "_id_translated")?;
            let pdf = get(&reader, "pdf")?;
            translated.del_item(py_id(py, &pdf)?)?;
            Ok(())
        })();
        match attempted {
            Ok(()) => {}
            Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
            Err(_) => {}
        }
    } else {
        return Err(type_error(
            py,
            &format!("Invalid parameter {}", to_string(&reader)?),
        ));
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._repr_mimebundle_
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_repr_mimebundle_<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let include = arg(args, 1)?;
    let exclude = arg(args, 2)?;
    let buffer = py.import("io")?.getattr("BytesIO")?.call0()?;
    call1(&slf, "write", buffer.clone())?;
    let data = PyDict::new(py);
    data.set_item("application/pdf", buffer)?;
    let mut include_filtered = PyDict::new(py);
    if !is_none(&include) {
        for (key, value) in data.iter() {
            if include.contains(&key)? {
                include_filtered.set_item(key, value)?;
            }
        }
    } else {
        include_filtered = data;
    }
    let mut result = PyDict::new(py);
    if !is_none(&exclude) {
        for (key, value) in include_filtered.iter() {
            if !exclude.contains(&key)? {
                result.set_item(key, value)?;
            }
        }
    } else {
        result = include_filtered;
    }
    Ok(result.into_any())
}

// @pypdf: pypdf._writer._pdf_objectify
fn writer_pdf_objectify<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    if crate::base_mod::is_pdf_object_like(py, obj)? {
        return Ok(obj.clone());
    }
    if is_inst_builtin(obj, "dict")? {
        let to_add = new_g(py, "DictionaryObject", vec![])?;
        for pair in items(&obj.call_method0("items")?)? {
            let key = pair.get_item(0)?;
            let value = pair.get_item(1)?;
            let name = name_obj(py, &to_string(&key)?)?;
            let converted = writer_pdf_objectify(py, &value)?;
            set_item(&to_add, &name, &converted)?;
        }
        return Ok(to_add);
    }
    if is_inst_builtin(obj, "str")? {
        let text = to_string(obj)?;
        if text.starts_with('/') {
            return name_obj(py, &text);
        }
        return text_obj(py, &text);
    }
    if is_inst_builtin(obj, "float")? || is_inst_builtin(obj, "int")? {
        return new_g(py, "FloatObject", vec![obj.clone()]);
    }
    if is_inst_builtin(obj, "list")? {
        let mut values: Vec<Bound<'py, PyAny>> = Vec::new();
        for item in items(obj)? {
            values.push(writer_pdf_objectify(py, &item)?);
        }
        return array_obj(py, values);
    }
    let kind = to_string(&builtins(py)?.getattr("type")?.call1((obj.clone(),))?)?;
    Err(PyNotImplementedError::new_err(format!(
        "type(obj)={kind} could not be cast to a PdfObject"
    )))
}

// @pypdf: pypdf._writer._pdf_objectify
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_private_pdf_objectify<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let obj = arg(args, 0)?;
    writer_pdf_objectify(py, &obj)
}

// @pypdf: pypdf._writer._create_outline_item
fn writer_create_outline_item<'py>(
    py: Python<'py>,
    action_ref: &Bound<'py, PyAny>,
    title: &Bound<'py, PyAny>,
    color: &Bound<'py, PyAny>,
    italic: &Bound<'py, PyAny>,
    bold: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let outline_item = gclass(py, "TreeObject")?.call0()?;
    if !is_none(action_ref) {
        let key = name_obj(py, "/A")?;
        set_item(&outline_item, &key, action_ref)?;
    }
    let update = pydict_obj(
        py,
        vec![(
            name_obj(py, "/Title")?,
            py.import("pypdf.generic")?
                .getattr("create_string_object")?
                .call1((title.clone(),))?,
        )],
    )?;
    call1(&outline_item, "update", update)?;
    if truthy(color)? {
        let color = if is_inst_builtin(color, "str")? {
            py.import("pypdf.generic")?
                .getattr("hex_to_rgb")?
                .call1((color.clone(),))?
        } else {
            color.clone()
        };
        let mut values: Vec<Bound<'py, PyAny>> = Vec::new();
        for component in items(&color)? {
            values.push(new_g(py, "FloatObject", vec![component])?);
        }
        let update = pydict_obj(py, vec![(name_obj(py, "/C")?, array_obj(py, values)?)])?;
        call1(&outline_item, "update", update)?;
    }
    if truthy(italic)? || truthy(bold)? {
        let flag = py.import("pypdf.constants")?.getattr("OutlineFontFlag")?;
        let mut format_flag = py_int(py, 0);
        if truthy(italic)? {
            format_flag = py_or(&format_flag, &flag.getattr("italic")?)?;
        }
        if truthy(bold)? {
            format_flag = py_or(&format_flag, &flag.getattr("bold")?)?;
        }
        let number = number_obj(py, to_i64(&format_flag)?)?;
        let update = pydict_obj(py, vec![(name_obj(py, "/F")?, number)])?;
        call1(&outline_item, "update", update)?;
    }
    Ok(outline_item)
}

// @pypdf: pypdf._writer._create_outline_item
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_private_create_outline_item<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let action_ref = arg(args, 0)?;
    let title = arg(args, 1)?;
    let color = arg(args, 2)?;
    let italic = arg(args, 3)?;
    let bold = arg(args, 4)?;
    writer_create_outline_item(py, &action_ref, &title, &color, &italic, &bold)
}

// ===========================================================================
// append / merge
// ===========================================================================

// @pypdf: pypdf._writer.PdfWriter.append
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_append<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let fileobj = arg(args, 1)?;
    let outline_item = arg(args, 2)?;
    let mut pages = arg(args, 3)?;
    let mut import_outline = arg(args, 4)?;
    let mut excluded_fields = arg(args, 5)?;
    if is_none(&excluded_fields) {
        excluded_fields = PyTuple::empty(py).into_any();
    }
    let is_page_range = is_instance_of(
        &outline_item,
        &py.import("pypdf.pagerange")?.getattr("PageRange")?,
    )?;
    if is_inst_builtin(&outline_item, "tuple")?
        || is_inst_builtin(&outline_item, "list")?
        || is_page_range
    {
        if is_inst_builtin(&pages, "bool")? {
            if !is_inst_builtin(&import_outline, "bool")? {
                excluded_fields = import_outline.clone();
            }
            import_outline = pages.clone();
        }
        pages = outline_item.clone();
        callv(
            &slf,
            "merge",
            vec![
                none(py),
                fileobj,
                none(py),
                pages,
                import_outline,
                excluded_fields,
            ],
        )?;
    } else {
        callv(
            &slf,
            "merge",
            vec![
                none(py),
                fileobj,
                outline_item,
                pages,
                import_outline,
                excluded_fields,
            ],
        )?;
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter.merge
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_merge<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut position = arg(args, 1)?;
    let fileobj = arg(args, 2)?;
    let outline_item = arg(args, 3)?;
    let mut pages = arg(args, 4)?;
    let import_outline = arg(args, 5)?;
    let mut excluded_fields = arg(args, 6)?;
    let doc_common = py.import("pypdf._doc_common")?.getattr("PdfDocCommon")?;
    let reader = if is_instance_of(&fileobj, &doc_common)? {
        fileobj.clone()
    } else {
        let stream = call1(&slf, "_create_stream", fileobj.clone())?;
        let reader_class = py.import("pypdf._reader")?.getattr("PdfReader")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("strict", false)?;
        reader_class.call((stream,), Some(&kwargs))?
    };
    if is_none(&excluded_fields) {
        excluded_fields = PyTuple::empty(py).into_any();
    }
    let reader_pages = get(&reader, "pages")?;
    // Find the range of pages to merge.
    if is_none(&pages) {
        pages = array_obj(
            py,
            (0..py_len(&reader_pages)?)
                .map(|index| py_int(py, index as i64))
                .collect(),
        )?;
    } else if is_instance_of(&pages, &py.import("pypdf.pagerange")?.getattr("PageRange")?)? {
        let indices = call1(&pages, "indices", py_int(py, py_len(&reader_pages)? as i64))?;
        let range_module = builtins(py)?.getattr("range")?;
        let range_object = range_module.call1(indices.cast_into::<PyTuple>()?)?;
        pages = builtins(py)?.getattr("list")?.call1((range_object,))?;
    } else if is_inst_builtin(&pages, "list")? {
        // keep unchanged
    } else if is_inst_builtin(&pages, "tuple")? {
        if py_len(&pages)? <= 3 {
            let range_module = builtins(py)?.getattr("range")?;
            let range_object = range_module.call1(pages.cast_into::<PyTuple>()?)?;
            pages = builtins(py)?.getattr("list")?.call1((range_object,))?;
        } else {
            return Err(type_error(
                py,
                "\"pages\" must be a tuple of (start, stop[, step]) or a list",
            ));
        }
    } else {
        return Err(type_error(
            py,
            "\"pages\" must be a tuple of (start, stop[, step]) or a list",
        ));
    }
    let srcpages = PyDict::new(py);
    let mut excluded_list_items: Vec<Bound<'py, PyAny>> = items(&excluded_fields)?;
    excluded_list_items.push(py_int(py, 1));
    excluded_list_items.push(py_str(py, "/B"));
    excluded_list_items.push(py_int(py, 1));
    excluded_list_items.push(py_str(py, "/Annots"));
    let page_class = py.import("pypdf._page")?.getattr("PageObject")?;
    for page in items(&pages)? {
        let pg = if is_instance_of(&page, &page_class)? {
            page.clone()
        } else {
            let index = to_i64(&page)?;
            get_item(&reader_pages, &py_int(py, index))?
        };
        let reference = get(&pg, "indirect_reference")?;
        if is_none(&reference) {
            return Err(pyo3::exceptions::PyAssertionError::new_err(""));
        }
        let idnum = get(&reference, "idnum")?;
        let excluded = array_obj(py, excluded_list_items.clone())?;
        let new_page = if is_none(&position) {
            callv(&slf, "add_page", vec![pg.clone(), excluded])?
        } else {
            let inserted = callv(
                &slf,
                "insert_page",
                vec![pg.clone(), position.clone(), excluded],
            )?;
            position = call1(&position, "__add__", py_int(py, 1))?;
            inserted
        };
        new_page.setattr("original_page", pg.clone())?;
        srcpages.set_item(idnum, new_page)?;
    }
    let reader_named_destinations = get(&reader, "named_destinations")?;
    reader.setattr("_named_destinations", reader_named_destinations.clone())?;
    let writer_module = py.import("pypdf._writer")?;
    let index_minimum = to_usize(&writer_module.getattr("_NAMED_DESTINATION_INDEX_MINIMUM")?)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item(
        "is_enabled",
        py_len(&reader_named_destinations)? >= index_minimum,
    )?;
    let named_destinations = writer_module
        .getattr("_NamedDestinationIndex")?
        .call((), Some(&kwargs))?;
    let process = (|| -> PyResult<()> {
        for dest in items(&reader_named_destinations.call_method0("values")?)? {
            writer_merge_process_named_dests(
                py,
                &slf,
                &dest,
                &reader,
                &srcpages,
                &named_destinations,
            )?;
        }
        Ok(())
    })();
    call0(&named_destinations, "flush")?;
    process?;
    let outline_item_typ = if !is_none(&outline_item) {
        let first = items(&srcpages.call_method0("values")?)?.into_iter().next();
        let page_number = match first {
            Some(value) => get(&value, "indirect_reference")?,
            None => none(py),
        };
        let added = callkw(
            &slf,
            "add_outline_item",
            vec![],
            vec![
                ("title", text_obj(py, &to_string(&outline_item)?)?),
                ("page_number", page_number),
                ("fit", py.import("pypdf.generic")?.getattr("PAGE_FIT")?),
            ],
        )?;
        call0(&added, "get_object")?
    } else {
        call0(&slf, "get_outline_root")?
    };
    let root = get(&reader, "root_object")?;
    let outlines_key = const_str(py, "Core", "OUTLINES")?;
    if truthy(&import_outline)? && root.contains(&outlines_key)? {
        let outline = writer_get_filtered_outline(
            py,
            &slf,
            &gget(&root, &outlines_key, &none(py))?,
            &srcpages,
            &reader,
            &none(py),
        )?;
        writer_insert_filtered_outline(py, &slf, &outline, &outline_item_typ, &none(py))?;
    }
    let annots_key = py_str(py, "/Annots");
    let excluded_annots = items(&excluded_fields)?
        .iter()
        .any(|item| item.eq(&annots_key).unwrap_or(false));
    if !excluded_annots {
        let parent_fields = PyDict::new(py);
        for page in items(&srcpages.call_method0("values")?)? {
            let original_page = get(&page, "original_page")?;
            let annots = gget(&original_page, &annots_key, &array_obj(py, vec![])?)?;
            let lst = writer_insert_filtered_annotations(
                py,
                &slf,
                &annots,
                &page,
                &srcpages,
                &reader,
                Some(parent_fields.clone()),
            )?;
            if py_len(&lst)? > 0 {
                let key = name_obj(py, "/Annots")?;
                set_item(&page, &key, &lst)?;
            }
            call1(&slf, "clean_page", page.clone())?;
        }
        writer_set_cloned_kids(py, &slf, &parent_fields, &reader)?;
    }
    let acro_key = py_str(py, "/AcroForm");
    let mut acro_form = gget(&root, &acro_key, &none(py))?;
    if !is_none(&acro_form) {
        acro_form = call0(&acro_form, "get_object")?;
        let is_null_or_none = generic(py)?.getattr("is_null_or_none")?;
        if !truthy(&is_null_or_none.call1((acro_form.clone(),))?)?
            && !is_inst(&acro_form, "DictionaryObject")?
        {
            let values = PyDict::new(py);
            values.set_item("acro_form", acro_form.clone())?;
            logger_warning(
                py,
                "AcroForm is not a dictionary: %(acro_form)s",
                Some(&values),
            )?;
        }
    }
    if is_inst(&acro_form, "DictionaryObject")? {
        let writer_root = get(&slf, "_root_object")?;
        let writer_acro_form = gget(&writer_root, &acro_key, &none(py))?;
        let mut arr;
        if is_none(&writer_acro_form)
            || !is_inst(&call0(&writer_acro_form, "get_object")?, "DictionaryObject")?
        {
            let ignored = PyTuple::new(py, [py_str(py, "/Fields")])?;
            let cloned = callv(
                &acro_form,
                "clone",
                vec![slf.clone(), py_bool(py, false), ignored.into_any()],
            )?;
            let added = call1(&slf, "_add_object", cloned)?;
            let key = name_obj(py, "/AcroForm")?;
            set_item(&writer_root, &key, &added)?;
            arr = array_obj(py, vec![])?;
        } else {
            let inner = call0(&writer_acro_form, "get_object")?;
            arr = call0(
                &gget(&inner, &py_str(py, "/Fields"), &array_obj(py, vec![])?)?,
                "get_object",
            )?;
            if !is_inst(&arr, "ArrayObject")? {
                arr = array_obj(py, vec![])?;
            }
        }
        let mut fields = call0(
            &gget(&acro_form, &py_str(py, "/Fields"), &array_obj(py, vec![])?)?,
            "get_object",
        )?;
        if !is_inst(&fields, "ArrayObject")? {
            let values = PyDict::new(py);
            values.set_item("fields", fields.clone())?;
            logger_warning(
                py,
                "AcroForm /Fields is not an array: %(fields)s",
                Some(&values),
            )?;
            fields = array_obj(py, vec![])?;
        }
        let translated = get(&slf, "_id_translated")?;
        let trslat = get_item(&translated, &py_id(py, &reader)?)?;
        for field in items(&fields)? {
            if is_inst(&field, "IndirectObject")? {
                let idnum = get(&field, "idnum")?;
                if trslat.contains(&idnum)? {
                    let mapped = get_item(&trslat, &idnum)?;
                    let ind = indirect_obj(py, to_usize(&mapped)?, 0, &slf)?;
                    if !arr.contains(&ind)? {
                        call1(&arr, "append", ind)?;
                    }
                }
            }
        }
        let root_acrof = get_item(&get(&slf, "_root_object")?, &name_obj(py, "/AcroForm")?)?;
        set_item(&root_acrof, &name_obj(py, "/Fields")?, &arr)?;
    }
    let b_key = py_str(py, "/B");
    let excluded_b = items(&excluded_fields)?
        .iter()
        .any(|item| item.eq(&b_key).unwrap_or(false));
    if !excluded_b {
        callv(
            &slf,
            "add_filtered_articles",
            vec![py_str(py, ""), srcpages.into_any(), reader.clone()],
        )?;
    }
    Ok(none(py))
}

fn writer_merge_process_named_dests<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    dest: &Bound<'py, PyAny>,
    reader: &Bound<'py, PyAny>,
    srcpages: &Bound<'py, PyDict>,
    named_destinations: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let arr = get(dest, "dest_array")?;
    let root = get(slf, "_root_object")?;
    let names_key = py_str(py, "/Names");
    let mut already = false;
    if root.contains(&names_key)? {
        let root_names = get_item(&root, &names_key)?;
        let dests = gget(
            &root_names,
            &py_str(py, "/Dests"),
            &new_g(py, "DictionaryObject", vec![])?,
        )?;
        let names_array = gget(
            &dests,
            &py_str(py, "/Names"),
            &new_g(py, "DictionaryObject", vec![])?,
        )?;
        let title = get_item_str(dest, "/Title")?;
        already = truthy(&callv(
            named_destinations,
            "contains",
            vec![names_array, title],
        )?)?;
    }
    let page = get_item_str(dest, "/Page")?;
    if already {
        return Ok(());
    }
    let is_null_or_none = generic(py)?.getattr("is_null_or_none")?;
    if is_none(&page) || truthy(&is_null_or_none.call1((page.clone(),))?)? {
        return Ok(());
    }
    if is_inst_builtin(&page, "int")? {
        let reader_pages = get(reader, "pages")?;
        let index = to_i64(&page)?;
        let p = match get_item(&reader_pages, &py_int(py, index)) {
            Ok(value) => value,
            Err(error) if error.is_instance_of::<PyIndexError>(py) => return Ok(()),
            Err(error) => return Err(error),
        };
        let reference = get(&p, "indirect_reference")?;
        let idnum = get(&reference, "idnum")?;
        if srcpages.contains(&idnum)? {
            let target = get_item(srcpages, &idnum)?;
            let page_number = get(&target, "page_number")?;
            arr.set_item(py_int(py, 0), number_obj(py, to_i64(&page_number)?)?)?;
            let title = get_item_str(dest, "/Title")?;
            writer_add_named_destination_array_indexed(py, slf, &title, &arr, named_destinations)?;
        } else {
            return Ok(());
        }
        return Ok(());
    }
    let page_reference = get(&page, "indirect_reference")?;
    let idnum = get(&page_reference, "idnum")?;
    if srcpages.contains(&idnum)? {
        let target = get_item(srcpages, &idnum)?;
        arr.set_item(py_int(py, 0), get(&target, "indirect_reference")?)?;
        let title = get_item_str(dest, "/Title")?;
        writer_add_named_destination_array_indexed(py, slf, &title, &arr, named_destinations)?;
    }
    Ok(())
}

fn writer_add_named_destination_array_indexed<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    title: &Bound<'py, PyAny>,
    destination: &Bound<'py, PyAny>,
    named_destinations: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let names = call0(slf, "get_named_dest_root")?;
    let inserted = callv(
        named_destinations,
        "insert",
        vec![names.clone(), title.clone(), destination.clone()],
    )?;
    if !truthy(&inserted)? {
        callv(
            slf,
            "add_named_destination_array",
            vec![title.clone(), destination.clone()],
        )?;
        callv(
            named_destinations,
            "record_insertion",
            vec![names, title.clone()],
        )?;
    }
    let _ = py;
    Ok(())
}

// @pypdf: pypdf._writer.PdfWriter._merge__process_named_dests
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_merge_process_named_dests<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let dest = arg(args, 1)?;
    let reader = arg(args, 2)?;
    let srcpages = arg(args, 3)?;
    let srcpages = srcpages.cast_into::<PyDict>()?;
    let named_destinations = arg(args, 4)?;
    writer_merge_process_named_dests(py, &slf, &dest, &reader, &srcpages, &named_destinations)?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._add_named_destination_array_indexed
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_add_named_destination_array_indexed<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let title = arg(args, 1)?;
    let destination = arg(args, 2)?;
    let named_destinations = arg(args, 3)?;
    writer_add_named_destination_array_indexed(
        py,
        &slf,
        &title,
        &destination,
        &named_destinations,
    )?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._get_cloned_page
fn writer_get_cloned_page<'py>(
    py: Python<'py>,
    page: &Bound<'py, PyAny>,
    pages: &Bound<'py, PyDict>,
    reader: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut page = page.clone();
    if is_inst(&page, "NullObject")? {
        return Ok(none(py));
    }
    if is_inst_builtin(&page, "int")? {
        let reader_pages = get(reader, "pages")?;
        page = match get_item(&reader_pages, &page) {
            Ok(value) => get(&value, "indirect_reference")?,
            Err(error) if error.is_instance_of::<PyIndexError>(py) => return Ok(none(py)),
            Err(error) => return Err(error),
        };
    }
    let reference;
    if is_inst(&page, "DictionaryObject")?
        && eq(
            &gget(&page, &py_str(py, "/Type"), &py_str(py, ""))?,
            &py_str(py, "/Page"),
        )?
    {
        reference = get(&page, "indirect_reference")?;
    } else if is_inst(&page, "IndirectObject")? {
        reference = page.clone();
    } else {
        // Unresolvable reference: fall through to the KeyError handling below.
        reference = page.clone();
    }
    let idnum = match get(&reference, "idnum") {
        Ok(value) => value,
        Err(_) => return Ok(none(py)),
    };
    if !pages.contains(&idnum)? {
        return Ok(none(py));
    }
    let target = get_item(pages, &idnum)?;
    get(&target, "indirect_reference")
}

// @pypdf: pypdf._writer.PdfWriter._get_cloned_page
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_get_cloned_page<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let page = arg(args, 1)?;
    let pages = arg(args, 2)?;
    let pages = pages.cast_into::<PyDict>()?;
    let reader = arg(args, 3)?;
    writer_get_cloned_page(py, &page, &pages, &reader)
}

// @pypdf: pypdf._writer.PdfWriter._add_articles_thread
fn writer_add_articles_thread<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    thread: &Bound<'py, PyAny>,
    pages: &Bound<'py, PyDict>,
    reader: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let ignored = PyTuple::new(py, [py_str(py, "/F")])?;
    let new_thread = callv(
        thread,
        "clone",
        vec![slf.clone(), py_bool(py, true), ignored.into_any()],
    )?;
    let threads = call0(slf, "get_threads_root")?;
    call1(&threads, "append", get(&new_thread, "indirect_reference")?)?;
    let first_article = get_item_str(thread, "/F")?;
    let mut current_article = Some(first_article.clone());
    let mut new_article: Option<Bound<'py, PyAny>> = None;
    let mut new_first: Option<Bound<'py, PyAny>> = None;
    let visited = PySet::empty(py)?;
    while let Some(article) = current_article.clone() {
        let article_id = py_id(py, &article)?;
        if visited.contains(&article_id)? {
            return Err(errors::limit_reached_error(
                py,
                "Detected cyclic article structure.",
            ));
        }
        call1(&visited, "add", article_id)?;
        let page_value = get_item_str(&article, "/P")?;
        let page = writer_get_cloned_page(py, &page_value, pages, reader)?;
        if !is_none(&page) {
            if new_article.is_none() {
                let created = call1(&slf, "_add_object", new_g(py, "DictionaryObject", vec![])?)?;
                let created = call0(&created, "get_object")?;
                new_first = Some(created.clone());
                new_article = Some(created.clone());
                set_item(
                    &new_thread,
                    &name_obj(py, "/F")?,
                    &get(&created, "indirect_reference")?,
                )?;
            } else {
                let previous = new_article.clone().unwrap_or_else(|| none(py));
                let created = call1(
                    &slf,
                    "_add_object",
                    dict_obj(
                        py,
                        vec![(name_obj(py, "/V")?, get(&previous, "indirect_reference")?)],
                    )?,
                )?;
                let created = call0(&created, "get_object")?;
                set_item(
                    &previous,
                    &name_obj(py, "/N")?,
                    &get(&created, "indirect_reference")?,
                )?;
                new_article = Some(created);
            }
            let article_object = new_article.clone().unwrap_or_else(|| none(py));
            set_item(&article_object, &name_obj(py, "/P")?, &page)?;
            set_item(
                &article_object,
                &name_obj(py, "/T")?,
                &get(&new_thread, "indirect_reference")?,
            )?;
            set_item(
                &article_object,
                &name_obj(py, "/R")?,
                &get_item_str(&article, "/R")?,
            )?;
            let page_object = call0(&page, "get_object")?;
            let b_key = name_obj(py, "/B")?;
            if !page_object.contains(&b_key)? {
                let array = array_obj(py, vec![])?;
                set_item(&page_object, &b_key, &array)?;
            }
            call1(
                &get_item(&page_object, &b_key)?,
                "append",
                get(&article_object, "indirect_reference")?,
            )?;
        }
        let next = get_item_str(&article, "/N")?;
        if eq(&next, &first_article)? {
            let article_object = new_article.clone().unwrap_or_else(|| none(py));
            let first_new = new_first.clone().unwrap_or_else(|| none(py));
            set_item(
                &article_object,
                &name_obj(py, "/N")?,
                &get(&first_new, "indirect_reference")?,
            )?;
            set_item(
                &first_new,
                &name_obj(py, "/V")?,
                &get(&article_object, "indirect_reference")?,
            )?;
            current_article = None;
        } else {
            current_article = Some(next);
        }
    }
    get(&new_thread, "indirect_reference")
}

// @pypdf: pypdf._writer.PdfWriter._add_articles_thread
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_add_articles_thread<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let thread = arg(args, 1)?;
    let pages = arg(args, 2)?;
    let pages = pages.cast_into::<PyDict>()?;
    let reader = arg(args, 3)?;
    writer_add_articles_thread(py, &slf, &thread, &pages, &reader)
}

// @pypdf: pypdf._writer.PdfWriter.add_filtered_articles
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_add_filtered_articles<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let fltr = arg(args, 1)?;
    let pages = arg(args, 2)?;
    let pages = pages.cast_into::<PyDict>()?;
    let reader = arg(args, 3)?;
    let re_module = py.import("re")?;
    let pattern_class = re_module.getattr("Pattern")?;
    let fltr = if is_inst_builtin(&fltr, "str")? {
        re_module.getattr("compile")?.call1((fltr,))?
    } else if !is_instance_of(&fltr, &pattern_class)? {
        re_module.getattr("compile")?.call1(("",))?
    } else {
        fltr
    };
    for page in items(&pages.call_method0("values")?)? {
        let original_page = get(&page, "original_page")?;
        let articles = gget(
            &original_page,
            &py_str(py, "/B"),
            &PyTuple::empty(py).into_any(),
        )?;
        for article in items(&articles)? {
            let article_object = call0(&article, "get_object")?;
            let is_null_or_none = generic(py)?.getattr("is_null_or_none")?;
            if truthy(&is_null_or_none.call1((article_object.clone(),))?)? {
                continue;
            }
            let thread = gget(&article_object, &py_str(py, "/T"), &none(py))?;
            if is_none(&thread) {
                continue;
            }
            let thread = call0(&thread, "get_object")?;
            let thread_reference = get(&thread, "indirect_reference")?;
            let translated = get(&slf, "_id_translated")?;
            let trslat = get_item(&translated, &py_id(py, &reader)?)?;
            let thread_idnum = get(&thread_reference, "idnum")?;
            let info = gget(&thread, &py_str(py, "/I"), &PyDict::new(py).into_any())?;
            let title = gget(&info, &py_str(py, "/Title"), &py_str(py, ""))?;
            if !trslat.contains(&thread_idnum)? && truthy(&call1(&fltr, "search", title)?)? {
                writer_add_articles_thread(py, &slf, &thread, &pages, &reader)?;
            }
        }
    }
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._insert_filtered_annotations
fn writer_insert_filtered_annotations<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    annots: &Bound<'py, PyAny>,
    page: &Bound<'py, PyAny>,
    pages: &Bound<'py, PyDict>,
    reader: &Bound<'py, PyAny>,
    parent_fields: Option<Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyAny>> {
    let outlist = array_obj(py, vec![])?;
    let mut annots = annots.clone();
    if is_inst(&annots, "IndirectObject")? {
        annots = call0(&annots, "get_object")?;
    }
    if is_none(&annots) {
        return Ok(outlist);
    }
    if !is_inst_builtin(&annots, "list")? {
        let values = PyDict::new(py);
        values.set_item("annots", annots.clone())?;
        values.set_item(
            "annots_type",
            get(
                &builtins(py)?.getattr("type")?.call1((annots.clone(),))?,
                "__name__",
            )?,
        )?;
        logger_warning(
            py,
            "Expected list of annotations, got %(annots)s of type %(annots_type)s.",
            Some(&values),
        )?;
        return Ok(outlist);
    }
    let _ = page;
    for annotated in items(&annots)? {
        let ano = call0(&annotated, "get_object")?;
        let subtype = gget(&ano, &py_str(py, "/Subtype"), &none(py))?;
        let action_object = gget(&ano, &py_str(py, "/A"), &none(py))?;
        let not_link = !eq(&subtype, &py_str(py, "/Link"))?
            || is_none(&action_object)
            || !eq(&get_item_str(&action_object, "/S")?, &py_str(py, "/GoTo"))?
            || ano.contains(&py_str(py, "/Dest"))?;
        if not_link {
            // upstream uses ``ano.get("/Parent")`` (raw dict access)
            let parent = gget(&ano, &py_str(py, "/Parent"), &none(py))?;
            if parent_fields.is_some()
                && eq(&subtype, &py_str(py, "/Widget"))?
                && is_inst(&parent, "IndirectObject")?
            {
                let cloned = writer_clone_widget_annotation(
                    py,
                    &slf,
                    &ano,
                    parent_fields.as_ref().unwrap(),
                )?;
                call1(&outlist, "append", cloned)?;
            } else if !ano.contains(&py_str(py, "/Dest"))? {
                let cloned = call1(&ano, "clone", slf.clone())?;
                let reference = call1(&slf, "_add_object", cloned)?;
                call1(&outlist, "append", reference)?;
            } else {
                let dest = get_item_str(&ano, "/Dest")?;
                if is_inst_builtin(&dest, "str")? {
                    let named = call0(slf, "get_named_dest_root")?;
                    if named.contains(&py_str(py, &to_string(&dest)?))? {
                        let cloned = call1(&ano, "clone", slf.clone())?;
                        call1(&outlist, "append", get(&cloned, "indirect_reference")?)?;
                    }
                } else {
                    if !is_inst(&dest, "ArrayObject")? || py_len(&dest)? == 0 {
                        continue;
                    }
                    let first = get_item(&dest, &py_int(py, 0))?;
                    let cloned_page = writer_get_cloned_page(py, &first, pages, reader)?;
                    if !is_none(&cloned_page) {
                        let ignored = PyTuple::new(py, [py_str(py, "/Dest")])?;
                        let anc = callv(
                            &ano,
                            "clone",
                            vec![slf.clone(), py_bool(py, false), ignored.into_any()],
                        )?;
                        let mut array_items = vec![cloned_page];
                        for rest in items(&dest)?.into_iter().skip(1) {
                            array_items.push(rest);
                        }
                        set_item(&anc, &name_obj(py, "/Dest")?, &array_obj(py, array_items)?)?;
                        let reference = call1(&slf, "_add_object", anc)?;
                        call1(&outlist, "append", reference)?;
                    }
                }
            }
        } else {
            let dest = gget(
                &call0(&action_object, "get_object")?,
                &py_str(py, "/D"),
                &null_obj(py)?,
            )?;
            let is_null_or_none = generic(py)?.getattr("is_null_or_none")?;
            if truthy(&is_null_or_none.call1((dest.clone(),))?)? {
                continue;
            }
            if is_inst_builtin(&dest, "str")? {
                let named = call0(slf, "get_named_dest_root")?;
                if named.contains(&py_str(py, &to_string(&dest)?))? {
                    let cloned = call1(&ano, "clone", slf.clone())?;
                    call1(&outlist, "append", get(&cloned, "indirect_reference")?)?;
                }
            } else {
                if !is_inst(&dest, "ArrayObject")? || py_len(&dest)? == 0 {
                    continue;
                }
                let first = get_item(&dest, &py_int(py, 0))?;
                let cloned_page = writer_get_cloned_page(py, &first, pages, reader)?;
                if !is_none(&cloned_page) {
                    let ignored = PyTuple::new(py, [py_str(py, "/D")])?;
                    let anc = callv(
                        &ano,
                        "clone",
                        vec![slf.clone(), py_bool(py, false), ignored.into_any()],
                    )?;
                    let mut array_items = vec![cloned_page];
                    for rest in items(&dest)?.into_iter().skip(1) {
                        array_items.push(rest);
                    }
                    let action = get_item_str(&anc, "/A")?;
                    set_item(&action, &name_obj(py, "/D")?, &array_obj(py, array_items)?)?;
                    let reference = call1(&slf, "_add_object", anc)?;
                    call1(&outlist, "append", reference)?;
                }
            }
        }
    }
    Ok(outlist)
}

// @pypdf: pypdf._writer.PdfWriter._insert_filtered_annotations
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_insert_filtered_annotations<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let annots = arg(args, 1)?;
    let page = arg(args, 2)?;
    let pages = arg(args, 3)?;
    let pages = pages.cast_into::<PyDict>()?;
    let reader = arg(args, 4)?;
    let parent_fields = arg(args, 5)?;
    let parent_fields = if is_none(&parent_fields) {
        None
    } else {
        Some(parent_fields.cast_into::<PyDict>()?)
    };
    writer_insert_filtered_annotations(py, &slf, &annots, &page, &pages, &reader, parent_fields)
}

// @pypdf: pypdf._writer.PdfWriter._clone_widget_annotation
fn writer_clone_widget_annotation<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    annotation: &Bound<'py, PyAny>,
    parent_fields: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyAny>> {
    let ignored = PyTuple::new(py, [py_int(py, 1), py_str(py, "/Parent")])?;
    let clone = callv(
        annotation,
        "clone",
        vec![slf.clone(), py_bool(py, false), ignored.into_any()],
    )?;
    let mut source = annotation.clone();
    let mut target = clone.clone();
    loop {
        let parent = gget(&source, &py_str(py, "/Parent"), &none(py))?;
        if !is_inst(&parent, "IndirectObject")? {
            break;
        }
        let source_parent = call0(&parent, "get_object")?;
        if !is_inst(&source_parent, "DictionaryObject")? {
            break;
        }
        let ignored = PyTuple::new(
            py,
            [
                py_int(py, 1),
                py_str(py, "/Kids"),
                py_int(py, 1),
                py_str(py, "/Parent"),
            ],
        )?;
        let parent_clone = callv(
            &source_parent,
            "clone",
            vec![slf.clone(), py_bool(py, false), ignored.into_any()],
        )?;
        set_item(
            &target,
            &name_obj(py, "/Parent")?,
            &get(&parent_clone, "indirect_reference")?,
        )?;
        let parent_idnum = get(&parent, "idnum")?;
        if parent_fields.contains(&parent_idnum)? {
            break;
        }
        parent_fields.set_item(&parent_idnum, source_parent.clone())?;
        source = source_parent;
        target = parent_clone;
    }
    let reference = get(&clone, "indirect_reference")?;
    if is_none(&reference) {
        return Err(pyo3::exceptions::PyAssertionError::new_err(""));
    }
    Ok(reference)
}

// @pypdf: pypdf._writer.PdfWriter._clone_widget_annotation
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_clone_widget_annotation<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let annotation = arg(args, 1)?;
    let parent_fields = arg(args, 2)?;
    let parent_fields = parent_fields.cast_into::<PyDict>()?;
    writer_clone_widget_annotation(py, &slf, &annotation, &parent_fields)
}

// @pypdf: pypdf._writer.PdfWriter._set_cloned_kids
fn writer_set_cloned_kids<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    parent_fields: &Bound<'py, PyDict>,
    reader: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let translated = get(&slf, "_id_translated")?;
    let translated = gget(
        &translated,
        &py_id(py, reader)?,
        &PyDict::new(py).into_any(),
    )?;
    for pair in parent_fields.iter() {
        let idnum = pair.0;
        let source_parent = pair.1;
        let kids = gget(
            &source_parent,
            &py_str(py, "/Kids"),
            &array_obj(py, vec![])?,
        )?;
        let mapped = get_item(&translated, &idnum)?;
        let parent = call1(slf, "get_object", mapped)?;
        let mut cloned_kids: Vec<Bound<'py, PyAny>> = Vec::new();
        for kid in items(&kids)? {
            if is_inst(&kid, "IndirectObject")? {
                let kid_idnum = get(&kid, "idnum")?;
                if translated.contains(&kid_idnum)? {
                    let target = get_item(&translated, &kid_idnum)?;
                    cloned_kids.push(indirect_obj(py, to_usize(&target)?, 0, slf)?);
                }
            }
        }
        set_item(
            &parent,
            &name_obj(py, "/Kids")?,
            &array_obj(py, cloned_kids)?,
        )?;
    }
    Ok(())
}

// @pypdf: pypdf._writer.PdfWriter._set_cloned_kids
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_set_cloned_kids<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let parent_fields = arg(args, 1)?;
    let parent_fields = parent_fields.cast_into::<PyDict>()?;
    let reader = arg(args, 2)?;
    writer_set_cloned_kids(py, &slf, &parent_fields, &reader)?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._get_filtered_outline
fn writer_get_filtered_outline<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    node: &Bound<'py, PyAny>,
    pages: &Bound<'py, PyDict>,
    reader: &Bound<'py, PyAny>,
    visited: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let visited = if is_none(visited) {
        PySet::empty(py)?.into_any()
    } else {
        visited.clone()
    };
    let new_outline = array_obj(py, vec![])?;
    if is_none(node) {
        return Ok(new_outline);
    }
    let mut node = call0(node, "get_object")?;
    let is_null_or_none = generic(py)?.getattr("is_null_or_none")?;
    if truthy(&is_null_or_none.call1((node.clone(),))?)? {
        node = new_g(py, "DictionaryObject", vec![])?;
    }
    if !is_inst(&node, "DictionaryObject")? {
        let values = PyDict::new(py);
        values.set_item("node", node.clone())?;
        logger_warning(
            py,
            "Outline node is not a dictionary: %(node)s",
            Some(&values),
        )?;
        return Ok(new_outline);
    }
    if eq(
        &gget(&node, &py_str(py, "/Type"), &py_str(py, ""))?,
        &py_str(py, "/Outlines"),
    )? || !node.contains(&py_str(py, "/Title"))?
    {
        let node_id = py_id(py, &node)?;
        if visited.contains(&node_id)? {
            logger_warning(py, "Detected cycle in outlines.", None)?;
            return Ok(array_obj(py, vec![])?);
        }
        call1(&visited, "add", node_id)?;
        let first = gget(&node, &py_str(py, "/First"), &none(py))?;
        if !is_none(&first) {
            let first = call0(&first, "get_object")?;
            let children = writer_get_filtered_outline(py, slf, &first, pages, reader, &visited)?;
            for child in items(&children)? {
                call1(&new_outline, "append", child)?;
            }
        }
    } else {
        loop {
            node = call0(&node, "get_object")?;
            if !is_inst(&node, "DictionaryObject")? {
                let values = PyDict::new(py);
                values.set_item("node", node.clone())?;
                logger_warning(
                    py,
                    "Outline node is not a dictionary: %(node)s",
                    Some(&values),
                )?;
                break;
            }
            let node_id = py_id(py, &node)?;
            if visited.contains(&node_id)? {
                logger_warning(py, "Detected cycle in outlines.", None)?;
                break;
            }
            call1(&visited, "add", node_id)?;
            let destination = call1(reader, "_build_outline_item", node.clone())?;
            let page_value = get_item_str(&destination, "/Page")?;
            let mut cloned_page = writer_get_cloned_page(py, &page_value, pages, reader)?;
            if is_none(&cloned_page) {
                cloned_page = null_obj(py)?;
            }
            set_item(&destination, &name_obj(py, "/Page")?, &cloned_page)?;
            let children = if node.contains(&py_str(py, "/First"))? {
                writer_get_filtered_outline(
                    py,
                    slf,
                    &get_item_str(&node, "/First")?,
                    pages,
                    reader,
                    &visited,
                )?
            } else {
                array_obj(py, vec![])?
            };
            destination.setattr("_filtered_children", children.clone())?;
            if !is_inst(&cloned_page, "NullObject")? || py_len(&children)? > 0 {
                call1(&new_outline, "append", destination.clone())?;
            }
            if !node.contains(&py_str(py, "/Next"))? {
                break;
            }
            node = get_item_str(&node, "/Next")?;
        }
    }
    Ok(new_outline)
}

// @pypdf: pypdf._writer.PdfWriter._get_filtered_outline
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_get_filtered_outline<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let node = arg(args, 1)?;
    let pages = arg(args, 2)?;
    let pages = pages.cast_into::<PyDict>()?;
    let reader = arg(args, 3)?;
    let visited = arg(args, 4)?;
    writer_get_filtered_outline(py, &slf, &node, &pages, &reader, &visited)
}

// @pypdf: pypdf._writer.PdfWriter._clone_outline
fn writer_clone_outline<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    dest: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let outline = gclass(py, "TreeObject")?.call0()?;
    call1(slf, "_add_object", outline.clone())?;
    set_item(
        &outline,
        &name_obj(py, "/Title")?,
        &text_obj(py, &to_string(&get_item_str(dest, "/Title")?)?)?,
    )?;
    let page = get_item_str(dest, "/Page")?;
    if !is_inst(&page, "NullObject")? {
        let node = get_def(dest, "node", &none(py))?;
        if !is_none(&node) && node.contains(&py_str(py, "/A"))? {
            let action = call1(&get_item_str(&node, "/A")?, "clone", slf.clone())?;
            set_item(&outline, &name_obj(py, "/A")?, &action)?;
        } else {
            let dest_array = get(dest, "dest_array")?;
            set_item(&outline, &name_obj(py, "/Dest")?, &dest_array)?;
        }
    }
    let node = get_def(dest, "node", &none(py))?;
    if !is_none(&node) {
        let format = number_obj(
            py,
            to_i64(&gget(&node, &py_str(py, "/F"), &py_int(py, 0))?)?,
        )?;
        set_item(&outline, &name_obj(py, "/F")?, &format)?;
        let mut color = call0(
            &gget(&node, &py_str(py, "/C"), &null_obj(py)?)?,
            "get_object",
        )?;
        if !is_inst_builtin(&color, "list")? {
            color = array_obj(
                py,
                vec![
                    new_g(py, "FloatObject", vec![py_float(py, 0.0)])?,
                    new_g(py, "FloatObject", vec![py_float(py, 0.0)])?,
                    new_g(py, "FloatObject", vec![py_float(py, 0.0)])?,
                ],
            )?;
        }
        set_item(
            &outline,
            &name_obj(py, "/C")?,
            &array_obj(py, items(&color)?)?,
        )?;
    }
    Ok(outline)
}

// @pypdf: pypdf._writer.PdfWriter._clone_outline
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_clone_outline<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let dest = arg(args, 1)?;
    writer_clone_outline(py, &slf, &dest)
}

fn writer_insert_filtered_outline<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    outlines: &Bound<'py, PyAny>,
    parent: &Bound<'py, PyAny>,
    before: &Bound<'py, PyAny>,
) -> PyResult<()> {
    for dest in items(outlines)? {
        let is_outlines = eq(
            &gget(&dest, &py_str(py, "/Type"), &py_str(py, ""))?,
            &py_str(py, "/Outlines"),
        )?;
        let np = if is_outlines || !dest.contains(&py_str(py, "/Title"))? {
            parent.clone()
        } else {
            let cloned = writer_clone_outline(py, slf, &dest)?;
            let parent_object = call0(parent, "get_object")?;
            callv(
                &parent_object,
                "insert_child",
                vec![cloned.clone(), before.clone(), slf.clone()],
            )?;
            cloned
        };
        let children = get_def(&dest, "_filtered_children", &none(py))?;
        writer_insert_filtered_outline(py, slf, &children, &np, &none(py))?;
    }
    Ok(())
}

// @pypdf: pypdf._writer.PdfWriter._insert_filtered_outline
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_insert_filtered_outline<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let outlines = arg(args, 1)?;
    let parent = arg(args, 2)?;
    let before = arg(args, 3)?;
    writer_insert_filtered_outline(py, &slf, &outlines, &parent, &before)?;
    Ok(none(py))
}

// ===========================================================================
// Form field values
// ===========================================================================

// @pypdf: pypdf._writer.PdfWriter.update_page_form_field_values
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_update_page_form_field_values<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let mut page = arg(args, 1)?;
    let fields = arg(args, 2)?;
    let flags = arg(args, 3)?;
    let auto_regenerate = arg(args, 4)?;
    let flatten = arg(args, 5)?;
    let root = get(&slf, "_root_object")?;
    let acro_key = const_str(py, "CatalogAttributes", "ACRO_FORM")?;
    let fields_key = const_str(py, "InteractiveFormDictEntries", "Fields")?;
    if !root.contains(&acro_key)? {
        return Err(errors::raise_msg(
            py,
            "PyPdfError",
            "No /AcroForm dictionary in PDF of PdfWriter Object",
        ));
    }
    let acro_form = get_item(&root, &acro_key)?;
    if !acro_form.contains(&fields_key)? {
        return Err(errors::raise_msg(
            py,
            "PyPdfError",
            "No /Fields dictionary in PDF of PdfWriter Object",
        ));
    }
    if is_inst_builtin(&auto_regenerate, "bool")? {
        call1(&slf, "set_need_appearances_writer", auto_regenerate)?;
    }
    if is_none(&page) {
        page = py_list(py, &get(&slf, "pages")?)?;
    }
    if is_inst_builtin(&page, "list")? {
        for single in items(&page)? {
            if single.contains(&const_str(py, "PageAttributes", "ANNOTS")?)? {
                callkw(
                    &slf,
                    "update_page_form_field_values",
                    vec![single, fields.clone(), flags.clone(), none(py)],
                    vec![("flatten", flatten.clone())],
                )?;
            }
        }
        return Ok(none(py));
    }
    let annots_key = const_str(py, "PageAttributes", "ANNOTS")?;
    if !page.contains(&annots_key)? {
        logger_warning(py, "No fields to update on this page", None)?;
        return Ok(none(py));
    }
    let mut appearance_stream_obj = none(py);
    for annotation in items(&get_item(&page, &annots_key)?)? {
        let annotation = call0(&annotation, "get_object")?;
        if !eq(
            &gget(&annotation, &py_str(py, "/Subtype"), &py_str(py, ""))?,
            &py_str(py, "/Widget"),
        )? {
            continue;
        }
        let parent_annotation;
        if annotation.contains(&py_str(py, "/FT"))? && annotation.contains(&py_str(py, "/T"))? {
            parent_annotation = annotation.clone();
        } else {
            parent_annotation = call0(
                &gget(
                    &annotation,
                    &const_str(py, "PageAttributes", "PARENT")?,
                    &new_g(py, "DictionaryObject", vec![])?,
                )?,
                "get_object",
            )?;
        }
        for pair in items(&call0(&fields, "items")?)? {
            let field = pair.get_item(0)?;
            let value = pair.get_item(1)?;
            let rectangle = get_item(
                &annotation,
                &const_str(py, "AnnotationDictionaryAttributes", "Rect")?,
            )?;
            let qualified = callkw(
                &slf,
                "_get_qualified_field_name",
                vec![],
                vec![("parent", parent_annotation.clone())],
            )?;
            if !eq(&qualified, &field)?
                && !eq(
                    &gget(&parent_annotation, &py_str(py, "/T"), &none(py))?,
                    &field,
                )?
            {
                continue;
            }
            if eq(
                &gget(&parent_annotation, &py_str(py, "/FT"), &none(py))?,
                &py_str(py, "/Ch"),
            )? && parent_annotation.contains(&py_str(py, "/I"))?
            {
                parent_annotation.del_item(py_str(py, "/I"))?;
            }
            if truthy(&flags)? {
                let ff_key = name_obj(
                    py,
                    &to_string(&const_str(py, "FieldDictionaryAttributes", "Ff")?)?,
                )?;
                let number = number_obj(py, to_i64(&flags)?)?;
                set_item(&annotation, &ff_key, &number)?;
            }
            // Set the field value
            if !(is_none(&value) && truthy(&flatten)?) {
                let v_key = name_obj(
                    py,
                    &to_string(&const_str(py, "FieldDictionaryAttributes", "V")?)?,
                )?;
                if is_inst_builtin(&value, "list")? {
                    let mut converted: Vec<Bound<'py, PyAny>> = Vec::new();
                    for item in items(&value)? {
                        converted.push(text_obj(py, &to_string(&item)?)?);
                    }
                    let array = array_obj(py, converted)?;
                    set_item(&parent_annotation, &v_key, &array)?;
                } else if is_inst_builtin(&value, "tuple")? {
                    let first = get_item(&value, &py_int(py, 0))?;
                    let text = text_obj(py, &to_string(&first)?)?;
                    set_item(&parent_annotation, &v_key, &text)?;
                } else {
                    let text = text_obj(py, &to_string(&value)?)?;
                    set_item(&parent_annotation, &v_key, &text)?;
                }
            }
            // Get or create the field's appearance stream object
            let field_type = const_str(py, "FieldDictionaryAttributes", "FT")?;
            if eq(
                &gget(&parent_annotation, &field_type, &none(py))?,
                &py_str(py, "/Btn"),
            )? {
                let v = name_obj(py, &to_string(&value)?)?;
                let ap = get_item(
                    &annotation,
                    &const_str(py, "AnnotationDictionaryAttributes", "AP")?,
                )?;
                let normal_ap = get_item_str(&ap, "/N")?;
                let mut v = v;
                if !normal_ap.contains(&v)? {
                    v = name_obj(py, "/Off")?;
                }
                appearance_stream_obj = gget(&normal_ap, &v, &none(py))?;
                let as_key = name_obj(
                    py,
                    &to_string(&const_str(py, "AnnotationDictionaryAttributes", "AS")?)?,
                )?;
                set_item(&annotation, &as_key, &v)?;
                let v_key = name_obj(
                    py,
                    &to_string(&const_str(py, "FieldDictionaryAttributes", "V")?)?,
                )?;
                set_item(&annotation, &v_key, &v)?;
            } else if eq(
                &gget(&parent_annotation, &field_type, &none(py))?,
                &py_str(py, "/Tx"),
            )? || eq(
                &gget(&parent_annotation, &field_type, &none(py))?,
                &py_str(py, "/Ch"),
            )? {
                let appearance_class = py
                    .import("pypdf.generic._appearance_stream")?
                    .getattr("TextStreamAppearance")?;
                let created = if is_inst_builtin(&value, "tuple")? {
                    callv(
                        &appearance_class,
                        "from_text_annotation",
                        vec![
                            slf.clone(),
                            page.clone(),
                            flatten.clone(),
                            acro_form.clone(),
                            parent_annotation.clone(),
                            annotation.clone(),
                            get_item(&value, &py_int(py, 1))?,
                            get_item(&value, &py_int(py, 2))?,
                        ],
                    )?
                } else {
                    callv(
                        &appearance_class,
                        "from_text_annotation",
                        vec![
                            slf.clone(),
                            page.clone(),
                            flatten.clone(),
                            acro_form.clone(),
                            parent_annotation.clone(),
                            annotation.clone(),
                        ],
                    )?
                };
                appearance_stream_obj = created.clone();
                let ap_key = name_obj(
                    py,
                    &to_string(&const_str(py, "AnnotationDictionaryAttributes", "AP")?)?,
                )?;
                if !annotation.contains(&ap_key)? {
                    let reference = call1(&slf, "_add_object", created)?;
                    let inner = dict_obj(py, vec![(name_obj(py, "/N")?, reference)])?;
                    set_item(&annotation, &ap_key, &inner)?;
                } else {
                    let ap = get_item(&annotation, &ap_key)?;
                    let normal_key = name_obj(py, "/N")?;
                    if !ap.contains(&normal_key)? {
                        let reference = call1(&slf, "_add_object", created)?;
                        set_item(&ap, &normal_key, &reference)?;
                    } else {
                        let reference = get_item(&ap, &normal_key)?;
                        let idnum =
                            to_usize(&get(&get(&reference, "indirect_reference")?, "idnum")?)?;
                        let objects = get(&slf, "_objects")?;
                        objects.set_item(idnum - 1, created.clone())?;
                        created.setattr("indirect_reference", indirect_obj(py, idnum, 0, &slf)?)?;
                    }
                }
            } else if eq(
                &gget(&annotation, &field_type, &none(py))?,
                &py_str(py, "/Sig"),
            )? {
                logger_warning(py, "Signature forms not implemented yet", None)?;
            }
            if truthy(&appearance_stream_obj)? && truthy(&flatten)? {
                let first = get_item(&rectangle, &py_int(py, 0))?;
                let second = get_item(&rectangle, &py_int(py, 1))?;
                callv(
                    &slf,
                    "_add_apstream_object",
                    vec![
                        page.clone(),
                        appearance_stream_obj.clone(),
                        field.clone(),
                        first,
                        second,
                    ],
                )?;
            }
        }
    }
    Ok(none(py))
}

// ===========================================================================
// Page labels
// ===========================================================================

// @pypdf: pypdf._writer.PdfWriter.set_page_label
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_set_page_label<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let page_index_from = arg(args, 1)?;
    let page_index_to = arg(args, 2)?;
    let style = arg(args, 3)?;
    let prefix = arg(args, 4)?;
    let start = arg(args, 5)?;
    let style_enum = py.import("pypdf.constants")?.getattr("PageLabelStyle")?;
    if is_none(&style) && is_none(&prefix) {
        return Err(value_error(
            py,
            "At least one of style and prefix must be given",
        ));
    }
    if !is_none(&style) {
        let values = builtins(py)?.getattr("tuple")?.call1((style_enum,))?;
        if !values.contains(&style)? {
            let mut names: Vec<String> = Vec::new();
            for value in items(&values)? {
                names.push(to_string(&value)?);
            }
            return Err(value_error(
                py,
                &format!(
                    "style must be one of: {}, got {}",
                    names.join(", "),
                    pyutil::py_repr(&style)?
                ),
            ));
        }
    }
    if to_i64(&page_index_from)? < 0 {
        return Err(value_error(
            py,
            "page_index_from must be greater or equal than 0",
        ));
    }
    if to_i64(&page_index_to)? < to_i64(&page_index_from)? {
        return Err(value_error(
            py,
            "page_index_to must be greater or equal than page_index_from",
        ));
    }
    let pages = get(&slf, "pages")?;
    if to_i64(&page_index_to)? >= py_len(&pages)? as i64 {
        return Err(value_error(py, "page_index_to exceeds number of pages"));
    }
    if !is_none(&start) && to_i64(&start)? != 0 && to_i64(&start)? < 1 {
        return Err(value_error(
            py,
            "If given, start must be greater or equal than one",
        ));
    }
    callv(
        &slf,
        "_set_page_label",
        vec![page_index_from, page_index_to, style, prefix, start],
    )?;
    Ok(none(py))
}

// @pypdf: pypdf._writer.PdfWriter._set_page_label
#[pyfunction]
#[pyo3(signature = (*args))]
fn writermod_pdfwriter_private_set_page_label<'py>(
    py: Python<'py>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let slf = arg(args, 0)?;
    let page_index_from = arg(args, 1)?;
    let page_index_to = arg(args, 2)?;
    let style = arg(args, 3)?;
    let prefix = arg(args, 4)?;
    let start = arg(args, 5)?;
    let default_label = dict_obj(py, vec![(name_obj(py, "/S")?, name_obj(py, "/D")?)])?;
    let new_label = dict_obj(py, vec![])?;
    if !is_none(&style) {
        set_item(
            &new_label,
            &name_obj(py, "/S")?,
            &name_obj(py, &to_string(&style)?)?,
        )?;
    }
    if !is_none(&prefix) {
        set_item(
            &new_label,
            &name_obj(py, "/P")?,
            &text_obj(py, &to_string(&prefix)?)?,
        )?;
    }
    if !is_none(&start) && to_i64(&start)? != 0 {
        set_item(
            &new_label,
            &name_obj(py, "/St")?,
            &number_obj(py, to_i64(&start)?)?,
        )?;
    }
    let root = get(&slf, "_root_object")?;
    let labels_key = const_str(py, "CatalogAttributes", "PAGE_LABELS")?;
    if !root.contains(&labels_key)? {
        let nums = array_obj(py, vec![])?;
        py.import("pypdf._page_labels")?
            .getattr("nums_insert")?
            .call1((number_obj(py, 0)?, default_label.clone(), nums.clone()))?;
        let page_labels = gclass(py, "TreeObject")?.call0()?;
        set_item(&page_labels, &name_obj(py, "/Nums")?, &nums)?;
        let key = name_obj(py, &to_string(&labels_key)?)?;
        set_item(&root, &key, &page_labels)?;
    }
    let page_labels = get_item(&root, &name_obj(py, &to_string(&labels_key)?)?)?;
    let nums = get_item(&page_labels, &name_obj(py, "/Nums")?)?;
    let nums_insert = py.import("pypdf._page_labels")?.getattr("nums_insert")?;
    nums_insert.call1((
        number_obj(py, to_i64(&page_index_from)?)?,
        new_label,
        nums.clone(),
    ))?;
    py.import("pypdf._page_labels")?
        .getattr("nums_clear_range")?
        .call1((
            number_obj(py, to_i64(&page_index_from)?)?,
            page_index_to.clone(),
            nums.clone(),
        ))?;
    let next_result = py
        .import("pypdf._page_labels")?
        .getattr("nums_next")?
        .call1((number_obj(py, to_i64(&page_index_from)?)?, nums.clone()))?;
    let next_label_pos = next_result.get_item(0)?;
    let next_page = to_i64(&page_index_to)? + 1;
    let pages = get(&slf, "pages")?;
    if !eq(&next_label_pos, &number_obj(py, next_page)?)? && next_page < py_len(&pages)? as i64 {
        nums_insert.call1((number_obj(py, next_page)?, default_label, nums.clone()))?;
    }
    set_item(&page_labels, &name_obj(py, "/Nums")?, &nums)?;
    set_item(
        &root,
        &name_obj(py, &to_string(&labels_key)?)?,
        &page_labels,
    )?;
    Ok(none(py))
}

// --STUBS--
// ---------------------------------------------------------------------------
// PyO3 registration
// ---------------------------------------------------------------------------

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(
        writermod_nameddestinationindex_init,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_nameddestinationindex_private_update,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_nameddestinationindex_contains,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_nameddestinationindex_insert,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_nameddestinationindex_record_insertion,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_nameddestinationindex_flush,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_init, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_get_is_encrypted,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_get_root_object,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_get_private_info,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_property_set_private_info,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_get_xmp_metadata,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_property_set_xmp_metadata,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_enter, module)?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_exit, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_get_pdf_header,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_property_set_pdf_header,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_add_object,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_get_object, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_replace_object,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_add_page,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_set_need_appearances_writer,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_create_viewer_preferences,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_add_page, module)?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_insert_page, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_get_page_number_by_indirect,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_add_blank_page,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_insert_blank_page,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_get_open_destination,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_property_set_open_destination,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_add_js, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_add_open_action,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_add_attachment,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_append_pages_from_reader,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_merge_content_stream_to_page,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_add_apstream_object,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_update_page_form_field_values,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_reattach_fields,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_collect_incremental_clone_object_ids,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_clone_reader_document_root,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_clone_document_from_reader,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_generate_file_identifiers,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_resolve_links,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_write_stream, module)?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_write, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_list_objects_in_increment,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_write_increment,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_write_pdf_structure,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_write_xref_table,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_write_trailer,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_get_metadata, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_property_set_metadata,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_add_metadata, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_compress_identical_objects,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_get_reference, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_get_outline_root,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_get_threads_root,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_get_threads, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_add_outline_item_destination,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_add_outline_item_dict,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_add_outline_item,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_add_outline, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_add_named_destination_array,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_add_named_destination_object,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_get_page_reference,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_add_named_destination,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_remove_links, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_remove_annotations,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_remove_annots_from_page,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_remove_objects_from_page,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_remove_objects_from_page_clean,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_remove_objects_from_page_clean_forms,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_remove_images, module)?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_remove_text, module)?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_add_uri, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_get_page_layout,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_set_page_layout,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_set_page_layout,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_get_page_layout,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_property_set_page_layout,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_get_page_mode,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_get_page_mode, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_property_set_page_mode,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_add_annotation,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_clean_page, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_create_stream,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_append, module)?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_merge, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_merge_process_named_dests,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_add_named_destination_array_indexed,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_add_articles_thread,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_add_filtered_articles,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_get_cloned_page,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_insert_filtered_annotations,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_clone_widget_annotation,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_set_cloned_kids,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_get_filtered_outline,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_clone_outline,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_insert_filtered_outline,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_pdfwriter_close, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_find_outline_item,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_reset_translation,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_set_page_label,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_set_page_label,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        writermod_pdfwriter_private_repr_mimebundle_,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(writermod_private_pdf_objectify, module)?)?;
    module.add_function(wrap_pyfunction!(
        writermod_private_create_outline_item,
        module
    )?)?;
    Ok(())
}
