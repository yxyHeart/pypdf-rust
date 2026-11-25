//! Value classes from `pypdf/_utils.py`: `File`, `Version`, `_TraversalState`.
//!
//! All of them are Python classes upstream (`Version` an ordered class,
//! `File`/`_TraversalState` dataclasses), so they are exposed as
//! (subclassable) pyclasses with the same public surface.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyBytes, PyDict, PyList, PyString, PyTuple};

use crate::pyutil;

use super::common;
use super::misc;

/// Shared `@dataclass`-style argument parsing: positional fields followed by
/// keyword fields, with CPython's `__init__` error messages.
fn dataclass_arguments<'py>(
    class_name: &str,
    fields: &[&str],
    args: &Bound<'py, PyTuple>,
    kwargs: Option<&Bound<'py, PyDict>>,
) -> PyResult<Vec<Option<Py<PyAny>>>> {
    if args.len() > fields.len() {
        return Err(PyTypeError::new_err(format!(
            "{class_name}.__init__() takes from 1 to {} positional arguments but {} were given",
            fields.len() + 1,
            args.len() + 1
        )));
    }
    let mut values: Vec<Option<Py<PyAny>>> = (0..fields.len()).map(|_| None).collect();
    for (index, value) in args.iter().enumerate() {
        values[index] = Some(value.unbind());
    }
    if let Some(kwargs) = kwargs {
        for (key, value) in kwargs.iter() {
            let key_text: String = key.extract()?;
            let Some(index) = fields.iter().position(|field| *field == key_text) else {
                return Err(PyTypeError::new_err(format!(
                    "{class_name}.__init__() got an unexpected keyword argument '{key_text}'"
                )));
            };
            if values[index].is_some() {
                return Err(PyTypeError::new_err(format!(
                    "{class_name}.__init__() got multiple values for argument '{key_text}'"
                )));
            }
            values[index] = Some(value.unbind());
        }
    }
    Ok(values)
}

const FILE_FIELDS: [&str; 3] = ["name", "data", "indirect_reference"];

// @pypdf: pypdf._utils.File
#[pyclass(name = "File", module = "pypdf._utils", subclass, dict)]
pub struct File {
    // @pypdf: pypdf._utils.File.name
    #[pyo3(get, set)]
    name: Py<PyAny>,
    // @pypdf: pypdf._utils.File.data
    #[pyo3(get, set)]
    data: Py<PyAny>,
    // @pypdf: pypdf._utils.File.indirect_reference
    #[pyo3(get, set)]
    indirect_reference: Py<PyAny>,
}

#[pymethods]
impl File {
    /// Upstream is a `@dataclass`.  Arguments are parsed by hand so that an
    /// omitted field is filled with the dataclass default while an explicitly
    /// passed `None` is stored as `None` (PyO3's optional-argument lowering
    /// cannot distinguish the two).
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn new<'py>(
        py: Python<'py>,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Self> {
        let mut values = dataclass_arguments("File", &FILE_FIELDS, args, kwargs)?;
        Ok(Self {
            name: values
                .remove(0)
                .unwrap_or_else(|| PyString::new(py, "").into_any().unbind()),
            data: values
                .remove(0)
                .unwrap_or_else(|| PyBytes::new(py, b"").into_any().unbind()),
            indirect_reference: values.remove(0).unwrap_or_else(|| py.None()),
        })
    }

    // @pypdf: pypdf._utils.File.__str__
    fn __str__(slf: &Bound<'_, Self>) -> PyResult<String> {
        let py = slf.py();
        let class_name: String = slf.get_type().getattr("__name__")?.extract()?;
        let name = slf.getattr("name")?;
        let data = slf.getattr("data")?;
        let length_text =
            misc::human_readable_bytes(py, &common::py_int(py, pyutil::py_len(&data)? as i64))?;
        Ok(format!(
            "{class_name}(name={}, data: {length_text})",
            pyutil::py_str(&name)?
        ))
    }

    // @pypdf: pypdf._utils.File.__repr__
    fn __repr__(slf: &Bound<'_, Self>) -> PyResult<String> {
        let py = slf.py();
        let text: String = slf.call_method0("__str__")?.extract()?;
        let text_object = PyString::new(py, &text);
        let last = common::py_int(py, -1);
        let slice = pyutil::py_slice(py, None, Some(last.as_any()), None)?;
        let head: String = text_object.get_item(slice)?.extract()?;
        let hash_value = slf.getattr("data")?.hash()?;
        Ok(format!("{head}, hash: {hash_value})"))
    }

    fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        if !slf.get_type().eq(other.get_type())? {
            return Ok(false);
        }
        let py = slf.py();
        let left = PyTuple::new(
            py,
            [
                slf.getattr("name")?,
                slf.getattr("data")?,
                slf.getattr("indirect_reference")?,
            ],
        )?;
        let right = PyTuple::new(
            py,
            [
                other.getattr("name")?,
                other.getattr("data")?,
                other.getattr("indirect_reference")?,
            ],
        )?;
        left.eq(&right)
    }

    fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
        let class_name: String = slf.get_type().getattr("__name__")?.extract()?;
        Err(PyTypeError::new_err(format!(
            "unhashable type: '{class_name}'"
        )))
    }
}

