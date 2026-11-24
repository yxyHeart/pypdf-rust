//! `pypdf._page.Transformation` (upstream pypdf 7.1.1).
//!
//! The class itself lives in the Python shim (`python/pypdf/_page.py`); every
//! method body delegates here.  Matrix arithmetic goes through the *Python*
//! `matrix_multiply` / operator protocols so that `int`/`float`/`FloatObject`
//! semantics and the resulting representations are identical to upstream.

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyTuple};

use pypdf_rs_core::pyfmt::py_float_fixed;

use super::common::{self, globals, op_add, op_mul, op_neg, py_float_of, py_int, py_str};

fn ctm_of<'py>(slf: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    slf.getattr("ctm")
}

fn transformation_class<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    globals(py, "Transformation")
}

fn matrix_multiply<'py>(
    py: Python<'py>,
    left: &Bound<'py, PyAny>,
    right: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    globals(py, "matrix_multiply")?.call1((left, right))
}

/// `Transformation.compress(matrix)`.
fn compress_matrix<'py>(
    py: Python<'py>,
    matrix: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    transformation_class(py)?
        .getattr("compress")?
        .call1((matrix,))
}

fn new_transformation<'py>(py: Python<'py>, ctm: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    transformation_class(py)?.call1((ctm,))
}

fn matrix_from<'py>(slf: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    slf.getattr("matrix")
}

/// @pypdf: pypdf._page.Transformation.matrix
#[pyfunction]
pub fn page_transformation_matrix<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let ctm = ctm_of(slf)?;
    let zero = || py_int(py, 0);
    let one = || py_int(py, 1);
    let row0 = PyTuple::new(py, [ctm.get_item(0)?, ctm.get_item(1)?, zero()])?;
    let row1 = PyTuple::new(py, [ctm.get_item(2)?, ctm.get_item(3)?, zero()])?;
    let row2 = PyTuple::new(py, [ctm.get_item(4)?, ctm.get_item(5)?, one()])?;
    let matrix = PyTuple::new(py, [row0.into_any(), row1.into_any(), row2.into_any()])?;
    Ok(matrix.into_any().unbind())
}

/// @pypdf: pypdf._page.Transformation.compress
#[pyfunction]
pub fn page_transformation_compress<'py>(
    py: Python<'py>,
    matrix: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let row0 = matrix.get_item(0)?;
    let row1 = matrix.get_item(1)?;
    let row2 = matrix.get_item(2)?;
    let values = vec![
        row0.get_item(0)?,
        row0.get_item(1)?,
        row1.get_item(0)?,
        row1.get_item(1)?,
        row2.get_item(0)?,
        row2.get_item(1)?,
    ];
    Ok(PyTuple::new(py, values)?.into_any().unbind())
}

/// @pypdf: pypdf._page.Transformation._to_cm
#[pyfunction]
pub fn page_transformation_to_cm<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<String> {
    let ctm = ctm_of(slf)?;
    let mut parts: Vec<String> = Vec::with_capacity(6);
    for index in 0..6 {
        // Upstream: f"{self.ctm[index]:.4f}"
        let value = py_float_of(py, &ctm.get_item(index)?)?;
        parts.push(py_float_fixed(value, 4));
    }
    Ok(format!(
        "{} {} {} {} {} {} cm",
        parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]
    ))
}

/// @pypdf: pypdf._page.Transformation.transform
#[pyfunction]
pub fn page_transformation_transform<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    m: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let left = matrix_from(slf)?;
    let right = matrix_from(m)?;
    let multiplied = matrix_multiply(py, &left, &right)?;
    let ctm = compress_matrix(py, &multiplied)?;
    Ok(new_transformation(py, ctm)?.unbind())
}

/// @pypdf: pypdf._page.Transformation.translate
#[pyfunction]
#[pyo3(signature = (slf, tx=None, ty=None))]
pub fn page_transformation_translate<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    tx: Option<Bound<'py, PyAny>>,
    ty: Option<Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let tx = tx.unwrap_or_else(|| py_int(py, 0));
    let ty = ty.unwrap_or_else(|| py_int(py, 0));
    let m = ctm_of(slf)?;
    let values = vec![
        m.get_item(0)?,
        m.get_item(1)?,
        m.get_item(2)?,
        m.get_item(3)?,
        op_add(py, &m.get_item(4)?, &tx)?,
        op_add(py, &m.get_item(5)?, &ty)?,
    ];
    let ctm = PyTuple::new(py, values)?.into_any();
    let kwargs = PyDict::new(py);
    kwargs.set_item("ctm", ctm)?;
    Ok(transformation_class(py)?.call((), Some(&kwargs))?.unbind())
}

