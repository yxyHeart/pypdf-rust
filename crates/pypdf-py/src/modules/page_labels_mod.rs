//! Rust port of `pypdf/_page_labels.py` (upstream pypdf 7.1.1).
//!
//! # Design
//!
//! The four helper classes of the upstream module (`_FirstKidFinder`,
//! `_TreeNode`, `_KidPages`, `_LeafPages`) stay *Python* definitions in the
//! hand-written shim `python/pypdf/_page_labels.py`: `_TreeNode`, `_KidPages`
//! and `_LeafPages` are `typing.NamedTuple` data containers (part of the
//! observable Python object model), and `_FirstKidFinder.__init__` only assigns
//! attributes.  Every method body and every module-level function is
//! implemented here and reached through `pypdf._pypdf`.
//!
//! Upstream works with Python objects throughout (dictionary subclasses,
//! `IndirectObject`, monkey-patchable helpers), therefore this port performs the
//! same Python-level operations (`get_object()`, `in`, `isinstance`,
//! `bisect`, `+`) instead of re-implementing them.  That keeps the exact
//! upstream behaviour, including its error messages and `logger_warning`
//! texts, and keeps traversal order deterministic (no Rust hash containers are
//! used anywhere; the caches that upstream keeps as `dict` stay Python dicts).

use pyo3::exceptions::{PyMemoryError, PyNotImplementedError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyModule, PyString, PyTuple};

use crate::pyutil;

/// Upstream module name; used as `logger_warning(..., source=__name__)`.
const SOURCE: &str = "pypdf._page_labels";

/// `MAXIMUM_ROMAN_NUMERAL` (upstream module constant).
const MAXIMUM_ROMAN_NUMERAL: i64 = 3_999;

/// `MAXIMUM_PAGE_LABEL_LENGTH` (upstream module constant).
const MAXIMUM_PAGE_LABEL_LENGTH: usize = 512;

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    crate::pyutil::cached_module(py, "builtins")
}

fn shim<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    crate::pyutil::cached_module(py, "pypdf._page_labels")
}

/// Fetch a name from the module's Python shim (upstream references module
/// globals such as `_NUMBERING_STYLES` and `is_null_or_none` by name).
fn shim_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim(py)?.getattr(name)
}

fn generic_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    crate::pyutil::cached_module(py, "pypdf.generic")?.getattr(name)
}

fn py_int<'py>(py: Python<'py>, value: i64) -> Bound<'py, PyAny> {
    pyo3::types::PyInt::new(py, value).into_any()
}

fn py_none<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    py.None().into_bound(py)
}

fn py_list<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyList::new(py, items)?.into_any())
}

fn py_tuple<'py>(py: Python<'py>, items: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyTuple::new(py, items)?.into_any())
}

/// `isinstance(obj, <class>)`.
fn is_instance<'py>(obj: &Bound<'py, PyAny>, class: &Bound<'py, PyAny>) -> PyResult<bool> {
    obj.is_instance(class)
}

fn builtin_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr(name)
}

/// `str(obj)`.
fn str_obj<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    Ok(obj.str()?.into_any())
}

/// `len(obj)`.
fn len_of(obj: &Bound<'_, PyAny>) -> PyResult<usize> {
    obj.len()
}

/// `bisect.bisect_left(list, value)`.
fn bisect_left<'py>(
    py: Python<'py>,
    list: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<i64> {
    py.import("bisect")?
        .getattr("bisect_left")?
        .call1((list, value))?
        .extract()
}

/// `bisect.bisect_right(list, value)`.
fn bisect_right<'py>(
    py: Python<'py>,
    list: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<i64> {
    py.import("bisect")?
        .getattr("bisect_right")?
        .call1((list, value))?
        .extract()
}

/// `slice(start, stop)` for list slice assignment (upstream `list[start:end] = ...`).
fn py_slice<'py>(
    py: Python<'py>,
    start: Option<i64>,
    stop: Option<i64>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut args: Vec<Bound<'py, PyAny>> = Vec::with_capacity(2);
    match start {
        Some(value) => args.push(py_int(py, value)),
        None => args.push(py_none(py)),
    }
    match stop {
        Some(value) => args.push(py_int(py, value)),
        None => args.push(py_none(py)),
    }
    builtins(py)?
        .getattr("slice")?
        .call1(PyTuple::new(py, args)?)
}

/// Emulate `logger_warning(message, source=__name__, **values)`.
fn logger_warning(
    py: Python<'_>,
    message: &str,
    values: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    pyutil::logger_warning(py, SOURCE, message, values)
}

/// Emulate `logger_warning(message, source=__name__)` (no interpolation values).
fn logger_warning_plain(py: Python<'_>, message: &str) -> PyResult<()> {
    pyutil::logger_warning(py, SOURCE, message, None)
}

/// `isinstance(value, (int, float))`.
fn is_int_or_float<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<bool> {
    if is_instance(value, &builtin_class(py, "int")?)? {
        return Ok(true);
    }
    is_instance(value, &builtin_class(py, "float")?)
}

/// `str(index + 1)` (the fallback label used throughout upstream).
fn fallback_label<'py>(index: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let incremented = index.add(1)?;
    str_obj(&incremented)
}

// ---------------------------------------------------------------------------
// Numbering styles
// ---------------------------------------------------------------------------

