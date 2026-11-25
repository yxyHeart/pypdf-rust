//! Rust port of `pypdf/generic/_base.py` (upstream pypdf 7.1.1).
//!
//! # Design: Python class model, Rust logic
//!
//! Upstream's object model is load-bearing for observable behaviour:
//!
//! * `FloatObject`/`NumberObject`/`NameObject`/`TextStringObject`/
//!   `ByteStringObject` **are** `float`/`int`/`str`/`bytes` subclasses and they
//!   inherit `PdfObject` (`class FloatObject(float, PdfObject)`), so every scalar
//!   instance also has `get_object()`, `_reference_clone()`, `hash_value()`, ...
//! * `isinstance(x, PdfObject)` is used all over upstream (e.g.
//!   `DictionaryObject.__setitem__`, `is_null_or_none`, `_pdf_objectify`) and in
//!   the official tests.
//! * The official test suite counts *exact* Python frame counts
//!   (`sys.settrace`, `test_cloning_direct_objects__function_calls_and_exceptions`
//!   and `test_read_array_of_numbers_and_references__function_calls`).
//!
//! PyO3 cannot inherit from an arbitrary Python class, and a native `#[pyclass]`
//! has a fixed native layout, so a `#[pyclass]` `PdfObject` would force the
//! scalar classes (which must subclass `float`/`str`/...) to *not* inherit it --
//! losing `isinstance` identity and the `get_object`/`_reference_clone` surface
//! that every PDF value is expected to expose, plus changing the traced Python
//! call graph.  The whole hierarchy therefore lives in the hand-written shim
//! `python/pypdf/generic/_base.py` with upstream's exact class/method structure,
//! and every method body delegates to a `#[pyfunction]` implemented here.  This
//! keeps the Python call graph (and therefore the `sys.settrace` counts) equal to
//! upstream while all logic lives in Rust.

use std::fmt::Write as _;

use pyo3::exceptions::{
    PyAssertionError, PyAttributeError, PyIndexError, PyKeyError, PyMemoryError,
    PyNotImplementedError, PyOverflowError, PyUnicodeDecodeError, PyUnicodeEncodeError,
    PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyBytes, PyDict, PyInt, PyModule, PyString, PyTuple};

use pypdf_rs_core::pyfmt::{float_myrepr, py_float_fixed, MyReprError};

use crate::errors;
use crate::pyutil;

/// Upstream module name; used as `logger_warning(..., source=__name__)`.
const SOURCE: &str = "pypdf.generic._base";

/// `IndirectObject._MAXIMUM_PART_LENGTH` (upstream class attribute).
pub const MAXIMUM_PART_LENGTH: usize = 64;

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    crate::pyutil::cached_module(py, "builtins")
}

fn base_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    pyutil::cached_module(py, "pypdf.generic._base")
}

/// Fetch a class from the shim module (upstream references these by global name).
fn base_class<'py>(py: Python<'py>, name: &'static str) -> PyResult<Bound<'py, PyAny>> {
    pyutil::cached_attr(py, "pypdf.generic._base", name)
}

fn utils_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    pyutil::cached_module(py, "pypdf._utils")
}

fn py_bytes<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, data).into_any()
}

fn int_obj<'py>(py: Python<'py>, value: i64) -> Bound<'py, PyAny> {
    PyInt::new(py, value).into_any()
}

fn py_bool<'py>(py: Python<'py>, value: bool) -> Bound<'py, PyAny> {
    PyBool::new(py, value).to_owned().into_any()
}

/// `id(obj)` as a Python int (used as a dict key, exactly like upstream).
fn py_id<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("id")?.call1((obj,))
}

fn tuple_hash<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<isize> {
    PyTuple::new(py, items)?.hash()
}

/// Slice an object (`obj[start:stop]`) using Python's own slicing semantics.
fn slice_of<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    start: Option<Bound<'py, PyAny>>,
    stop: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let slice = pyutil::py_slice(py, start.as_ref(), stop.as_ref(), None)?;
    obj.get_item(&slice)
}

/// `raise <builtin>(...)` for exception classes that take no arguments.
fn empty_exception(py: Python<'_>, class_name: &str) -> PyErr {
    match builtins(py)
        .and_then(|builtins| builtins.getattr(class_name))
        .and_then(|class| class.call0())
    {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    }
}

/// `raise <binary>(message)` built from a Python exception class.
fn message_exception(py: Python<'_>, class_name: &str, message: &str) -> PyErr {
    match builtins(py)
        .and_then(|builtins| builtins.getattr(class_name))
        .and_then(|class| class.call1((message,)))
    {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    }
}

/// `class_name.__name__` of an instance's type (upstream `self.__class__.__name__`).
fn class_name(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    pyutil::py_str(&obj.get_type().getattr("__name__")?)
}

/// Port-level `isinstance(x, PdfObject)` used by upstream's `assert ... # mypy`.
///
/// `PdfObject` is a plain Python class, so the `_base` classes are covered by the
/// `isinstance` fast path.  Container classes ported as `#[pyclass]` subclasses of
/// `dict`/`list` cannot inherit `PdfObject` (PyO3 limitation); for the assertions
/// that upstream only performs for type-checkers the port also accepts the
/// documented PDF-object protocol surface.
pub fn is_pdf_object_like(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    if obj.is_instance(&base_class(py, "PdfObject")?)? {
        return Ok(true);
    }
    Ok(obj.hasattr("get_object")? && obj.hasattr("write_to_stream")? && obj.hasattr("clone")?)
}

fn assert_pdf_object_like(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<()> {
    if is_pdf_object_like(py, obj)? {
        Ok(())
    } else {
        Err(PyAssertionError::new_err("mypy"))
    }
}

fn assertion(py: Python<'_>, message: Option<&str>) -> PyErr {
    match message {
        Some(message) => PyAssertionError::new_err(message.to_string()),
        None => empty_exception(py, "AssertionError"),
    }
}

fn int_type<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("int")
}

fn raise_unconvertible_float(py: Python<'_>, value: f64) -> PyErr {
    // Upstream evaluates `int(log10(abs(self)))`: `int(inf)` raises OverflowError
    // and `int(nan)` raises ValueError.  Let CPython produce the exact messages.
    match int_type(py).and_then(|int| int.call1((value,))) {
        Ok(_) => PyValueError::new_err("cannot convert float NaN to integer"),
        Err(error) => error,
    }
}

/// `FloatObject.myrepr()` (with the module-global `FLOAT_WRITE_PRECISION`).
pub(crate) fn float_myrepr_with_precision(py: Python<'_>, value: f64, precision: i32) -> PyResult<String> {
    if value == 0.0 {
        return Ok("0.0".to_string());
    }
    if precision == 8 {
        // Fast path: verified against CPython by `pypdf_rs_core::pyfmt`.
        return match float_myrepr(value) {
            Ok(text) => Ok(text),
            Err(MyReprError::Overflow) => Err(raise_unconvertible_float(py, f64::INFINITY)),
            Err(MyReprError::ValueError) => Err(raise_unconvertible_float(py, f64::NAN)),
        };
    }
    // The module constant is documented as user-adjustable ("shall be min 5
    // digits max, allow user adj"); support it with the same algorithm.
    let magnitude = value.abs();
    if magnitude.is_infinite() {
        return Err(raise_unconvertible_float(py, f64::INFINITY));
    }
    if magnitude.is_nan() {
        return Err(raise_unconvertible_float(py, f64::NAN));
    }
    let nb = precision - (magnitude.log10() as i32);
    let rendered = py_float_fixed(value, nb.max(1) as usize);
    Ok(rendered
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string())
}

/// Read `WHITESPACES` from `pypdf._utils` (upstream iterates the tuple verbatim).
fn whitespaces(utils: &Bound<'_, PyModule>) -> PyResult<Vec<Vec<u8>>> {
    let tuple = utils.getattr("WHITESPACES")?;
    let mut result = Vec::new();
    for item in tuple.try_iter()? {
        let item = item?;
        result.push(item.extract::<Vec<u8>>()?);
    }
    Ok(result)
}

fn raise_pdf_error_with_py_message(
    py: Python<'_>,
    class_name: &str,
    message: Bound<'_, PyAny>,
) -> PyErr {
    match PyTuple::new(py, [message]) {
        Ok(args) => errors::raise(py, class_name, args),
        Err(error) => error,
    }
}

