//! Rust port of the six small `pypdf/generic` modules (upstream pypdf 7.1.1):
//!
//! * `pypdf/generic/_color.py`      (`Color`, `DeviceGray`, `DeviceRGB`, `DeviceCMYK`)
//! * `pypdf/generic/_fit.py`        (`Fit`, `DEFAULT_FIT`)
//! * `pypdf/generic/_outline.py`    (`OutlineItem`)
//! * `pypdf/generic/_link.py`       (`NamedReferenceLink`, `DirectReferenceLink`, `extract_links`)
//! * `pypdf/generic/_viewerpref.py` (`ViewerPreferences`)
//! * `pypdf/generic/_rectangle.py`  (`RectangleObject`)
//!
//! # Design (see `docs/PORTING_GUIDE.md` §1)
//!
//! Every class stays a *real Python class* in the corresponding hand-written
//! shim (`python/pypdf/generic/_{color,fit,outline,link,viewerpref,rectangle}.py`)
//! with upstream's exact base classes, decorators, class attributes,
//! properties and dataclass semantics.  Each method body there is a one-line
//! delegation to a `#[pyfunction]` implemented here, so MRO, `isinstance`,
//! `dataclasses` equality/`repr`, `property` descriptors and subclassing are
//! byte-for-byte what upstream exposes while all behaviour lives in Rust.
//!
//! # Fidelity notes
//!
//! * Python-level names that upstream resolves through module globals (e.g.
//!   `_fit.is_null_or_none`, `_link._build_link`, `_outline.NameObject`) are
//!   looked up in the shim module at call time, exactly like upstream's global
//!   lookup, so monkeypatching those names keeps working.
//! * `ViewerPreferences.__new__` stays in the shim: it is pure descriptor
//!   wiring (`property(...)` definitions on the class, re-created on every
//!   instantiation just like upstream); the property getters/setters it wires
//!   up delegate to the Rust functions below.
//! * No `HashMap`/`HashSet` iteration can affect output.  Every mapping that
//!   reaches the output stays a Python `dict`/`set`, matching upstream order.

use pyo3::exceptions::{PyAttributeError, PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyBytes, PyDict, PyList, PyModule, PyString, PyTuple};

use crate::pyutil;

/// Upstream module name; used as `logger_warning(..., source=__name__)`.
const LINK_SOURCE: &str = "pypdf.generic._link";

const COLOR_MODULE: &str = "pypdf.generic._color";
const FIT_MODULE: &str = "pypdf.generic._fit";
const OUTLINE_MODULE: &str = "pypdf.generic._outline";
const LINK_MODULE: &str = "pypdf.generic._link";
const VIEWERPREF_MODULE: &str = "pypdf.generic._viewerpref";
const RECTANGLE_MODULE: &str = "pypdf.generic._rectangle";
const BASE_MODULE: &str = "pypdf.generic._base";
const DATA_STRUCTURES_MODULE: &str = "pypdf.generic._data_structures";

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("builtins")
}

fn module<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyModule>> {
    py.import(name)
}

/// Fetch a name from a shim module (upstream resolves these as module globals).
fn module_attr<'py>(py: Python<'py>, module_name: &str, name: &str) -> PyResult<Bound<'py, PyAny>> {
    module(py, module_name)?.getattr(name)
}

/// Fetch a class from `pypdf.generic._base` (upstream imports live in `_base`).
fn base_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    module_attr(py, BASE_MODULE, name)
}

/// Fetch a class from `pypdf.generic._data_structures`.
fn data_structures_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    module_attr(py, DATA_STRUCTURES_MODULE, name)
}

fn py_bytes<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, data).into_any()
}

fn py_int<'py>(py: Python<'py>, value: i64) -> Bound<'py, PyAny> {
    pyo3::types::PyInt::new(py, value).into_any()
}

fn py_str<'py>(py: Python<'py>, value: &str) -> Bound<'py, PyAny> {
    PyString::new(py, value).into_any()
}

fn py_string(py: Python<'_>, value: &str) -> Py<PyAny> {
    PyString::new(py, value).into_any().unbind()
}

/// `None` as a bound object.
fn none<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    py.None().into_bound(py)
}

/// `obj.get(key, default)` through Python's dynamic dispatch.
fn dict_get<'py>(
    obj: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    default: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    match default {
        Some(default) => obj.call_method1("get", (key, default)),
        None => obj.call_method1("get", (key,)),
    }
}