/// `Version._parse_version` implementation (also used by `Version.__init__`).
fn parse_version(py: Python<'_>, version_str: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let pattern = py.get_type::<Version>().getattr("COMPONENT_PATTERN")?;
    let components = version_str.call_method1("split", (".",))?;
    let parsed = PyList::empty(py);
    let int_type = crate::pyutil::cached_module(py, "builtins")?.getattr("int")?;
    for component in components.try_iter()? {
        let component = component?;
        let matched = pattern.call_method1("match", (component.clone(),))?;
        if matched.is_none() {
            parsed.append(PyTuple::new(
                py,
                [common::py_int(py, 0).into_any(), component],
            )?)?;
            continue;
        }
        let integer_prefix = matched.call_method1("group", (1,))?;
        let suffix = matched.call_method1("group", (2,))?;
        let value = int_type.call1((integer_prefix,))?;
        parsed.append(PyTuple::new(py, [value, suffix])?)?;
    }
    Ok(parsed.unbind().into_any())
}

// @pypdf: pypdf._utils.Version
#[pyclass(name = "Version", module = "pypdf._utils", subclass, dict)]
pub struct Version {
    #[pyo3(get, set)]
    version_str: Py<PyAny>,
    #[pyo3(get, set)]
    components: Py<PyAny>,
}

#[pymethods]
impl Version {
    // @pypdf: pypdf._utils.Version.COMPONENT_PATTERN
    #[classattr]
    #[allow(non_snake_case)]
    fn COMPONENT_PATTERN(py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(py
            .import("re")?
            .call_method1("compile", (r"^(\d+)(.*)$",))?
            .unbind())
    }

    // @pypdf: pypdf._utils.Version.__init__
    #[new]
    fn new(py: Python<'_>, version_str: &Bound<'_, PyAny>) -> PyResult<Self> {
        let components = parse_version(py, version_str)?;
        Ok(Self {
            version_str: version_str.clone().unbind(),
            components,
        })
    }

    // @pypdf: pypdf._utils.Version._parse_version
    fn _parse_version(
        &self,
        py: Python<'_>,
        version_str: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        parse_version(py, version_str)
    }