// ---------------------------------------------------------------------------
// PdfObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._base.PdfObject.hash_bin
#[pyfunction]
fn basemod_pdfobject_hash_bin<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Err(PyNotImplementedError::new_err(format!(
        "{} does not implement .hash_bin() so far",
        class_name(slf)?
    )))
}

// @pypdf: pypdf.generic._base.PdfObject.hash_value_data
#[pyfunction]
fn basemod_pdfobject_hash_value_data<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    // Upstream: `f"{self}".encode()`
    let formatted = builtins(py)?.getattr("format")?.call1((slf, ""))?;
    Ok(formatted.call_method0("encode")?.unbind())
}

// @pypdf: pypdf.generic._base.PdfObject.hash_value
#[pyfunction]
fn basemod_pdfobject_hash_value<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    // Upstream:
    //   return (f"{self.__class__.__name__}:"
    //           f"{self.hash_func(self.hash_value_data()).hexdigest()}").encode()
    let name = class_name(slf)?;
    let hash_func = slf.getattr("hash_func")?;
    let data = slf.call_method0("hash_value_data")?;
    let digest = hash_func.call1((data,))?.call_method0("hexdigest")?;
    let rendered = format!("{}:{}", name, pyutil::py_str(&digest)?);
    Ok(PyString::new(py, &rendered)
        .call_method0("encode")?
        .unbind())
}

// @pypdf: pypdf.generic._base.PdfObject.replicate
#[pyfunction]
fn basemod_pdfobject_replicate<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(slf.call_method1("clone", (pdf_dest,))?.unbind())
}

// @pypdf: pypdf.generic._base.PdfObject.clone
#[pyfunction]
fn basemod_pdfobject_clone<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    _pdf_dest: &Bound<'py, PyAny>,
    _force_duplicate: bool,
    _ignore_fields: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Err(PyNotImplementedError::new_err(format!(
        "{} does not implement .clone so far",
        class_name(slf)?
    )))
}

/// `PdfObject._reference_clone`.
///
/// Shared by every ported class (including `#[pyclass]` containers): exposed as
/// `pub` so other modules can reuse the exact upstream algorithm.
pub fn reference_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    clone_obj: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
) -> PyResult<Py<PyAny>> {
    // @pypdf: pypdf.generic._base.PdfObject._reference_clone
    if !slf.hasattr("indirect_reference")? {
        // Direct object: nothing to reference.
        return Ok(clone_obj.clone().unbind());
    }
    // try: if not force_duplicate and clone.indirect_reference is not None
    //          and clone.indirect_reference.pdf == pdf_dest: return clone
    let probe: PyResult<bool> = (|| {
        if force_duplicate {
            return Ok(false);
        }
        let reference = clone_obj.getattr("indirect_reference")?;
        if reference.is_none() {
            return Ok(false);
        }
        let reference_pdf = reference.getattr("pdf")?;
        reference_pdf.eq(pdf_dest)
    })();
    match probe {
        Ok(true) => return Ok(clone_obj.clone().unbind()),
        Ok(false) => {}
        Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
        Err(_) => {}
    }

    let ind = slf.getattr("indirect_reference")?;
    let objects = pdf_dest.getattr("_objects")?;
    let object_count = builtins(py)?.getattr("len")?.call1((&objects,))?;

    let i: Bound<'py, PyAny> = if pdf_dest.getattr("incremental")?.is_truthy()?
        && !ind.is_none()
        && ind.getattr("pdf")?.eq(&pdf_dest.getattr("_reader")?)?
        && ind.getattr("idnum")?.le(&object_count)?
    {
        ind.getattr("idnum")?
    } else {
        object_count.call_method1("__add__", (1,))?
    };

    if !ind.is_none() {
        let ind_pdf = ind.getattr("pdf")?;
        let key = py_id(py, &ind_pdf)?;
        let id_translated = pdf_dest.getattr("_id_translated")?;
        if !id_translated.contains(&key)? {
            id_translated.set_item(&key, PyDict::new(py))?;
            let mapping = id_translated.get_item(&key)?;
            mapping.set_item("PreventGC", &ind_pdf)?;
        }
        let mapping = id_translated.get_item(&key)?;
        let idnum = ind.getattr("idnum")?;
        if !force_duplicate && mapping.contains(&idnum)? {
            let translated = mapping.get_item(&idnum)?;
            let obj = pdf_dest.call_method1("get_object", (translated,))?;
            assert_pdf_object_like(py, &obj)?;
            return Ok(obj.unbind());
        }
        mapping.set_item(&idnum, &i)?;
    }

    let index = i.call_method1("__sub__", (1,))?;
    match objects.set_item(&index, clone_obj) {
        Ok(()) => {}
        Err(error) if error.is_instance_of::<PyIndexError>(py) => {
            objects.call_method1("append", (clone_obj,))?;
            let length = builtins(py)?.getattr("len")?.call1((&objects,))?;
            return finish_reference(py, clone_obj, pdf_dest, &length);
        }
        Err(error) => return Err(error),
    }
    finish_reference(py, clone_obj, pdf_dest, &i)
}

// @pypdf: pypdf.generic._base.PdfObject._reference_clone
#[pyfunction]
fn basemod_pdfobject_reference_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    clone_obj: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
) -> PyResult<Py<PyAny>> {
    reference_clone(py, slf, clone_obj, pdf_dest, force_duplicate)
}

fn finish_reference<'py>(
    py: Python<'py>,
    clone_obj: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    i: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let indirect = base_class(py, "IndirectObject")?.call1((i, 0, pdf_dest))?;
    clone_obj.setattr("indirect_reference", indirect)?;
    Ok(clone_obj.clone().unbind())
}

// @pypdf: pypdf.generic._base.PdfObject.get_object
#[pyfunction]
fn basemod_pdfobject_get_object<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(slf.clone().unbind())
}

// @pypdf: pypdf.generic._base.PdfObject.write_to_stream
#[pyfunction]
fn basemod_pdfobject_write_to_stream<'py>(
    py: Python<'py>,
    _slf: &Bound<'py, PyAny>,
    _stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    Err(empty_exception(py, "NotImplementedError"))
}

// ---------------------------------------------------------------------------
// NullObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._base.NullObject.clone
#[pyfunction]
fn basemod_null_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
) -> PyResult<Py<PyAny>> {
    if !slf.hasattr("indirect_reference")? {
        // Direct object: shortcut, as some arrays consist mostly of nulls.
        return Ok(base_class(py, "NullObject")?.call0()?.unbind());
    }
    let clone = base_class(py, "NullObject")?.call0()?;
    Ok(slf
        .call_method1("_reference_clone", (clone, pdf_dest, force_duplicate))?
        .unbind())
}

// @pypdf: pypdf.generic._base.NullObject.hash_bin
#[pyfunction]
fn basemod_null_hash_bin<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<isize> {
    tuple_hash(py, vec![slf.get_type().into_any()])
}

// @pypdf: pypdf.generic._base.NullObject.write_to_stream
#[pyfunction]
fn basemod_null_write_to_stream<'py>(
    py: Python<'py>,
    _slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    stream.call_method1("write", (py_bytes(py, b"null"),))?;
    Ok(())
}

// @pypdf: pypdf.generic._base.NullObject.read_from_stream
#[pyfunction]
fn basemod_null_read_from_stream<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let null_txt = stream.call_method1("read", (4,))?;
    if !null_txt.eq(py_bytes(py, b"null"))? {
        return Err(errors::pdf_read_error(py, "Could not read Null object"));
    }
    Ok(base_class(py, "NullObject")?.call0()?.unbind())
}

// @pypdf: pypdf.generic._base.NullObject.__repr__
#[pyfunction]
fn basemod_null_repr<'py>(_py: Python<'py>, _slf: &Bound<'py, PyAny>) -> PyResult<String> {
    Ok("NullObject".to_string())
}

// @pypdf: pypdf.generic._base.NullObject.__eq__
#[pyfunction]
fn basemod_null_eq<'py>(
    py: Python<'py>,
    _slf: &Bound<'py, PyAny>,
    other: &Bound<'py, PyAny>,
) -> PyResult<bool> {
    other.is_instance(&base_class(py, "NullObject")?)
}

// @pypdf: pypdf.generic._base.NullObject.__hash__
#[pyfunction]
fn basemod_null_hash<'py>(_py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(slf.call_method0("hash_bin")?.unbind())
}

