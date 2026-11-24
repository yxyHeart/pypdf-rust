//! Rust port of `pypdf/pagerange.py` (upstream pypdf 7.1.1).
//!
//! `PageRange` is a slice-like value object.  To stay exact with CPython
//! semantics (arbitrary precision ints, `slice.indices`, `__class__` in the
//! hash) the port stores a real Python `slice` and delegates primitive
//! operations to Python instead of reimplementing slice arithmetic.

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyList, PyString, PyTuple};

use crate::errors;
use crate::pyutil;

/// A decimal int, without allowing "-0" (upstream `_INT_RE`).
pub const INT_RE: &str = r"(0|-?[1-9]\d*)";

/// Upstream `PAGE_RANGE_RE`, built from `INT_RE` exactly like the original.
#[must_use]
pub fn page_range_pattern() -> String {
    let int_re = INT_RE;
    format!("^({int_re}|({int_re}?(:{int_re}?(:{int_re}?)?)))$")
}

#[pyclass(name = "PageRange", module = "pypdf.pagerange", subclass, dict)]
pub struct PageRange {
    slice: Py<PyAny>,
}

#[pymethods]
impl PageRange {
    // @pypdf: pypdf.pagerange.PageRange.__init__
    #[new]
    fn new(py: Python<'_>, arg: &Bound<'_, PyAny>) -> PyResult<Self> {
        let builtins = py.import("builtins")?;
        let slice_type = builtins.getattr("slice")?;
        let int_type = builtins.getattr("int")?;
        let zero = int_type.call1((0,))?;

        if arg.is_instance(&slice_type)? {
            let step = arg.getattr("step")?;
            if !step.is_none() && step.eq(&zero)? {
                return Err(errors::parse_error(py, arg));
            }
            return Ok(Self {
                slice: arg.clone().unbind(),
            });
        }

        if arg.is_instance_of::<PageRange>() {
            let other = arg.extract::<PyRef<'_, PageRange>>()?;
            return Ok(Self {
                slice: other.slice.clone_ref(py),
            });
        }

        let Ok(text) = arg.extract::<String>() else {
            return Err(errors::parse_error(py, arg));
        };
        let re_module = py.import("re")?;
        let matched = re_module
            .call_method1("match", (page_range_pattern(), PyString::new(py, &text)))?;
        if matched.is_none() {
            return Err(errors::parse_error(py, arg));
        }

        let group2 = matched.call_method1("group", (2,))?;
        let slice_obj = if !group2.is_none() && group2.is_truthy()? {
            // A plain int means a range of exactly one page.
            let start = int_type.call1((group2,))?;
            let stop = if start.eq(int_type.call1((-1,))?)? {
                py.None().into_bound(py)
            } else {
                start.call_method1("__add__", (int_type.call1((1,))?,))?
            };
            pyutil::py_slice(py, Some(&start), Some(&stop), None)?
        } else {
            let bounds = matched.call_method1("group", (4, 6, 8))?;
            let bounds = bounds.cast::<PyTuple>()?;
            let mut parsed: Vec<Option<Bound<'_, PyAny>>> = Vec::with_capacity(3);
            for item in bounds.iter() {
                parsed.push(if item.is_none() {
                    None
                } else {
                    Some(int_type.call1((item,))?)
                });
            }
            if let Some(step) = &parsed[2] {
                if step.eq(&zero)? {
                    return Err(errors::parse_error(py, arg));
                }
            }
            pyutil::py_slice(
                py,
                parsed[0].as_ref(),
                parsed[1].as_ref(),
                parsed[2].as_ref(),
            )?
        };

        Ok(Self {
            slice: slice_obj.unbind(),
        })
    }