/// @pypdf: pypdf._page.Transformation.scale
#[pyfunction]
#[pyo3(signature = (slf, sx=None, sy=None))]
pub fn page_transformation_scale<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    sx: Option<Bound<'py, PyAny>>,
    sy: Option<Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let mut sx = sx;
    let mut sy = sy;
    if sx.is_none() && sy.is_none() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "Either sx or sy must be specified",
        ));
    }
    if sx.is_none() {
        sx = sy.clone();
    }
    if sy.is_none() {
        sy = sx.clone();
    }
    // Upstream: assert sx is not None; assert sy is not None
    let Some(sx) = sx else {
        return Err(common::assertion_error(py, "mypy"));
    };
    let Some(sy) = sy else {
        return Err(common::assertion_error(py, "mypy"));
    };
    let zero = || py_int(py, 0);
    let one = || py_int(py, 1);
    let op = PyTuple::new(
        py,
        [
            PyTuple::new(py, [sx.clone(), zero(), zero()])?.into_any(),
            PyTuple::new(py, [zero(), sy.clone(), zero()])?.into_any(),
            PyTuple::new(py, [zero(), zero(), one()])?.into_any(),
        ],
    )?
    .into_any();
    let left = matrix_from(slf)?;
    let multiplied = matrix_multiply(py, &left, &op)?;
    let ctm = compress_matrix(py, &multiplied)?;
    Ok(new_transformation(py, ctm)?.unbind())
}

/// @pypdf: pypdf._page.Transformation.rotate
#[pyfunction]
pub fn page_transformation_rotate<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    rotation: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let math = py.import("math")?;
    let radians = math.getattr("radians")?.call1((rotation,))?;
    let cos = math.getattr("cos")?.call1((&radians,))?;
    let sin = math.getattr("sin")?.call1((&radians,))?;
    let zero = || py_int(py, 0);
    let one = || py_int(py, 1);
    let op = PyTuple::new(
        py,
        [
            PyTuple::new(py, [cos.clone(), sin.clone(), zero()])?.into_any(),
            PyTuple::new(py, [op_neg(py, &sin)?, cos.clone(), zero()])?.into_any(),
            PyTuple::new(py, [zero(), zero(), one()])?.into_any(),
        ],
    )?
    .into_any();
    let left = matrix_from(slf)?;
    let multiplied = matrix_multiply(py, &left, &op)?;
    let ctm = compress_matrix(py, &multiplied)?;
    Ok(new_transformation(py, ctm)?.unbind())
}

/// @pypdf: pypdf._page.Transformation.__repr__
#[pyfunction]
pub fn page_transformation_repr<'py>(slf: &Bound<'py, PyAny>) -> PyResult<String> {
    let ctm = ctm_of(slf)?;
    Ok(format!("Transformation(ctm={})", py_str(&ctm)?))
}

/// @pypdf: pypdf._page.Transformation.apply_on
#[pyfunction]
#[pyo3(signature = (slf, pt, as_object=false))]
pub fn page_transformation_apply_on<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    pt: &Bound<'py, PyAny>,
    as_object: bool,
) -> PyResult<Py<PyAny>> {
    let ctm = ctm_of(slf)?;
    let builtins = common::builtins(py)?;
    let float_type = builtins.getattr("float")?;
    let typ: Bound<'py, PyAny> = if as_object {
        common::generic_class(py, "FloatObject")?
    } else {
        float_type.clone()
    };
    let pt0 = float_type.call1((pt.get_item(0)?,))?;
    let pt1 = float_type.call1((pt.get_item(1)?,))?;
    let x = op_add(
        py,
        &op_add(
            py,
            &op_mul(py, &pt0, &ctm.get_item(0)?)?,
            &op_mul(py, &pt1, &ctm.get_item(2)?)?,
        )?,
        &ctm.get_item(4)?,
    )?;
    let y = op_add(
        py,
        &op_add(
            py,
            &op_mul(py, &pt0, &ctm.get_item(1)?)?,
            &op_mul(py, &pt1, &ctm.get_item(3)?)?,
        )?,
        &ctm.get_item(5)?,
    )?;
    let pair = PyTuple::new(py, [typ.call1((x,))?, typ.call1((y,))?])?.into_any();
    let is_list = pt.is_instance(&builtins.getattr("list")?)?;
    let result = if is_list {
        builtins.getattr("list")?.call1((&pair,))?
    } else {
        pair
    };
    Ok(result.unbind())
}

pub fn register(module: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    module.add_function(pyo3::wrap_pyfunction!(page_transformation_matrix, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(
        page_transformation_compress,
        module
    )?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_transformation_to_cm, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(
        page_transformation_transform,
        module
    )?)?;
    module.add_function(pyo3::wrap_pyfunction!(
        page_transformation_translate,
        module
    )?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_transformation_scale, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_transformation_rotate, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(page_transformation_repr, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(
        page_transformation_apply_on,
        module
    )?)?;
    Ok(())
}