// ---------------------------------------------------------------------------
// pypdf/generic/_color.py
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._color.Color.from_normalized_values
#[pyfunction]
#[pyo3(signature = (color, operator=None))]
fn genbasic_color_from_normalized_values<'py>(
    py: Python<'py>,
    color: &Bound<'py, PyAny>,
    operator: Option<&Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    // `if color is not None and (color_length := len(color)) in color_types ...`
    if color.is_none() {
        return Ok(py.None());
    }
    let color_length = color.len()?;
    let subclass_name = match color_length {
        1 => "DeviceGray",
        3 => "DeviceRGB",
        4 => "DeviceCMYK",
        _ => return Ok(py.None()),
    };
    let subclass = module_attr(py, COLOR_MODULE, subclass_name)?;
    // `(operator is None or color_types[color_length].color_operator == operator)`
    if let Some(operator) = operator {
        if !operator.is_none() && !subclass.getattr("color_operator")?.eq(operator)? {
            return Ok(py.None());
        }
    }
    // `all(isinstance(val, (int, float)) and 0.0 <= val <= 1.0 for val in color)`
    let builtins = builtins(py)?;
    let int_type = builtins.getattr("int")?;
    let float_type = builtins.getattr("float")?;
    for value in color.try_iter()? {
        let value = value?;
        if !(value.is_instance(&int_type)? || value.is_instance(&float_type)?) {
            return Ok(py.None());
        }
        // `0.0 <= val <= 1.0` (numeric types only at this point, so the
        // reversed comparison is equivalent and needs no extra allocation).
        if !(value.ge(0.0f64)? && value.le(1.0f64)?) {
            return Ok(py.None());
        }
    }
    // `kwargs = dict(zip(color_subclass._ordered_fields, color))`
    let kwargs = PyDict::new(py);
    let ordered_fields = subclass.getattr("_ordered_fields")?;
    for (field, value) in ordered_fields.try_iter()?.zip(color.try_iter()?) {
        kwargs.set_item(field?, value?)?;
    }
    Ok(subclass.call((), Some(&kwargs))?.unbind())
}

// @pypdf: pypdf.generic._color.Color.as_operator
#[pyfunction]
#[pyo3(signature = (slf, stroke=false))]
fn genbasic_color_as_operator<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stroke: bool,
) -> PyResult<Py<PyAny>> {
    let builtins = builtins(py)?;
    let round = builtins.getattr("round")?;
    let py_format = builtins.getattr("format")?;
    let mut values: Vec<String> = Vec::new();
    for field in slf.getattr("_ordered_fields")?.try_iter()? {
        // `_ordered_fields` holds plain `str` names, so `getattr(self, field)`
        // is equivalent to a string attribute lookup.
        let field: String = field?.extract()?;
        let value = slf.getattr(field.as_str())?;
        let rounded = round.call1((value, 3))?;
        values.push(pyutil::py_str(&py_format.call1((rounded, "g"))?)?);
    }
    let operator = slf.getattr("color_operator")?;
    let operator = if stroke {
        operator.call_method0("upper")?
    } else {
        operator
    };
    let text = format!("{} {}", values.join(" "), pyutil::py_str(&operator)?);
    Ok(py_string(py, &text))
}

// ---------------------------------------------------------------------------
// pypdf/generic/_fit.py
// ---------------------------------------------------------------------------

/// `Fit(fit_type=..., fit_args=...)` (upstream hard-codes the `Fit` class).
fn new_fit<'py>(
    py: Python<'py>,
    fit_type: &str,
    fit_args: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let fit_class = module_attr(py, FIT_MODULE, "Fit")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("fit_type", fit_type)?;
    kwargs.set_item("fit_args", fit_args)?;
    Ok(fit_class.call((), Some(&kwargs))?.unbind())
}

// @pypdf: pypdf.generic._fit.Fit.__init__
#[pyfunction]
fn genbasic_fit_new<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    fit_type: &Bound<'py, PyAny>,
    fit_args: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let name_class = base_class(py, "NameObject")?;
    slf.setattr("fit_type", name_class.call1((fit_type,))?)?;

    // Upstream imports `is_null_or_none` at module level, so resolve it through
    // the shim module globals.
    let is_null_or_none = module_attr(py, FIT_MODULE, "is_null_or_none")?;
    let null_class = base_class(py, "NullObject")?;
    let float_class = base_class(py, "FloatObject")?;
    let args = PyList::empty(py);
    for arg in fit_args.try_iter()? {
        let arg = arg?;
        let value = if is_null_or_none.call1((&arg,))?.is_truthy()? {
            null_class.call0()?
        } else {
            float_class.call1((&arg,))?
        };
        args.append(value)?;
    }
    slf.setattr("fit_args", args)?;
    Ok(())
}

// @pypdf: pypdf.generic._fit.Fit.xyz
#[pyfunction]
fn genbasic_fit_xyz<'py>(
    py: Python<'py>,
    left: &Bound<'py, PyAny>,
    top: &Bound<'py, PyAny>,
    zoom: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let fit_args = PyTuple::new(py, [left, top, zoom])?;
    new_fit(py, "/XYZ", fit_args.into_any())
}

// @pypdf: pypdf.generic._fit.Fit.fit
#[pyfunction]
fn genbasic_fit_fit<'py>(py: Python<'py>) -> PyResult<Py<PyAny>> {
    new_fit(py, "/Fit", PyTuple::empty(py).into_any())
}