// ---------------------------------------------------------------------------
// BooleanObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._base.BooleanObject.clone
#[pyfunction]
fn basemod_boolean_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
) -> PyResult<Py<PyAny>> {
    let value = slf.getattr("value")?;
    let clone = base_class(py, "BooleanObject")?.call1((value,))?;
    Ok(slf
        .call_method1("_reference_clone", (clone, pdf_dest, force_duplicate))?
        .unbind())
}

// @pypdf: pypdf.generic._base.BooleanObject.hash_bin
#[pyfunction]
fn basemod_boolean_hash_bin<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<isize> {
    tuple_hash(py, vec![slf.get_type().into_any(), slf.getattr("value")?])
}

// @pypdf: pypdf.generic._base.BooleanObject.__eq__
#[pyfunction]
fn basemod_boolean_eq<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    other: &Bound<'py, PyAny>,
) -> PyResult<bool> {
    let value = slf.getattr("value")?;
    if other.is_instance(&base_class(py, "BooleanObject")?)? {
        return value.eq(&other.getattr("value")?);
    }
    if other.is_instance(&builtins(py)?.getattr("bool")?)? {
        return value.eq(other);
    }
    Ok(false)
}

// @pypdf: pypdf.generic._base.BooleanObject.__hash__
#[pyfunction]
fn basemod_boolean_hash<'py>(_py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(slf.call_method0("hash_bin")?.unbind())
}

// @pypdf: pypdf.generic._base.BooleanObject.__repr__
#[pyfunction]
fn basemod_boolean_repr<'py>(_py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<String> {
    if slf.getattr("value")?.is_truthy()? {
        Ok("True".to_string())
    } else {
        Ok("False".to_string())
    }
}

// @pypdf: pypdf.generic._base.BooleanObject.write_to_stream
#[pyfunction]
fn basemod_boolean_write_to_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let text: &[u8] = if slf.getattr("value")?.is_truthy()? {
        b"true"
    } else {
        b"false"
    };
    stream.call_method1("write", (py_bytes(py, text),))?;
    Ok(())
}

// @pypdf: pypdf.generic._base.BooleanObject.read_from_stream
#[pyfunction]
fn basemod_boolean_read_from_stream<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let boolean_class = base_class(py, "BooleanObject")?;
    let word = stream.call_method1("read", (4,))?;
    if word.eq(py_bytes(py, b"true"))? {
        return Ok(boolean_class.call1((py_bool(py, true),))?.unbind());
    }
    if word.eq(py_bytes(py, b"fals"))? {
        stream.call_method1("read", (1,))?;
        return Ok(boolean_class.call1((py_bool(py, false),))?.unbind());
    }
    Err(errors::pdf_read_error(py, "Could not read Boolean object"))
}

// ---------------------------------------------------------------------------
// IndirectObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._base.IndirectObject.__hash__
#[pyfunction]
fn basemod_indirect_hash<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<isize> {
    let pdf = slf.getattr("pdf")?;
    tuple_hash(
        py,
        vec![
            slf.getattr("idnum")?,
            slf.getattr("generation")?,
            py_id(py, &pdf)?,
        ],
    )
}

// @pypdf: pypdf.generic._base.IndirectObject.hash_bin
#[pyfunction]
fn basemod_indirect_hash_bin<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<isize> {
    let pdf = slf.getattr("pdf")?;
    tuple_hash(
        py,
        vec![
            slf.get_type().into_any(),
            slf.getattr("idnum")?,
            slf.getattr("generation")?,
            py_id(py, &pdf)?,
        ],
    )
}

// @pypdf: pypdf.generic._base.IndirectObject.replicate
#[pyfunction]
fn basemod_indirect_replicate<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(base_class(py, "IndirectObject")?
        .call1((slf.getattr("idnum")?, slf.getattr("generation")?, pdf_dest))?
        .unbind())
}

// @pypdf: pypdf.generic._base.IndirectObject.clone
#[pyfunction]
fn basemod_indirect_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
    ignore_fields: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let pdf = slf.getattr("pdf")?;
    if !force_duplicate && pdf.eq(pdf_dest)? {
        // Already duplicated and no extra duplication required.
        return Ok(slf.clone().unbind());
    }
    let key = py_id(py, &pdf)?;
    let id_translated = pdf_dest.getattr("_id_translated")?;
    if !id_translated.contains(&key)? {
        id_translated.set_item(&key, PyDict::new(py))?;
        let mapping = id_translated.get_item(&key)?;
        mapping.set_item("PreventGC", &pdf)?;
    }
    let mapping = id_translated.get_item(&key)?;
    let idnum = slf.getattr("idnum")?;
    let dup: Bound<'py, PyAny> = if mapping.contains(&idnum)? {
        let translated = mapping.get_item(&idnum)?;
        let resolved = pdf_dest.call_method1("get_object", (translated,))?;
        if force_duplicate {
            if resolved.is_none() {
                return Err(assertion(py, None));
            }
            let idref = resolved.getattr("indirect_reference")?;
            if idref.is_none() {
                return Err(assertion(py, None));
            }
            return Ok(base_class(py, "IndirectObject")?
                .call1((
                    idref.getattr("idnum")?,
                    idref.getattr("generation")?,
                    idref.getattr("pdf")?,
                ))?
                .unbind());
        }
        resolved
    } else {
        let mut obj = slf.call_method0("get_object")?;
        // Case observed: a pointed object can not be found.
        if obj.is_none() {
            obj = base_class(py, "NullObject")?.call0()?;
            if !slf.is_instance(&base_class(py, "IndirectObject")?)? {
                return Err(assertion(py, None));
            }
            obj.setattr("indirect_reference", slf)?;
        }
        let cloned = obj.call_method1("clone", (pdf_dest, force_duplicate, ignore_fields))?;
        pdf_dest.call_method1("_add_object", (cloned,))?
    };
    assert_pdf_object_like(py, &dup)?;
    let reference = dup.getattr("indirect_reference")?;
    if reference.is_none() {
        return Err(assertion(py, Some("mypy")));
    }
    Ok(reference.unbind())
}

// @pypdf: pypdf.generic._base.IndirectObject.get_object
#[pyfunction]
fn basemod_indirect_get_object<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let pdf = slf.getattr("pdf")?;
    Ok(pdf.call_method1("get_object", (slf,))?.unbind())
}

// @pypdf: pypdf.generic._base.IndirectObject.__deepcopy__
#[pyfunction]
fn basemod_indirect_deepcopy<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    _memo: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(base_class(py, "IndirectObject")?
        .call1((
            slf.getattr("idnum")?,
            slf.getattr("generation")?,
            slf.getattr("pdf")?,
        ))?
        .unbind())
}

// @pypdf: pypdf.generic._base.IndirectObject._get_object_with_check
#[pyfunction]
fn basemod_indirect_get_object_with_check<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let obj = slf.call_method0("get_object")?;
    if obj.is_instance(&base_class(py, "IndirectObject")?)? {
        let self_repr = slf.call_method0("__repr__")?;
        let obj_repr = obj.call_method0("__repr__")?;
        let message = format!(
            "{} references an IndirectObject {}",
            pyutil::py_str(&self_repr)?,
            pyutil::py_str(&obj_repr)?
        );
        return Err(errors::pdf_stream_error(py, &message));
    }
    Ok(obj.unbind())
}

// @pypdf: pypdf.generic._base.IndirectObject.__getattr__
#[pyfunction]
fn basemod_indirect_getattr<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    name: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let pointed = slf.call_method0("_get_object_with_check")?;
    let name = pyutil::py_str(name)?;
    match pointed.getattr(name.as_str()) {
        Ok(value) => Ok(value.unbind()),
        Err(error) if error.is_instance_of::<PyAttributeError>(py) => {
            let message = format!(
                "No attribute {} found in IndirectObject or pointed object",
                name
            );
            let wrapped = PyAttributeError::new_err(message);
            wrapped.set_cause(py, Some(error));
            Err(wrapped)
        }
        Err(error) => Err(error),
    }
}

// @pypdf: pypdf.generic._base.IndirectObject.__getitem__
#[pyfunction]
fn basemod_indirect_getitem<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let pointed = slf.call_method0("_get_object_with_check")?;
    Ok(pointed.get_item(key)?.unbind())
}

// @pypdf: pypdf.generic._base.IndirectObject.__contains__
#[pyfunction]
fn basemod_indirect_contains<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
) -> PyResult<bool> {
    let pointed = slf.call_method0("_get_object_with_check")?;
    pointed.contains(key)
}