    // @pypdf: pypdf._utils.Version.__eq__
    fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = slf.py();
        let version_type = py.get_type::<Version>();
        if !other.is_instance(version_type.as_any())? {
            return Ok(false);
        }
        slf.getattr("components")?.eq(&other.getattr("components")?)
    }

    // @pypdf: pypdf._utils.Version.__hash__
    fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
        let py = slf.py();
        let components = slf.getattr("components")?;
        let tuple_components = py
            .import("builtins")?
            .getattr("tuple")?
            .call1((components,))?;
        PyTuple::new(py, [slf.get_type().into_any(), tuple_components])?.hash()
    }

    // @pypdf: pypdf._utils.Version.__lt__
    fn __lt__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = slf.py();
        let version_type = py.get_type::<Version>();
        if !other.is_instance(version_type.as_any())? {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Version cannot be compared against {}",
                pyutil::py_str(other.get_type().as_any())?
            )));
        }
        let self_components: Vec<Bound<'_, PyAny>> = slf
            .getattr("components")?
            .try_iter()?
            .collect::<PyResult<_>>()?;
        let other_components: Vec<Bound<'_, PyAny>> = other
            .getattr("components")?
            .try_iter()?
            .collect::<PyResult<_>>()?;
        for (self_component, other_component) in self_components.iter().zip(other_components.iter())
        {
            let self_value = self_component.get_item(0)?;
            let self_suffix = self_component.get_item(1)?;
            let other_value = other_component.get_item(0)?;
            let other_suffix = other_component.get_item(1)?;
            if self_value.lt(&other_value)? {
                return Ok(true);
            }
            if self_value.gt(&other_value)? {
                return Ok(false);
            }
            if self_suffix.lt(&other_suffix)? {
                return Ok(true);
            }
            if self_suffix.gt(&other_suffix)? {
                return Ok(false);
            }
        }
        Ok(self_components.len() < other_components.len())
    }

    /// `functools.total_ordering` generates `__gt__`/`__le__`/`__ge__` from
    /// `__lt__` and `__eq__`; reproduced here with the same semantics.
    fn __gt__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let less = Self::__lt__(slf, other)?;
        let equal = Self::__eq__(slf, other)?;
        Ok(!less && !equal)
    }

    fn __le__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let less = Self::__lt__(slf, other)?;
        let equal = Self::__eq__(slf, other)?;
        Ok(less || equal)
    }

    fn __ge__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(!Self::__lt__(slf, other)?)
    }
}

const TRAVERSAL_FIELDS: [&str; 2] = ["entry_count", "has_logged"];

// @pypdf: pypdf._utils._TraversalState
#[pyclass(name = "_TraversalState", module = "pypdf._utils", subclass, dict)]
pub struct TraversalState {
    // @pypdf: pypdf._utils._TraversalState.entry_count
    #[pyo3(get, set)]
    entry_count: Py<PyAny>,
    // @pypdf: pypdf._utils._TraversalState.has_logged
    #[pyo3(get, set)]
    has_logged: Py<PyAny>,
}

#[pymethods]
impl TraversalState {
    /// Upstream is a `@dataclass`; an omitted argument gets the dataclass
    /// default and an explicit `None` is stored as `None`.
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn new<'py>(
        py: Python<'py>,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Self> {
        let mut values = dataclass_arguments("_TraversalState", &TRAVERSAL_FIELDS, args, kwargs)?;
        Ok(Self {
            entry_count: values
                .remove(0)
                .unwrap_or_else(|| common::py_int(py, 0).into_any().unbind()),
            has_logged: values
                .remove(0)
                .unwrap_or_else(|| PyBool::new(py, false).to_owned().into_any().unbind()),
        })
    }

    fn __repr__(slf: &Bound<'_, Self>) -> PyResult<String> {
        // `@dataclass` renders `self.__class__.__qualname__` as the prefix.
        let qualname: String = slf.get_type().getattr("__qualname__")?.extract()?;
        Ok(format!(
            "{qualname}(entry_count={}, has_logged={})",
            pyutil::py_repr(&slf.getattr("entry_count")?)?,
            pyutil::py_repr(&slf.getattr("has_logged")?)?
        ))
    }

    fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        if !slf.get_type().eq(other.get_type())? {
            return Ok(false);
        }
        let py = slf.py();
        let left = PyTuple::new(
            py,
            [slf.getattr("entry_count")?, slf.getattr("has_logged")?],
        )?;
        let right = PyTuple::new(
            py,
            [other.getattr("entry_count")?, other.getattr("has_logged")?],
        )?;
        left.eq(&right)
    }

    fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
        let class_name: String = slf.get_type().getattr("__name__")?.extract()?;
        Err(PyTypeError::new_err(format!(
            "unhashable type: '{class_name}'"
        )))
    }
}