// @pypdf: pypdf.generic._fit.Fit.fit_horizontally
#[pyfunction]
fn genbasic_fit_fit_horizontally<'py>(
    py: Python<'py>,
    top: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let fit_args = PyTuple::new(py, [top])?;
    new_fit(py, "/FitH", fit_args.into_any())
}

// @pypdf: pypdf.generic._fit.Fit.fit_vertically
#[pyfunction]
fn genbasic_fit_fit_vertically<'py>(
    py: Python<'py>,
    left: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let fit_args = PyTuple::new(py, [left])?;
    new_fit(py, "/FitV", fit_args.into_any())
}

// @pypdf: pypdf.generic._fit.Fit.fit_rectangle
#[pyfunction]
fn genbasic_fit_fit_rectangle<'py>(
    py: Python<'py>,
    left: &Bound<'py, PyAny>,
    bottom: &Bound<'py, PyAny>,
    right: &Bound<'py, PyAny>,
    top: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let fit_args = PyTuple::new(py, [left, bottom, right, top])?;
    new_fit(py, "/FitR", fit_args.into_any())
}

// @pypdf: pypdf.generic._fit.Fit.fit_box
#[pyfunction]
fn genbasic_fit_fit_box<'py>(py: Python<'py>) -> PyResult<Py<PyAny>> {
    new_fit(py, "/FitB", PyTuple::empty(py).into_any())
}

// @pypdf: pypdf.generic._fit.Fit.fit_box_horizontally
#[pyfunction]
fn genbasic_fit_fit_box_horizontally<'py>(
    py: Python<'py>,
    top: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let fit_args = PyTuple::new(py, [top])?;
    new_fit(py, "/FitBH", fit_args.into_any())
}

// @pypdf: pypdf.generic._fit.Fit.fit_box_vertically
#[pyfunction]
fn genbasic_fit_fit_box_vertically<'py>(
    py: Python<'py>,
    left: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let fit_args = PyTuple::new(py, [left])?;
    new_fit(py, "/FitBV", fit_args.into_any())
}

// @pypdf: pypdf.generic._fit.Fit.__str__
#[pyfunction]
fn genbasic_fit_str<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let fit_type = pyutil::py_str(&slf.getattr("fit_type")?)?;
    let fit_args = slf.getattr("fit_args")?;
    let text = if fit_args.is_truthy()? {
        format!("Fit({}, {})", fit_type, pyutil::py_str(&fit_args)?)
    } else {
        format!("Fit({})", fit_type)
    };
    Ok(py_string(py, &text))
}

// ---------------------------------------------------------------------------
// pypdf/generic/_outline.py
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._outline.OutlineItem.write_to_stream
#[pyfunction]
fn genbasic_outlineitem_write_to_stream<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    stream: &Bound<'py, PyAny>,
) -> PyResult<()> {
    // Upstream resolves `NameObject` from its module globals.
    let name_class = module_attr(py, OUTLINE_MODULE, "NameObject")?;
    stream.call_method1("write", (py_bytes(py, b"<<\n"),))?;
    for name in ["/Title", "/Parent", "/First", "/Last", "/Next", "/Prev"] {
        // The comprehension filter (`x in self`) is evaluated before
        // `NameObject(x)` is constructed.
        if !slf.contains(name)? {
            continue;
        }
        let key = name_class.call1((name,))?;
        key.call_method1("write_to_stream", (stream,))?;
        stream.call_method1("write", (py_bytes(py, b" "),))?;
        let value = slf.call_method1("raw_get", (&key,))?;
        value.call_method1("write_to_stream", (stream,))?;
        stream.call_method1("write", (py_bytes(py, b"\n"),))?;
    }
    let key = name_class.call1(("/Dest",))?;
    key.call_method1("write_to_stream", (stream,))?;
    stream.call_method1("write", (py_bytes(py, b" "),))?;
    // `dest_array` is a property, so read it instead of calling it.
    let value = slf.getattr("dest_array")?;
    value.call_method1("write_to_stream", (stream,))?;
    stream.call_method1("write", (py_bytes(py, b"\n"),))?;
    stream.call_method1("write", (py_bytes(py, b">>"),))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// pypdf/generic/_link.py
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._link.NamedReferenceLink.__init__
#[pyfunction]
fn genbasic_named_reference_link_new<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    reference: &Bound<'py, PyAny>,
    source_pdf: &Bound<'py, PyAny>,
) -> PyResult<()> {
    slf.setattr("_reference", reference)?;
    slf.setattr("_source_pdf", source_pdf)?;
    Ok(())
}

// @pypdf: pypdf.generic._link.NamedReferenceLink.find_referenced_page
#[pyfunction]
fn genbasic_named_reference_link_find_referenced_page<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let source_pdf = slf.getattr("_source_pdf")?;
    let named_destinations = source_pdf.getattr("named_destinations")?;
    let key = slf.getattr("_reference")?.str()?;
    let destination = named_destinations.call_method1("get", (key,))?;
    if destination.is_truthy()? {
        Ok(destination.getattr("page")?.unbind())
    } else {
        Ok(py.None())
    }
}