// @pypdf: pypdf._page_labels.number2decimal_arabic_numeral
#[pyfunction]
fn pagelabels_number2decimal_arabic_numeral<'py>(
    _py: Python<'py>,
    number: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if number.le(0)? {
        return Err(PyValueError::new_err("Expecting a positive number."));
    }
    Ok(str_obj(number)?.unbind())
}

// @pypdf: pypdf._page_labels.number2uppercase_roman_numeral
#[pyfunction]
fn pagelabels_number2uppercase_roman_numeral<'py>(
    py: Python<'py>,
    num: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if num.le(0)? {
        return Err(PyValueError::new_err("Expecting a positive number."));
    }
    if num.gt(MAXIMUM_ROMAN_NUMERAL)? {
        return Err(PyValueError::new_err("Number is out of range."));
    }
    let roman: [(i64, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let divmod = builtins(py)?.getattr("divmod")?;
    let result = PyList::empty(py);
    let mut remaining = num.clone();
    // Upstream: `for value, symbol in roman: count, num = divmod(num, value);
    // result.append(symbol * count)`.  The arithmetic stays at Python level so
    // that non-integer input fails exactly like upstream (`str * float`
    // raises `TypeError`, which `_get_label` catches and reports).
    for (value, symbol) in roman {
        let pair = divmod.call1((&remaining, value))?;
        let count = pair.get_item(0)?;
        remaining = pair.get_item(1)?;
        let piece = PyString::new(py, symbol).into_any().mul(&count)?;
        result.append(piece)?;
    }
    Ok(PyString::new(py, "")
        .call_method1("join", (result,))?
        .unbind())
}

// @pypdf: pypdf._page_labels.number2lowercase_roman_numeral
#[pyfunction]
fn pagelabels_number2lowercase_roman_numeral<'py>(
    py: Python<'py>,
    number: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    // Upstream calls the module-global function (so monkey-patching is observable).
    let upper = shim_attr(py, "number2uppercase_roman_numeral")?.call1((number,))?;
    Ok(upper.call_method0("lower")?.unbind())
}

// @pypdf: pypdf._page_labels.number2uppercase_letter
#[pyfunction]
fn pagelabels_number2uppercase_letter<'py>(
    py: Python<'py>,
    number: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if number.le(0)? {
        return Err(PyValueError::new_err("Expecting a positive number."));
    }
    // A to Z, then the same letter repeated: AA to ZZ, AAA to ZZZ, and so on.
    let shifted = number.sub(1)?;
    let pair = shifted.divmod(26)?;
    let repetitions = pair.get_item(0)?;
    let position = pair.get_item(1)?;
    let length = repetitions.add(1)?;
    if length.gt(MAXIMUM_PAGE_LABEL_LENGTH)? {
        return Err(PyValueError::new_err("Number is too large."));
    }
    let ascii_uppercase = py.import("string")?.getattr("ascii_uppercase")?;
    let letter = ascii_uppercase.get_item(&position)?;
    let count = repetitions.add(1)?;
    Ok(letter.mul(&count)?.unbind())
}

// @pypdf: pypdf._page_labels.number2lowercase_letter
#[pyfunction]
fn pagelabels_number2lowercase_letter<'py>(
    py: Python<'py>,
    number: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let upper = shim_attr(py, "number2uppercase_letter")?.call1((number,))?;
    Ok(upper.call_method0("lower")?.unbind())
}

// ---------------------------------------------------------------------------
// Label computation
// ---------------------------------------------------------------------------

/// `_get_label(value, start_index, index)` implementation.
fn get_label_impl<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
    start_index: &Bound<'py, PyAny>,
    index: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // if /Nums array is not following the specification or if /Nums is empty
    if !is_instance(value, &builtin_class(py, "dict")?)? {
        // Fallback
        return fallback_label(index);
    }
    let start = value.call_method1("get", ("/St", 1))?;
    let prefix = value.call_method1("get", ("/P", ""))?;
    let style = value.call_method1("get", ("/S", py_none(py)))?;
    let numbering = shim_attr(py, "_NUMBERING_STYLES")?;
    // `_NUMBERING_STYLES.get(...)` raises for unhashable styles, like upstream.
    let mapping_function = numbering.call_method1("get", (&style,))?;
    if mapping_function.is_none() {
        // Unknown /S numbering style; fall back to the page position.
        let values = PyDict::new(py);
        values.set_item("style", &style)?;
        logger_warning(
            py,
            "Ignoring unknown page label numbering style %(style)r in /Nums.",
            Some(&values),
        )?;
        return fallback_label(index);
    }

    let computed: PyResult<Bound<'py, PyAny>> = (|| {
        // Upstream: `prefix + mapping_function(index - start_index + start)`.
        // Every operator stays at Python level so that malformed values raise
        // the exact upstream `TypeError`/`ValueError` messages.
        let difference = index.sub(start_index)?;
        let number = difference.add(&start)?;
        let mapped = mapping_function.call1((number,))?;
        let label = prefix.add(&mapped)?;
        if len_of(&label)? > MAXIMUM_PAGE_LABEL_LENGTH {
            return Err(PyValueError::new_err("Page label is too long."));
        }
        Ok(label)
    })();
    match computed {
        Ok(label) => Ok(label),
        // Malformed /St or /P value or too large value; fall back to the page position.
        Err(error)
            if error.is_instance_of::<PyTypeError>(py)
                || error.is_instance_of::<PyValueError>(py) =>
        {
            let values = PyDict::new(py);
            values.set_item("start", &start)?;
            values.set_item("prefix", &prefix)?;
            values.set_item("exception", error.value(py))?;
            logger_warning(
                py,
                "Ignoring malformed page label entry in /Nums (/St=%(start)r, /P=%(prefix)r): %(exception)s",
                Some(&values),
            )?;
            fallback_label(index)
        }
        Err(error) => Err(error),
    }
}