// @pypdf: pypdf.generic._base.IndirectObject.__iter__
#[pyfunction]
fn basemod_indirect_iter<'py>(_py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let pointed = slf.call_method0("_get_object_with_check")?;
    Ok(pointed.call_method0("__iter__")?.unbind())
}

// @pypdf: pypdf.generic._base.IndirectObject.__float__
#[pyfunction]
fn basemod_indirect_float<'py>(_py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let pointed = slf.call_method0("get_object")?;
    Ok(pointed.call_method0("__float__")?.unbind())
}

// @pypdf: pypdf.generic._base.IndirectObject.__int__
#[pyfunction]
fn basemod_indirect_int<'py>(_py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let pointed = slf.call_method0("get_object")?;
    Ok(pointed.call_method0("__int__")?.unbind())
}

// @pypdf: pypdf.generic._base.IndirectObject.__str__
#[pyfunction]
fn basemod_indirect_str<'py>(_py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let pointed = slf.call_method0("get_object")?;
    Ok(pointed.call_method0("__str__")?.unbind())
}

// @pypdf: pypdf.generic._base.IndirectObject.__repr__
#[pyfunction]
fn basemod_indirect_repr<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<String> {
    let pdf = slf.getattr("pdf")?;
    let id = py_id(py, &pdf)?;
    Ok(format!(
        "IndirectObject({}, {}, {})",
        pyutil::py_repr(&slf.getattr("idnum")?)?,
        pyutil::py_repr(&slf.getattr("generation")?)?,
        pyutil::py_str(&id)?
    ))
}

// @pypdf: pypdf.generic._base.IndirectObject.__eq__
#[pyfunction]
fn basemod_indirect_eq<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    other: &Bound<'py, PyAny>,
) -> PyResult<bool> {
    if other.is_none() || !other.is_instance(&base_class(py, "IndirectObject")?)? {
        return Ok(false);
    }
    if !slf.getattr("idnum")?.eq(&other.getattr("idnum")?)? {
        return Ok(false);
    }
    if !slf
        .getattr("generation")?
        .eq(&other.getattr("generation")?)?
    {
        return Ok(false);
    }
    Ok(slf.getattr("pdf")?.is(other.getattr("pdf")?))
}

// @pypdf: pypdf.generic._base.IndirectObject.__ne__
#[pyfunction]
fn basemod_indirect_ne<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    other: &Bound<'py, PyAny>,
) -> PyResult<bool> {
    let equal = slf.call_method1("__eq__", (other,))?;
    Ok(!equal.is_truthy()?)
}

// @pypdf: pypdf.generic._base.IndirectObject.write_to_stream
#[pyfunction]
fn basemod_indirect_write_to_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    // Upstream: f"{self.idnum} {self.generation} R".encode()
    let rendered = format!(
        "{} {} R",
        pyutil::py_str(&slf.getattr("idnum")?)?,
        pyutil::py_str(&slf.getattr("generation")?)?
    );
    let encoded = PyString::new(py, &rendered).call_method0("encode")?;
    stream.call_method1("write", (encoded,))?;
    Ok(())
}

// @pypdf: pypdf.generic._base.IndirectObject.read_from_stream
#[pyfunction]
fn basemod_indirect_read_from_stream<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let indirect_class = base_class(py, "IndirectObject")?;
    let max_length: usize = indirect_class.getattr("_MAXIMUM_PART_LENGTH")?.extract()?;
    let utils = utils_module(py)?;
    let whitespaces = whitespaces(&utils)?;

    let idnum = read_indirect_part(py, stream, "Object ID", false, &whitespaces, max_length)?;
    let generation = read_indirect_part(
        py,
        stream,
        "Generation number",
        true,
        &whitespaces,
        max_length,
    )?;

    let r = utils.call_method1("read_non_whitespace", (stream,))?;
    if !r.eq(py_bytes(py, b"R"))? {
        let position = builtins(py)?
            .getattr("hex")?
            .call1((stream.call_method0("tell")?,))?;
        return Err(errors::pdf_read_error(
            py,
            &format!(
                "Error reading indirect object reference at byte {}",
                pyutil::py_str(&position)?
            ),
        ));
    }

    let conversion: PyResult<Bound<'py, PyAny>> = (|| {
        let int = int_type(py)?;
        let idnum = int.call1((idnum.clone(),))?;
        let generation = int.call1((generation.clone(),))?;
        indirect_class.call1((idnum, generation, pdf))
    })();
    match conversion {
        Ok(value) => Ok(value.unbind()),
        Err(error)
            if error.is_instance_of::<PyValueError>(py)
                || error.is_instance_of::<PyOverflowError>(py) =>
        {
            let message = format!(
                "Invalid indirect object reference ({} {} R): {}",
                pyutil::py_repr(&idnum)?,
                pyutil::py_repr(&generation)?,
                pyutil::py_str(error.value(py).as_any())?
            );
            let wrapped = errors::pdf_read_error(py, &message);
            wrapped.set_cause(py, Some(error));
            Err(wrapped)
        }
        Err(error) => Err(error),
    }
}

/// Upstream `IndirectObject.read_from_stream.read_part` nested helper.
fn read_indirect_part<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
    name: &str,
    skip_leading_whitespace: bool,
    whitespaces: &[Vec<u8>],
    max_length: usize,
) -> PyResult<Bound<'py, PyAny>> {
    let mut result: Vec<u8> = Vec::new();
    loop {
        let tok = stream.call_method1("read", (1,))?;
        let raw: Vec<u8> = tok.extract()?;
        if raw.is_empty() {
            let message: String = py
                .import("pypdf.errors")?
                .getattr("STREAM_TRUNCATED_PREMATURELY")?
                .extract()?;
            return Err(errors::pdf_stream_error(py, &message));
        }
        let is_space = tok.call_method0("isspace")?.is_truthy()?;
        let in_whitespaces = whitespaces.iter().any(|entry| entry == &raw);
        if is_space || in_whitespaces {
            if skip_leading_whitespace && result.is_empty() {
                continue;
            }
            break;
        }
        if result.len() >= max_length {
            return Err(errors::pdf_read_error(
                py,
                &format!("{name} exceeds maximum length limit of {max_length}."),
            ));
        }
        result.push(raw[0]);
    }
    Ok(py_bytes(py, &result))
}

// ---------------------------------------------------------------------------
// FloatObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._base.FloatObject.__new__
#[pyfunction]
fn basemod_float_new<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
    _context: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let builtins = builtins(py)?;
    let float_type = builtins.getattr("float")?;
    let new = float_type.getattr("__new__")?;
    match float_type.call1((value,)) {
        Ok(converted) => Ok(new.call1((cls, converted))?.unbind()),
        Err(error) if error.is_instance_of::<PyMemoryError>(py) => Err(error),
        Err(error) => {
            // If this isn't a valid decimal (happens in malformed PDFs) fall
            // back to 0 and warn, exactly like upstream.
            let values = PyDict::new(py);
            values.set_item("error", error.value(py).as_any())?;
            values.set_item("value", value)?;
            pyutil::logger_warning(
                py,
                SOURCE,
                "%(error)s : FloatObject (%(value)s) invalid; use 0.0 instead",
                Some(&values),
            )?;
            Ok(new.call1((cls, py_float_zero(py)))?.unbind())
        }
    }
}

fn py_float_zero<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    pyo3::types::PyFloat::new(py, 0.0).into_any()
}

// @pypdf: pypdf.generic._base.FloatObject.clone
#[pyfunction]
fn basemod_float_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
) -> PyResult<Py<PyAny>> {
    let clone = base_class(py, "FloatObject")?.call1((slf,))?;
    Ok(slf
        .call_method1("_reference_clone", (clone, pdf_dest, force_duplicate))?
        .unbind())
}

// @pypdf: pypdf.generic._base.FloatObject.hash_bin
#[pyfunction]
fn basemod_float_hash_bin<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<isize> {
    let numeric = slf.call_method0("as_numeric")?;
    tuple_hash(py, vec![slf.get_type().into_any(), numeric])
}

// @pypdf: pypdf.generic._base.FloatObject.myrepr
#[pyfunction]
fn basemod_float_myrepr<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    precision: i32,
) -> PyResult<String> {
    let value: f64 = slf.extract()?;
    float_myrepr_with_precision(py, value, precision)
}

