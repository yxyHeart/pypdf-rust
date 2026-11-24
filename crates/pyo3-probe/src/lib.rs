//! P0 feasibility probe: can we mirror pypdf's object model with PyO3?
//!
//! The official pypdf test suite requires:
//!   * `DictionaryObject` to be a real `dict` (isinstance / dict methods / keys)
//!   * `ArrayObject` to be a real `list`
//!   * `FloatObject` to be a real `float`
//!   * Python-level subclassing (`class MyPage(PageObject)`)
//!   * arbitrary attribute assignment (`obj.indirect_reference = ...`)
//!   * `copy.deepcopy()` of container objects

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFloat, PyList, PyTuple};

#[pyclass(extends=PyDict, subclass, dict)]
struct DictObj {
    #[pyo3(get, set)]
    indirect_reference: Option<Py<PyAny>>,
}

#[pymethods]
impl DictObj {
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn new(args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        let _ = (args, kwargs);
        Self { indirect_reference: None }
    }

    fn describe(&self) -> String {
        "DictObj".to_string()
    }
}

#[pyclass(extends=PyList, subclass, dict)]
struct ArrayObj;

#[pymethods]
impl ArrayObj {
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn new(args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        let _ = (args, kwargs);
        Self
    }
}

#[pyclass(extends=PyFloat, subclass, dict)]
struct FloatObj {
    #[pyo3(get, set)]
    extra: Option<Py<PyAny>>,
}

#[pymethods]
impl FloatObj {
    #[new]
    #[pyo3(signature = (value=0.0))]
    fn new(value: f64) -> Self {
        let _ = value;
        Self { extra: None }
    }
}

#[pymodule]
fn pyo3_probe(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<DictObj>()?;
    m.add_class::<ArrayObj>()?;
    m.add_class::<FloatObj>()?;
    Ok(())
}