// @pypdf: pypdf._page_labels._get_label
#[pyfunction]
fn pagelabels_get_label<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
    start_index: &Bound<'py, PyAny>,
    index: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(get_label_impl(py, value, start_index, index)?.unbind())
}

// @pypdf: pypdf._page_labels.get_label_from_nums
#[pyfunction]
fn pagelabels_get_label_from_nums<'py>(
    py: Python<'py>,
    dictionary_object: &Bound<'py, PyAny>,
    index: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let array_class = generic_class(py, "ArrayObject")?;
    let nums = dictionary_object.get_item("/Nums")?;
    if !is_instance(&nums, &array_class)? {
        let values = PyDict::new(py);
        values.set_item("nums", &nums)?;
        logger_warning(
            py,
            "Page label numbers are not an array: %(nums)s",
            Some(&values),
        )?;
        // Fallback
        return Ok(fallback_label(index)?.unbind());
    }
    let nums_length = len_of(&nums)? as i64;
    let mut i: i64 = 0;
    let mut value: Bound<'py, PyAny> = py_none(py);
    let mut start_index: Bound<'py, PyAny> = py_int(py, 0);
    while i < nums_length {
        if i + 1 >= nums_length {
            logger_warning_plain(py, "Ignoring last /Nums key without a value.")?;
            break;
        }
        start_index = nums.get_item(i)?;
        value = nums.get_item(i + 1)?.call_method0("get_object")?;
        if i + 2 == nums_length {
            break;
        }
        let next_key = nums.get_item(i + 2)?;
        if !is_int_or_float(py, &next_key)? {
            let values = PyDict::new(py);
            values.set_item("key", &next_key)?;
            logger_warning(
                py,
                "Ignoring malformed page label key %(key)r in /Nums.",
                Some(&values),
            )?;
            // Fallback
            return Ok(fallback_label(index)?.unbind());
        }
        if next_key.gt(index)? {
            break;
        }
        i += 2;
    }
    // Upstream calls the module-global `_get_label`, so a monkey-patched
    // `pypdf._page_labels._get_label` is observed here as well.
    Ok(shim_attr(py, "_get_label")?
        .call1((&value, &start_index, index))?
        .unbind())
}

// @pypdf: pypdf._page_labels.index2label
#[pyfunction]
fn pagelabels_index2label<'py>(
    py: Python<'py>,
    reader: &Bound<'py, PyAny>,
    index: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let dict_class = generic_class(py, "DictionaryObject")?;
    let array_class = generic_class(py, "ArrayObject")?;
    let null_class = generic_class(py, "NullObject")?;
    let list_class = builtin_class(py, "list")?;

    let root = reader.getattr("root_object")?;
    if !root.contains("/PageLabels")? {
        // Fallback
        return Ok(fallback_label(index)?.unbind());
    }
    let mut number_tree = root.get_item("/PageLabels")?.call_method0("get_object")?;
    if !is_instance(&number_tree, &dict_class)? {
        let values = PyDict::new(py);
        values.set_item("number_tree", &number_tree)?;
        logger_warning(
            py,
            "Page labels are not a dictionary: %(number_tree)s",
            Some(&values),
        )?;
        // Fallback
        return Ok(fallback_label(index)?.unbind());
    }
    if number_tree.contains("/Nums")? {
        return Ok(shim_attr(py, "get_label_from_nums")?
            .call1((&number_tree, index))?
            .unbind());
    }
    if number_tree.contains("/Kids")? && !is_instance(&number_tree.get_item("/Kids")?, &null_class)?
    {
        // number_tree = {'/Kids': [IndirectObject(7333, 0, 140132998195856), ...]}
        // Limit maximum depth.
        let mut level: i64 = 0;
        'outer: while level < 100 {
            let kids = number_tree.get_item("/Kids")?.call_method0("get_object")?;
            if !is_instance(&kids, &array_class)? {
                let values = PyDict::new(py);
                values.set_item("kids", &kids)?;
                logger_warning(
                    py,
                    "Page label kids are not an array: %(kids)s",
                    Some(&values),
                )?;
                break 'outer;
            }
            // The keys are page indices where a labelling range starts, so a page
            // between the /Limits of two kids belongs to the range started by the
            // largest key before it, the last key of the preceding kid.
            let mut preceding_kid: Option<Bound<'py, PyAny>> = None;
            let mut found_kid: Option<Bound<'py, PyAny>> = None;
            for kid in kids.try_iter()? {
                // kid = {'/Limits': [0, 63], '/Nums': [0, {'/P': 'C1'}, ...]}
                let kid = kid?.call_method0("get_object")?;
                if !is_instance(&kid, &dict_class)? {
                    logger_warning_plain(
                        py,
                        "Ignoring kid which is not a dictionary in /PageLabels.",
                    )?;
                    continue;
                }
                let default_limits = null_class.call0()?;
                let limits = kid
                    .call_method1("get", ("/Limits", default_limits))?
                    .call_method0("get_object")?;
                if !is_instance(&limits, &list_class)? || len_of(&limits)? < 2 {
                    // Skip kids whose /Limits range is missing or malformed.
                    logger_warning_plain(
                        py,
                        "Ignoring kid with missing or malformed /Limits in /PageLabels.",
                    )?;
                    continue;
                }
                let low = limits.get_item(0)?;
                let high = limits.get_item(1)?;
                if low.le(index)? && index.le(&high)? {
                    found_kid = Some(kid);
                    break;
                }
                if low.le(index)? {
                    preceding_kid = Some(kid);
                }
            }
            let kid = match found_kid {
                Some(kid) => kid,
                None => match preceding_kid {
                    Some(kid) => kid,
                    // When no kid starts at or before the index, make sure to exit
                    // the `while` loop directly and continue with the fallback.
                    None => break 'outer,
                },
            };
            let kids_attribute = kid.call_method1("get", ("/Kids", py_none(py)))?;
            let is_null_or_none = shim_attr(py, "is_null_or_none")?;
            if !is_null_or_none.call1((kids_attribute,))?.is_truthy()? {
                // Recursive definition.
                level += 1;
                if level == 100 {
                    return Err(PyNotImplementedError::new_err(
                        "Too deep nesting is not supported.",
                    ));
                }
                number_tree = kid;
                // Continue at the next level with the next iteration of the loop.
                continue;
            }
            return Ok(shim_attr(py, "get_label_from_nums")?
                .call1((&kid, index))?
                .unbind());
        }
    }

    let values = PyDict::new(py);
    values.set_item("index", index)?;
    logger_warning(
        py,
        "Could not reliably determine page label for %(index)d.",
        Some(&values),
    )?;
    // Fallback if neither /Nums nor /Kids is in the number_tree
    Ok(fallback_label(index)?.unbind())
}