// @pypdf: pypdf.generic._base.FloatObject.__repr__
#[pyfunction]
fn basemod_float_repr<'py>(_py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    // Upstream delegates to `self.myrepr()` (keeps overrides/monkeypatching).
    Ok(slf.call_method0("myrepr")?.unbind())
}

// @pypdf: pypdf.generic._base.FloatObject.as_numeric
#[pyfunction]
fn basemod_float_as_numeric<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(builtins(py)?.getattr("float")?.call1((slf,))?.unbind())
}

// @pypdf: pypdf.generic._base.FloatObject.write_to_stream
#[pyfunction]
fn basemod_float_write_to_stream<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let rendered = slf.call_method0("myrepr")?;
    let encoded = rendered.call_method1("encode", ("utf8",))?;
    stream.call_method1("write", (encoded,))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// NumberObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._base.NumberObject.__new__
#[pyfunction]
fn basemod_number_new<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let int = int_type(py)?;
    let new = int.getattr("__new__")?;
    match int.call1((value,)) {
        Ok(converted) => Ok(new.call1((cls, converted))?.unbind()),
        Err(error) if error.is_instance_of::<PyValueError>(py) => {
            let values = PyDict::new(py);
            values.set_item("value", value)?;
            pyutil::logger_warning(
                py,
                SOURCE,
                "NumberObject(%(value)s) invalid; use 0 instead",
                Some(&values),
            )?;
            Ok(new.call1((cls, 0))?.unbind())
        }
        Err(error) => Err(error),
    }
}

// @pypdf: pypdf.generic._base.NumberObject.clone
#[pyfunction]
fn basemod_number_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
) -> PyResult<Py<PyAny>> {
    let clone = base_class(py, "NumberObject")?.call1((slf,))?;
    Ok(slf
        .call_method1("_reference_clone", (clone, pdf_dest, force_duplicate))?
        .unbind())
}

// @pypdf: pypdf.generic._base.NumberObject.hash_bin
#[pyfunction]
fn basemod_number_hash_bin<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<isize> {
    let numeric = slf.call_method0("as_numeric")?;
    tuple_hash(py, vec![slf.get_type().into_any(), numeric])
}

// @pypdf: pypdf.generic._base.NumberObject.as_numeric
#[pyfunction]
fn basemod_number_as_numeric<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    // Upstream: int(repr(self).encode("utf8"))
    let encoded = slf
        .call_method0("__repr__")?
        .call_method1("encode", ("utf8",))?;
    Ok(int_type(py)?.call1((encoded,))?.unbind())
}

// @pypdf: pypdf.generic._base.NumberObject.write_to_stream
#[pyfunction]
fn basemod_number_write_to_stream<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let encoded = slf
        .call_method0("__repr__")?
        .call_method1("encode", ("utf8",))?;
    stream.call_method1("write", (encoded,))?;
    Ok(())
}

// @pypdf: pypdf.generic._base.NumberObject.read_from_stream
#[pyfunction]
fn basemod_number_read_from_stream<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let number_class = base_class(py, "NumberObject")?;
    let pattern = number_class.getattr("NumberPattern")?;
    let length = number_class.getattr("_LENGTH_LIMIT")?;
    let utils = utils_module(py)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("stream", stream)?;
    kwargs.set_item("regex", &pattern)?;
    kwargs.set_item("length", &length)?;
    let num = utils.call_method("read_until_regex", (), Some(&kwargs))?;
    if num.contains(py_bytes(py, b"."))? {
        Ok(base_class(py, "FloatObject")?.call1((num,))?.unbind())
    } else {
        Ok(number_class.call1((num,))?.unbind())
    }
}

// ---------------------------------------------------------------------------
// ByteStringObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._base.ByteStringObject.clone
#[pyfunction]
fn basemod_bytestring_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
) -> PyResult<Py<PyAny>> {
    let raw = builtins(py)?.getattr("bytes")?.call1((slf,))?;
    let clone = base_class(py, "ByteStringObject")?.call1((raw,))?;
    Ok(slf
        .call_method1("_reference_clone", (clone, pdf_dest, force_duplicate))?
        .unbind())
}

// @pypdf: pypdf.generic._base.ByteStringObject.hash_bin
#[pyfunction]
fn basemod_bytestring_hash_bin<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<isize> {
    let raw = builtins(py)?.getattr("bytes")?.call1((slf,))?;
    tuple_hash(py, vec![slf.get_type().into_any(), raw])
}

// @pypdf: pypdf.generic._base.ByteStringObject.original_bytes
#[pyfunction]
fn basemod_bytestring_original_bytes<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    // Upstream: `return self`
    Ok(slf.clone().unbind())
}

// @pypdf: pypdf.generic._base.ByteStringObject.write_to_stream
#[pyfunction]
fn basemod_bytestring_write_to_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let raw = builtins(py)?.getattr("bytes")?.call1((slf,))?;
    let data = raw.cast::<PyBytes>()?.as_bytes();
    let mut hex = String::with_capacity(data.len() * 2);
    for byte in data {
        let _ = write!(hex, "{byte:02x}");
    }
    stream.call_method1("write", (py_bytes(py, b"<"),))?;
    stream.call_method1("write", (py_bytes(py, hex.as_bytes()),))?;
    stream.call_method1("write", (py_bytes(py, b">"),))?;
    Ok(())
}

// @pypdf: pypdf.generic._base.ByteStringObject.__str__
#[pyfunction]
fn basemod_bytestring_str<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let charsets = base_class(py, "NameObject")?.getattr("CHARSETS")?;
    let mut encodings: Vec<Bound<'py, PyAny>> = vec![PyString::new(py, "utf-16").into_any()];
    for encoding in charsets.try_iter()? {
        encodings.push(encoding?);
    }
    for encoding in encodings {
        match slf.call_method1("decode", (&encoding,)) {
            Ok(decoded) => return Ok(decoded.unbind()),
            Err(error) if error.is_instance_of::<PyUnicodeDecodeError>(py) => {}
            Err(error) => return Err(error),
        }
    }
    Err(errors::pdf_read_error(
        py,
        "Cannot decode ByteStringObject.",
    ))
}

