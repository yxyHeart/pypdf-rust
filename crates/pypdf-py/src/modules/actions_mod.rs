//! Rust port of `pypdf/actions/_actions.py` (upstream pypdf 7.1.1).
//!
//! # Design
//!
//! `PageTrigger` is a Python `StrEnum` and `Action`/`JavaScript` are Python
//! `DictionaryObject` subclasses defined in the hand-written shim
//! `python/pypdf/actions/_actions.py`; the enum members, the `ABC` base and the
//! classmethods stay exactly as upstream.  All logic (including the
//! additional-actions chain walking, the strict/non-strict error paths and the
//! `uuid4` based JavaScript name generation) is implemented in Rust here.
//!
//! Cross-object work goes through the Python object protocol like upstream:
//! `writer.root_object`, `page["/AA"]`, `x.get_object()`,
//! `writer._add_object(action)`, `is_null_or_none(...)`,
//! `create_string_object(str(uuid.uuid4()))`.  `uuid.uuid4` is therefore looked
//! up on the `uuid` module at call time, so the deterministic test hook that
//! patches `uuid.uuid4` is honoured.

use pyo3::exceptions::{PyAttributeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyModule, PySet};

use crate::errors;
use crate::pyutil;

const SOURCE: &str = "pypdf.actions._actions";
const ACTIONS_MODULE: &str = "pypdf.actions._actions";

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn generic_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    crate::pyutil::cached_module(py, "pypdf.generic")?.getattr(name)
}

fn generic_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    generic_attr(py, name)
}

fn actions_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import(ACTIONS_MODULE)?.getattr(name)
}

fn constants_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    crate::pyutil::cached_module(py, "pypdf.constants")?.getattr(name)
}

fn name_object<'py>(py: Python<'py>, value: &str) -> PyResult<Bound<'py, PyAny>> {
    crate::pyutil::cached_module(py, "pypdf.generic._base")?
        .getattr("NameObject")?
        .call1((value,))
}

fn null_object<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "NullObject")?.call0()
}

fn dictionary_object<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "DictionaryObject")?.call0()
}

fn array_object<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "ArrayObject")?.call0()
}

fn is_null_or_none<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<bool> {
    generic_attr(py, "is_null_or_none")?
        .call1((value,))?
        .is_truthy()
}

/// `type(value).__name__`.
fn type_name(value: &Bound<'_, PyAny>) -> PyResult<String> {
    pyutil::py_str(&value.get_type().getattr("__name__")?)
}

/// `id(value)` as a Python int (used as a set member, exactly like upstream).
fn object_id<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    crate::pyutil::cached_module(py, "builtins")?.getattr("id")?.call1((value,))
}

/// `str(uuid.uuid4())`.
fn new_uuid_name<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    Ok(py
        .import("uuid")?
        .getattr("uuid4")?
        .call0()?
        .str()?
        .into_any())
}

/// `logger_warning(message, source=__name__, **values)`.
fn log_warning(py: Python<'_>, message: &str, values: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    pyutil::logger_warning(py, SOURCE, message, values)
}

// ---------------------------------------------------------------------------
// PageTrigger._name_object
// ---------------------------------------------------------------------------

// @pypdf: pypdf.actions._actions.PageTrigger._name_object
#[pyfunction]
fn actionsmod_page_trigger_name_object(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    // ``mapping = {PageTrigger.OPEN: NameObject("/O"),
    //             PageTrigger.CLOSE: NameObject("/C")}`` then ``mapping[self]``;
    // a non-member triggers the same ``KeyError`` as upstream.
    let trigger_class = actions_attr(py, "PageTrigger")?;
    let mapping = PyDict::new(py);
    mapping.set_item(trigger_class.getattr("OPEN")?, name_object(py, "/O")?)?;
    mapping.set_item(trigger_class.getattr("CLOSE")?, name_object(py, "/C")?)?;
    Ok(mapping.as_any().get_item(slf)?.unbind())
}

// ---------------------------------------------------------------------------
// Action
// ---------------------------------------------------------------------------

