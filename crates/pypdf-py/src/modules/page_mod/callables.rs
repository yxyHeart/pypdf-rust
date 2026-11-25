//! Small Python-callable objects used by the `pypdf._page` port.
//!
//! Upstream builds these as Python lambdas at a handful of call sites
//! (`lambda: lst`, `lambda idx: self[indices[idx]]`, the `merge_transformed_page`
//! transformation hook).  Python lambdas cannot be created from Rust, so three
//! tiny `#[pyclass]` callables stand in for them.  They are never part of the
//! public API: they only live in the `pypdf._pypdf` extension module and are
//! passed to the Python shim classes like the upstream lambdas.

use pyo3::prelude::*;
use pyo3::types::PyAnyMethods;

/// `lambda: value` — a constant-returning callable.
#[pyclass(name = "_PageConstantCallable", module = "pypdf._pypdf")]
pub struct ConstantCallable {
    value: Py<PyAny>,
}

#[pymethods]
impl ConstantCallable {
    fn __call__(&self, py: Python<'_>) -> Py<PyAny> {
        self.value.clone_ref(py)
    }
}

impl ConstantCallable {
    pub fn new(_py: Python<'_>, value: Bound<'_, PyAny>) -> Self {
        Self {
            value: value.unbind(),
        }
    }
}

/// `lambda page2_content: PageObject._add_transformation_matrix(page2_content, page2.pdf, ctm)`.
#[pyclass(name = "_PageTransformationCallable", module = "pypdf._pypdf")]
pub struct TransformationCallable {
    page2: Py<PyAny>,
    ctm: Py<PyAny>,
}

#[pymethods]
impl TransformationCallable {
    fn __call__(&self, py: Python<'_>, page2_content: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let shim = crate::pyutil::cached_module(py, "pypdf._page")?;
        let page_object = shim.getattr("PageObject")?;
        let pdf = self.page2.bind(py).getattr("pdf")?;
        let result = page_object.getattr("_add_transformation_matrix")?.call1((
            page2_content,
            pdf,
            self.ctm.bind(py),
        ))?;
        Ok(result.unbind())
    }
}

impl TransformationCallable {
    pub fn new(page2: Bound<'_, PyAny>, ctm: Bound<'_, PyAny>) -> Self {
        Self {
            page2: page2.unbind(),
            ctm: ctm.unbind(),
        }
    }
}

/// `lambda idx: owner[indices[idx]]` — used by `_VirtualList.__getitem__` slices.
#[pyclass(name = "_PageIndexCallable", module = "pypdf._pypdf")]
pub struct IndexCallable {
    owner: Py<PyAny>,
    indices: Py<PyAny>,
}

#[pymethods]
impl IndexCallable {
    fn __call__(&self, py: Python<'_>, idx: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let index = self.indices.bind(py).get_item(idx)?;
        let value = self.owner.bind(py).get_item(index)?;
        Ok(value.unbind())
    }
}

impl IndexCallable {
    pub fn new(owner: Bound<'_, PyAny>, indices: Bound<'_, PyAny>) -> Self {
        Self {
            owner: owner.unbind(),
            indices: indices.unbind(),
        }
    }
}

/// `lambda: lst` — used by `VirtualListImages.__getitem__` slices.
pub fn constant_callable(py: Python<'_>, value: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(Py::new(py, ConstantCallable::new(py, value))?.into_any())
}

pub fn register(module: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    module.add_class::<ConstantCallable>()?;
    module.add_class::<TransformationCallable>()?;
    module.add_class::<IndexCallable>()?;
    Ok(())
}
