//! `pypdf._text_extraction._layout_mode._fixed_width_page` (upstream
//! `_layout_mode/_fixed_width_page.py`).
//!
//! The layout-mode entry points (`text_show_operations`, `y_coordinate_groups`,
//! `fixed_char_width`, `fixed_width_page`) are called from
//! `PageObject._layout_mode_text`; the recursive operator walk reproduces
//! upstream's `recurse_to_target_op`, including its lazy consumption of the
//! content-stream operator iterator and its exact warning texts.

use pyo3::exceptions::{PyStopIteration, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyTuple};

use super::callables::{GroupKey, SortByTx, SortKey, TjDefault};
use super::common;

fn layout_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyModule>> {
    crate::pyutil::cached_module(py, "pypdf._text_extraction._layout_mode._fixed_width_page")
}

fn module_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    layout_module(py)?.getattr(name)
}

/// Unpack `for operands, op in ops`.
fn unpack_operator<'py>(
    _py: Python<'py>,
    item: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let length = common::len_of(item)?;
    if length != 2 {
        let message = if length < 2 {
            "not enough values to unpack (expected 2)"
        } else {
            "too many values to unpack (expected 2)"
        };
        return Err(PyValueError::new_err(message.to_owned()));
    }
    Ok((item.get_item(0usize)?, item.get_item(1usize)?))
}

fn warn(py: Python<'_>, message: &str, values: &[(&str, Bound<'_, PyAny>)]) -> PyResult<()> {
    common::logger_warning(py, common::SOURCE_LAYOUT, message, values)
}

// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.set_font_operation
#[pyfunction]
pub fn layout_set_font_operation<'py>(
    py: Python<'py>,
    state_mgr: &Bound<'py, PyAny>,
    fonts: &Bound<'py, PyAny>,
    operands: &Bound<'py, PyAny>,
) -> PyResult<()> {
    // A missing font name resolves the same uninterpretable placeholder font as
    // an unknown one (see resolve_font()). A missing or non-numeric size leaves
    // the current size in place, matching the ROB handling of plain-mode Tf.
    let name = if operands.is_truthy()? {
        operands.get_item(0usize)?
    } else {
        common::py_str_val(py, "")
    };
    let size_operand = if common::len_of(operands)? > 1 {
        operands.get_item(1usize)?
    } else {
        common::none(py)
    };
    let font_size = state_mgr.getattr("font_size")?;
    let size = state_mgr.call_method1("_as_float", (size_operand, font_size))?;
    let font = resolve_font(py, fonts, &name)?;
    state_mgr.call_method1("set_font", (font, size))?;
    Ok(())
}

/// `resolve_font(fonts, name)`.
pub fn resolve_font<'py>(
    py: Python<'py>,
    fonts: &Bound<'py, PyAny>,
    name: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    if fonts.contains(name)? {
        return fonts.get_item(name);
    }
    warn(
        py,
        "Font %(name)s is not in the page resources.",
        &[("name", name.clone())],
    )?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("encoding", PyDict::new(py))?;
    kwargs.set_item("interpretable", false)?;
    common::font_class(py)?.call(("Unknown",), Some(&kwargs))
}

// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.resolve_font
#[pyfunction]
pub fn layout_resolve_font<'py>(
    py: Python<'py>,
    fonts: &Bound<'py, PyAny>,
    name: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    resolve_font(py, fonts, name)
}

/// `bt_group(tj_op, rendered_text, displaced_tx)`.
pub fn build_bt_group<'py>(
    py: Python<'py>,
    tj_op: &Bound<'py, PyAny>,
    rendered_text: &Bound<'py, PyAny>,
    displaced_tx: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let flip_vertical = tj_op.getattr("flip_vertical")?.is_truthy()?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("tx", tj_op.getattr("tx")?)?;
    kwargs.set_item("ty", tj_op.getattr("ty")?)?;
    kwargs.set_item("font_size", tj_op.getattr("font_size")?)?;
    kwargs.set_item("font_height", tj_op.getattr("font_height")?)?;
    kwargs.set_item("text", rendered_text)?;
    kwargs.set_item("displaced_tx", displaced_tx)?;
    kwargs.set_item(
        "flip_sort",
        common::py_int(py, if flip_vertical { -1 } else { 1 }),
    )?;
    module_attr(py, "BTGroup")?.call((), Some(&kwargs))
}

// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.bt_group
#[pyfunction]
pub fn layout_bt_group<'py>(
    py: Python<'py>,
    tj_op: &Bound<'py, PyAny>,
    rendered_text: &Bound<'py, PyAny>,
    displaced_tx: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    build_bt_group(py, tj_op, rendered_text, displaced_tx)
}

/// `_show_text_operand(operands, index, text_state_mgr, tj_ops)`.
pub fn show_text_operand<'py>(
    py: Python<'py>,
    operands: &Bound<'py, PyAny>,
    index: usize,
    text_state_mgr: &Bound<'py, PyAny>,
    tj_ops: &Bound<'py, PyAny>,
) -> PyResult<()> {
    // A missing or non-string operand shows no text.
    if index < common::len_of(operands)? && common::is_str_or_bytes(py, &operands.get_item(index)?)?
    {
        let operand = operands.get_item(index)?;
        let params = text_state_mgr.call_method1("text_state_params", (operand,))?;
        tj_ops.call_method1("append", (params,))?;
    }
    Ok(())
}

// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page._show_text_operand
#[pyfunction]
pub fn layout_show_text_operand<'py>(
    py: Python<'py>,
    operands: &Bound<'py, PyAny>,
    index: usize,
    text_state_mgr: &Bound<'py, PyAny>,
    tj_ops: &Bound<'py, PyAny>,
) -> PyResult<()> {
    show_text_operand(py, operands, index, text_state_mgr, tj_ops)
}

