//! Shared helpers for the `pypdf._text_extraction` port.
//!
//! Upstream text extraction is Python code operating on Python objects with very
//! precise semantics (string ordering, `int`/`float` coercions, `round`,
//! `math.sqrt`, `isinstance`, monkeypatchable module globals).  The port keeps
//! exactly that: every helper here wraps a Python semantic instead of
//! re-implementing it in Rust, so that observable behaviour stays identical.
//!
//! A hard rule that shapes this file: text runs may contain lone surrogates
//! (decoding with ``surrogatepass`` can produce them) and therefore must never be
//! converted to a Rust `String`. All string payloads stay `Bound<PyAny>`;
//! `String` is only used for messages built from Rust literals.

use pyo3::exceptions::{PyAttributeError, PyKeyError, PyMemoryError, PyStopIteration};
use pyo3::prelude::*;
use pyo3::types::{
    PyAny, PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyModule, PyString, PyTuple,
};

pub const SOURCE: &str = "pypdf._text_extraction";
pub const SOURCE_EXTRACTOR: &str = "pypdf._text_extraction._text_extractor";
pub const SOURCE_LAYOUT: &str = "pypdf._text_extraction._layout_mode._fixed_width_page";
pub const SOURCE_MANAGER: &str = "pypdf._text_extraction._layout_mode._text_state_manager";
pub const SOURCE_PARAMS: &str = "pypdf._text_extraction._layout_mode._text_state_params";

pub fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    crate::pyutil::cached_module(py, "builtins")
}

/// The `pypdf._text_extraction` shim module; its globals drive RTL handling.
pub fn init_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    crate::pyutil::cached_module(py, "pypdf._text_extraction")
}

/// `pypdf._text_extraction.<name>` resolved at call time, like upstream's
/// module-global lookup (tests monkeypatch `is_char_neutral`/`is_char_rtl`).
pub fn init_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    init_module(py)?.getattr(name)
}

pub fn generic<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    crate::pyutil::cached_module(py, "pypdf.generic")
}

pub fn generic_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    generic(py)?.getattr(name)
}

pub fn font_class<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    crate::pyutil::cached_module(py, "pypdf.generic._font")?.getattr("Font")
}

pub fn font_descriptor_class<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    crate::pyutil::cached_module(py, "pypdf.generic._font")?.getattr("FontDescriptor")
}

pub fn orientation_error<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    init_attr(py, "OrientationNotFoundError")
}

/// `raise OrientationNotFoundError` (no arguments, like upstream).
pub fn raise_orientation_not_found(py: Python<'_>) -> PyErr {
    match orientation_error(py).and_then(|class| class.call0()) {
        Ok(instance) => PyErr::from_value(instance),
        Err(error) => error,
    }
}

// ---------------------------------------------------------------------------
// Object construction
// ---------------------------------------------------------------------------

pub fn none<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    py.None().into_bound(py)
}

pub fn py_int<'py>(py: Python<'py>, value: i64) -> Bound<'py, PyAny> {
    PyInt::new(py, value).into_any()
}

pub fn py_float<'py>(py: Python<'py>, value: f64) -> Bound<'py, PyAny> {
    PyFloat::new(py, value).into_any()
}

pub fn py_str_val<'py>(py: Python<'py>, value: &str) -> Bound<'py, PyAny> {
    PyString::new(py, value).into_any()
}

pub fn py_bytes_val<'py>(py: Python<'py>, value: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, value).into_any()
}

pub fn py_bool<'py>(py: Python<'py>, value: bool) -> Bound<'py, PyAny> {
    PyBool::new(py, value).to_owned().into_any()
}

pub fn py_list<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyList::new(py, items)?.into_any())
}

pub fn py_tuple<'py>(
    py: Python<'py>,
    items: Vec<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyTuple::new(py, items)?.into_any())
}

/// `[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]` (a fresh list, as upstream builds one).
pub fn identity_matrix<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    py_list(
        py,
        vec![
            py_float(py, 1.0),
            py_float(py, 0.0),
            py_float(py, 0.0),
            py_float(py, 1.0),
            py_float(py, 0.0),
            py_float(py, 0.0),
        ],
    )
}

