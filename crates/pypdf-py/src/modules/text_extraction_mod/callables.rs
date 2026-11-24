//! Small Python-callable objects used by the text-extraction port.
//!
//! Upstream builds these as Python lambdas at a handful of call sites (the
//! `sorted`/`groupby` keys of the layout-mode functions, `json.dumps` default
//! hooks).  Python lambdas cannot be created from Rust, so these tiny
//! `#[pyclass]` callables stand in for them.  They are never part of the public
//! API: they only live in the `pypdf._pypdf` extension module.

use pyo3::prelude::*;
use pyo3::types::PyAny;

use super::common;

/// `lambda bt_grp: int(bt_grp["ty"] * bt_grp["flip_sort"])` — `groupby` key.
#[pyclass(name = "_TextExtractionGroupKey", module = "pypdf._pypdf")]
pub struct GroupKey;

#[pymethods]
impl GroupKey {
    fn __call__(&self, py: Python<'_>, bt_grp: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let ty = bt_grp.get_item("ty")?;
        let flip_sort = bt_grp.get_item("flip_sort")?;
        Ok(common::int_of(py, &ty.mul(flip_sort)?)?.unbind())
    }
}

/// `lambda x: x["tx"]` — `sorted` key of the layout-mode functions.
#[pyclass(name = "_TextExtractionSortByTx", module = "pypdf._pypdf")]
pub struct SortByTx;

#[pymethods]
impl SortByTx {
    fn __call__<'py>(&self, bt_grp: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        bt_grp.get_item("tx")
    }
}

/// `lambda x: (x["ty"] * x["flip_sort"], -x["tx"])` — `sorted` key of
/// `text_show_operations`.
#[pyclass(name = "_TextExtractionSortKey", module = "pypdf._pypdf")]
pub struct SortKey;

#[pymethods]
impl SortKey {
    fn __call__(&self, py: Python<'_>, bt_grp: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let ty = bt_grp.get_item("ty")?;
        let flip_sort = bt_grp.get_item("flip_sort")?;
        let tx = bt_grp.get_item("tx")?;
        let key = common::py_tuple(py, vec![ty.mul(flip_sort)?, tx.neg()?])?;
        Ok(key.unbind())
    }
}

/// `lambda x: getattr(x, "to_dict", str)(x)` — `json.dumps(default=...)` hook for
/// the `tjs.json` debug output.
#[pyclass(name = "_TextExtractionTjDefault", module = "pypdf._pypdf")]
pub struct TjDefault;

#[pymethods]
impl TjDefault {
    fn __call__(&self, py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        match obj.getattr_opt("to_dict")? {
            Some(to_dict) => Ok(to_dict.call1((obj,))?.unbind()),
            None => Ok(common::builtins(py)?
                .getattr("str")?
                .call1((obj,))?
                .unbind()),
        }
    }
}