// @pypdf: pypdf._page_labels._get_labels_for_page_indexes
#[pyfunction]
fn pagelabels_get_labels_for_page_indexes<'py>(
    py: Python<'py>,
    nums: &Bound<'py, PyAny>,
    indexes: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let nums_length = len_of(nums)? as i64;
    let labels = PyList::empty(py);
    let mut i: i64 = 0;
    let mut resolved: i64 = -1;
    let mut start_index: Bound<'py, PyAny> = py_int(py, 0);
    let mut value: Bound<'py, PyAny> = py_none(py);
    let get_label = shim_attr(py, "_get_label")?;
    for index in indexes.try_iter()? {
        let index = index?;
        let mut is_key_malformed = false;
        while i < nums_length {
            if i + 1 >= nums_length {
                logger_warning_plain(py, "Ignoring last /Nums key without a value.")?;
                break;
            }
            if resolved != i {
                start_index = nums.get_item(i)?;
                value = nums.get_item(i + 1)?.call_method0("get_object")?;
                resolved = i;
            }
            if i + 2 == nums_length {
                break;
            }
            let next_key = nums.get_item(i + 2)?;
            if !is_int_or_float(py, &next_key)? {
                let values = PyDict::new(py);
                values.set_item("key", &next_key)?;
                logger_warning(
                    py,
                    "Ignoring malformed page label key %(key)r in /Nums.",
                    Some(&values),
                )?;
                is_key_malformed = true;
                break;
            }
            if next_key.gt(&index)? {
                break;
            }
            i += 2;
        }
        let label = if is_key_malformed {
            fallback_label(&index)?
        } else {
            get_label.call1((&value, &start_index, &index))?
        };
        labels.append(label)?;
    }
    Ok(labels.into_any().unbind())
}

/// `_get_page_range(limits, page_count)` implementation.
fn get_page_range_impl<'py>(
    py: Python<'py>,
    limits: &Bound<'py, PyAny>,
    page_count: &Bound<'py, PyAny>,
) -> PyResult<Option<(Bound<'py, PyAny>, Bound<'py, PyAny>)>> {
    let low = limits.get_item(0)?;
    let high = limits.get_item(1)?;
    if low.ge(page_count)? || high.lt(0)? {
        return Ok(None);
    }
    let last_page = page_count.sub(1)?;
    let math = py.import("math")?;
    let first = if low.le(0)? {
        py_int(py, 0)
    } else {
        math.getattr("ceil")?.call1((&low,))?
    };
    let last = if high.ge(&last_page)? {
        last_page.clone()
    } else {
        math.getattr("floor")?.call1((&high,))?
    };
    if first.le(&last)? {
        Ok(Some((first, last)))
    } else {
        Ok(None)
    }
}

// @pypdf: pypdf._page_labels._get_page_range
#[pyfunction]
fn pagelabels_get_page_range<'py>(
    py: Python<'py>,
    limits: &Bound<'py, PyAny>,
    page_count: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    match get_page_range_impl(py, limits, page_count)? {
        None => Ok(py_none(py).unbind()),
        Some((first, last)) => Ok(py_tuple(py, vec![first, last])?.unbind()),
    }
}