// ---------------------------------------------------------------------------
// Python builtins / operators
// ---------------------------------------------------------------------------

/// `float(obj)` through the builtin (`ValueError`/`TypeError` like Python).
pub fn float_of<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("float")?.call1((obj,))
}

/// `int(obj)` through the builtin.
pub fn int_of<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("int")?.call1((obj,))
}

/// `round(obj)` — no `ndigits`, so a float becomes an int like upstream.
pub fn round_int<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("round")?.call1((obj,))
}

/// `round(obj, ndigits)`.
pub fn round_digits<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    digits: i64,
) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("round")?.call1((obj, digits))
}

pub fn abs_of<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("abs")?.call1((obj,))
}

pub fn min_of<'py>(
    py: Python<'py>,
    a: &Bound<'py, PyAny>,
    b: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("min")?.call1((a, b))
}

pub fn max_of<'py>(
    py: Python<'py>,
    a: &Bound<'py, PyAny>,
    b: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("max")?.call1((a, b))
}

pub fn ceil_of<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    py.import("math")?.getattr("ceil")?.call1((obj,))
}

pub fn sqrt_of<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    py.import("math")?.getattr("sqrt")?.call1((obj,))
}

pub fn chr_of<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("chr")?.call1((obj,))
}

pub fn len_of(obj: &Bound<'_, PyAny>) -> PyResult<usize> {
    obj.len()
}

/// `iter(obj)` through the builtin, like a Python `for` loop does.
pub fn iter_of<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("iter")?.call1((obj,))
}

/// `next(iterator)`, with `StopIteration` mapped to `None`.
/// `slice(start, stop, step)`.
pub fn slice_of<'py>(
    py: Python<'py>,
    start: Option<&Bound<'py, PyAny>>,
    stop: Option<&Bound<'py, PyAny>>,
    step: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let slice_type = crate::pyutil::cached_attr(py, "builtins", "slice")?;
    let start = match start {
        Some(value) => value.clone(),
        None => none(py),
    };
    let stop = match stop {
        Some(value) => value.clone(),
        None => none(py),
    };
    let step = match step {
        Some(value) => value.clone(),
        None => none(py),
    };
    slice_type.call1((start, stop, step))
}

/// `x[-1:]` — the slice object used by upstream's `(output + text)[-1:]`.
pub fn slice_from_end(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let start = py_int(py, -1);
    slice_of(py, Some(&start), None, None)
}

pub fn next_item<'py>(iter: &Bound<'py, PyAny>) -> PyResult<Option<Bound<'py, PyAny>>> {
    match iter.call_method0("__next__") {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.is_instance_of::<PyStopIteration>(iter.py()) => Ok(None),
        Err(error) => Err(error),
    }
}

/// `dict[key]` with Python's `KeyError` for a missing key.
///
/// `PyDict` has an inherent `get_item` returning `Option`; upstream indexing must
/// raise `KeyError` instead, so go through `PyAny`'s `__getitem__`.
pub fn dict_get<'py>(
    dict: &Bound<'py, PyDict>,
    key: impl IntoPyObject<'py>,
) -> PyResult<Bound<'py, PyAny>> {
    dict.as_any().get_item(key)
}

/// `isinstance(obj, cls)`.
pub fn is_instance(obj: &Bound<'_, PyAny>, class: &Bound<'_, PyAny>) -> PyResult<bool> {
    obj.is_instance(class)
}

/// `isinstance(obj, <builtins name>)`, e.g. `is_int`, `is_str`.
pub fn is_instance_builtin(py: Python<'_>, obj: &Bound<'_, PyAny>, name: &str) -> PyResult<bool> {
    use pyo3::sync::PyOnceLock;
    static INT: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    static FLOAT: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    static STR: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    static BYTES: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    static LIST: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    static DICT: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    static TUPLE: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    static BOOL: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    let cell = match name {
        "int" => Some(&INT),
        "float" => Some(&FLOAT),
        "str" => Some(&STR),
        "bytes" => Some(&BYTES),
        "list" => Some(&LIST),
        "dict" => Some(&DICT),
        "tuple" => Some(&TUPLE),
        "bool" => Some(&BOOL),
        _ => None,
    };
    let class = match cell {
        Some(cell) => cell
            .get_or_try_init(py, || {
                builtins(py)?.getattr(name).map(|class| class.unbind())
            })?
            .bind(py)
            .clone(),
        None => builtins(py)?.getattr(name)?,
    };
    obj.is_instance(&class)
}