// @pypdf: pypdf.generic._link.NamedReferenceLink.patch_reference
#[pyfunction]
#[pyo3(signature = (slf, target_pdf, new_page, known_names=None))]
fn genbasic_named_reference_link_patch_reference<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    target_pdf: &Bound<'py, PyAny>,
    new_page: &Bound<'py, PyAny>,
    known_names: Option<&Bound<'py, PyAny>>,
) -> PyResult<()> {
    let name = slf.getattr("_reference")?.str()?.into_any();
    let known_names = match known_names {
        Some(known_names) if !known_names.is_none() => known_names.clone(),
        _ => builtins(py)?
            .getattr("set")?
            .call1((target_pdf.getattr("named_destinations")?,))?,
    };
    // point named destination in new PDF to the new page
    if !known_names.contains(&name)? {
        let page_number = new_page.getattr("page_number")?;
        target_pdf.call_method1("add_named_destination", (&name, page_number))?;
        known_names.call_method1("add", (&name,))?;
    }
    Ok(())
}

// @pypdf: pypdf.generic._link.DirectReferenceLink.__init__
#[pyfunction]
fn genbasic_direct_reference_link_new<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    reference: &Bound<'py, PyAny>,
) -> PyResult<()> {
    slf.setattr("_reference", reference)?;
    Ok(())
}

// @pypdf: pypdf.generic._link.DirectReferenceLink.find_referenced_page
#[pyfunction]
fn genbasic_direct_reference_link_find_referenced_page<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let reference = slf.getattr("_reference")?;
    if !reference.is_truthy()? {
        return Ok(py.None());
    }
    Ok(reference.get_item(0)?.unbind())
}

// @pypdf: pypdf.generic._link.DirectReferenceLink.patch_reference
#[pyfunction]
fn genbasic_direct_reference_link_patch_reference<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    _target_pdf: &Bound<'py, PyAny>,
    new_page: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let reference = slf.getattr("_reference")?;
    reference.set_item(0, new_page)?;
    Ok(())
}

// @pypdf: pypdf.generic._link.extract_links
#[pyfunction]
fn genbasic_extract_links<'py>(
    py: Python<'py>,
    new_page: &Bound<'py, PyAny>,
    old_page: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let array_class = data_structures_class(py, "ArrayObject")?;
    let mut new_annotations = dict_get(
        new_page,
        &py_str(py, "/Annots"),
        Some(&array_class.call0()?),
    )?
    .call_method0("get_object")?;
    let mut old_annotations = dict_get(
        old_page,
        &py_str(py, "/Annots"),
        Some(&array_class.call0()?),
    )?
    .call_method0("get_object")?;
    let is_null_or_none = module_attr(py, LINK_MODULE, "is_null_or_none")?;
    if is_null_or_none.call1((&new_annotations,))?.is_truthy()? {
        new_annotations = array_class.call0()?;
    }
    if is_null_or_none.call1((&old_annotations,))?.is_truthy()? {
        old_annotations = array_class.call0()?;
    }
    if !(new_annotations.is_instance(&array_class)? && old_annotations.is_instance(&array_class)?) {
        let values = PyDict::new(py);
        values.set_item("old_annotations", &old_annotations)?;
        values.set_item("new_annotations", &new_annotations)?;
        pyutil::logger_warning(
            py,
            LINK_SOURCE,
            "Expected annotation arrays: %(old_annotations)s %(new_annotations)s. Ignoring annotations.",
            Some(&values),
        )?;
        return Ok(PyList::empty(py).into_any().unbind());
    }

    // `_build_link` is a module global upstream; resolve it dynamically.
    let build_link = module_attr(py, LINK_MODULE, "_build_link")?;
    let new_links = PyList::empty(py);
    for annotation in new_annotations.try_iter()? {
        let annotation = annotation?;
        let link = build_link.call1((&annotation, new_page))?;
        if !link.is_none() {
            new_links.append(link)?;
        }
    }
    let old_links = PyList::empty(py);
    for annotation in old_annotations.try_iter()? {
        let annotation = annotation?;
        let link = build_link.call1((&annotation, old_page))?;
        if !link.is_none() {
            old_links.append(link)?;
        }
    }

    if new_links.len() != old_links.len() {
        let values = PyDict::new(py);
        values.set_item("old_count", py_int(py, old_links.len() as i64))?;
        values.set_item("new_count", py_int(py, new_links.len() as i64))?;
        pyutil::logger_warning(
            py,
            LINK_SOURCE,
            "Annotation sizes differ: %(old_count)d vs. %(new_count)d",
            Some(&values),
        )?;
    }

    let result = PyList::empty(py);
    for (new_link, old_link) in new_links.iter().zip(old_links.iter()) {
        result.append(PyTuple::new(py, [new_link, old_link])?)?;
    }
    Ok(result.into_any().unbind())
}

