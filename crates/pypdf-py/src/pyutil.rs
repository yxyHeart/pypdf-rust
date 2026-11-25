//! Small helpers for working with pypdf's Python-native object model.
//!
//! The port deliberately manipulates Python objects (see PORTING_GUIDE §1), so
//! most helpers here are thin, exact wrappers around Python semantics rather
//! than Rust reinterpretations of them.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

/// Fetch a class object from a pypdf module, e.g. `("pypdf.generic", "NameObject")`.
pub fn pypdf_class<'py>(py: Python<'py>, module: &str, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import(module)?.getattr(name)
}

/// `str(obj)` through Python's `__str__`.
pub fn py_str(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    obj.str()?.extract()
}

/// `repr(obj)` through Python's `__repr__`.
pub fn py_repr(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    obj.repr()?.extract()
}

/// `a + b` using Python's operator protocol.
pub fn py_add<'py>(a: &Bound<'py, PyAny>, b: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    a.call_method1("__add__", (b,))
}

/// `a == b` using Python equality semantics.
pub fn py_eq(a: &Bound<'_, PyAny>, b: &Bound<'_, PyAny>) -> PyResult<bool> {
    a.eq(b)
}

/// `hash(obj)`.
pub fn py_hash(obj: &Bound<'_, PyAny>) -> PyResult<isize> {
    obj.hash()
}

/// `slice(start, stop, step)`.
pub fn py_slice<'py>(
    py: Python<'py>,
    start: Option<&Bound<'py, PyAny>>,
    stop: Option<&Bound<'py, PyAny>>,
    step: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let slice_type = py.import("builtins")?.getattr("slice")?;
    let mut args: Vec<Bound<'py, PyAny>> = Vec::with_capacity(3);
    for value in [start, stop, step] {
        match value {
            Some(value) => args.push(value.clone()),
            None => args.push(py.None().into_bound(py)),
        }
    }
    // ``slice(None, None, None)`` is valid and equals ``slice()``; always pass
    // all three positions so the argument count matches upstream calls.
    slice_type.call1(PyTuple::new(py, args)?)
}

/// `isinstance(obj, cls)`.
pub fn is_instance(obj: &Bound<'_, PyAny>, class: &Bound<'_, PyAny>) -> PyResult<bool> {
    obj.is_instance(class)
}

/// `len(obj)`.
pub fn py_len(obj: &Bound<'_, PyAny>) -> PyResult<usize> {
    obj.len()
}

/// Emulate ``logger_warning(message, source=..., **values)`` from pypdf._utils.
///
/// Upstream logs through `logging.getLogger(source).warning(message, values)`
/// when values are present and `.warning(message)` otherwise; reproducing both
/// shapes keeps `caplog` output identical.
pub fn logger_warning(
    py: Python<'_>,
    source: &str,
    message: &str,
    values: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let logger = py.import("logging")?.call_method1("getLogger", (source,))?;
    match values {
        Some(values) if !values.is_empty() => {
            logger.call_method1("warning", (message, values))?;
        }
        _ => {
            logger.call_method1("warning", (message,))?;
        }
    }
    Ok(())
}

/// `warnings.warn(message, category, stacklevel=...)`.
pub fn warn(py: Python<'_>, message: &str, category: &str, stacklevel: usize) -> PyResult<()> {
    let warnings = py.import("warnings")?;
    let category = py.import("builtins")?.getattr(category)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("stacklevel", stacklevel)?;
    warnings.call_method("warn", (message, category), Some(&kwargs))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Cached Python lookups
//
// `py.import()` and `module.attr` are C-level but cost ~100 ns each; the port's
// hot paths call them per token / per object.  Module objects and class
// references are stable for the lifetime of the interpreter (tests patch
// attributes on the class, not the module binding), so they can be cached.
// The cache is leaked on purpose: dropping `Py` objects after interpreter
// finalisation is not allowed.
// ---------------------------------------------------------------------------

type ModuleCache = std::sync::Mutex<std::collections::HashMap<&'static str, Py<PyAny>>>;
type AttrCache = std::sync::Mutex<std::collections::HashMap<(&'static str, &'static str), Py<PyAny>>>;

fn module_cache() -> &'static ModuleCache {
    static CACHE: std::sync::OnceLock<&'static ModuleCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Box::leak(Box::new(std::sync::Mutex::new(std::collections::HashMap::new()))))
}

fn attr_cache() -> &'static AttrCache {
    static CACHE: std::sync::OnceLock<&'static AttrCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Box::leak(Box::new(std::sync::Mutex::new(std::collections::HashMap::new()))))
}

/// `import name`, cached per process.
pub fn cached_module<'py>(
    py: Python<'py>,
    name: &'static str,
) -> PyResult<Bound<'py, pyo3::types::PyModule>> {
    // Lock-free fast path for the modules used by the hottest helper functions.
    {
        use pyo3::sync::PyOnceLock;
        static BUILTINS: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
        static TEXT_EXTRACTION: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
        static GENERIC_BASE: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
        static GENERIC_DATA: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
        static GENERIC: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
        static UTILS: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
        let cell = match name {
            "builtins" => Some(&BUILTINS),
            "pypdf._text_extraction" => Some(&TEXT_EXTRACTION),
            "pypdf.generic._base" => Some(&GENERIC_BASE),
            "pypdf.generic._data_structures" => Some(&GENERIC_DATA),
            "pypdf.generic" => Some(&GENERIC),
            "pypdf._utils" => Some(&UTILS),
            _ => None,
        };
        if let Some(cell) = cell {
            let module = cell.get_or_try_init(py, || {
                py.import(name).map(|module| module.into_any().unbind())
            })?;
            return Ok(module.bind(py).clone().cast_into::<pyo3::types::PyModule>()?);
        }
    }
    if let Some(module) = module_cache()
        .lock()
        .expect("module cache")
        .get(name)
    {
        return Ok(module.bind(py).clone().cast_into::<pyo3::types::PyModule>()?);
    }
    let module = py.import(name)?;
    module_cache()
        .lock()
        .expect("module cache")
        .insert(name, module.clone().into_any().unbind());
    Ok(module)
}

/// `module.attr`, cached per process.
pub fn cached_attr<'py>(
    py: Python<'py>,
    module: &'static str,
    name: &'static str,
) -> PyResult<Bound<'py, PyAny>> {
    if let Some(value) = attr_cache()
        .lock()
        .expect("attr cache")
        .get(&(module, name))
    {
        return Ok(value.bind(py).clone());
    }
    let value = cached_module(py, module)?.getattr(name)?;
    attr_cache()
        .lock()
        .expect("attr cache")
        .insert((module, name), value.clone().unbind());
    Ok(value)
}

/// `module.outer.inner`, cached per process.
pub fn cached_attr2<'py>(
    py: Python<'py>,
    module: &'static str,
    outer: &'static str,
    inner: &'static str,
) -> PyResult<Bound<'py, PyAny>> {
    type Cache = std::sync::Mutex<
        std::collections::HashMap<(&'static str, &'static str, &'static str), Py<PyAny>>,
    >;
    static CACHE: std::sync::OnceLock<&'static Cache> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| Box::leak(Box::new(std::sync::Mutex::new(std::collections::HashMap::new()))));
    if let Some(value) = cache.lock().expect("attr2 cache").get(&(module, outer, inner)) {
        return Ok(value.bind(py).clone());
    }
    let value = cached_module(py, module)?.getattr(outer)?.getattr(inner)?;
    cache
        .lock()
        .expect("attr2 cache")
        .insert((module, outer, inner), value.clone().unbind());
    Ok(value)
}