    // @pypdf: pypdf.pagerange.PageRange.valid
    #[staticmethod]
    fn valid(py: Python<'_>, input: &Bound<'_, PyAny>) -> PyResult<bool> {
        let builtins = py.import("builtins")?;
        let slice_type = builtins.getattr("slice")?;
        if input.is_instance_of::<PageRange>() {
            return Ok(true);
        }
        if !(input.is_instance_of::<PyString>() || input.is_instance(&slice_type)?) {
            return Ok(false);
        }
        match Self::new(py, input) {
            Ok(_) => Ok(true),
            Err(error) if is_parse_error(py, &error)? => Ok(false),
            Err(error) => Err(error),
        }
    }

    #[getter(_slice)]
    fn slice_attribute(&self, py: Python<'_>) -> Py<PyAny> {
        self.slice.clone_ref(py)
    }

    #[setter(_slice)]
    fn set_slice_attribute(&mut self, value: Bound<'_, PyAny>) {
        self.slice = value.unbind();
    }

    // @pypdf: pypdf.pagerange.PageRange.to_slice
    fn to_slice(&self, py: Python<'_>) -> Py<PyAny> {
        self.slice.clone_ref(py)
    }

    // @pypdf: pypdf.pagerange.PageRange.indices
    fn indices(&self, py: Python<'_>, n: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        Ok(self.slice.bind(py).call_method1("indices", (n,))?.unbind())
    }

    fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        let slice = self.slice.bind(py);
        let start = slice.getattr("start")?;
        let stop = slice.getattr("stop")?;
        let step = slice.getattr("step")?;
        let int_type = py.import("builtins")?.getattr("int")?;

        let mut indices: Vec<Bound<'_, PyAny>> = Vec::with_capacity(3);
        if step.is_none() {
            if !start.is_none() {
                let incremented = start.call_method1("__add__", (int_type.call1((1,))?,))?;
                if stop.eq(&incremented)? && !start.eq(int_type.call1((-1,))?)? {
                    return pyutil::py_str(&start);
                }
            }
            indices.push(start);
            indices.push(stop);
        } else {
            indices.push(start);
            indices.push(stop);
            indices.push(step);
        }
        let rendered: Vec<String> = indices
            .iter()
            .map(|value| {
                if value.is_none() {
                    Ok(String::new())
                } else {
                    pyutil::py_str(value)
                }
            })
            .collect::<PyResult<_>>()?;
        Ok(rendered.join(":"))
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let text = self.__str__(py)?;
        let repr = pyutil::py_repr(PyString::new(py, &text).as_any())?;
        Ok(format!("PageRange({repr})"))
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        if !other.is_instance_of::<PageRange>() {
            return Ok(false);
        }
        let other = other.extract::<PyRef<'_, PageRange>>()?;
        self.slice.bind(py).eq(other.slice.bind(py))
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        // Upstream: hash((self.__class__, (start, stop, step)))
        let class = py.import("pypdf.pagerange")?.getattr("PageRange")?;
        let slice = self.slice.bind(py);
        let triple = PyTuple::new(
            py,
            [
                slice.getattr("start")?,
                slice.getattr("stop")?,
                slice.getattr("step")?,
            ],
        )?;
        PyTuple::new(py, [class, triple.into_any()])?.as_any().hash()
    }

    // @pypdf: pypdf.pagerange.PageRange.__add__
    fn __add__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<Py<PageRange>> {
        if !other.is_instance_of::<PageRange>() {
            let type_text = pyutil::py_str(other.get_type().as_any())?;
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Can't add PageRange and {type_text}"
            )));
        }
        let other = other.extract::<PyRef<'_, PageRange>>()?;
        let a = self.slice.bind(py);
        let b = other.slice.bind(py);
        let a_step = a.getattr("step")?;
        let b_step = b.getattr("step")?;
        if !a_step.is_none() || !b_step.is_none() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Can't add PageRange with stride",
            ));
        }

        let mut a_start = a.getattr("start")?;
        let mut a_stop = a.getattr("stop")?;
        let mut b_start = b.getattr("start")?;
        let mut b_stop = b.getattr("stop")?;

        let float_type = py.import("builtins")?.getattr("float")?;
        let neg_inf = float_type.call1(("-inf",))?;
        let pos_inf = float_type.call1(("inf",))?;

        fn start_key<'py>(
            value: &Bound<'py, PyAny>,
            neg_inf: &Bound<'py, PyAny>,
        ) -> PyResult<Bound<'py, PyAny>> {
            if value.is_none() {
                Ok(neg_inf.clone())
            } else {
                Ok(value.clone())
            }
        }
        fn stop_key<'py>(
            value: &Bound<'py, PyAny>,
            pos_inf: &Bound<'py, PyAny>,
        ) -> PyResult<Bound<'py, PyAny>> {
            if value.is_none() {
                Ok(pos_inf.clone())
            } else {
                Ok(value.clone())
            }
        }

        if start_key(&a_start, &neg_inf)?.gt(&start_key(&b_start, &neg_inf)?)? {
            std::mem::swap(&mut a_start, &mut b_start);
            std::mem::swap(&mut a_stop, &mut b_stop);
        }
        if start_key(&b_start, &neg_inf)?.gt(&stop_key(&a_stop, &pos_inf)?)? {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Can't add PageRanges with gap",
            ));
        }
        let stop = if stop_key(&b_stop, &pos_inf)?.gt(&stop_key(&a_stop, &pos_inf)?)? {
            b_stop
        } else {
            a_stop
        };
        let slice = pyutil::py_slice(py, Some(&a_start), Some(&stop), None)?;
        Ok(Py::new(py, Self::new(py, &slice)?)?)
    }

    fn __deepcopy__(&self, py: Python<'_>, _memo: &Bound<'_, PyAny>) -> PyResult<Py<PageRange>> {
        let slice = self.slice.bind(py);
        let copied = pyutil::py_slice(
            py,
            Some(&slice.getattr("start")?),
            Some(&slice.getattr("stop")?),
            Some(&slice.getattr("step")?),
        )?;
        Ok(Py::new(py, Self::new(py, &copied)?)?)
    }
}