pub fn is_int(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    is_instance_builtin(py, obj, "int")
}

pub fn is_float(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    is_instance_builtin(py, obj, "float")
}

pub fn is_number(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(is_int(py, obj)? || is_float(py, obj)?)
}

pub fn is_str(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    is_instance_builtin(py, obj, "str")
}

pub fn is_bytes(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    is_instance_builtin(py, obj, "bytes")
}

pub fn is_str_or_bytes(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(is_str(py, obj)? || is_bytes(py, obj)?)
}

pub fn is_list(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    is_instance_builtin(py, obj, "list")
}

/// `str(obj)` (used for messages / debug output only).
pub fn str_of(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    obj.str()?.extract()
}

/// `len(a) + len(b)` for two strings, i.e. the length of `a + b`.
pub fn join_strings<'py>(
    py: Python<'py>,
    separator: &Bound<'py, PyAny>,
    items: &[Bound<'py, PyAny>],
) -> PyResult<Bound<'py, PyAny>> {
    let list = PyList::new(py, items)?;
    separator.call_method1("join", (list,))
}

pub fn concat<'py>(a: &Bound<'py, PyAny>, b: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    a.add(b)
}

// ---------------------------------------------------------------------------
// `mult` / `orient` — hot primitives shared by every text-extraction path
// ---------------------------------------------------------------------------

/// `pypdf._text_extraction.mult` (upstream list/matrix multiplication).
pub fn mult<'py>(
    py: Python<'py>,
    m: &Bound<'py, PyAny>,
    n: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // Matrices produced by the text extractor are lists of floats; doing the
    // 2x3 affine product on doubles avoids ~30 C-API arithmetic calls.
    if let (Ok(m_values), Ok(n_values)) = (m.extract::<Vec<f64>>(), n.extract::<Vec<f64>>()) {
        let float_type = crate::pyutil::cached_attr(py, "builtins", "float")?;
        let floats = |matrix: &Bound<'py, PyAny>| -> PyResult<bool> {
            for index in 0..6usize {
                if !matrix.get_item(index)?.is_instance(&float_type)? {
                    return Ok(false);
                }
            }
            Ok(true)
        };
        if m_values.len() == 6
            && n_values.len() == 6
            && floats(m)?
            && floats(n)?
        {
            let r0 = m_values[0] * n_values[0] + m_values[1] * n_values[2];
            let r1 = m_values[0] * n_values[1] + m_values[1] * n_values[3];
            let r2 = m_values[2] * n_values[0] + m_values[3] * n_values[2];
            let r3 = m_values[2] * n_values[1] + m_values[3] * n_values[3];
            let r4 = m_values[4] * n_values[0] + m_values[5] * n_values[2] + n_values[4];
            let r5 = m_values[4] * n_values[1] + m_values[5] * n_values[3] + n_values[5];
            return py_list(
                py,
                vec![
                    py_float(py, r0),
                    py_float(py, r1),
                    py_float(py, r2),
                    py_float(py, r3),
                    py_float(py, r4),
                    py_float(py, r5),
                ],
            );
        }
    }
    let m0 = m.get_item(0usize)?;
    let m1 = m.get_item(1usize)?;
    let m2 = m.get_item(2usize)?;
    let m3 = m.get_item(3usize)?;
    let m4 = m.get_item(4usize)?;
    let m5 = m.get_item(5usize)?;
    let n0 = n.get_item(0usize)?;
    let n1 = n.get_item(1usize)?;
    let n2 = n.get_item(2usize)?;
    let n3 = n.get_item(3usize)?;
    let n4 = n.get_item(4usize)?;
    let n5 = n.get_item(5usize)?;

    let r0 = m0.mul(&n0)?.add(m1.mul(&n2)?)?;
    let r1 = m0.mul(&n1)?.add(m1.mul(&n3)?)?;
    let r2 = m2.mul(&n0)?.add(m3.mul(&n2)?)?;
    let r3 = m2.mul(&n1)?.add(m3.mul(&n3)?)?;
    let r4 = m4.mul(&n0)?.add(m5.mul(&n2)?)?.add(n4)?;
    let r5 = m4.mul(&n1)?.add(m5.mul(&n3)?)?.add(n5)?;
    py_list(py, vec![r0, r1, r2, r3, r4, r5])
}