// ---------------------------------------------------------------------------
// TextStringObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._base.TextStringObject.__new__
#[pyfunction]
fn basemod_textstring_new<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let builtins = builtins(py)?;
    let str_type = builtins.getattr("str")?;
    let bytes_type = builtins.getattr("bytes")?;
    let codecs = py.import("codecs")?;
    let str_new = str_type.getattr("__new__")?;

    let mut original_bytes: Option<Bound<'py, PyAny>> = None;
    let mut text_value = value.clone();
    if value.is_instance(&bytes_type)? {
        original_bytes = Some(value.clone());
        text_value = value.call_method1("decode", ("charmap",))?;
    }
    let mut obj = str_new.call1((cls, text_value))?;
    match &original_bytes {
        Some(bytes) => obj.setattr("_original_bytes", bytes)?,
        None => obj.setattr("_original_bytes", py.None())?,
    }
    obj.setattr("autodetect_utf16", false)?;
    obj.setattr("autodetect_pdfdocencoding", false)?;
    obj.setattr("utf16_bom", py_bytes(py, b""))?;

    let bom_le = codecs.getattr("BOM_UTF16_LE")?;
    let bom_be = codecs.getattr("BOM_UTF16_BE")?;
    let looks_like_bom = match &original_bytes {
        Some(bytes) => {
            let first_two = slice_of(py, bytes, Some(int_obj(py, 0)), Some(int_obj(py, 2)))?;
            first_two.eq(&bom_le)? || first_two.eq(&bom_be)?
        }
        None => false,
    };

    if looks_like_bom {
        let bytes = match &original_bytes {
            Some(bytes) => bytes,
            None => return Err(PyValueError::new_err("unreachable: BOM without bytes")),
        };
        match bytes.call_method1("decode", ("utf-16",)) {
            Ok(decoded) => obj = str_new.call1((cls, decoded))?,
            Err(error) if error.is_instance_of::<PyUnicodeDecodeError>(py) => {
                let exception = error.value(py);
                let initial = exception.as_any().getattr("object")?;
                let values = PyDict::new(py);
                values.set_item("exception", exception.as_any())?;
                values.set_item("initial_string", &initial)?;
                pyutil::logger_warning(
                    py,
                    SOURCE,
                    "%(exception)s; initial string: %(initial_string)r",
                    Some(&values),
                )?;
                let start = exception.as_any().getattr("start")?;
                let prefix = slice_of(py, bytes, None, Some(start))?;
                let decoded = prefix.call_method1("decode", ("utf-16",))?;
                obj = str_new.call1((cls, decoded))?;
            }
            Err(error) => return Err(error),
        }
        obj.setattr("_original_bytes", bytes)?;
        obj.setattr("autodetect_utf16", true)?;
        let bom = slice_of(py, bytes, Some(int_obj(py, 0)), Some(int_obj(py, 2)))?;
        obj.setattr("utf16_bom", bom)?;
    } else {
        // Upstream calls the module-level function so monkeypatching keeps working.
        match base_module(py)?
            .getattr("encode_pdfdocencoding")?
            .call1((&obj,))
        {
            Ok(_) => obj.setattr("autodetect_pdfdocencoding", true)?,
            Err(error) if error.is_instance_of::<PyUnicodeEncodeError>(py) => {
                obj.setattr("autodetect_utf16", true)?;
                obj.setattr("utf16_bom", &bom_be)?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(obj.unbind())
}

// @pypdf: pypdf.generic._base.TextStringObject.clone
#[pyfunction]
fn basemod_textstring_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
) -> PyResult<Py<PyAny>> {
    let obj = base_class(py, "TextStringObject")?.call1((slf,))?;
    obj.setattr("_original_bytes", slf.getattr("_original_bytes")?)?;
    obj.setattr(
        "autodetect_pdfdocencoding",
        slf.getattr("autodetect_pdfdocencoding")?,
    )?;
    obj.setattr("autodetect_utf16", slf.getattr("autodetect_utf16")?)?;
    obj.setattr("utf16_bom", slf.getattr("utf16_bom")?)?;
    Ok(slf
        .call_method1("_reference_clone", (obj, pdf_dest, force_duplicate))?
        .unbind())
}

// @pypdf: pypdf.generic._base.TextStringObject.hash_bin
#[pyfunction]
fn basemod_textstring_hash_bin<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<isize> {
    let original = slf.getattr("original_bytes")?;
    tuple_hash(py, vec![slf.get_type().into_any(), original])
}

// @pypdf: pypdf.generic._base.TextStringObject.original_bytes
#[pyfunction]
fn basemod_textstring_original_bytes<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let raw = slf.getattr("_original_bytes")?;
    if !raw.is_none() {
        return Ok(raw.unbind());
    }
    Ok(slf.call_method0("get_original_bytes")?.unbind())
}

// @pypdf: pypdf.generic._base.TextStringObject.get_original_bytes
#[pyfunction]
fn basemod_textstring_get_original_bytes<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let codecs = py.import("codecs")?;
    if slf.getattr("autodetect_utf16")?.is_truthy()? {
        let bom = slf.getattr("utf16_bom")?;
        let bom_le = codecs.getattr("BOM_UTF16_LE")?;
        let bom_be = codecs.getattr("BOM_UTF16_BE")?;
        if bom.eq(&bom_le)? {
            let encoded = slf.call_method1("encode", ("utf-16le",))?;
            return Ok(bom_le.call_method1("__add__", (encoded,))?.unbind());
        }
        if bom.eq(&bom_be)? {
            let encoded = slf.call_method1("encode", ("utf-16be",))?;
            return Ok(bom_be.call_method1("__add__", (encoded,))?.unbind());
        }
        return Ok(slf.call_method1("encode", ("utf-16be",))?.unbind());
    }
    if slf.getattr("autodetect_pdfdocencoding")?.is_truthy()? {
        return Ok(base_module(py)?
            .getattr("encode_pdfdocencoding")?
            .call1((slf,))?
            .unbind());
    }
    Err(message_exception(
        py,
        "Exception",
        "no information about original bytes",
    ))
}

// @pypdf: pypdf.generic._base.TextStringObject.get_encoded_bytes
#[pyfunction]
fn basemod_textstring_get_encoded_bytes<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let builtins = builtins(py)?;
    let inner: PyResult<Bound<'py, PyAny>> = (|| {
        let raw = slf.getattr("_original_bytes")?;
        if !raw.is_none() {
            return Ok(raw);
        }
        if slf.getattr("autodetect_utf16")?.is_truthy()? {
            // Forced UnicodeEncodeError("", "forced", -1, -1, "") as upstream.
            let exc = builtins
                .getattr("UnicodeEncodeError")?
                .call1(("", "forced", -1, -1, ""))?;
            return Err(PyErr::from_value(exc));
        }
        base_module(py)?
            .getattr("encode_pdfdocencoding")?
            .call1((slf,))
    })();
    match inner {
        Ok(bytes) => Ok(bytes.unbind()),
        Err(error) if error.is_instance_of::<PyUnicodeEncodeError>(py) => {
            let codecs = py.import("codecs")?;
            let bom = slf.getattr("utf16_bom")?;
            let bom_le = codecs.getattr("BOM_UTF16_LE")?;
            let bom_be = codecs.getattr("BOM_UTF16_BE")?;
            let encoded = if bom.eq(&bom_le)? {
                let encoded = slf.call_method1("encode", ("utf-16le",))?;
                bom_le.call_method1("__add__", (encoded,))?
            } else if bom.eq(&bom_be)? {
                let encoded = slf.call_method1("encode", ("utf-16be",))?;
                bom_be.call_method1("__add__", (encoded,))?
            } else {
                slf.call_method1("encode", ("utf-16be",))?
            };
            Ok(encoded.unbind())
        }
        Err(error) => Err(error),
    }
}

// @pypdf: pypdf.generic._base.TextStringObject.write_to_stream
#[pyfunction]
fn basemod_textstring_write_to_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let bytearr = slf.call_method0("get_encoded_bytes")?;
    let raw = builtins(py)?.getattr("bytes")?.call1((bytearr,))?;
    let data = raw.cast::<PyBytes>()?.as_bytes().to_vec();
    stream.call_method1("write", (py_bytes(py, b"("),))?;
    for byte in data {
        if byte.is_ascii_alphanumeric() || byte == b' ' {
            stream.call_method1("write", (py_bytes(py, &[byte]),))?;
        } else {
            let escaped = format!("\\{byte:03o}");
            stream.call_method1("write", (py_bytes(py, escaped.as_bytes()),))?;
        }
    }
    stream.call_method1("write", (py_bytes(py, b")"),))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// NameObject
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._base.NameObject.clone
#[pyfunction]
fn basemod_name_clone<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pdf_dest: &Bound<'py, PyAny>,
    force_duplicate: bool,
) -> PyResult<Py<PyAny>> {
    let clone = base_class(py, "NameObject")?.call1((slf,))?;
    Ok(slf
        .call_method1("_reference_clone", (clone, pdf_dest, force_duplicate))?
        .unbind())
}

// @pypdf: pypdf.generic._base.NameObject.hash_bin
#[pyfunction]
fn basemod_name_hash_bin<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<isize> {
    tuple_hash(py, vec![slf.get_type().into_any(), slf.clone()])
}

// @pypdf: pypdf.generic._base.NameObject.write_to_stream
#[pyfunction]
fn basemod_name_write_to_stream<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let rendered = slf.call_method0("renumber")?;
    stream.call_method1("write", (rendered,))?;
    Ok(())
}


// ---------------------------------------------------------------------------
// Byte-level writer fast path (P6): render an object tree to bytes, then write
// once per top-level object.  Deep-immutable objects are rendered from their
// exact classes; anything else (subclasses, stream objects, big integers)
// reports `None` so the caller falls back to the per-entry writer.
// ---------------------------------------------------------------------------

struct LeafClasses {
    name: Py<PyAny>,
    number: Py<PyAny>,
    float: Py<PyAny>,
    boolean: Py<PyAny>,
    null: Py<PyAny>,
    indirect: Py<PyAny>,
    bytestring: Py<PyAny>,
    textstring: Py<PyAny>,
}