// @pypdf: pypdf.generic._link._build_link
#[pyfunction]
fn genbasic_build_link<'py>(
    py: Python<'py>,
    indirect_object: &Bound<'py, PyAny>,
    page: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let source_pdf = page.getattr("pdf")?;
    let link = indirect_object.call_method0("get_object")?;
    let dictionary_class = data_structures_class(py, "DictionaryObject")?;
    if !link.is_instance(&dictionary_class)? {
        return Ok(py.None());
    }
    if link
        .call_method1("get", (py_str(py, "/Subtype"),))?
        .ne(py_str(py, "/Link"))?
    {
        return Ok(py.None());
    }

    // Resolve the module-level helpers dynamically, like upstream's globals.
    let create_link = module_attr(py, LINK_MODULE, "_create_link")?;
    if link.contains("/A")? {
        let action = link.get_item("/A")?;
        if action
            .call_method1("get", (py_str(py, "/S"),))?
            .ne(py_str(py, "/GoTo"))?
        {
            return Ok(py.None());
        }
        if !action.contains("/D")? {
            return Ok(py.None());
        }
        let reference = action.get_item("/D")?;
        return Ok(create_link.call1((reference, source_pdf))?.unbind());
    }

    if link.contains("/Dest")? {
        let reference = link.get_item("/Dest")?;
        return Ok(create_link.call1((reference, source_pdf))?.unbind());
    }

    Ok(py.None())
}

// @pypdf: pypdf.generic._link._create_link
#[pyfunction]
fn genbasic_create_link<'py>(
    py: Python<'py>,
    reference: &Bound<'py, PyAny>,
    source_pdf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let text_string_class = base_class(py, "TextStringObject")?;
    if reference.is_instance(&text_string_class)? {
        let link_class = module_attr(py, LINK_MODULE, "NamedReferenceLink")?;
        return Ok(link_class.call1((reference, source_pdf))?.unbind());
    }
    let array_class = data_structures_class(py, "ArrayObject")?;
    if reference.is_instance(&array_class)? {
        let link_class = module_attr(py, LINK_MODULE, "DirectReferenceLink")?;
        return Ok(link_class.call1((reference,))?.unbind());
    }
    Ok(py.None())
}

// ---------------------------------------------------------------------------
// pypdf/generic/_viewerpref.py
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._viewerpref.ViewerPreferences.__init__
#[pyfunction]
fn genbasic_viewerpreferences_new<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    obj: &Bound<'py, PyAny>,
) -> PyResult<()> {
    // `super().__init__(self)`: the next class after `ViewerPreferences` in the
    // MRO is `DictionaryObject`, whose `__init__` is inherited from `dict`.
    // Upstream passes `self` as the only argument, i.e. `init(self, self)`.
    data_structures_class(py, "DictionaryObject")?
        .getattr("__init__")?
        .call1((slf, slf))?;
    let is_null_or_none = module_attr(py, VIEWERPREF_MODULE, "is_null_or_none")?;
    if !is_null_or_none.call1((obj,))?.is_truthy()? {
        slf.call_method1("update", (obj.call_method0("items")?,))?;
    }
    match obj.getattr("indirect_reference") {
        Ok(indirect_reference) => slf.setattr("indirect_reference", indirect_reference)?,
        Err(error) if error.is_instance_of::<PyAttributeError>(py) => {}
        Err(error) => return Err(error),
    }
    Ok(())
}

/// `self[NameObject(key)]` (a `DictionaryObject.__setitem__` call).
fn viewerpref_set_item<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    value: Bound<'py, PyAny>,
) -> PyResult<()> {
    let name_class = base_class(py, "NameObject")?;
    slf.set_item(name_class.call1((key,))?, value)
}

// @pypdf: pypdf.generic._viewerpref.ViewerPreferences._get_bool
#[pyfunction]
fn genbasic_viewerpreferences_get_bool<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    default: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(dict_get(slf, key, Some(default))?.unbind())
}

// @pypdf: pypdf.generic._viewerpref.ViewerPreferences._set_bool
#[pyfunction]
fn genbasic_viewerpreferences_set_bool<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let boolean_class = base_class(py, "BooleanObject")?;
    // Upstream: `BooleanObject(v is True)` -- an identity test, not truthiness.
    let flag = boolean_class.call1((value.is(PyBool::new(py, true)),))?;
    viewerpref_set_item(py, slf, key, flag)
}

// @pypdf: pypdf.generic._viewerpref.ViewerPreferences._get_name
#[pyfunction]
fn genbasic_viewerpreferences_get_name<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    default: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(dict_get(slf, key, Some(default))?.unbind())
}

// @pypdf: pypdf.generic._viewerpref.ViewerPreferences._set_name
#[pyfunction]
fn genbasic_viewerpreferences_set_name<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    available: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let empty = PyList::empty(py);
    if !available.eq(&empty)? && !available.contains(value)? {
        return Err(PyValueError::new_err(format!(
            "{} is an unacceptable value",
            pyutil::py_str(value)?
        )));
    }
    let name_class = base_class(py, "NameObject")?;
    viewerpref_set_item(py, slf, key, name_class.call1((value,))?)
}