// ---------------------------------------------------------------------------
// _FirstKidFinder
// ---------------------------------------------------------------------------

// @pypdf: pypdf._page_labels._FirstKidFinder.find
#[pyfunction]
fn pagelabels_firstkidfinder_find<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    index: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    loop {
        let starts = slf.getattr("_starts")?;
        let position = bisect_right(py, &starts, index)? - 1;
        if position >= 0 {
            let ends = slf.getattr("_ends")?;
            if index.le(&ends.get_item(position)?)? {
                let first_kids = slf.getattr("_first_kids")?;
                return Ok(first_kids.get_item(position)?.unbind());
            }
        }
        let next_kid = slf.getattr("_next_kid")?.extract::<i64>()?;
        let kid_count = slf.getattr("_kid_count")?.extract::<i64>()?;
        if next_kid >= kid_count {
            return Ok(py_none(py).unbind());
        }
        let kids = slf.getattr("_kids")?;
        let kid_range = slf.call_method1("_read_kid", (kids.get_item(next_kid)?,))?;
        slf.setattr("_next_kid", next_kid + 1)?;
        if !kid_range.is_none() {
            let first = kid_range.get_item(0)?;
            let last = kid_range.get_item(1)?;
            let kid = kid_range.get_item(2)?;
            slf.call_method1("_cover", (first, last, kid))?;
        }
    }
}

// @pypdf: pypdf._page_labels._FirstKidFinder.find_preceding
#[pyfunction]
fn pagelabels_firstkidfinder_find_preceding<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    index: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let read_lows = slf.getattr("_read_lows")?;
    let position = bisect_right(py, &read_lows, index)? - 1;
    if position < 0 {
        return Ok(py_none(py).unbind());
    }
    let read_kids = slf.getattr("_read_kids")?;
    Ok(read_kids.get_item(position)?.unbind())
}

// @pypdf: pypdf._page_labels._FirstKidFinder._read_kid
#[pyfunction]
fn pagelabels_firstkidfinder_read_kid<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    kid: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let dict_class = generic_class(py, "DictionaryObject")?;
    let null_class = generic_class(py, "NullObject")?;

    let parsed: PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> = (|| {
        let kid = kid.call_method0("get_object")?;
        let limits = if is_instance(&kid, &dict_class)? {
            kid.call_method1("get", ("/Limits", null_class.call0()?))?
                .call_method0("get_object")?
        } else {
            py_none(py)
        };
        Ok((kid, limits))
    })();
    let (kid, limits) = match parsed {
        Ok(parsed) => parsed,
        Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
        Err(error) => {
            let page_count = slf.getattr("_page_count")?;
            let last = page_count.sub(1)?;
            let exception = error.value(py).clone().into_any();
            return Ok(py_tuple(py, vec![py_int(py, 0), last, exception])?.unbind());
        }
    };
    if !is_instance(&kid, &dict_class)? {
        logger_warning_plain(py, "Ignoring kid which is not a dictionary in /PageLabels.")?;
        return Ok(py_none(py).unbind());
    }
    let list_class = builtin_class(py, "list")?;
    let mut valid_limits = is_instance(&limits, &list_class)? && len_of(&limits)? >= 2;
    if valid_limits {
        for position in 0..2 {
            if !is_int_or_float(py, &limits.get_item(position)?)? {
                valid_limits = false;
                break;
            }
        }
    }
    if !valid_limits {
        logger_warning_plain(
            py,
            "Ignoring kid with missing or malformed /Limits in /PageLabels.",
        )?;
        return Ok(py_none(py).unbind());
    }
    slf.getattr("_read_kids")?.call_method1("append", (&kid,))?;
    slf.getattr("_read_lows")?
        .call_method1("append", (limits.get_item(0)?,))?;
    let page_count = slf.getattr("_page_count")?;
    match get_page_range_impl(py, &limits, &page_count)? {
        None => Ok(py_none(py).unbind()),
        Some((first, last)) => Ok(py_tuple(py, vec![first, last, kid])?.unbind()),
    }
}