/// `recurse_to_target_op(ops, text_state_mgr, end_target, fonts, strip_rotated)`.
pub fn recurse<'py>(
    py: Python<'py>,
    ops: &Bound<'py, PyAny>,
    text_state_mgr: &Bound<'py, PyAny>,
    end_target: &Bound<'py, PyAny>,
    fonts: &Bound<'py, PyAny>,
    strip_rotated: bool,
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let bytes = |value: &[u8]| common::py_bytes_val(py, value);
    let text = |value: &str| common::py_str_val(py, value);
    let empty = || text("");

    // 1 entry per line of text rendered within each BT/ET operation.
    let bt_groups = PyList::empty(py);
    // 1 entry per text show operator (Tj/TJ/'/")
    let tj_ops = PyList::empty(py);

    let end_is_q = end_target.eq(bytes(b"Q"))?;
    if end_is_q {
        // add new q level. cm's added at this level will be popped at next b'Q'
        text_state_mgr.call_method0("add_q")?;
    }

    let mut balanced = false;
    while let Some(item) = common::next_item(ops)? {
        // The loop is broken by the end target, or exits normally when there are no more ops.
        let (operands, op) = unpack_operator(py, &item)?;
        if op.eq(end_target)? {
            balanced = true;
            if op.eq(bytes(b"Q"))? {
                text_state_mgr.call_method0("remove_q")?;
            }
            if op.eq(bytes(b"ET"))? {
                if !tj_ops.is_truthy()? {
                    return Ok((bt_groups.into_any(), tj_ops.into_any()));
                }
                let mut current_text = empty();
                let mut bt_idx: usize = 0; // idx of first tj in this bt group
                let mut last_displaced_tx = tj_ops.get_item(0usize)?.getattr("displaced_tx")?;
                let mut last_ty = tj_ops.get_item(0usize)?.getattr("ty")?;
                let count = tj_ops.len();
                for idx in 0..count {
                    // ... build text from new Tj operators
                    let tj = tj_ops.get_item(idx)?;
                    if strip_rotated && tj.getattr("rotated")?.is_truthy()? {
                        continue;
                    }
                    if !tj.getattr("font")?.getattr("interpretable")?.is_truthy()? {
                        // generates warning
                        continue;
                    }
                    // if the y position of the text is greater than the font height, assume
                    // the text is on a new line and start a new group
                    let delta_ty = AxisDelta::new(py, tj.getattr("ty")?, &last_ty)?;
                    if delta_ty.absolute()?.gt(tj.getattr("font_height")?)? {
                        if text_truthy(&current_text)? {
                            let group = module_attr(py, "bt_group")?.call1((
                                tj_ops.get_item(bt_idx)?,
                                &current_text,
                                &last_displaced_tx,
                            ))?;
                            bt_groups.call_method1("append", (group,))?;
                        }
                        bt_idx = idx;
                        current_text = empty();
                    }

                    // if the x position of the text is less than the last x position by
                    // more than 5 spaces widths, assume the text order should be flipped
                    // and start a new group
                    let backwards = last_displaced_tx.sub(&tj.getattr("tx")?)?.gt(&tj
                        .getattr("space_tx")?
                        .mul(module_attr(py, "LAYOUT_NEW_BT_GROUP_SPACE_WIDTHS")?)?)?;
                    if backwards {
                        if text_truthy(&current_text)? {
                            let group = module_attr(py, "bt_group")?.call1((
                                tj_ops.get_item(bt_idx)?,
                                &current_text,
                                &last_displaced_tx,
                            ))?;
                            bt_groups.call_method1("append", (group,))?;
                        }
                        bt_idx = idx;
                        last_displaced_tx = tj.getattr("displaced_tx")?;
                        current_text = empty();
                    }

                    // calculate excess x translation based on ending tx of previous Tj.
                    // multiply by bool (_idx != bt_idx) to ensure spaces aren't double
                    // applied to the first tj of a BTGroup in fixed_width_page().
                    let excess_tx =
                        common::round_digits(py, &tj.getattr("tx")?.sub(&last_displaced_tx)?, 3)?
                            .mul(common::py_bool(py, idx != bt_idx))?;
                    // space_tx is measured in text space, while excess_tx is measured
                    // in page-space x coordinates. Convert the space width using the
                    // effective horizontal transform. This matters for PDFs that use
                    // Tf 1 and put the font size in Tm (e.g. Illustrator/InDesign/Figma).
                    // The backward-jump test above stays on text-space space_tx:
                    // scaling it splits same-line runs when a CTM scale < 1 makes
                    // 5 page-space widths tiny (resources/toy.pdf).
                    let transform = tj.getattr("transform")?;
                    let page_space_tx = tj
                        .getattr("space_tx")?
                        .mul(common::abs_of(py, &transform.get_item(0usize)?)?)?;
                    // page_space_tx is 0 when Tz, font_size, or the horizontal scale is 0.
                    let spaces = if excess_tx.gt(0i64)? && page_space_tx.is_truthy()? {
                        common::round_int(py, &excess_tx.div(&page_space_tx)?)?
                    } else {
                        common::py_int(py, 0)
                    };
                    let mut spaces_value = spaces;
                    let whitespace_limit = module_attr(py, "WHITESPACE_LIMIT")?;
                    if spaces_value.gt(&whitespace_limit)? {
                        warn(
                            py,
                            "Limiting excessive whitespace from %(actual)d to %(limit)d characters.",
                            &[
                                ("actual", spaces_value.clone()),
                                ("limit", whitespace_limit.clone()),
                            ],
                        )?;
                        spaces_value = whitespace_limit;
                    }
                    let spaces_count: i64 = spaces_value.extract()?;
                    let padding = text(&" ".repeat(spaces_count.max(0) as usize));
                    let new_text = common::concat(&padding, &tj.getattr("text")?)?;

                    last_ty = tj.getattr("ty")?;
                    current_text = common::concat(&current_text, &new_text)?;
                    last_displaced_tx = tj.getattr("displaced_tx")?;
                }
                if current_text.is_truthy()? {
                    let group = module_attr(py, "bt_group")?.call1((
                        tj_ops.get_item(bt_idx)?,
                        &current_text,
                        &last_displaced_tx,
                    ))?;
                    bt_groups.call_method1("append", (group,))?;
                }
                text_state_mgr.call_method0("reset_tm")?;
            }
            break;
        }
        let is_q_op = op.eq(bytes(b"q"))?;
        let is_bt_op = op.eq(bytes(b"BT"))?;
        if is_q_op || (is_bt_op && end_is_q) {
            let target = if is_bt_op { bytes(b"ET") } else { bytes(b"Q") };
            // Upstream recurses through the module global, so a monkeypatched
            // `recurse_to_target_op` takes effect at every nesting level.
            let result = module_attr(py, "recurse_to_target_op")?.call1((
                ops,
                text_state_mgr,
                &target,
                fonts,
                strip_rotated,
            ))?;
            let bts = result.get_item(0usize)?;
            let tjs = result.get_item(1usize)?;
            bt_groups.call_method1("extend", (bts,))?;
            tj_ops.call_method1("extend", (tjs,))?;
        } else if op.eq(bytes(b"cm"))? {
            let args: Vec<Bound<'py, PyAny>> =
                operands.try_iter()?.collect::<PyResult<Vec<_>>>()?;
            text_state_mgr.call_method("add_cm", PyTuple::new(py, args)?, None)?;
        } else if is_bt_op {
            // text objects cannot be nested and end at the first ET: a BT inside
            // one only resets the text matrix, as in plain mode
            text_state_mgr.call_method0("reset_tm")?;
        } else if op.eq(bytes(b"Tj"))?
            && operands.is_truthy()?
            && common::is_str_or_bytes(py, &operands.get_item(0usize)?)?
        {
            let params =
                text_state_mgr.call_method1("text_state_params", (operands.get_item(0usize)?,))?;
            tj_ops.call_method1("append", (params,))?;
        } else if op.eq(bytes(b"TJ"))?
            && operands.is_truthy()?
            && operands
                .get_item(0usize)?
                .is_instance(&common::generic_class(py, "ArrayObject")?)?
        {
            let mut tj = text_state_mgr.call_method0("text_state_params")?;
            // Only strings and numbers are valid TJ array items
            for item in operands.get_item(0usize)?.try_iter()? {
                let item = item?;
                if common::is_bytes(py, &item)? {
                    tj = text_state_mgr.call_method1("text_state_params", (&item,))?;
                    tj_ops.call_method1("append", (&tj,))?;
                } else if common::is_number(py, &item)? {
                    let matrix = tj.call_method(
                        "displacement_matrix",
                        (),
                        Some(&td_offset_kwargs(py, item.clone())?),
                    )?;
                    text_state_mgr.call_method1("add_trm", (matrix,))?;
                }
            }
        } else if op.eq(bytes(b"'"))? {
            // Move to the next line, then show the string if there is one, like Tj
            text_state_mgr.call_method0("reset_trm")?;
            let leading = text_state_mgr.getattr("TL")?;
            let tm_operands = common::py_list(py, vec![common::py_int(py, 0), leading.neg()?])?;
            text_state_mgr.call_method1("add_tm", (tm_operands,))?;
            module_attr(py, "_show_text_operand")?.call1((
                &operands,
                common::py_int(py, 0),
                text_state_mgr,
                &tj_ops,
            ))?;
        } else if op.eq(bytes(b"\""))? && common::len_of(&operands)? >= 3 {
            text_state_mgr.call_method0("reset_trm")?;
            text_state_mgr.call_method1(
                "set_state_param",
                (bytes(b"Tw"), operands.get_item(0usize)?),
            )?;
            text_state_mgr.call_method1(
                "set_state_param",
                (bytes(b"Tc"), operands.get_item(1usize)?),
            )?;
            let leading = text_state_mgr.getattr("TL")?;
            let tm_operands = common::py_list(py, vec![common::py_int(py, 0), leading.neg()?])?;
            text_state_mgr.call_method1("add_tm", (tm_operands,))?;
            module_attr(py, "_show_text_operand")?.call1((
                &operands,
                common::py_int(py, 2),
                text_state_mgr,
                &tj_ops,
            ))?;
        } else if op.eq(bytes(b"Td"))?
            || op.eq(bytes(b"Tm"))?
            || op.eq(bytes(b"TD"))?
            || op.eq(bytes(b"T*"))?
        {
            text_state_mgr.call_method0("reset_trm")?;
            let mut matrix_operands = operands.clone();
            if op.eq(bytes(b"Tm"))? {
                text_state_mgr.call_method0("reset_tm")?;
            } else if op.eq(bytes(b"TD"))?
                && common::len_of(&operands)? >= 2
                && common::is_number(py, &operands.get_item(1usize)?)?
            {
                // a missing or non-numeric ty leaves the leading unchanged; the
                // translation itself is handled like Td below
                text_state_mgr.call_method1(
                    "set_state_param",
                    (bytes(b"TL"), operands.get_item(1usize)?.neg()?),
                )?;
            } else if op.eq(bytes(b"T*"))? {
                let leading = text_state_mgr.getattr("TL")?;
                matrix_operands = common::py_list(py, vec![common::py_int(py, 0), leading.neg()?])?;
            }
            text_state_mgr.call_method1("add_tm", (matrix_operands,))?;
        } else if op.eq(bytes(b"Tf"))? {
            module_attr(py, "set_font_operation")?.call1((text_state_mgr, fonts, &operands))?;
        } else {
            // handle Tc, Tw, Tz, TL, and Ts operators
            text_state_mgr.call_method1("set_state_param", (&op, &operands))?;
        }
    }
    if !balanced {
        warn(
            py,
            "Unbalanced target operations, expected %(end_target)r.",
            &[("end_target", end_target.clone())],
        )?;
    }
    Ok((bt_groups.into_any(), tj_ops.into_any()))
}