// @pypdf: pypdf.generic._viewerpref.ViewerPreferences._get_arr
#[pyfunction]
fn genbasic_viewerpreferences_get_arr<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    default: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let default = if default.is_none() {
        none(py)
    } else {
        data_structures_class(py, "ArrayObject")?.call1((default,))?
    };
    Ok(dict_get(slf, key, Some(&default))?.unbind())
}

// @pypdf: pypdf.generic._viewerpref.ViewerPreferences._set_arr
#[pyfunction]
fn genbasic_viewerpreferences_set_arr<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    if value.is_none() {
        let name_class = base_class(py, "NameObject")?;
        return match slf.del_item(name_class.call1((key,))?) {
            Ok(()) => Ok(()),
            Err(error) if error.is_instance_of::<PyKeyError>(py) => Ok(()),
            Err(error) => Err(error),
        };
    }
    let array_class = data_structures_class(py, "ArrayObject")?;
    if !value.is_instance(&array_class)? {
        return Err(PyValueError::new_err("ArrayObject is expected"));
    }
    let length = value.len()?;
    if key.eq("/PrintPageRange")? && length % 2 != 0 {
        // The array holds first/last page pairs, so an odd length leaves a
        // range without its end.
        return Err(PyValueError::new_err(format!(
            "/PrintPageRange holds page pairs, got {} entries: {}",
            length,
            pyutil::py_str(value)?
        )));
    }
    viewerpref_set_item(py, slf, key, value.clone())
}

// @pypdf: pypdf.generic._viewerpref.ViewerPreferences._get_int
#[pyfunction]
fn genbasic_viewerpreferences_get_int<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    default: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(dict_get(slf, key, Some(default))?.unbind())
}

// @pypdf: pypdf.generic._viewerpref.ViewerPreferences._set_int
#[pyfunction]
fn genbasic_viewerpreferences_set_int<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    key: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    if value.lt(0)? {
        return Err(PyValueError::new_err(format!(
            "{} is an unacceptable value for {}",
            pyutil::py_str(value)?,
            pyutil::py_str(key)?
        )));
    }
    let number_class = base_class(py, "NumberObject")?;
    viewerpref_set_item(py, slf, key, number_class.call1((value,))?)
}

// @pypdf: pypdf.generic._viewerpref.ViewerPreferences.PRINT_SCALING
#[pyfunction]
fn genbasic_viewerpreferences_print_scaling<'py>(
    py: Python<'py>,
    _slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(base_class(py, "NameObject")?
        .call1(("/PrintScaling",))?
        .unbind())
}

// ---------------------------------------------------------------------------
// pypdf/generic/_rectangle.py
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._rectangle.RectangleObject.__init__
#[pyfunction]
fn genbasic_rectangleobject_new<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    arr: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let length = arr.len()?;
    if length != 4 {
        return Err(PyValueError::new_err(format!(
            "Expected four values for a rectangle, got {}: {}",
            length,
            pyutil::py_str(arr)?
        )));
    }
    // automatically convert arr[x] into NumberObject(arr[x]) if necessary
    let mut values: Vec<Bound<'py, PyAny>> = Vec::with_capacity(4);
    for value in arr.try_iter()? {
        values.push(slf.call_method1("_ensure_is_number", (value?,))?);
    }
    let list = PyList::new(py, values)?;
    // `ArrayObject.__init__(self, [...])` (list.__init__) instead of `super()`
    // so a subclassed `__init__` chain is not re-entered.
    data_structures_class(py, "ArrayObject")?
        .getattr("__init__")?
        .call1((slf, list))?;
    Ok(())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject._ensure_is_number
#[pyfunction]
fn genbasic_rectangleobject_ensure_is_number<'py>(
    py: Python<'py>,
    _slf: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let float_class = base_class(py, "FloatObject")?;
    let number_class = base_class(py, "NumberObject")?;
    if !(value.is_instance(&float_class)? || value.is_instance(&number_class)?) {
        return Ok(float_class.call1((value,))?.unbind());
    }
    Ok(value.clone().unbind())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.scale
#[pyfunction]
fn genbasic_rectangleobject_scale<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    sx: &Bound<'py, PyAny>,
    sy: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let float_class = builtins(py)?.getattr("float")?;
    let left = float_class.call1((slf.getattr("left")?,))?.mul(sx)?;
    let bottom = float_class.call1((slf.getattr("bottom")?,))?.mul(sy)?;
    let right = float_class.call1((slf.getattr("right")?,))?.mul(sx)?;
    let top = float_class.call1((slf.getattr("top")?,))?.mul(sy)?;
    let arguments = PyTuple::new(py, [left, bottom, right, top])?;
    let rectangle_class = module_attr(py, RECTANGLE_MODULE, "RectangleObject")?;
    Ok(rectangle_class.call1((arguments,))?.unbind())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.__repr__