fn is_parse_error(py: Python<'_>, error: &PyErr) -> PyResult<bool> {
    let parse_error = py.import("pypdf.errors")?.getattr("ParseError")?;
    error.get_type(py).is_subclass(&parse_error)
}

// @pypdf: pypdf.pagerange.parse_filename_page_ranges
#[pyfunction]
fn parse_filename_page_ranges(py: Python<'_>, args: &Bound<'_, PyAny>) -> PyResult<Py<PyList>> {
    let module = py.import("pypdf.pagerange")?;
    let page_range_class = module.getattr("PageRange")?;
    let page_range_all = module.getattr("PAGE_RANGE_ALL")?;

    let mut items: Vec<Bound<'_, PyAny>> = args.try_iter()?.collect::<PyResult<_>>()?;
    items.push(py.None().into_bound(py));

    let mut pdf_filename: Option<Bound<'_, PyAny>> = None;
    let mut did_page_range = false;
    let mut pairs: Vec<Bound<'_, PyAny>> = Vec::new();

    for arg in items {
        if PageRange::valid(py, &arg)? {
            let Some(filename) = pdf_filename.as_ref() else {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "The first argument must be a filename, not a page range.",
                ));
            };
            let range = page_range_class.call1((&arg,))?;
            pairs.push(PyTuple::new(py, [filename.clone(), range])?.into_any());
            did_page_range = true;
        } else {
            if let Some(filename) = pdf_filename.take() {
                if !did_page_range {
                    pairs.push(
                        PyTuple::new(py, [filename, page_range_all.clone()])?.into_any(),
                    );
                }
            }
            pdf_filename = Some(arg);
            did_page_range = false;
        }
    }

    Ok(PyList::new(py, pairs)?.unbind())
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PageRange>()?;
    module.add_function(wrap_pyfunction!(parse_filename_page_ranges, module)?)?;
    module.add("PAGE_RANGE_RE", page_range_pattern())?;
    Ok(())
}