/// `pypdf._text_extraction.orient` (upstream orientation classification).
pub fn orient(py: Python<'_>, m: &Bound<'_, PyAny>) -> PyResult<i32> {
    let m3 = m.get_item(3usize)?;
    if m3.gt(1e-6f64)? {
        return Ok(0);
    }
    if m3.lt(-1e-6f64)? {
        return Ok(180);
    }
    let m1 = m.get_item(1usize)?;
    if m1.gt(0i64)? {
        return Ok(90);
    }
    let _ = py;
    Ok(270)
}

// ---------------------------------------------------------------------------
// errors / logging
// ---------------------------------------------------------------------------

/// Emulate ``logger_warning(message, source=..., **values)``.
pub fn logger_warning(
    py: Python<'_>,
    source: &str,
    message: &str,
    values: &[(&str, Bound<'_, PyAny>)],
) -> PyResult<()> {
    if values.is_empty() {
        crate::pyutil::logger_warning(py, source, message, None)
    } else {
        let kwargs = PyDict::new(py);
        for (key, value) in values {
            kwargs.set_item(*key, value)?;
        }
        crate::pyutil::logger_warning(py, source, message, Some(&kwargs))
    }
}

/// `{name: True}` — keyword arguments for `**{...}` call sites.
pub fn bool_kwargs<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyDict>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item(name, true)?;
    Ok(kwargs)
}

/// `mult` as resolved from the shim module globals (upstream `from .. import mult`).
pub fn manager_mult<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    crate::pyutil::cached_module(py, "pypdf._text_extraction._layout_mode._text_state_manager")?
        .getattr("mult")
}

pub fn is_memory_error(py: Python<'_>, error: &PyErr) -> bool {
    error.is_instance_of::<PyMemoryError>(py)
}

pub fn is_attribute_error(py: Python<'_>, error: &PyErr) -> bool {
    error.is_instance_of::<PyAttributeError>(py)
}

pub fn is_key_error(py: Python<'_>, error: &PyErr) -> bool {
    error.is_instance_of::<PyKeyError>(py)
}

pub fn is_index_error(py: Python<'_>, error: &PyErr) -> bool {
    error.is_instance_of::<pyo3::exceptions::PyIndexError>(py)
}

pub fn is_type_error(py: Python<'_>, error: &PyErr) -> bool {
    error.is_instance_of::<pyo3::exceptions::PyTypeError>(py)
}

pub fn is_value_error(py: Python<'_>, error: &PyErr) -> bool {
    error.is_instance_of::<pyo3::exceptions::PyValueError>(py)
}

/// `except (TypeError, ValueError)`.
pub fn is_type_or_value_error(py: Python<'_>, error: &PyErr) -> bool {
    is_type_error(py, error) || is_value_error(py, error)
}

/// `except (KeyError, IndexError)`.
pub fn is_key_or_index_error(py: Python<'_>, error: &PyErr) -> bool {
    is_key_error(py, error) || is_index_error(py, error)
}

/// `except (MemoryError, ...)`: the wide `except Exception` of upstream does not
/// catch `MemoryError`, so it is re-raised there.  Callers test this first.
pub fn is_not_catchable_as_exception(py: Python<'_>, error: &PyErr) -> bool {
    // `MemoryError` derives from `Exception`, but upstream explicitly re-raises it
    // before the broad handler in the `_handle_*` methods that have one.
    is_memory_error(py, error)
}

/// `except (AttributeError, KeyError)`.
pub fn is_attribute_or_key_error(py: Python<'_>, error: &PyErr) -> bool {
    is_attribute_error(py, error) || is_key_error(py, error)
}