#[pyfunction]
fn genbasic_rectangleobject_repr<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let list = builtins(py)?.getattr("list")?.call1((slf,))?;
    Ok(py_string(
        py,
        &format!("RectangleObject({})", pyutil::py_repr(&list)?),
    ))
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.left
#[pyfunction]
fn genbasic_rectangleobject_get_left<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(slf.get_item(0)?.unbind())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.left
#[pyfunction]
fn genbasic_rectangleobject_set_left<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let float_class = base_class(py, "FloatObject")?;
    slf.set_item(0, float_class.call1((value,))?)?;
    Ok(())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.bottom
#[pyfunction]
fn genbasic_rectangleobject_get_bottom<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(slf.get_item(1)?.unbind())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.bottom
#[pyfunction]
fn genbasic_rectangleobject_set_bottom<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let float_class = base_class(py, "FloatObject")?;
    slf.set_item(1, float_class.call1((value,))?)?;
    Ok(())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.right
#[pyfunction]
fn genbasic_rectangleobject_get_right<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(slf.get_item(2)?.unbind())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.right
#[pyfunction]
fn genbasic_rectangleobject_set_right<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let float_class = base_class(py, "FloatObject")?;
    slf.set_item(2, float_class.call1((value,))?)?;
    Ok(())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.top
#[pyfunction]
fn genbasic_rectangleobject_get_top<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(slf.get_item(3)?.unbind())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.top
#[pyfunction]
fn genbasic_rectangleobject_set_top<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let float_class = base_class(py, "FloatObject")?;
    slf.set_item(3, float_class.call1((value,))?)?;
    Ok(())
}

/// `self[0], self[1] = (self._ensure_is_number(x) for x in value)` for the two
/// tuple-valued coordinate properties.
fn unpack_two<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let mut values: Vec<Bound<'py, PyAny>> = Vec::with_capacity(2);
    for item in value.try_iter()? {
        values.push(slf.call_method1("_ensure_is_number", (item?,))?);
    }
    let length = values.len();
    if length < 2 {
        return Err(PyValueError::new_err(format!(
            "not enough values to unpack (expected 2, got {length})"
        )));
    }
    if length > 2 {
        return Err(PyValueError::new_err(
            "too many values to unpack (expected 2)",
        ));
    }
    let first = values.remove(0);
    let second = values.remove(0);
    let _ = py;
    Ok((first, second))
}

/// Tuple of two properties, `(self.<first>, self.<second>)`.
fn coordinate_pair<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    first: &str,
    second: &str,
) -> PyResult<Py<PyAny>> {
    let pair = PyTuple::new(py, [slf.getattr(first)?, slf.getattr(second)?])?;
    Ok(pair.into_any().unbind())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.lower_left
#[pyfunction]
fn genbasic_rectangleobject_get_lower_left<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    coordinate_pair(py, slf, "left", "bottom")
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.lower_left
#[pyfunction]
fn genbasic_rectangleobject_set_lower_left<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let (first, second) = unpack_two(py, slf, value)?;
    slf.set_item(0, first)?;
    slf.set_item(1, second)?;
    Ok(())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.lower_right
#[pyfunction]
fn genbasic_rectangleobject_get_lower_right<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    coordinate_pair(py, slf, "right", "bottom")
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.lower_right
#[pyfunction]
fn genbasic_rectangleobject_set_lower_right<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let (first, second) = unpack_two(py, slf, value)?;
    // Upstream assigns `self[2], self[1]` for the lower-right corner.
    slf.set_item(2, first)?;
    slf.set_item(1, second)?;
    Ok(())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.upper_left
#[pyfunction]
fn genbasic_rectangleobject_get_upper_left<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    coordinate_pair(py, slf, "left", "top")
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.upper_left
#[pyfunction]
fn genbasic_rectangleobject_set_upper_left<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let (first, second) = unpack_two(py, slf, value)?;
    slf.set_item(0, first)?;
    slf.set_item(3, second)?;
    Ok(())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.upper_right
#[pyfunction]
fn genbasic_rectangleobject_get_upper_right<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    coordinate_pair(py, slf, "right", "top")
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.upper_right
#[pyfunction]
fn genbasic_rectangleobject_set_upper_right<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    value: &Bound<'py, PyAny>,
) -> PyResult<()> {
    let (first, second) = unpack_two(py, slf, value)?;
    slf.set_item(2, first)?;
    slf.set_item(3, second)?;
    Ok(())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.width
#[pyfunction]
fn genbasic_rectangleobject_width<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let right = slf.getattr("right")?;
    let left = slf.getattr("left")?;
    Ok(right.sub(left)?.unbind())
}