fn text_truthy(value: &Bound<'_, PyAny>) -> PyResult<bool> {
    value.call_method0("strip")?.is_truthy()
}

fn td_offset_kwargs<'py>(
    py: Python<'py>,
    value: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("td_offset", value)?;
    Ok(kwargs)
}

/// `abs(a - b)` computed with Python's `abs` and `-`.
struct AxisDelta<'py> {
    py: Python<'py>,
    value: Bound<'py, PyAny>,
}

impl<'py> AxisDelta<'py> {
    fn new(py: Python<'py>, a: Bound<'py, PyAny>, b: &Bound<'py, PyAny>) -> PyResult<Self> {
        Ok(Self {
            py,
            value: a.sub(b)?,
        })
    }

    fn absolute(&self) -> PyResult<Bound<'py, PyAny>> {
        common::abs_of(self.py, &self.value)
    }
}

// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.recurse_to_target_op
#[pyfunction]
#[pyo3(signature = (ops, text_state_mgr, end_target, fonts, strip_rotated=true))]
pub fn layout_recurse_to_target_op<'py>(
    py: Python<'py>,
    ops: &Bound<'py, PyAny>,
    text_state_mgr: &Bound<'py, PyAny>,
    end_target: &Bound<'py, PyAny>,
    fonts: &Bound<'py, PyAny>,
    strip_rotated: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let iterator = common::iter_of(py, ops)?;
    let (bt_groups, tj_ops) = recurse(
        py,
        &iterator,
        text_state_mgr,
        end_target,
        fonts,
        strip_rotated,
    )?;
    common::py_tuple(py, vec![bt_groups, tj_ops])
}

// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.y_coordinate_groups
#[pyfunction]
#[pyo3(signature = (bt_groups, debug_path=None))]
pub fn layout_y_coordinate_groups<'py>(
    py: Python<'py>,
    bt_groups: &Bound<'py, PyAny>,
    debug_path: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let group_key = Py::new(py, GroupKey)?.into_bound(py);
    let sort_by_tx = Py::new(py, SortByTx)?.into_bound(py);
    let sorted_builtin = common::builtins(py)?.getattr("sorted")?;

    let ty_groups = PyDict::new(py);
    let mut current_key: Option<Bound<'py, PyAny>> = None;
    let mut current_group: Vec<Bound<'py, PyAny>> = Vec::new();
    for bt_grp in bt_groups.try_iter()? {
        let bt_grp = bt_grp?;
        let key = group_key.call1((&bt_grp,))?;
        let same_group = match current_key.as_ref() {
            Some(current) => current.eq(&key)?,
            None => false,
        };
        if same_group {
            current_group.push(bt_grp);
        } else {
            if let Some(previous_key) = current_key.take() {
                let sorted = sorted_builtin.call(
                    (PyList::new(py, &current_group)?,),
                    Some(&sorted_kwargs(py, &sort_by_tx)?),
                )?;
                ty_groups.set_item(&previous_key, sorted)?;
                current_group = Vec::new();
            }
            current_key = Some(key);
            current_group.push(bt_grp);
        }
    }
    if let Some(previous_key) = current_key.take() {
        let sorted = sorted_builtin.call(
            (PyList::new(py, &current_group)?,),
            Some(&sorted_kwargs(py, &sort_by_tx)?),
        )?;
        ty_groups.set_item(&previous_key, sorted)?;
    }
    // combine groups whose y coordinates differ by less than the effective font height
    // (accounts for mixed fonts and other minor oddities)
    let keys: Vec<Bound<'py, PyAny>> = ty_groups
        .call_method0("keys")?
        .try_iter()?
        .collect::<PyResult<Vec<_>>>()?;
    if keys.is_empty() {
        return Err(PyStopIteration::new_err(()));
    }
    let mut last_ty = keys[0].clone();
    let mut last_txs = text_positions(py, &common::dict_get(&ty_groups, &last_ty)?)?;
    for ty in keys[1..].iter() {
        let group = common::dict_get(&ty_groups, ty)?;
        let last_group = common::dict_get(&ty_groups, &last_ty)?;
        let fh = group.get_item(0usize)?.get_item("font_height")?;
        let last_fh = last_group.get_item(0usize)?.get_item("font_height")?;
        let fsz = common::min_of(py, &fh, &last_fh)?;
        let txs = text_positions(py, &group)?;
        // prevent merge if both groups are rendering in the same x position.
        let overlap = txs.call_method1("__and__", (&last_txs,))?;
        let no_text_overlap = !overlap.is_truthy()?;
        let offset = common::abs_of(py, &ty.sub(&last_ty)?)?;
        let offset_less_than_font_height = offset.lt(&fsz)?;
        if no_text_overlap && offset_less_than_font_height {
            let removed = common::dict_get(&ty_groups, ty)?;
            ty_groups.del_item(ty)?;
            let combined = removed.add(&common::dict_get(&ty_groups, &last_ty)?)?;
            let sorted =
                sorted_builtin.call((combined,), Some(&sorted_kwargs(py, &sort_by_tx)?))?;
            ty_groups.set_item(&last_ty, sorted)?;
            last_txs.call_method1("update", (&txs,))?;
        } else {
            last_ty = ty.clone();
            last_txs = txs;
        }
    }
    if let Some(debug_path) = debug_path {
        if !debug_path.is_none() && debug_path.is_truthy()? {
            let json = py.import("json")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("indent", 2)?;
            kwargs.set_item("default", common::builtins(py)?.getattr("str")?)?;
            let dumped = json.call_method("dumps", (&ty_groups,), Some(&kwargs))?;
            debug_path
                .call_method1("joinpath", ("bt_groups.json",))?
                .call_method1("write_text", (dumped, "utf-8"))?;
        }
    }
    Ok(ty_groups.into_any())
}