// @pypdf: pypdf._page_labels._FirstKidFinder._cover
#[pyfunction]
fn pagelabels_firstkidfinder_cover<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    first: &Bound<'py, PyAny>,
    last: &Bound<'py, PyAny>,
    kid: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let mut first = first.extract::<i64>()?;
    let mut last = last.extract::<i64>()?;
    let covered_starts = slf.getattr("_covered_starts")?;
    let covered_ends = slf.getattr("_covered_ends")?;
    let starts = slf.getattr("_starts")?;
    let ends = slf.getattr("_ends")?;

    let start = bisect_left(py, &covered_ends, &py_int(py, first))?;
    let end = bisect_right(py, &covered_starts, &py_int(py, last))?;
    let mut gaps: Vec<(i64, i64)> = Vec::new();
    let mut position = first;
    for offset in start..end {
        let covered_start = covered_starts.get_item(offset)?.extract::<i64>()?;
        let covered_end = covered_ends.get_item(offset)?.extract::<i64>()?;
        if covered_start > position {
            gaps.push((position, covered_start - 1));
        }
        position = position.max(covered_end + 1);
    }
    if position <= last {
        gaps.push((position, last));
    }
    if start < end {
        first = first.min(covered_starts.get_item(start)?.extract::<i64>()?);
        last = last.max(covered_ends.get_item(end - 1)?.extract::<i64>()?);
    }
    let slice = py_slice(py, Some(start), Some(end))?;
    covered_starts.set_item(&slice, py_list(py, vec![py_int(py, first)])?)?;
    covered_ends.set_item(&slice, py_list(py, vec![py_int(py, last)])?)?;
    for (gap_start, gap_end) in gaps {
        let position = bisect_left(py, &starts, &py_int(py, gap_start))?;
        starts.call_method1("insert", (position, py_int(py, gap_start)))?;
        ends.call_method1("insert", (position, py_int(py, gap_end)))?;
        slf.getattr("_first_kids")?
            .call_method1("insert", (position, kid))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Bulk label computation for /Kids number trees
// ---------------------------------------------------------------------------

// @pypdf: pypdf._page_labels._get_labels_from_kids
#[pyfunction]
fn pagelabels_get_labels_from_kids<'py>(
    py: Python<'py>,
    number_tree: &Bound<'py, PyAny>,
    page_count: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let array_class = generic_class(py, "ArrayObject")?;
    let exception_class = builtin_class(py, "Exception")?;
    let tree_node_class = shim_attr(py, "_TreeNode")?;
    let kid_pages_class = shim_attr(py, "_KidPages")?;
    let leaf_pages_class = shim_attr(py, "_LeafPages")?;
    let finder_class = shim_attr(py, "_FirstKidFinder")?;
    let is_null_or_none = shim_attr(py, "is_null_or_none")?;
    let range = builtin_class(py, "range")?.call1((page_count,))?;
    let empty_indexes = builtin_class(py, "list")?.call1((range,))?;

    // The finders keep their /Kids arrays, so the ids stay unique.
    let finders = PyDict::new(py);
    let leaves = PyDict::new(py);
    let unmatched = PyList::empty(py);
    let nodes = PyList::empty(py);
    nodes.append(tree_node_class.call1((number_tree, empty_indexes, 0))?)?;
    while !nodes.is_empty() {
        let node = nodes.call_method0("pop")?;
        let node_indexes = node.getattr("indexes")?;
        let kids = node
            .getattr("node")?
            .get_item("/Kids")?
            .call_method0("get_object")?;
        if !is_instance(&kids, &array_class)? {
            let values = PyDict::new(py);
            values.set_item("kids", &kids)?;
            logger_warning(
                py,
                "Page label kids are not an array: %(kids)s",
                Some(&values),
            )?;
            unmatched.call_method1("extend", (&node_indexes,))?;
            continue;
        }
        // Upstream keys the cache by `id(kids)`; a Python dict keeps the exact
        // insertion-ordered iteration semantics.
        let kids_id = builtins(py)?.getattr("id")?.call1((&kids,))?;
        let finder = match finders.get_item(&kids_id) {
            Ok(Some(finder)) => finder,
            Ok(None) => {
                let finder = finder_class.call1((&kids, page_count))?;
                finders.set_item(&kids_id, &finder)?;
                finder
            }
            Err(error) => return Err(error),
        };
        let kid_pages = PyDict::new(py);
        for index in node_indexes.try_iter()? {
            let index = index?;
            let mut kid = finder.call_method1("find", (&index,))?;
            if is_instance(&kid, &exception_class)? {
                return Err(PyErr::from_value(kid));
            }
            if kid.is_none() {
                kid = finder.call_method1("find_preceding", (&index,))?;
            }
            if kid.is_none() {
                unmatched.append(&index)?;
            } else {
                let kid_id = builtins(py)?.getattr("id")?.call1((&kid,))?;
                let entry = match kid_pages.get_item(&kid_id) {
                    Ok(Some(entry)) => entry,
                    Ok(None) => {
                        let entry = kid_pages_class.call1((&kid, PyList::empty(py)))?;
                        kid_pages.set_item(&kid_id, &entry)?;
                        entry
                    }
                    Err(error) => return Err(error),
                };
                entry
                    .getattr("indexes")?
                    .call_method1("append", (&index,))?;
            }
        }
        for entry in kid_pages.call_method0("values")?.try_iter()? {
            let entry = entry?;
            let kid = entry.getattr("kid")?;
            let indexes = entry.getattr("indexes")?;
            let kids_attribute = kid.call_method1("get", ("/Kids", py_none(py)))?;
            if !is_null_or_none.call1((kids_attribute,))?.is_truthy()? {
                // Same nesting limit as in index2label().
                let level = node.getattr("level")?.add(1)?;
                if level.eq(100)? {
                    return Err(PyNotImplementedError::new_err(
                        "Too deep nesting is not supported.",
                    ));
                }
                nodes.append(tree_node_class.call1((&kid, &indexes, level))?)?;
                continue;
            }
            // Leaves sharing a /Nums array are read together.
            let nums = kid.get_item("/Nums")?;
            if is_instance(&nums, &array_class)? {
                let nums_id = builtins(py)?.getattr("id")?.call1((&nums,))?;
                let entry = match leaves.get_item(&nums_id) {
                    Ok(Some(entry)) => entry,
                    Ok(None) => {
                        let entry = leaf_pages_class.call1((&nums, &kid, PyList::empty(py)))?;
                        leaves.set_item(&nums_id, &entry)?;
                        entry
                    }
                    Err(error) => return Err(error),
                };
                entry
                    .getattr("indexes")?
                    .call_method1("extend", (&indexes,))?;
            } else {
                let kid_id = builtins(py)?.getattr("id")?.call1((&kid,))?;
                let entry = match leaves.get_item(&kid_id) {
                    Ok(Some(entry)) => entry,
                    Ok(None) => {
                        let entry =
                            leaf_pages_class.call1((py_none(py), &kid, PyList::empty(py)))?;
                        leaves.set_item(&kid_id, &entry)?;
                        entry
                    }
                    Err(error) => return Err(error),
                };
                entry
                    .getattr("indexes")?
                    .call_method1("extend", (&indexes,))?;
            }
        }
    }

    let empty_labels = PyList::new(py, [PyString::new(py, "")])?;
    let labels = empty_labels.mul(page_count)?;
    let get_labels_for_page_indexes = shim_attr(py, "_get_labels_for_page_indexes")?;
    let get_label_from_nums = shim_attr(py, "get_label_from_nums")?;
    for entry in leaves.call_method0("values")?.try_iter()? {
        let entry = entry?;
        let leaf_nums = entry.getattr("nums")?;
        let leaf = entry.getattr("leaf")?;
        let leaf_indexes = entry.getattr("indexes")?;
        leaf_indexes.call_method0("sort")?;
        let leaf_labels = if leaf_nums.is_none() {
            let produced = PyList::empty(py);
            for index in leaf_indexes.try_iter()? {
                produced.append(get_label_from_nums.call1((&leaf, index?))?)?;
            }
            produced.into_any()
        } else {
            get_labels_for_page_indexes.call1((&leaf_nums, &leaf_indexes))?
        };
        for pair in leaf_indexes.try_iter()?.zip(leaf_labels.try_iter()?) {
            let (index, label) = pair;
            labels.set_item(index?, label?)?;
        }
    }
    let sorted_unmatched = builtin_class(py, "sorted")?.call1((&unmatched,))?;
    for index in sorted_unmatched.try_iter()? {
        let index = index?;
        let values = PyDict::new(py);
        values.set_item("index", &index)?;
        logger_warning(
            py,
            "Could not reliably determine page label for %(index)d.",
            Some(&values),
        )?;
        labels.set_item(&index, fallback_label(&index)?)?;
    }
    Ok(labels.unbind())
}