// @pypdf: pypdf.generic._rectangle.RectangleObject.height
#[pyfunction]
fn genbasic_rectangleobject_height<'py>(
    _py: Python<'py>,
    slf: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let top = slf.getattr("top")?;
    let bottom = slf.getattr("bottom")?;
    Ok(top.sub(bottom)?.unbind())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(
        genbasic_color_from_normalized_values,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(genbasic_color_as_operator, module)?)?;

    module.add_function(wrap_pyfunction!(genbasic_fit_new, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_fit_xyz, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_fit_fit, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_fit_fit_horizontally, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_fit_fit_vertically, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_fit_fit_rectangle, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_fit_fit_box, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_fit_fit_box_horizontally, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_fit_fit_box_vertically, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_fit_str, module)?)?;

    module.add_function(wrap_pyfunction!(
        genbasic_outlineitem_write_to_stream,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(genbasic_named_reference_link_new, module)?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_named_reference_link_find_referenced_page,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_named_reference_link_patch_reference,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_direct_reference_link_new,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_direct_reference_link_find_referenced_page,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_direct_reference_link_patch_reference,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(genbasic_extract_links, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_build_link, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_create_link, module)?)?;

    module.add_function(wrap_pyfunction!(genbasic_viewerpreferences_new, module)?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_viewerpreferences_get_bool,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_viewerpreferences_set_bool,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_viewerpreferences_get_name,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_viewerpreferences_set_name,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_viewerpreferences_get_arr,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_viewerpreferences_set_arr,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_viewerpreferences_get_int,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_viewerpreferences_set_int,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_viewerpreferences_print_scaling,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(genbasic_rectangleobject_new, module)?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_rectangleobject_ensure_is_number,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(genbasic_rectangleobject_scale, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_rectangleobject_repr, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_rectangleobject_get_left, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_rectangleobject_set_left, module)?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_rectangleobject_get_bottom,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_rectangleobject_set_bottom,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_rectangleobject_get_right,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_rectangleobject_set_right,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(genbasic_rectangleobject_get_top, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_rectangleobject_set_top, module)?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_rectangleobject_get_lower_left,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_rectangleobject_set_lower_left,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_rectangleobject_get_lower_right,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_rectangleobject_set_lower_right,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_rectangleobject_get_upper_left,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_rectangleobject_set_upper_left,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_rectangleobject_get_upper_right,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        genbasic_rectangleobject_set_upper_right,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(genbasic_rectangleobject_width, module)?)?;
    module.add_function(wrap_pyfunction!(genbasic_rectangleobject_height, module)?)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Symbols implemented in the hand-written shims
// (`python/pypdf/generic/_{color,fit,outline,link,viewerpref,rectangle}.py`).
//
// Class objects, class attributes, module constants, dataclass definitions and
// the descriptor wiring of `ViewerPreferences.__new__` stay Python definitions:
// they are part of the observable Python object model (MRO, `isinstance`,
// `property` descriptors, dataclass `__eq__`/`__repr__`).  Every method body
// lives in this file.
//
// @pypdf: pypdf.generic._color.Color
// @pypdf: pypdf.generic._color.Color.color_operator
// @pypdf: pypdf.generic._color.Color._ordered_fields
// @pypdf: pypdf.generic._color.DeviceGray
// @pypdf: pypdf.generic._color.DeviceGray.gray
// @pypdf: pypdf.generic._color.DeviceGray.color_operator
// @pypdf: pypdf.generic._color.DeviceGray._ordered_fields
// @pypdf: pypdf.generic._color.DeviceRGB
// @pypdf: pypdf.generic._color.DeviceRGB.red
// @pypdf: pypdf.generic._color.DeviceRGB.green
// @pypdf: pypdf.generic._color.DeviceRGB.blue
// @pypdf: pypdf.generic._color.DeviceRGB.color_operator
// @pypdf: pypdf.generic._color.DeviceRGB._ordered_fields
// @pypdf: pypdf.generic._color.DeviceCMYK
// @pypdf: pypdf.generic._color.DeviceCMYK.cyan
// @pypdf: pypdf.generic._color.DeviceCMYK.magenta
// @pypdf: pypdf.generic._color.DeviceCMYK.yellow
// @pypdf: pypdf.generic._color.DeviceCMYK.black
// @pypdf: pypdf.generic._color.DeviceCMYK.color_operator
// @pypdf: pypdf.generic._color.DeviceCMYK._ordered_fields
// @pypdf: pypdf.generic._fit.Fit
// @pypdf: pypdf.generic._fit.DEFAULT_FIT
// @pypdf: pypdf.generic._outline.OutlineItem
// @pypdf: pypdf.generic._link.NamedReferenceLink
// @pypdf: pypdf.generic._link.DirectReferenceLink
// @pypdf: pypdf.generic._link.ReferenceLink
// @pypdf: pypdf.generic._viewerpref.f_obj
// @pypdf: pypdf.generic._viewerpref.BOX_NAMES
// @pypdf: pypdf.generic._viewerpref.ViewerPreferences
// @pypdf: pypdf.generic._viewerpref.ViewerPreferences.__new__
// @pypdf: pypdf.generic._rectangle.RectangleObject