fn leaf_classes(py: Python<'_>) -> PyResult<&'static LeafClasses> {
    static CLASSES: pyo3::sync::PyOnceLock<LeafClasses> = pyo3::sync::PyOnceLock::new();
    CLASSES.get_or_try_init(py, || {
        let base = crate::pyutil::cached_module(py, "pypdf.generic._base")?;
        Ok(LeafClasses {
            name: base.getattr("NameObject")?.unbind(),
            number: base.getattr("NumberObject")?.unbind(),
            float: base.getattr("FloatObject")?.unbind(),
            boolean: base.getattr("BooleanObject")?.unbind(),
            null: base.getattr("NullObject")?.unbind(),
            indirect: base.getattr("IndirectObject")?.unbind(),
            bytestring: base.getattr("ByteStringObject")?.unbind(),
            textstring: base.getattr("TextStringObject")?.unbind(),
        })
    })
}

/// Render one leaf object exactly like its `write_to_stream`; `None` when the
/// object is not a supported exact class.
pub(crate) fn render_leaf_bytes(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Option<Vec<u8>>> {
    let classes = leaf_classes(py)?;
    if value.is_exact_instance(&classes.name.bind(py)) {
        let rendered: Vec<u8> = basemod_name_renumber(py, value)?.extract(py)?;
        return Ok(Some(rendered));
    }
    if value.is_exact_instance(&classes.number.bind(py)) {
        return Ok(value.extract::<i64>().ok().map(|number| number.to_string().into_bytes()));
    }
    if value.is_exact_instance(&classes.float.bind(py)) {
        let precision: i32 = py
            .import("pypdf.generic._base")?
            .getattr("FLOAT_WRITE_PRECISION")?
            .extract()?;
        let value: f64 = value.extract()?;
        let rendered = float_myrepr_with_precision(py, value, precision)?;
        return Ok(Some(rendered.into_bytes()));
    }
    if value.is_exact_instance(&classes.boolean.bind(py)) {
        let truthy = value.getattr("value")?.is_truthy()?;
        return Ok(Some(if truthy { b"true".to_vec() } else { b"false".to_vec() }));
    }
    if value.is_exact_instance(&classes.null.bind(py)) {
        return Ok(Some(b"null".to_vec()));
    }
    if value.is_exact_instance(&classes.indirect.bind(py)) {
        let idnum: i64 = value.getattr("idnum")?.extract()?;
        let generation: i64 = value.getattr("generation")?.extract()?;
        return Ok(Some(format!("{idnum} {generation} R").into_bytes()));
    }
    if value.is_exact_instance(&classes.bytestring.bind(py)) {
        let raw: Vec<u8> = value.extract()?;
        let mut out = Vec::with_capacity(raw.len() * 2 + 2);
        out.push(b'<');
        for byte in &raw {
            out.extend_from_slice(format!("{byte:02x}").as_bytes());
        }
        out.push(b'>');
        return Ok(Some(out));
    }
    if value.is_exact_instance(&classes.textstring.bind(py)) {
        let data: Vec<u8> = basemod_textstring_get_encoded_bytes(py, value)?.extract(py)?;
        let mut out = Vec::with_capacity(data.len() + 2);
        out.push(b'(');
        for byte in data {
            if byte.is_ascii_alphanumeric() || byte == b' ' {
                out.push(byte);
            } else {
                out.extend_from_slice(format!("\\{byte:03o}").as_bytes());
            }
        }
        out.push(b')');
        return Ok(Some(out));
    }
    Ok(None)
}

// @pypdf: pypdf.generic._base.NameObject.renumber
#[pyfunction]
fn basemod_name_renumber<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    // Upstream: `out = self[0].encode("utf-8")`, then per-character escaping with
    // `renumber_table`.  Doing this on the Rust string avoids a Python slice, an
    // iterator and several `encode()` calls per name (names are renumbered for
    // every key written by the writer).
    let name: String = slf.extract()?;
    if name.is_empty() {
        return Err(PyIndexError::new_err("string index out of range"));
    }
    if !name.starts_with('/') {
        let message = format!(
            "Incorrect first char in NameObject, should start with '/': ({name})"
        );
        return Err(raise_pdf_error_with_py_message(
            py,
            "PdfStreamError",
            PyString::new(py, &message).into_any(),
        ));
    }
    let mut parts: Vec<u8> = Vec::with_capacity(name.len() + 8);
    parts.push(b'/');
    let mut table: Option<Bound<'py, PyAny>> = None;
    let mut encoded = [0u8; 4];
    for character in name[1..].chars() {
        if character > '~' {
            let mut escaped = String::with_capacity(3);
            for byte in character.to_string().as_bytes() {
                escaped.clear();
                let _ = write!(escaped, "#{byte:02X}");
                parts.extend_from_slice(escaped.as_bytes());
            }
        } else {
            let key = PyString::new(py, character.encode_utf8(&mut encoded));
            let table_ref = match &table {
                Some(found) => found,
                None => {
                    table = Some(base_class(py, "NameObject")?.getattr("renumber_table")?);
                    table.as_ref().expect("renumber table")
                }
            };
            match table_ref.call_method1("__getitem__", (&key,)) {
                Ok(replacement) => {
                    let replacement: Vec<u8> = replacement.extract()?;
                    parts.extend_from_slice(&replacement);
                }
                Err(error) if error.is_instance_of::<PyKeyError>(py) => {
                    parts.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
                }
                Err(error) => return Err(error),
            }
        }
    }
    Ok(py_bytes(py, &parts).unbind())
}

// @pypdf: pypdf.generic._base.NameObject._sanitize
#[pyfunction]
fn basemod_name_sanitize<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let name = builtins(py)?
        .getattr("str")?
        .call1((slf,))?
        .call_method1("removeprefix", (PyString::new(py, "/"),))?;
    let re = py.import("re")?;
    let name = re.call_method1(
        "sub",
        (PyString::new(py, " "), PyString::new(py, "_"), name),
    )?;
    let name = re.call_method1(
        "sub",
        (
            PyString::new(py, "[^a-zA-Z0-9_-]"),
            PyString::new(py, "_"),
            name,
        ),
    )?;
    let prefixed = PyString::new(py, "/").call_method1("__add__", (name,))?;
    Ok(base_class(py, "NameObject")?.call1((prefixed,))?.unbind())
}

// @pypdf: pypdf.generic._base.NameObject.unnumber
#[pyfunction]
fn basemod_name_unnumber<'py>(py: Python<'py>, sin: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let data: Vec<u8> = sin.extract()?;
    if !data.contains(&b'#') {
        return Ok(py_bytes(py, &data).unbind());
    }
    let mut result: Vec<u8> = Vec::with_capacity(data.len());
    let int = int_type(py)?;
    let mut i = 0usize;
    while i < data.len() {
        if data[i] == b'#' {
            let end = (i + 3).min(data.len());
            let slice = py_bytes(py, &data[i + 1..end]);
            let parsed: PyResult<Bound<'py, PyAny>> = int.call1((slice, 16));
            if let Ok(value) = parsed {
                if let Ok(value) = value.extract::<i64>() {
                    if (0..=255).contains(&value) {
                        result.push(value as u8);
                        i += 3;
                        continue;
                    }
                }
            }
        }
        result.push(data[i]);
        i += 1;
    }
    Ok(py_bytes(py, &result).unbind())
}