// @pypdf: pypdf._page_labels._index2labels_page_by_page
#[pyfunction]
fn pagelabels_index2labels_page_by_page<'py>(
    py: Python<'py>,
    pdf: &Bound<'py, PyAny>,
    page_count: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let index2label = shim_attr(py, "index2label")?;
    let range = builtin_class(py, "range")?.call1((page_count,))?;
    let labels = PyList::empty(py);
    for index in range.try_iter()? {
        labels.append(index2label.call1((pdf, index?))?)?;
    }
    Ok(labels.into_any().unbind())
}

// @pypdf: pypdf._page_labels.index2labels
#[pyfunction]
fn pagelabels_index2labels<'py>(
    py: Python<'py>,
    pdf: &Bound<'py, PyAny>,
    page_count: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if page_count.le(0)? {
        return Ok(PyList::empty(py).into_any().unbind());
    }
    let dict_class = generic_class(py, "DictionaryObject")?;
    let array_class = generic_class(py, "ArrayObject")?;
    let null_class = generic_class(py, "NullObject")?;

    let root = pdf.getattr("root_object")?;
    if !root.contains("/PageLabels")? {
        return Ok(shim_attr(py, "_index2labels_page_by_page")?
            .call1((pdf, page_count))?
            .unbind());
    }
    let number_tree = root.get_item("/PageLabels")?.call_method0("get_object")?;
    if !is_instance(&number_tree, &dict_class)? {
        return Ok(shim_attr(py, "_index2labels_page_by_page")?
            .call1((pdf, page_count))?
            .unbind());
    }
    if number_tree.contains("/Nums")? {
        let nums = number_tree.get_item("/Nums")?;
        if !is_instance(&nums, &array_class)? {
            let get_label_from_nums = shim_attr(py, "get_label_from_nums")?;
            let range = builtin_class(py, "range")?.call1((page_count,))?;
            let labels = PyList::empty(py);
            for index in range.try_iter()? {
                labels.append(get_label_from_nums.call1((&number_tree, index?))?)?;
            }
            return Ok(labels.into_any().unbind());
        }
        let range = builtin_class(py, "range")?.call1((page_count,))?;
        return Ok(shim_attr(py, "_get_labels_for_page_indexes")?
            .call1((&nums, range))?
            .unbind());
    }
    if number_tree.contains("/Kids")? && !is_instance(&number_tree.get_item("/Kids")?, &null_class)?
    {
        return Ok(shim_attr(py, "_get_labels_from_kids")?
            .call1((&number_tree, page_count))?
            .unbind());
    }
    Ok(shim_attr(py, "_index2labels_page_by_page")?
        .call1((pdf, page_count))?
        .unbind())
}

// ---------------------------------------------------------------------------
// Number-tree mutation helpers (used by PdfWriter)
// ---------------------------------------------------------------------------

