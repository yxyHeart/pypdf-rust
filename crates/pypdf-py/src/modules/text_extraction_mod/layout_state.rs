//! `_text_state_manager` and `_text_state_params` of the layout-mode port
//! (upstream `_layout_mode/_text_state_manager.py` and
//! `_layout_mode/_text_state_params.py`).
//!
//! `TextStateManager` and the `TextStateParams` dataclass stay Python objects in
//! the shims; this module holds their method bodies.  The `ChainMap`/`Counter`
//! stacks are manipulated through their Python API so that the map ordering —
//! which decides the effective transform and therefore the output — is Python's.

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyTuple};

use super::common;

fn manager_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyModule>> {
    crate::pyutil::cached_module(py, "pypdf._text_extraction._layout_mode._text_state_manager")
}

fn params_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyModule>> {
    crate::pyutil::cached_module(py, "pypdf._text_extraction._layout_mode._text_state_params")
}

fn get_attr<'py>(slf: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    slf.getattr(name)
}

fn set_attr<'py>(
    slf: &Bound<'py, PyAny>,
    name: &str,
    value: impl IntoPyObject<'py>,
) -> PyResult<()> {
    slf.setattr(name, value)
}

/// `mult` as resolved from the shim module globals (upstream `from .. import mult`).
fn mult_fn<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    manager_module(py)?.getattr("mult")
}

/// `TextStateManager._as_float(value, default)`.
pub fn as_float<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
    default: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    match common::float_of(py, value) {
        Ok(converted) => Ok(converted),
        Err(error) if common::is_type_or_value_error(py, &error) => Ok(default.clone()),
        Err(error) => Err(error),
    }
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager._as_float
#[pyfunction]
pub fn text_state_manager_as_float<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
    default: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    as_float(py, value, default)
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.raw_transform
#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn text_state_manager_raw_transform<'py>(
    py: Python<'py>,
    _a: &Bound<'py, PyAny>,
    _b: &Bound<'py, PyAny>,
    _c: &Bound<'py, PyAny>,
    _d: &Bound<'py, PyAny>,
    _e: &Bound<'py, PyAny>,
    _f: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let args = [
        _a.clone(),
        _b.clone(),
        _c.clone(),
        _d.clone(),
        _e.clone(),
        _f.clone(),
    ];
    raw_transform(py, &args)
}

/// `TextStateManager.raw_transform(*args)` with upstream's default matrix.
fn raw_transform_dict<'py>(
    py: Python<'py>,
    args: &[Bound<'py, PyAny>],
) -> PyResult<Bound<'py, PyDict>> {
    const DEFAULTS: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let result = PyDict::new(py);
    for (index, default) in DEFAULTS.iter().enumerate() {
        let default = common::py_float(py, *default);
        let value = match args.get(index) {
            Some(value) => as_float(py, value, &default)?,
            None => default,
        };
        result.set_item(index, value)?;
    }
    Ok(result)
}