// @pypdf: pypdf.actions._actions.Action.__init__
#[pyfunction]
fn actionsmod_action_init(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<()> {
    crate::pyutil::cached_module(py, "pypdf.generic._data_structures")?
        .getattr("DictionaryObject")?
        .getattr("__init__")?
        .call1((slf,))?;
    slf.set_item(name_object(py, "/Type")?, name_object(py, "/Action")?)?;
    // The next action or sequence of actions that shall be performed after the
    // action represented by this dictionary. The value is either a single
    // action dictionary or an array of action dictionaries that shall be
    // performed in order.
    slf.set_item(name_object(py, "/Next")?, null_object(py)?)?;
    Ok(())
}

// @pypdf: pypdf.actions._actions.Action._create_open_action
#[pyfunction]
fn actionsmod_action_create_open_action(
    py: Python<'_>,
    writer: &Bound<'_, PyAny>,
    action: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let names_key = constants_attr(py, "CatalogAttributes")?.getattr("NAMES")?;
    let root_object = writer.getattr("root_object")?;
    if !root_object.contains(&names_key)? {
        root_object.set_item(name_object(py, "/Names")?, dictionary_object(py)?)?;
    }
    let names = root_object.get_item(&names_key)?;
    let javascript_key = "/JavaScript";
    if !names.contains(javascript_key)? {
        let inner = PyDict::new(py);
        inner.set_item(name_object(py, "/Names")?, array_object(py)?)?;
        let javascript = generic_class(py, "DictionaryObject")?.call1((inner,))?;
        names.set_item(name_object(py, javascript_key)?, javascript)?;
    }
    let javascript = names.get_item(javascript_key)?;
    let js = javascript.get_item("/Names")?;

    // We need a name for parameterized JavaScript in the PDF file, but it can
    // be anything.
    let uuid_name = new_uuid_name(py)?;
    let created = generic_attr(py, "create_string_object")?.call1((uuid_name,))?;
    js.call_method1("append", (created,))?;
    js.call_method1("append", (writer.call_method1("_add_object", (action,))?,))?;
    Ok(())
}

// @pypdf: pypdf.actions._actions.Action._create_new
#[pyfunction]
fn actionsmod_action_create_new(
    py: Python<'_>,
    page: &Bound<'_, PyAny>,
    trigger: &Bound<'_, PyAny>,
    action: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let trigger_name = trigger.getattr("_name_object")?;

    if !page.contains("/AA")? {
        // Additional actions key not present
        let additional_actions = PyDict::new(py);
        additional_actions.set_item(&trigger_name, action)?;
        let wrapped = generic_class(py, "DictionaryObject")?.call1((additional_actions,))?;
        page.set_item(name_object(py, "/AA")?, wrapped)?;
        return Ok(());
    }

    if page
        .get_item("/AA")?
        .is_instance(&generic_class(py, "NullObject")?)?
    {
        page.set_item(name_object(py, "/AA")?, dictionary_object(py)?)?;
    }

    let dictionary_class = generic_class(py, "DictionaryObject")?;
    let additional_value = page.get_item("/AA")?;
    if !additional_value
        .call_method0("get_object")?
        .is_instance(&dictionary_class)?
    {
        let current_type = type_name(&additional_value)?;
        let pdf = page.getattr("pdf")?;
        let strict = if pdf.is_none() {
            false
        } else {
            match pdf.getattr("strict") {
                Ok(value) => value.is_truthy()?,
                Err(error) if error.is_instance_of::<PyAttributeError>(py) => false,
                Err(error) => return Err(error),
            }
        };
        if !pdf.is_none() && strict {
            return Err(errors::raise_msg(
                py,
                "ParseError",
                &format!(
                    "The type in a page object's additional-actions key must be a \
                     DictionaryObject: received type {current_type}."
                ),
            ));
        }
        let values = PyDict::new(py);
        values.set_item("type", current_type)?;
        log_warning(
            py,
            "The type in a page object's additional-actions key must be a \
             DictionaryObject: received type %(type)s.",
            Some(&values),
        )?;
        return Ok(());
    }

    let additional_actions = page.get_item("/AA")?;
    let current_value = additional_actions.call_method1("get", (&trigger_name,))?;
    if is_null_or_none(py, &current_value)? {
        let update = PyDict::new(py);
        update.set_item(&trigger_name, action)?;
        additional_actions.call_method1("update", (update,))?;
        return Ok(());
    }

    // The action dictionary's Next entry allows sequences of actions to be
    // chained together. For example, the effect of clicking a link annotation
    // with the mouse can be to play a sound, jump to a new page, and start up a
    // movie. Note that the Next entry is not restricted to a single action but
    // can contain an array of actions, each of which in turn can have a Next
    // entry of its own.  §12.6.2 Action dictionaries ISO 32000-2:2020
    let head = additional_actions.call_method1("get", (&trigger_name, null_object(py)?))?;
    let array_class = generic_class(py, "ArrayObject")?;
    let mut current = head.call_method0("get_object")?;
    if !current.is_instance(&dictionary_class)? {
        return Err(errors::raise_msg(
            py,
            "ParseError",
            &format!(
                "The type in a page object's additional-actions key must be a \
                 DictionaryObject: received type {}.",
                type_name(&current)?
            ),
        ));
    }

    let visited = PySet::empty(py)?;
    let mut next_node;
    loop {
        next_node = current
            .call_method1("get", ("/Next", null_object(py)?))?
            .call_method0("get_object")?;
        if is_null_or_none(py, &next_node)? {
            break;
        }
        if !(next_node.is_instance(&array_class)? || next_node.is_instance(&dictionary_class)?) {
            return Err(PyTypeError::new_err(format!(
                "An action dictionary’s Next entry must be an Action dictionary or an \
                 array of Action dictionaries: received type {}.",
                type_name(&next_node)?
            )));
        }
        let id_next = object_id(py, &next_node)?;
        if visited.contains(&id_next)? {
            let values = PyDict::new(py);
            values.set_item("current", &current)?;
            log_warning(
                py,
                "Detected cycle in the action tree for %(current)s",
                Some(&values),
            )?;
            break;
        }
        visited.add(&id_next)?;
        if next_node.is_instance(&array_class)? {
            let last = next_node.get_item(-1)?;
            current = last.call_method0("get_object")?;
        } else {
            current = next_node;
        }
    }

    let tail_next = current.call_method1("get", ("/Next",))?;
    if !is_null_or_none(py, &tail_next)? && visited.contains(object_id(py, &tail_next)?)? {
        let values = PyDict::new(py);
        values.set_item("current", &current)?;
        log_warning(
            py,
            "Detected cycle in the action tree for %(current)s",
            Some(&values),
        )?;
    }

    current.set_item(name_object(py, "/Next")?, action)?;
    let update = PyDict::new(py);
    update.set_item(&trigger_name, &head)?;
    additional_actions.call_method1("update", (update,))?;
    Ok(())
}

// @pypdf: pypdf.actions._actions.Action._delete
#[pyfunction]
fn actionsmod_action_delete(page: &Bound<'_, PyAny>, trigger: &Bound<'_, PyAny>) -> PyResult<()> {
    if !page.contains("/AA")? {
        return Ok(());
    }
    let trigger_name = trigger.getattr("_name_object")?;
    let additional_actions = page.get_item("/AA")?;
    if !additional_actions.contains(&trigger_name)? {
        return Ok(());
    }
    additional_actions.del_item(&trigger_name)?;
    if !additional_actions.is_truthy()? {
        page.del_item("/AA")?;
    }
    Ok(())
}

// @pypdf: pypdf.actions._actions.JavaScript.__init__
#[pyfunction]
fn actionsmod_javascript_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    java_script: &Bound<'_, PyAny>,
) -> PyResult<()> {
    actionsmod_action_init(py, slf)?;
    slf.set_item(name_object(py, "/S")?, name_object(py, "/JavaScript")?)?;
    slf.set_item(
        name_object(py, "/JS")?,
        crate::pyutil::cached_module(py, "pypdf.generic._base")?
            .getattr("TextStringObject")?
            .call1((java_script,))?,
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Definitions that stay on the Python side
// ---------------------------------------------------------------------------
//
// `PageTrigger` is a Python `StrEnum` (its members must be real enum members so
// `PageTrigger("open")`, hashing and `str()` match upstream) and
// `Action`/`JavaScript` are Python `DictionaryObject` subclasses (MRO, `ABC`
// base, classmethods).  They are listed here for gate A6; every method body
// lives in this file.
//
// @pypdf: pypdf.actions._actions.StrEnum
// @pypdf: pypdf.actions._actions.StrEnum.__str__
// @pypdf: pypdf.actions._actions.PageTrigger
// @pypdf: pypdf.actions._actions.PageTrigger.OPEN
// @pypdf: pypdf.actions._actions.PageTrigger.CLOSE
// @pypdf: pypdf.actions._actions.Action
// @pypdf: pypdf.actions._actions.JavaScript

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(
        actionsmod_page_trigger_name_object,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(actionsmod_action_init, module)?)?;
    module.add_function(wrap_pyfunction!(
        actionsmod_action_create_open_action,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(actionsmod_action_create_new, module)?)?;
    module.add_function(wrap_pyfunction!(actionsmod_action_delete, module)?)?;
    module.add_function(wrap_pyfunction!(actionsmod_javascript_init, module)?)?;
    Ok(())
}