// @pypdf: pypdf.generic._base.NameObject.read_from_stream
#[pyfunction]
fn basemod_name_read_from_stream<'py>(
    py: Python<'py>,
    stream: &Bound<'py, PyAny>,
    pdf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let name_class = base_class(py, "NameObject")?;
    let prefix = name_class.getattr("prefix")?;
    let name = stream.call_method1("read", (1,))?;
    if !name.eq(&prefix)? {
        return Err(errors::pdf_read_error(py, "Name read error"));
    }
    let delimiter_pattern = name_class.getattr("delimiter_pattern")?;
    let length = name_class.getattr("_LENGTH_LIMIT")?;
    let utils = utils_module(py)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("stream", stream)?;
    kwargs.set_item("regex", &delimiter_pattern)?;
    kwargs.set_item("length", &length)?;
    let tail = utils.call_method("read_until_regex", (), Some(&kwargs))?;
    let name = name.call_method1("__add__", (tail,))?;

    let unnumbered = name_class.call_method1("unnumber", (&name,))?;
    let outcome: PyResult<Bound<'py, PyAny>> = (|| {
        let charsets = name_class.getattr("CHARSETS")?;
        for encoding in charsets.try_iter()? {
            let encoding = encoding?;
            match unnumbered.call_method1("decode", (&encoding,)) {
                Ok(decoded) => return name_class.call1((decoded,)),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(_) => continue,
            }
        }
        let exc = builtins(py)?.getattr("UnicodeDecodeError")?.call1((
            "",
            unnumbered.clone(),
            0,
            0,
            "Code Not Found",
        ))?;
        Err(PyErr::from_value(exc))
    })();

    match outcome {
        Ok(value) => Ok(value.unbind()),
        Err(error)
            if error.is_instance_of::<PyUnicodeDecodeError>(py)
                || error.is_instance_of::<PyUnicodeEncodeError>(py) =>
        {
            if !pdf.getattr("strict")?.is_truthy()? {
                let values = PyDict::new(py);
                // Upstream logs the (unnumbered) raw bytes.
                values.set_item("name", &unnumbered)?;
                pyutil::logger_warning(
                    py,
                    SOURCE,
                    "Illegal character in NameObject (%(name)r), you may need to adjust NameObject.CHARSETS",
                    Some(&values),
                )?;
                let decoded = unnumbered.call_method1("decode", ("charmap",))?;
                return Ok(name_class.call1((decoded,))?.unbind());
            }
            let message = format!(
                "Illegal character in NameObject ({}). You may need to adjust NameObject.CHARSETS.",
                pyutil::py_repr(&unnumbered)?
            );
            let wrapped = errors::pdf_read_error(py, &message);
            wrapped.set_cause(py, Some(error));
            Err(wrapped)
        }
        Err(error) => Err(error),
    }
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._base.encode_pdfdocencoding
#[pyfunction]
fn basemod_encode_pdfdocencoding<'py>(
    py: Python<'py>,
    unicode_string: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let rev = py
        .import("pypdf._codecs")?
        .getattr("_pdfdoc_encoding_rev")?;
    let mut result: Vec<u8> = Vec::new();
    for key in unicode_string.try_iter()? {
        let key = key?;
        match rev.get_item(&key) {
            Ok(value) => match value.extract::<u8>() {
                Ok(byte) => result.push(byte),
                Err(_) => return Err(PyValueError::new_err("bytes must be in range(0, 256)")),
            },
            Err(error) if error.is_instance_of::<PyKeyError>(py) => {
                let args = PyTuple::new(
                    py,
                    [
                        PyString::new(py, "pdfdocencoding").into_any(),
                        unicode_string.clone(),
                        int_obj(py, -1),
                        int_obj(py, -1),
                        PyString::new(py, "does not exist in translation table").into_any(),
                    ],
                )?;
                let exc = builtins(py)?.getattr("UnicodeEncodeError")?.call1(args)?;
                let wrapped = PyErr::from_value(exc);
                wrapped.set_cause(py, Some(error));
                return Err(wrapped);
            }
            Err(error) => return Err(error),
        }
    }
    Ok(py_bytes(py, &result).unbind())
}

// @pypdf: pypdf.generic._base.is_null_or_none
#[pyfunction]
fn basemod_is_null_or_none<'py>(py: Python<'py>, x: &Bound<'py, PyAny>) -> PyResult<bool> {
    if x.is_none() {
        return Ok(true);
    }
    if !x.is_instance(&base_class(py, "PdfObject")?)? {
        return Ok(false);
    }
    // Upstream calls `get_object()` twice; keep the exact call pattern.
    let obj = x.call_method0("get_object")?;
    if obj.is_none() {
        return Ok(true);
    }
    let obj = x.call_method0("get_object")?;
    obj.is_instance(&base_class(py, "NullObject")?)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(basemod_pdfobject_hash_bin, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_pdfobject_hash_value_data, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_pdfobject_hash_value, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_pdfobject_replicate, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_pdfobject_clone, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_pdfobject_reference_clone, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_pdfobject_get_object, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_pdfobject_write_to_stream, module)?)?;

    module.add_function(wrap_pyfunction!(basemod_null_clone, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_null_hash_bin, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_null_write_to_stream, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_null_read_from_stream, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_null_repr, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_null_eq, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_null_hash, module)?)?;

    module.add_function(wrap_pyfunction!(basemod_boolean_clone, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_boolean_hash_bin, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_boolean_eq, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_boolean_hash, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_boolean_repr, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_boolean_write_to_stream, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_boolean_read_from_stream, module)?)?;

    module.add_function(wrap_pyfunction!(basemod_indirect_hash, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_hash_bin, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_replicate, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_clone, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_get_object, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_deepcopy, module)?)?;
    module.add_function(wrap_pyfunction!(
        basemod_indirect_get_object_with_check,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_getattr, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_getitem, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_contains, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_iter, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_float, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_int, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_str, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_repr, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_eq, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_ne, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_write_to_stream, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_indirect_read_from_stream, module)?)?;

    module.add_function(wrap_pyfunction!(basemod_float_new, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_float_clone, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_float_hash_bin, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_float_myrepr, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_float_repr, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_float_as_numeric, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_float_write_to_stream, module)?)?;

    module.add_function(wrap_pyfunction!(basemod_number_new, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_number_clone, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_number_hash_bin, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_number_as_numeric, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_number_write_to_stream, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_number_read_from_stream, module)?)?;

    module.add_function(wrap_pyfunction!(basemod_bytestring_clone, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_bytestring_hash_bin, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_bytestring_original_bytes, module)?)?;
    module.add_function(wrap_pyfunction!(
        basemod_bytestring_write_to_stream,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(basemod_bytestring_str, module)?)?;

    module.add_function(wrap_pyfunction!(basemod_textstring_new, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_textstring_clone, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_textstring_hash_bin, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_textstring_original_bytes, module)?)?;
    module.add_function(wrap_pyfunction!(
        basemod_textstring_get_original_bytes,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        basemod_textstring_get_encoded_bytes,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        basemod_textstring_write_to_stream,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(basemod_name_clone, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_name_hash_bin, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_name_write_to_stream, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_name_renumber, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_name_sanitize, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_name_unnumber, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_name_read_from_stream, module)?)?;

    module.add_function(wrap_pyfunction!(basemod_encode_pdfdocencoding, module)?)?;
    module.add_function(wrap_pyfunction!(basemod_is_null_or_none, module)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Symbols implemented in the hand-written shim
// (`python/pypdf/generic/_base.py`).
//
// The class objects, their class attributes, the pure attribute-assignment
// `__init__`s and the identity `IndirectObject.indirect_reference` property are
// *definitions*, not logic: they must exist on the Python side so that the type
// hierarchy, `isinstance()` results, subclassing and the traced Python call graph
// match upstream exactly.  Each of them is listed here for gate A6; the
// implementation of every method body lives in this file.
//
// @pypdf: pypdf.generic._base.__author__
// @pypdf: pypdf.generic._base.__author_email__
// @pypdf: pypdf.generic._base.PdfObject
// @pypdf: pypdf.generic._base.PdfObject.hash_func
// @pypdf: pypdf.generic._base.PdfObject.indirect_reference
// @pypdf: pypdf.generic._base.NullObject
// @pypdf: pypdf.generic._base.BooleanObject
// @pypdf: pypdf.generic._base.BooleanObject.value
// @pypdf: pypdf.generic._base.BooleanObject.__init__
// @pypdf: pypdf.generic._base.IndirectObject
// @pypdf: pypdf.generic._base.IndirectObject._MAXIMUM_PART_LENGTH
// @pypdf: pypdf.generic._base.IndirectObject.__init__
// @pypdf: pypdf.generic._base.IndirectObject.indirect_reference
// @pypdf: pypdf.generic._base.FLOAT_WRITE_PRECISION
// @pypdf: pypdf.generic._base.FloatObject
// @pypdf: pypdf.generic._base.NumberObject
// @pypdf: pypdf.generic._base.NumberObject.NumberPattern
// @pypdf: pypdf.generic._base.NumberObject._LENGTH_LIMIT
// @pypdf: pypdf.generic._base.ByteStringObject
// @pypdf: pypdf.generic._base.TextStringObject
// @pypdf: pypdf.generic._base.TextStringObject.autodetect_pdfdocencoding
// @pypdf: pypdf.generic._base.TextStringObject.autodetect_utf16
// @pypdf: pypdf.generic._base.TextStringObject.utf16_bom
// @pypdf: pypdf.generic._base.TextStringObject._original_bytes
// @pypdf: pypdf.generic._base.NameObject
// @pypdf: pypdf.generic._base.NameObject.delimiter_pattern
// @pypdf: pypdf.generic._base.NameObject.prefix
// @pypdf: pypdf.generic._base.NameObject.renumber_table
// @pypdf: pypdf.generic._base.NameObject._LENGTH_LIMIT
// @pypdf: pypdf.generic._base.NameObject.CHARSETS