// @pypdf: pypdf._page_labels.nums_insert
#[pyfunction]
fn pagelabels_nums_insert<'py>(
    _py: Python<'py>,
    key: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
    nums: &Bound<'py, PyAny>,
) -> PyResult<()> {
    if len_of(nums)? % 2 != 0 {
        return Err(PyValueError::new_err(
            "A nums like array must have an even number of elements",
        ));
    }
    let nums_length = len_of(nums)? as i64;
    let mut i = nums_length;
    while i != 0 && key.le(&nums.get_item(i - 2)?)? {
        i -= 2;
    }
    if i < nums_length && key.eq(&nums.get_item(i)?)? {
        nums.set_item(i + 1, value)?;
    } else {
        nums.call_method1("insert", (i, key))?;
        nums.call_method1("insert", (i + 1, value))?;
    }
    Ok(())
}

// @pypdf: pypdf._page_labels.nums_clear_range
#[pyfunction]
fn pagelabels_nums_clear_range<'py>(
    _py: Python<'py>,
    key: &Bound<'py, PyAny>,
    page_index_to: &Bound<'py, PyAny>,
    nums: &Bound<'py, PyAny>,
) -> PyResult<()> {
    if len_of(nums)? % 2 != 0 {
        return Err(PyValueError::new_err(
            "A nums like array must have an even number of elements",
        ));
    }
    if page_index_to.lt(key)? {
        return Err(PyValueError::new_err(
            "page_index_to must be greater or equal than key",
        ));
    }
    let i = nums.call_method1("index", (key,))?.extract::<i64>()? + 2;
    while i < len_of(nums)? as i64 && nums.get_item(i)?.le(page_index_to)? {
        nums.call_method1("pop", (i,))?;
        nums.call_method1("pop", (i,))?;
    }
    Ok(())
}

// @pypdf: pypdf._page_labels.nums_next
#[pyfunction]
fn pagelabels_nums_next<'py>(
    py: Python<'py>,
    key: &Bound<'py, PyAny>,
    nums: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if len_of(nums)? % 2 != 0 {
        return Err(PyValueError::new_err(
            "A nums like array must have an even number of elements",
        ));
    }
    let i = nums.call_method1("index", (key,))?.extract::<i64>()? + 2;
    if i < len_of(nums)? as i64 {
        return Ok(py_tuple(py, vec![nums.get_item(i)?, nums.get_item(i + 1)?])?.unbind());
    }
    Ok(py_tuple(py, vec![py_none(py), py_none(py)])?.unbind())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Symbols implemented in the hand-written shim
/// (`python/pypdf/_page_labels.py`): the module docstring, the module
/// constants, the `_NUMBERING_STYLES` table (it maps to the Python wrapper
/// functions so that it stays observable), the `typing.NamedTuple` data
/// containers and `_FirstKidFinder.__init__` (pure attribute assignment).
///
// @pypdf: pypdf._page_labels.MAXIMUM_ROMAN_NUMERAL
// @pypdf: pypdf._page_labels.MAXIMUM_PAGE_LABEL_LENGTH
// @pypdf: pypdf._page_labels._NUMBERING_STYLES
// @pypdf: pypdf._page_labels._FirstKidFinder.__init__
// @pypdf: pypdf._page_labels._FirstKidFinder
// @pypdf: pypdf._page_labels._TreeNode
// @pypdf: pypdf._page_labels._TreeNode.node
// @pypdf: pypdf._page_labels._TreeNode.indexes
// @pypdf: pypdf._page_labels._TreeNode.level
// @pypdf: pypdf._page_labels._KidPages
// @pypdf: pypdf._page_labels._KidPages.kid
// @pypdf: pypdf._page_labels._KidPages.indexes
// @pypdf: pypdf._page_labels._LeafPages
// @pypdf: pypdf._page_labels._LeafPages.nums
// @pypdf: pypdf._page_labels._LeafPages.leaf
// @pypdf: pypdf._page_labels._LeafPages.indexes
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(
        pagelabels_number2decimal_arabic_numeral,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        pagelabels_number2uppercase_roman_numeral,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        pagelabels_number2lowercase_roman_numeral,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        pagelabels_number2uppercase_letter,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        pagelabels_number2lowercase_letter,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(pagelabels_get_label, module)?)?;
    module.add_function(wrap_pyfunction!(pagelabels_get_label_from_nums, module)?)?;
    module.add_function(wrap_pyfunction!(pagelabels_index2label, module)?)?;
    module.add_function(wrap_pyfunction!(
        pagelabels_get_labels_for_page_indexes,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(pagelabels_get_page_range, module)?)?;
    module.add_function(wrap_pyfunction!(pagelabels_firstkidfinder_find, module)?)?;
    module.add_function(wrap_pyfunction!(
        pagelabels_firstkidfinder_find_preceding,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        pagelabels_firstkidfinder_read_kid,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(pagelabels_firstkidfinder_cover, module)?)?;
    module.add_function(wrap_pyfunction!(pagelabels_get_labels_from_kids, module)?)?;
    module.add_function(wrap_pyfunction!(
        pagelabels_index2labels_page_by_page,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(pagelabels_index2labels, module)?)?;
    module.add_function(wrap_pyfunction!(pagelabels_nums_insert, module)?)?;
    module.add_function(wrap_pyfunction!(pagelabels_nums_clear_range, module)?)?;
    module.add_function(wrap_pyfunction!(pagelabels_nums_next, module)?)?;
    Ok(())
}