fn sorted_kwargs<'py>(py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("key", key)?;
    Ok(kwargs)
}

/// `{int(_t["tx"]) for _t in group if _t["text"].strip()}`.
fn text_positions<'py>(py: Python<'py>, group: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let result = common::builtins(py)?.getattr("set")?.call0()?;
    for bt_op in group.try_iter()? {
        let bt_op = bt_op?;
        if bt_op.get_item("text")?.call_method0("strip")?.is_truthy()? {
            let tx = common::int_of(py, &bt_op.get_item("tx")?)?;
            result.call_method1("add", (tx,))?;
        }
    }
    Ok(result)
}

// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.text_show_operations
#[pyfunction]
#[pyo3(signature = (ops, fonts, strip_rotated=true, debug_path=None))]
pub fn layout_text_show_operations<'py>(
    py: Python<'py>,
    ops: &Bound<'py, PyAny>,
    fonts: &Bound<'py, PyAny>,
    strip_rotated: bool,
    debug_path: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    // transformation stack manager
    let state_mgr = module_attr(py, "TextStateManager")?.call0()?;
    // BT operator dict
    let bt_groups = PyList::empty(py);
    // Tj/TJ operator data
    let tj_ops = PyList::empty(py);
    let iterator = common::iter_of(py, ops)?;
    while let Some(item) = common::next_item(&iterator)? {
        let (operands, op) = unpack_operator(py, &item)?;
        let is_bt = op.eq(common::py_bytes_val(py, b"BT"))?;
        let is_q = op.eq(common::py_bytes_val(py, b"q"))?;
        if is_bt || is_q {
            let target = if is_bt {
                common::py_bytes_val(py, b"ET")
            } else {
                common::py_bytes_val(py, b"Q")
            };
            let result = module_attr(py, "recurse_to_target_op")?.call1((
                &iterator,
                &state_mgr,
                &target,
                fonts,
                strip_rotated,
            ))?;
            let bts = result.get_item(0usize)?;
            let tjs = result.get_item(1usize)?;
            bt_groups.call_method1("extend", (bts,))?;
            tj_ops.call_method1("extend", (tjs,))?;
        } else if op.eq(common::py_bytes_val(py, b"Tf"))? {
            module_attr(py, "set_font_operation")?.call1((&state_mgr, fonts, &operands))?;
        } else {
            // set Tc, Tw, Tz, TL, and Ts if required. ignores all other ops
            state_mgr.call_method1("set_state_param", (&op, &operands))?;
        }
    }

    let mut any_rotated = false;
    for tj in tj_ops.try_iter()? {
        if tj?.getattr("rotated")?.is_truthy()? {
            any_rotated = true;
            break;
        }
    }
    if any_rotated {
        if strip_rotated {
            warn(
                py,
                "Rotated text discovered. Output will be incomplete.",
                &[],
            )?;
        } else {
            warn(py, "Rotated text discovered. Layout will be degraded.", &[])?;
        }
    }
    let mut all_interpretable = true;
    for tj in tj_ops.try_iter()? {
        if !tj?.getattr("font")?.getattr("interpretable")?.is_truthy()? {
            all_interpretable = false;
            break;
        }
    }
    if !all_interpretable {
        warn(
            py,
            "PDF contains an uninterpretable font. Output will be incomplete.",
            &[],
        )?;
    }

    // left align the data, i.e. decrement all tx values by min(tx)
    let min_x = min_tx(py, &bt_groups)?;
    let sort_key = Py::new(py, SortKey)?.into_bound(py);
    let sort_kwargs = PyDict::new(py);
    sort_kwargs.set_item("key", &sort_key)?;
    sort_kwargs.set_item("reverse", true)?;
    let sorted = common::builtins(py)?
        .getattr("sorted")?
        .call((&bt_groups,), Some(&sort_kwargs))?;
    let result = PyList::empty(py);
    for original in sorted.try_iter()? {
        let original = original?;
        let tx = original.get_item("tx")?.sub(min_x.clone())?;
        let displaced_tx = original.get_item("displaced_tx")?.sub(min_x.clone())?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("tx", tx)?;
        kwargs.set_item("displaced_tx", displaced_tx)?;
        let group = common::builtins(py)?
            .getattr("dict")?
            .call((&original,), Some(&kwargs))?;
        result.call_method1("append", (group,))?;
    }

    if let Some(debug_path) = debug_path {
        if !debug_path.is_none() && debug_path.is_truthy()? {
            let json = py.import("json")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("indent", 2)?;
            kwargs.set_item("default", common::builtins(py)?.getattr("str")?)?;
            let dumped = json.call_method("dumps", (&result,), Some(&kwargs))?;
            debug_path
                .call_method1("joinpath", ("bts.json",))?
                .call_method1("write_text", (dumped, "utf-8"))?;
            let tj_default = Py::new(py, TjDefault)?.into_bound(py);
            let tj_kwargs = PyDict::new(py);
            tj_kwargs.set_item("indent", 2)?;
            tj_kwargs.set_item("default", tj_default)?;
            let dumped = json.call_method("dumps", (&tj_ops,), Some(&tj_kwargs))?;
            debug_path
                .call_method1("joinpath", ("tjs.json",))?
                .call_method1("write_text", (dumped, "utf-8"))?;
        }
    }
    Ok(result.into_any())
}