fn raw_transform<'py>(py: Python<'py>, args: &[Bound<'py, PyAny>]) -> PyResult<Bound<'py, PyAny>> {
    Ok(raw_transform_dict(py, args)?.into_any())
}

/// `TextStateManager.new_transform(*args, is_text=..., is_render=...)`.
fn new_transform<'py>(
    py: Python<'py>,
    args: &[Bound<'py, PyAny>],
    is_text: bool,
    is_render: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let dict = raw_transform_dict(py, args)?;
    dict.set_item("is_text", common::py_bool(py, is_text))?;
    dict.set_item("is_render", common::py_bool(py, is_render))?;
    Ok(dict.into_any())
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.new_transform
#[pyfunction]
#[pyo3(signature = (_a, _b, _c, _d, _e, _f, is_text, is_render))]
#[allow(clippy::too_many_arguments)]
pub fn text_state_manager_new_transform<'py>(
    py: Python<'py>,
    _a: &Bound<'py, PyAny>,
    _b: &Bound<'py, PyAny>,
    _c: &Bound<'py, PyAny>,
    _d: &Bound<'py, PyAny>,
    _e: &Bound<'py, PyAny>,
    _f: &Bound<'py, PyAny>,
    is_text: bool,
    is_render: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let args = [
        _a.clone(),
        _b.clone(),
        _c.clone(),
        _d.clone(),
        _e.clone(),
        _f.clone(),
    ];
    new_transform(py, &args, is_text, is_render)
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.__init__
#[pyfunction]
pub fn text_state_manager_init(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<()> {
    let new_transform_method = get_attr(slf, "new_transform")?;
    let transform = new_transform_method.call0()?;
    let chain_map = py.import("collections")?.getattr("ChainMap")?;
    set_attr(slf, "transform_stack", chain_map.call1((transform,))?)?;
    let counter = py.import("collections")?.getattr("Counter")?.call0()?;
    set_attr(slf, "q_queue", counter)?;
    set_attr(
        slf,
        "q_depth",
        common::py_list(py, vec![common::py_int(py, 0)])?,
    )?;
    set_attr(slf, "Tc", common::py_float(py, 0.0))?;
    set_attr(slf, "Tw", common::py_float(py, 0.0))?;
    set_attr(slf, "Tz", common::py_float(py, 100.0))?;
    set_attr(slf, "TL", common::py_float(py, 0.0))?;
    set_attr(slf, "Ts", common::py_float(py, 0.0))?;
    set_attr(slf, "font_stack", PyList::empty(py))?;
    set_attr(slf, "font", common::none(py))?;
    set_attr(slf, "font_size", common::py_int(py, 0))?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.set_state_param
#[pyfunction]
pub fn text_state_manager_set_state_param<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    op: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let supported = [
        common::py_bytes_val(py, b"Tc"),
        common::py_bytes_val(py, b"Tz"),
        common::py_bytes_val(py, b"Tw"),
        common::py_bytes_val(py, b"TL"),
        common::py_bytes_val(py, b"Ts"),
    ];
    let mut known = false;
    for candidate in &supported {
        if op.eq(candidate)? {
            known = true;
            break;
        }
    }
    if !known {
        return Ok(());
    }
    let name_value = op.call_method0("decode")?;
    let name: String = name_value.extract()?;
    let raw = if common::is_list(py, value)? {
        if value.is_truthy()? {
            value.get_item(0usize)?
        } else {
            common::none(py)
        }
    } else {
        value.clone()
    };
    // a missing or non-numeric operand (e.g. a name from a malformed content
    // stream) leaves the parameter unchanged, matching the ROB handling of Td/Tm/Tf
    let current = get_attr(slf, &name)?;
    let converted = as_float(py, &raw, &current)?;
    set_attr(slf, &name, converted)?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.set_font
#[pyfunction]
pub fn text_state_manager_set_font<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    font: &Bound<'py, PyAny>,
    size: &Bound<'py, PyAny>,
) -> PyResult<()> {
    set_attr(slf, "font", font)?;
    set_attr(slf, "font_size", size)?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.text_state_params
#[pyfunction]
pub fn text_state_manager_text_state_params<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let font = get_attr(slf, "font")?;
    let font_class = common::font_class(py)?;
    if !font.is_instance(&font_class)? {
        return Err(crate::errors::pdf_read_error(
            py,
            "font not set: is PDF missing a Tf operator?",
        ));
    }
    let text_state_params_class = params_module(py)?.getattr("TextStateParams")?;
    let effective = get_attr(slf, "effective_transform")?;
    text_state_params_class.call1((
        value,
        font,
        get_attr(slf, "font_size")?,
        get_attr(slf, "Tc")?,
        get_attr(slf, "Tw")?,
        get_attr(slf, "Tz")?,
        get_attr(slf, "TL")?,
        get_attr(slf, "Ts")?,
        effective,
    ))
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.reset_tm
#[pyfunction]
pub fn text_state_manager_reset_tm<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    loop {
        let transform_stack = get_attr(slf, "transform_stack")?;
        let top = transform_stack.getattr("maps")?.get_item(0usize)?;
        let is_text = top.get_item("is_text")?;
        let is_render = top.get_item("is_render")?;
        if is_text.is_truthy()? || is_render.is_truthy()? {
            set_attr(slf, "transform_stack", transform_stack.getattr("parents")?)?;
        } else {
            break;
        }
    }
    let _ = py;
    get_attr(slf, "transform_stack")
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.reset_trm
#[pyfunction]
pub fn text_state_manager_reset_trm<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    loop {
        let transform_stack = get_attr(slf, "transform_stack")?;
        let top = transform_stack.getattr("maps")?.get_item(0usize)?;
        let is_render = top.get_item("is_render")?;
        if is_render.is_truthy()? {
            set_attr(slf, "transform_stack", transform_stack.getattr("parents")?)?;
        } else {
            break;
        }
    }
    let _ = py;
    get_attr(slf, "transform_stack")
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.remove_q
#[pyfunction]
pub fn text_state_manager_remove_q<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let font_entry = get_attr(slf, "font_stack")?.call_method1("pop", (-1i64,))?;
    set_attr(slf, "font", font_entry.get_item(0usize)?)?;
    set_attr(slf, "font_size", font_entry.get_item(1usize)?)?;
    let transform_stack = slf.call_method0("reset_tm")?;
    set_attr(slf, "transform_stack", transform_stack)?;
    let depth = get_attr(slf, "q_depth")?.call_method0("pop")?;
    let count = get_attr(slf, "q_queue")?.call_method1("pop", (depth, common::py_int(py, 0)))?;
    let stack = get_attr(slf, "transform_stack")?;
    let maps = stack.getattr("maps")?;
    let tail = maps.get_item(common::slice_of(py, Some(&count), None, None)?)?;
    stack.setattr("maps", tail)?;
    get_attr(slf, "transform_stack")
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.add_q
#[pyfunction]
pub fn text_state_manager_add_q(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<()> {
    let entry = common::py_tuple(
        py,
        vec![get_attr(slf, "font")?, get_attr(slf, "font_size")?],
    )?;
    get_attr(slf, "font_stack")?.call_method1("append", (entry,))?;
    let depth = get_attr(slf, "q_depth")?;
    let length = common::py_int(py, common::len_of(&depth)? as i64);
    depth.call_method1("append", (length,))?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.add_cm
#[pyfunction]
#[pyo3(signature = (slf, *args))]
pub fn text_state_manager_add_cm<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let transform_stack = slf.call_method0("reset_tm")?;
    set_attr(slf, "transform_stack", transform_stack)?;
    // self.q_queue.update(self.q_depth[-1:])
    let depth_slice = get_attr(slf, "q_depth")?.get_item(common::slice_of(
        py,
        Some(&common::py_int(py, -1)),
        None,
        None,
    )?)?;
    get_attr(slf, "q_queue")?.call_method1("update", (depth_slice,))?;
    // self.transform_stack.new_child(self.new_transform(*args[-6:]))
    let items: Vec<Bound<'py, PyAny>> = args.iter().collect();
    let start = items.len().saturating_sub(6);
    let tail = &items[start..];
    let new_child = get_attr(slf, "new_transform")?.call1(PyTuple::new(py, tail)?)?;
    let stack = get_attr(slf, "transform_stack")?;
    let stacked = stack.call_method1("new_child", (new_child,))?;
    set_attr(slf, "transform_stack", stacked)?;
    get_attr(slf, "transform_stack")
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager._complete_matrix
#[pyfunction]
pub fn text_state_manager_complete_matrix<'py>(
    py: Python<'py>,
    _slf: &Bound<'py, PyAny>,
    operands: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let operands = if common::len_of(operands)? == 2 {
        // this is a Td operator or equivalent
        common::py_list(
            py,
            vec![
                common::py_float(py, 1.0),
                common::py_float(py, 0.0),
                common::py_float(py, 0.0),
                common::py_float(py, 1.0),
                operands.get_item(0usize)?,
                operands.get_item(1usize)?,
            ],
        )?
    } else {
        operands.clone()
    };
    // Tm consumes the 6 operands nearest to it; extra values from a
    // malformed content stream must not overflow the transform.
    operands.get_item(common::slice_of(
        py,
        Some(&common::py_int(py, -6)),
        None,
        None,
    )?)
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.add_tm
#[pyfunction]
pub fn text_state_manager_add_tm<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    operands: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // Build a new text transformation matrix from the supplied operands.
    let completed = slf.call_method1("_complete_matrix", (operands,))?;
    let values: Vec<Bound<'py, PyAny>> = completed.try_iter()?.collect::<PyResult<Vec<_>>>()?;
    let new_child = get_attr(slf, "new_transform")?.call(
        PyTuple::new(py, &values)?,
        Some(&common::bool_kwargs(py, "is_text")?),
    )?;
    // Get the most recently added transformation from the stack.
    let stack = get_attr(slf, "transform_stack")?;
    let maps = stack.getattr("maps")?;
    let top = maps.get_item(0usize)?;

    // If the current transformation is a text matrix and is not a rendering
    // transformation, combine the matrices instead of adding another stack
    // entry. This prevents unnecessary growth of the transform stack.
    // Non-rendering transformations do not emit anything and thus can be merged.
    let top_is_text = top.call_method1("get", ("is_text",))?.is_truthy()?;
    let top_is_render = top.call_method1("get", ("is_render",))?.is_truthy()?;
    if top_is_text && !top_is_render {
        // Multiply the new matrix by the current top matrix, preserving the
        // transformation order used when a new stack entry would be pushed.
        let child_values = common::builtins(py)?
            .getattr("list")?
            .call1((new_child.call_method0("values")?,))?;
        let combined = mult_fn(py)?.call1((child_values, &top))?;
        let combined_values: Vec<Bound<'py, PyAny>> =
            combined.try_iter()?.collect::<PyResult<Vec<_>>>()?;
        let replacement = get_attr(slf, "new_transform")?.call(
            PyTuple::new(py, &combined_values)?,
            Some(&common::bool_kwargs(py, "is_text")?),
        )?;
        // Replace the current top entry with the combined transformation.
        // The existing stack structure is preserved rather than extended.
        maps.set_item(0usize, replacement)?;
    } else {
        // If the current transformation is not a suitable text matrix to
        // combine with, push the new transformation onto the stack as before.
        let stacked = stack.call_method1("new_child", (new_child,))?;
        set_attr(slf, "transform_stack", stacked)?;
    }
    get_attr(slf, "transform_stack")
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.add_trm
#[pyfunction]
pub fn text_state_manager_add_trm<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    operands: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let completed = slf.call_method1("_complete_matrix", (operands,))?;
    let values: Vec<Bound<'py, PyAny>> = completed.try_iter()?.collect::<PyResult<Vec<_>>>()?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("is_text", true)?;
    kwargs.set_item("is_render", true)?;
    let new_child =
        get_attr(slf, "new_transform")?.call(PyTuple::new(py, &values)?, Some(&kwargs))?;
    let stack = get_attr(slf, "transform_stack")?;
    let stacked = stack.call_method1("new_child", (new_child,))?;
    set_attr(slf, "transform_stack", stacked)?;
    get_attr(slf, "transform_stack")
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.effective_transform
#[pyfunction]
pub fn text_state_manager_effective_transform<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let stack = get_attr(slf, "transform_stack")?;
    let maps = stack.getattr("maps")?;
    let top = maps.get_item(0usize)?;
    let mut effective = common::builtins(py)?
        .getattr("list")?
        .call1((top.call_method0("values")?,))?;
    let rest = maps.get_item(common::slice_of(
        py,
        Some(&common::py_int(py, 1)),
        None,
        None,
    )?)?;
    for transform in rest.try_iter()? {
        let transform = transform?;
        effective = mult_fn(py)?.call1((effective, transform))?;
    }
    Ok(effective)
}

// ---------------------------------------------------------------------------
// TextStateParams
// ---------------------------------------------------------------------------

// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.__post_init__
#[pyfunction]
pub fn text_state_params_post_init<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<()> {
    let value = get_attr(slf, "value")?;
    let font = get_attr(slf, "font")?;
    let empty = common::py_str_val(py, "");
    if common::is_bytes(py, &value)? {
        let encoding = font.getattr("encoding")?;
        let mut decoded_value = empty.clone();
        let raw_chars;
        if common::is_str(py, &encoding)? {
            match value.call_method1("decode", (&encoding, "surrogatepass")) {
                Ok(decoded) => raw_chars = decoded,
                Err(error)
                    if error.is_instance_of::<pyo3::exceptions::PyUnicodeDecodeError>(py) =>
                {
                    // Fallback for odd byte counts or unmapped 16-bit GIDs/CIDs
                    raw_chars = value.call_method1("decode", (&encoding, "replace"))?;
                }
                Err(error) => return Err(error),
            }
        } else {
            let mut raw: Vec<Bound<'py, PyAny>> = Vec::new();
            let mut chars: Vec<Bound<'py, PyAny>> = Vec::new();
            for byte in value.try_iter()? {
                let byte = byte?;
                let char = common::chr_of(py, &byte)?;
                raw.push(char.clone());
                chars.push(encoding.call_method1("get", (&byte, &char))?);
            }
            raw_chars = empty.call_method1("join", (PyList::new(py, raw)?,))?;
            decoded_value = empty.call_method1("join", (PyList::new(py, chars)?,))?;
        }
        set_attr(slf, "_raw_chars", raw_chars.clone())?;
        let source = if decoded_value.is_truthy()? {
            decoded_value
        } else {
            raw_chars
        };
        let character_map = font.getattr("character_map")?;
        let mut mapped: Vec<Bound<'py, PyAny>> = Vec::new();
        for char in source.try_iter()? {
            let char = char?;
            mapped.push(character_map.call_method1("get", (&char, &char))?);
        }
        let text = empty.call_method1("join", (PyList::new(py, mapped)?,))?;
        set_attr(slf, "text", text)?;
    } else {
        set_attr(slf, "text", value)?;
    }

    let orient_fn = common::init_attr(py, "orient")?;
    let mult_fn = common::init_attr(py, "mult")?;
    let mut transform = get_attr(slf, "transform")?;
    let orientation = orient_fn.call1((&transform,))?;
    let ninety = common::py_int(py, 90);
    let one_eighty = common::py_int(py, 180);
    let two_seventy = common::py_int(py, 270);
    if orientation.eq(&ninety)? || orientation.eq(&two_seventy)? {
        let flipped = common::py_list(
            py,
            vec![
                common::py_float(py, 1.0),
                transform.get_item(1usize)?.neg()?,
                transform.get_item(2usize)?.neg()?,
                common::py_float(py, 1.0),
                common::py_float(py, 0.0),
                common::py_float(py, 0.0),
            ],
        )?;
        transform = mult_fn.call1((flipped, &transform))?;
        set_attr(slf, "transform", transform.clone())?;
        set_attr(slf, "rotated", common::py_bool(py, true))?;
    }
    // self.transform[0] AND self.transform[3] < 0 indicates true rotation.
    // If only self.transform[3] < 0, the y coords are simply inverted.
    if orient_fn.call1((&transform,))?.eq(&one_eighty)?
        && transform.get_item(0usize)?.lt(-1e-6f64)?
    {
        let flip = common::py_list(
            py,
            vec![
                common::py_float(py, -1.0),
                common::py_float(py, 0.0),
                common::py_float(py, 0.0),
                common::py_float(py, -1.0),
                common::py_float(py, 0.0),
                common::py_float(py, 0.0),
            ],
        )?;
        transform = mult_fn.call1((flip, &transform))?;
        set_attr(slf, "transform", transform.clone())?;
        set_attr(slf, "rotated", common::py_bool(py, true))?;
    }
    let displaced = slf.call_method0("displaced_transform")?;
    set_attr(slf, "displaced_tx", displaced.get_item(4usize)?)?;
    set_attr(slf, "tx", get_attr(slf, "transform")?.get_item(4usize)?)?;
    let rendered = slf.call_method0("render_transform")?;
    set_attr(slf, "ty", rendered.get_item(5usize)?)?;
    let space_char = font.getattr("space_char")?;
    let space_tx = slf.call_method1("word_tx", (&space_char,))?;
    let rounded = common::round_digits(py, &space_tx, 3)?;
    set_attr(slf, "space_tx", rounded.clone())?;
    if rounded.lt(1e-6f64)? {
        // if the " " char is assigned 0 width (e.g. for fine tuned spacing
        // with TJ int operators a la crazyones.pdf), calculate space_tx as
        // a td_offset of -1 * font.space_width where font.space_width is
        // the space_width calculated in _font.py.
        let space_width = font.getattr("space_width")?;
        let negated = space_width.neg()?;
        let offset = slf.call_method1("word_tx", (&empty, negated))?;
        set_attr(slf, "space_tx", common::round_digits(py, &offset, 3)?)?;
    }
    let transform = get_attr(slf, "transform")?;
    // font_height = font_size * math.sqrt(transform[1] ** 2 + transform[3] ** 2)
    let scaled = transform
        .get_item(1usize)?
        .pow(2i64, py.None())?
        .add(transform.get_item(3usize)?.pow(2i64, py.None())?)?;
    let root = common::sqrt_of(py, &scaled)?;
    let font_height = get_attr(slf, "font_size")?.mul(root)?;
    set_attr(slf, "font_height", font_height)?;
    // flip_vertical handles PDFs generated by Microsoft Word's "publish" command.
    let flip_vertical = transform.get_item(3usize)?.lt(-1e-6f64)?;
    set_attr(slf, "flip_vertical", common::py_bool(py, flip_vertical))?;
    Ok(())
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.font_size_matrix
#[pyfunction]
pub fn text_state_params_font_size_matrix<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let font_size = get_attr(slf, "font_size")?;
    let tz = get_attr(slf, "Tz")?.div(100i64)?;
    common::py_list(
        py,
        vec![
            font_size.mul(tz)?,
            common::py_float(py, 0.0),
            common::py_float(py, 0.0),
            font_size,
            common::py_float(py, 0.0),
            get_attr(slf, "Ts")?,
        ],
    )
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.displaced_transform
#[pyfunction]
pub fn text_state_params_displaced_transform<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let matrix = slf.call_method0("displacement_matrix")?;
    common::init_attr(py, "mult")?.call1((matrix, get_attr(slf, "transform")?))
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.render_transform
#[pyfunction]
pub fn text_state_params_render_transform<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let matrix = slf.call_method0("font_size_matrix")?;
    common::init_attr(py, "mult")?.call1((matrix, get_attr(slf, "transform")?))
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.displacement_matrix
#[pyfunction]
#[pyo3(signature = (slf, word, td_offset))]
pub fn text_state_params_displacement_matrix<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    word: &Bound<'py, PyAny>,
    td_offset: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let word = if word.is_none() {
        get_attr(slf, "value")?
    } else {
        word.clone()
    };
    let word_tx = slf.call_method1("word_tx", (&word, td_offset))?;
    common::py_list(
        py,
        vec![
            common::py_float(py, 1.0),
            common::py_float(py, 0.0),
            common::py_float(py, 0.0),
            common::py_float(py, 1.0),
            word_tx,
            common::py_float(py, 0.0),
        ],
    )
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.word_tx
#[pyfunction]
#[pyo3(signature = (slf, word, td_offset))]
pub fn text_state_params_word_tx<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    word: &Bound<'py, PyAny>,
    td_offset: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let font = get_attr(slf, "font")?;
    let word = if common::is_bytes(py, word)? {
        get_attr(slf, "_raw_chars")?
    } else {
        word.clone()
    };
    let space_char = font.getattr("space_char")?;
    let space_width = font.getattr("space_width")?;
    let mut width = common::py_float(py, 0.0);
    for char in word.try_iter()? {
        let char = char?;
        if char.eq(&space_char)? {
            width = width.add(&space_width)?;
        } else {
            width = width.add(font.call_method1("get_text_width", (&char,))?)?;
        }
    }
    // (font_size * ((width - td_offset) / 1000.0)) + len(word) * Tc
    // + word.count(space_char) * Tw, scaled by Tz / 100.0
    let mut result = get_attr(slf, "font_size")?.mul(width.sub(td_offset)?.div(1000i64)?)?;
    let length = common::py_int(py, common::len_of(&word)? as i64);
    result = result.add(length.mul(get_attr(slf, "Tc")?)?)?;
    let count = word.call_method1("count", (&space_char,))?;
    result = result.add(count.mul(get_attr(slf, "Tw")?)?)?;
    result.mul(get_attr(slf, "Tz")?.div(100i64)?)
}

// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.to_dict
#[pyfunction]
pub fn text_state_params_to_dict<'py>(
    py: Python<'py>,
    inst: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let fields = get_attr(inst, "__dataclass_fields__")?;
    let result = PyDict::new(py);
    for key in fields.call_method0("keys")?.try_iter()? {
        let key = key?;
        if key.eq("font")? {
            continue;
        }
        result.set_item(&key, get_attr(inst, &key.extract::<String>()?)?)?;
    }
    Ok(result.into_any())
}