/// `min((x["tx"] for x in bt_groups), default=0.0)`.
fn min_tx<'py>(py: Python<'py>, bt_groups: &Bound<'py, PyList>) -> PyResult<Bound<'py, PyAny>> {
    let generator = PyList::empty(py);
    for group in bt_groups.iter() {
        generator.call_method1("append", (group.get_item("tx")?,))?;
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("default", common::py_float(py, 0.0))?;
    common::builtins(py)?
        .getattr("min")?
        .call((generator,), Some(&kwargs))
}

// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.fixed_char_width
#[pyfunction]
#[pyo3(signature = (bt_groups, scale_weight=None))]
pub fn layout_fixed_char_width<'py>(
    py: Python<'py>,
    bt_groups: &Bound<'py, PyAny>,
    scale_weight: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let scale_weight = match scale_weight {
        Some(value) => value.clone(),
        None => common::py_float(py, 1.25),
    };
    let mut weighted: Vec<Bound<'py, PyAny>> = Vec::new();
    let mut lengths: Vec<Bound<'py, PyAny>> = Vec::new();
    for bt in bt_groups.try_iter()? {
        let bt = bt?;
        let text = bt.get_item("text")?;
        let length = common::py_int(py, common::len_of(&text)? as i64).mul(&scale_weight)?;
        let width = bt
            .get_item("displaced_tx")?
            .sub(bt.get_item("tx")?)?
            .div(&length)?;
        weighted.push(width.mul(&length)?);
        lengths.push(length);
    }
    // `sum()` (not a manual accumulation): CPython's float `sum` uses compensated
    // summation, so the low bits of the result differ otherwise.
    let sum = common::builtins(py)?.getattr("sum")?;
    let weighted_sum = sum.call1((PyList::new(py, weighted)?,))?;
    let length_sum = sum.call1((PyList::new(py, lengths)?,))?;
    weighted_sum.div(length_sum)
}

// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.fixed_width_page
#[pyfunction]
pub fn layout_fixed_width_page<'py>(
    py: Python<'py>,
    ty_groups: &Bound<'py, PyAny>,
    char_width: &Bound<'py, PyAny>,
    space_vertically: bool,
    font_height_weight: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut lines: Vec<Bound<'py, PyAny>> = Vec::new();
    let mut last_y_coord = common::py_int(py, 0);
    // str.maketrans(dict.fromkeys(range(14, 32), " "))
    let range = common::builtins(py)?.getattr("range")?.call1((14, 32))?;
    let translation = common::builtins(py)?
        .getattr("dict")?
        .getattr("fromkeys")?
        .call1((range, " "))?;
    let table = common::builtins(py)?
        .getattr("str")?
        .getattr("maketrans")?
        .call1((translation,))?;
    let whitespace_limit = module_attr(py, "WHITESPACE_LIMIT")?;
    let newline_limit = module_attr(py, "NEWLINE_LIMIT")?;
    let space = common::py_str_val(py, " ");

    for pair in ty_groups.call_method0("items")?.try_iter()? {
        let pair = pair?;
        let y_coord = pair.get_item(0usize)?;
        let line_data = pair.get_item(1usize)?;
        if space_vertically && !lines.is_empty() {
            let fh = line_data.get_item(0usize)?.get_item("font_height")?;
            let mut blank_lines = if fh.eq(0i64)? {
                common::py_int(py, 0)
            } else {
                let delta = common::abs_of(py, &y_coord.sub(&last_y_coord)?)?;
                let scaled = fh.mul(font_height_weight)?;
                common::int_of(py, &delta.div(&scaled)?)?.sub(1i64)?
            };
            if blank_lines.gt(&newline_limit)? {
                warn(
                    py,
                    "Limiting excessive newlines from %(actual)d to %(limit)d.",
                    &[
                        ("actual", blank_lines.clone()),
                        ("limit", newline_limit.clone()),
                    ],
                )?;
                blank_lines = newline_limit.clone();
            }
            let count: i64 = blank_lines.extract()?;
            for _ in 0..count.max(0) {
                lines.push(common::py_str_val(py, ""));
            }
        }

        // It uses a list to construct the line, avoiding string concatenation.
        let mut line_parts: Vec<Bound<'py, PyAny>> = Vec::new();
        // Track the size with int instead of len(str) overhead.
        let mut current_len = common::py_int(py, 0);
        let mut last_disp = common::py_float(py, 0.0);
        for bt_op in line_data.try_iter()? {
            let bt_op = bt_op?;
            let tx = bt_op.get_item("tx")?;
            // Text drawn with no width (a font size or horizontal scaling of 0) has no column to align to.
            let offset = if char_width.is_truthy()? {
                common::int_of(py, &tx.floor_div(char_width)?)?
            } else {
                current_len.clone()
            };
            let needed_spaces = offset.sub(&current_len)?;
            if needed_spaces.gt(0i64)?
                && common::ceil_of(py, &last_disp)?.lt(common::int_of(py, &tx)?)?
            {
                let mut needed_spaces = needed_spaces;
                if needed_spaces.gt(&whitespace_limit)? {
                    warn(
                        py,
                        "Limiting excessive whitespace from %(actual)d to %(limit)d characters.",
                        &[
                            ("actual", needed_spaces.clone()),
                            ("limit", whitespace_limit.clone()),
                        ],
                    )?;
                    needed_spaces = whitespace_limit.clone();
                }
                let count: i64 = needed_spaces.extract()?;
                let padding = space.call_method1("__mul__", (count.max(0),))?;
                line_parts.push(padding);
                current_len = current_len.add(needed_spaces)?;
            }

            let raw_text = bt_op.get_item("text")?;
            let text = raw_text.call_method1("translate", (&table,))?;
            line_parts.push(text.clone());
            current_len = current_len.add(common::len_of(&text)? as i64)?;
            last_disp = bt_op.get_item("displaced_tx")?;
        }

        let joined =
            common::py_str_val(py, "").call_method1("join", (PyList::new(py, &line_parts)?,))?;
        let full_line = joined.call_method0("rstrip")?;
        if full_line.call_method0("strip")?.is_truthy()? || (space_vertically && !lines.is_empty())
        {
            lines.push(full_line);
        }

        last_y_coord = y_coord;
    }
    common::py_str_val(py, "\n").call_method1("join", (PyList::new(py, &lines)?,))
}
