//! Rust port of `pypdf/annotations/{_base,_markup_annotations,_non_markup_annotations}.py`
//! (upstream pypdf 7.1.1).
//!
//! # Design
//!
//! The annotation classes are *Python classes* in the hand-written shims
//! (`python/pypdf/annotations/_base.py`, `_markup_annotations.py`,
//! `_non_markup_annotations.py`), keeping upstream's MRO
//! (`DictionaryObject` subclass, `ABC`), keyword-only signatures and class
//! attributes.  Every `__init__` / property delegates to a `#[pyfunction]`
//! implemented here, so the dictionary contents, their insertion order and the
//! validation/warning/error behaviour are produced by Rust.
//!
//! Cross-object work is done by calling the Python classes/methods exactly like
//! upstream does (e.g. `RectangleObject(rect)`, `hex_to_rgb(...)`,
//! `parent.indirect_reference`, `self.flags = AnnotationFlag.PRINT`), so the
//! port inherits the behaviour of those modules (notably
//! `generic._rectangle` / `generic._fit`, which are ported separately) and stays
//! monkeypatchable.

use pyo3::exceptions::{PyAttributeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyInt, PyList, PyModule, PyString, PyTuple};

use crate::pyutil;

const BASE_MODULE: &str = "pypdf.annotations._base";
const MARKUP_MODULE: &str = "pypdf.annotations._markup_annotations";

/// Upstream module name of the non-markup shim; used for `logger_warning`.
const NON_MARKUP_SOURCE: &str = "pypdf.annotations._non_markup_annotations";

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn modules<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyModule>> {
    py.import(name)
}

fn class<'py>(py: Python<'py>, module: &str, name: &str) -> PyResult<Bound<'py, PyAny>> {
    modules(py, module)?.getattr(name)
}

fn generic_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    modules(py, "pypdf.generic")?.getattr(name)
}

fn generic_base_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    modules(py, "pypdf.generic._base")?.getattr(name)
}

fn constants_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    modules(py, "pypdf.constants")?.getattr(name)
}

fn rectangle_object<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    modules(py, "pypdf.generic._rectangle")?
        .getattr("RectangleObject")?
        .call1((value,))
}

fn hex_to_rgb<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    modules(py, "pypdf.generic._utils")?
        .getattr("hex_to_rgb")?
        .call1((value,))
}

fn name_object<'py>(py: Python<'py>, value: &str) -> PyResult<Bound<'py, PyAny>> {
    generic_base_class(py, "NameObject")?.call1((value,))
}

/// `str` object for a Rust string (used for the built strings upstream).
fn py_string<'py>(py: Python<'py>, value: &str) -> Bound<'py, PyAny> {
    PyString::new(py, value).into_any()
}

/// `slice(start, stop, None)` with optional bounds.
fn py_slice<'py>(
    py: Python<'py>,
    start: Option<i64>,
    stop: Option<i64>,
) -> PyResult<Bound<'py, PyAny>> {
    let start_bound = start.map(|value| PyInt::new(py, value).into_any());
    let stop_bound = stop.map(|value| PyInt::new(py, value).into_any());
    pyutil::py_slice(py, start_bound.as_ref(), stop_bound.as_ref(), None)
}

fn text_string<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    generic_base_class(py, "TextStringObject")?.call1((value,))
}

fn number_object<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    generic_base_class(py, "NumberObject")?.call1((value,))
}

fn float_object<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    generic_base_class(py, "FloatObject")?.call1((value,))
}

fn boolean_object<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    generic_base_class(py, "BooleanObject")?.call1((value,))
}

fn array_object<'py>(
    py: Python<'py>,
    items: Vec<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "ArrayObject")?.call1((PyList::new(py, items)?,))
}

fn dictionary_object<'py>(
    py: Python<'py>,
    pairs: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyAny>> {
    generic_class(py, "DictionaryObject")?.call1((pairs,))
}

/// `self[NameObject(key)] = value` (goes through `DictionaryObject.__setitem__`).
fn set_name_item(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    key: &str,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    slf.set_item(name_object(py, key)?, value)?;
    Ok(())
}

/// `self.update({NameObject(k): v, ...})` preserving insertion order.
fn update_named(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    pairs: &[(&str, Bound<'_, PyAny>)],
) -> PyResult<()> {
    let mapping = PyDict::new(py);
    for (key, value) in pairs {
        mapping.set_item(name_object(py, key)?, value)?;
    }
    slf.call_method1("update", (mapping,))?;
    Ok(())
}

/// `super().__init__(**kwargs)` for a specific parent class, dispatched
/// explicitly so `super()` semantics (MRO position of the shim class) hold.
fn call_parent_init(
    py: Python<'_>,
    module: &str,
    class_name: &str,
    slf: &Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    class(py, module, class_name)?
        .getattr("__init__")?
        .call((slf,), kwargs)?;
    Ok(())
}

/// `[NumberObject(n) for n in values]`.
fn number_list<'py>(
    py: Python<'py>,
    values: &Bound<'py, PyAny>,
) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let mut numbers = Vec::new();
    for value in values.try_iter()? {
        numbers.push(number_object(py, &value?)?);
    }
    Ok(numbers)
}

/// `[FloatObject(n) for n in hex_to_rgb(value)]`.
fn rgb_floats<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let mut floats = Vec::new();
    for component in hex_to_rgb(py, value)?.try_iter()? {
        floats.push(float_object(py, &component?)?);
    }
    Ok(floats)
}

// ---------------------------------------------------------------------------
// pypdf.annotations._base
// ---------------------------------------------------------------------------

// @pypdf: pypdf.annotations._base.AnnotationDictionary.__init__
#[pyfunction]
fn annotationsmod_annotation_dictionary_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
) -> PyResult<()> {
    // ``super().__init__()`` then the ``/Type`` entry.
    // ``/Rect`` should not be added here as Polygon and PolyLine can automatically
    // set it; the flags were NOT added to the constructor on purpose (most users
    // don't want to change the default, they can use the property instead).
    call_parent_init(
        py,
        "pypdf.generic._data_structures",
        "DictionaryObject",
        slf,
        None,
    )?;
    set_name_item(py, slf, "/Type", &name_object(py, "/Annot")?)
}

// @pypdf: pypdf.annotations._base.AnnotationDictionary.flags
#[pyfunction]
fn annotationsmod_annotation_dictionary_flags_get(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let default = 0i64.into_pyobject(py)?.into_any();
    let value = slf.call_method1("get", (name_object(py, "/F")?, default))?;
    Ok(constants_attr(py, "AnnotationFlag")?
        .call1((value,))?
        .unbind())
}

// @pypdf: pypdf.annotations._base.AnnotationDictionary.flags.setter
#[pyfunction]
fn annotationsmod_annotation_dictionary_flags_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    set_name_item(py, slf, "/F", &number_object(py, value)?)
}

// @pypdf: pypdf.annotations._base.NO_FLAGS
#[pyfunction]
fn annotationsmod_no_flags(py: Python<'_>) -> PyResult<Py<PyAny>> {
    Ok(constants_attr(py, "AnnotationFlag")?.call1((0,))?.unbind())
}

// ---------------------------------------------------------------------------
// pypdf.annotations._markup_annotations
// ---------------------------------------------------------------------------

/// `RectangleObject((x_min, y_min, x_max, y_max))` for the given vertices.
fn bounding_rectangle<'py>(
    py: Python<'py>,
    vertices: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let first = vertices.get_item(0)?;
    let minimum_of =
        |left: Bound<'py, PyAny>, right: Bound<'py, PyAny>| -> PyResult<Bound<'py, PyAny>> {
            crate::pyutil::cached_module(py, "builtins")?.getattr("min")?.call1((left, right))
        };
    let maximum_of =
        |left: Bound<'py, PyAny>, right: Bound<'py, PyAny>| -> PyResult<Bound<'py, PyAny>> {
            crate::pyutil::cached_module(py, "builtins")?.getattr("max")?.call1((left, right))
        };
    let mut x_min = first.get_item(0)?;
    let mut y_min = first.get_item(1)?;
    let mut x_max = x_min.clone();
    let mut y_max = y_min.clone();
    for vertex in vertices.try_iter()? {
        let vertex = vertex?;
        let x = vertex.get_item(0)?;
        let y = vertex.get_item(1)?;
        x_min = minimum_of(x_min, x.clone())?;
        y_min = minimum_of(y_min, y.clone())?;
        x_max = maximum_of(x_max, x)?;
        y_max = maximum_of(y_max, y)?;
    }
    let rect = PyTuple::new(py, [x_min, y_min, x_max, y_max])?;
    rectangle_object(py, rect.as_any())
}

// @pypdf: pypdf.annotations._markup_annotations._get_bounding_rectangle
#[pyfunction]
fn annotationsmod_get_bounding_rectangle<'py>(
    py: Python<'py>,
    vertices: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(bounding_rectangle(py, vertices)?.unbind())
}

// @pypdf: pypdf.annotations._markup_annotations.MarkupAnnotation.__init__
#[pyfunction]
fn annotationsmod_markup_annotation_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    title_bar: &Bound<'_, PyAny>,
    in_reply_to: &Bound<'_, PyAny>,
    reply_type: &Bound<'_, PyAny>,
    annotation_name: &Bound<'_, PyAny>,
) -> PyResult<()> {
    if !title_bar.is_none() {
        set_name_item(py, slf, "/T", &text_string(py, title_bar)?)?;
    }
    if !annotation_name.is_none() && in_reply_to.is_none() {
        return Err(PyValueError::new_err(
            "annotation_name is only supported when in_reply_to is set",
        ));
    }
    if !reply_type.eq("R")? && in_reply_to.is_none() {
        return Err(PyValueError::new_err(
            "reply_type is only meaningful when in_reply_to is set",
        ));
    }
    if !in_reply_to.is_none() {
        let indirect_object = generic_base_class(py, "IndirectObject")?;
        let reference = if in_reply_to.is_instance(&indirect_object)? {
            in_reply_to.clone()
        } else {
            let indirect_reference = match in_reply_to.getattr("indirect_reference") {
                Ok(reference) => reference,
                Err(error) if error.is_instance_of::<PyAttributeError>(py) => {
                    py.None().into_bound(py)
                }
                Err(error) => return Err(error),
            };
            if !indirect_reference.is_instance(&indirect_object)? {
                return Err(PyValueError::new_err(
                    "in_reply_to must be a registered annotation \
                     (added via writer.add_annotation() first)",
                ));
            }
            indirect_reference
        };
        set_name_item(py, slf, "/IRT", &reference)?;
        let reply_name = format!("/{}", pyutil::py_str(reply_type)?);
        set_name_item(py, slf, "/RT", &name_object(py, &reply_name)?)?;
        let annotation_name = if annotation_name.is_none() {
            py.import("uuid")?
                .getattr("uuid4")?
                .call0()?
                .str()?
                .into_any()
        } else {
            annotation_name.clone()
        };
        set_name_item(py, slf, "/NM", &text_string(py, &annotation_name)?)?;
    }
    Ok(())
}

// @pypdf: pypdf.annotations._markup_annotations.Text.__init__
#[pyfunction]
fn annotationsmod_text_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    rect: &Bound<'_, PyAny>,
    text: &Bound<'_, PyAny>,
    open: &Bound<'_, PyAny>,
    flags: &Bound<'_, PyAny>,
    kwargs: &Bound<'_, PyDict>,
) -> PyResult<()> {
    call_parent_init(py, MARKUP_MODULE, "MarkupAnnotation", slf, Some(kwargs))?;
    set_name_item(py, slf, "/Subtype", &name_object(py, "/Text")?)?;
    set_name_item(py, slf, "/Rect", &rectangle_object(py, rect)?)?;
    set_name_item(py, slf, "/Contents", &text_string(py, text)?)?;
    set_name_item(py, slf, "/Open", &boolean_object(py, open)?)?;
    set_name_item(py, slf, "/Flags", &number_object(py, flags)?)?;
    Ok(())
}

// @pypdf: pypdf.annotations._markup_annotations.FreeText.__init__
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn annotationsmod_free_text_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    text: &Bound<'_, PyAny>,
    rect: &Bound<'_, PyAny>,
    font: &Bound<'_, PyAny>,
    bold: &Bound<'_, PyAny>,
    italic: &Bound<'_, PyAny>,
    font_size: &Bound<'_, PyAny>,
    font_color: &Bound<'_, PyAny>,
    border_color: &Bound<'_, PyAny>,
    background_color: &Bound<'_, PyAny>,
    kwargs: &Bound<'_, PyDict>,
) -> PyResult<()> {
    call_parent_init(py, MARKUP_MODULE, "MarkupAnnotation", slf, Some(kwargs))?;

    set_name_item(py, slf, "/Subtype", &name_object(py, "/FreeText")?)?;
    set_name_item(py, slf, "/Rect", &rectangle_object(py, rect)?)?;

    // Table 225 of the 1.7 reference ("CSS2 style attributes used in rich text
    // strings")
    let mut font_str = String::from("font: ");
    if italic.is_truthy()? {
        font_str.push_str("italic ");
    } else {
        font_str.push_str("normal ");
    }
    if bold.is_truthy()? {
        font_str.push_str("bold ");
    } else {
        font_str.push_str("normal ");
    }
    font_str.push_str(&pyutil::py_str(font_size)?);
    font_str.push(' ');
    font_str.push_str(&pyutil::py_str(font)?);
    font_str.push_str(";text-align:left;color:#");
    font_str.push_str(&pyutil::py_str(font_color)?);

    let mut default_appearance_string = String::new();
    if font_color.is_truthy()? {
        for component in hex_to_rgb(py, font_color)?.try_iter()? {
            default_appearance_string.push_str(&pyutil::py_str(&component?)?);
            default_appearance_string.push(' ');
        }
        default_appearance_string.push_str("rg");
    }

    update_named(
        py,
        slf,
        &[
            ("/Subtype", name_object(py, "/FreeText")?),
            ("/Rect", rectangle_object(py, rect)?),
            ("/Contents", text_string(py, text)?),
            // font size color
            ("/DS", text_string(py, &py_string(py, &font_str))?),
            (
                "/DA",
                text_string(py, &py_string(py, &default_appearance_string))?,
            ),
        ],
    )?;

    if border_color.is_none() {
        // Border Style
        let width = PyDict::new(py);
        width.set_item(
            name_object(py, "/W")?,
            number_object(py, &0i64.into_pyobject(py)?.into_any())?,
        )?;
        set_name_item(py, slf, "/BS", &dictionary_object(py, &width)?)?;
    }
    if !background_color.is_none() {
        let colors = rgb_floats(py, background_color)?;
        set_name_item(py, slf, "/C", &array_object(py, colors)?)?;
    }
    Ok(())
}

// @pypdf: pypdf.annotations._markup_annotations.Line.__init__
#[pyfunction]
fn annotationsmod_line_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    p1: &Bound<'_, PyAny>,
    p2: &Bound<'_, PyAny>,
    rect: &Bound<'_, PyAny>,
    text: &Bound<'_, PyAny>,
    kwargs: &Bound<'_, PyDict>,
) -> PyResult<()> {
    call_parent_init(py, MARKUP_MODULE, "MarkupAnnotation", slf, Some(kwargs))?;
    let coordinates = vec![
        float_object(py, &p1.get_item(0)?)?,
        float_object(py, &p1.get_item(1)?)?,
        float_object(py, &p2.get_item(0)?)?,
        float_object(py, &p2.get_item(1)?)?,
    ];
    let line_endings = vec![name_object(py, "/None")?, name_object(py, "/None")?];
    let interior = vec![
        float_object(py, &0.5f64.into_pyobject(py)?.into_any())?,
        float_object(py, &0.5f64.into_pyobject(py)?.into_any())?,
        float_object(py, &0.5f64.into_pyobject(py)?.into_any())?,
    ];
    update_named(
        py,
        slf,
        &[
            ("/Subtype", name_object(py, "/Line")?),
            ("/Rect", rectangle_object(py, rect)?),
            ("/L", array_object(py, coordinates)?),
            ("/LE", array_object(py, line_endings)?),
            ("/IC", array_object(py, interior)?),
            ("/Contents", text_string(py, text)?),
        ],
    )
}

// @pypdf: pypdf.annotations._markup_annotations.PolyLine.__init__
#[pyfunction]
fn annotationsmod_polyline_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    vertices: &Bound<'_, PyAny>,
    kwargs: &Bound<'_, PyDict>,
) -> PyResult<()> {
    call_parent_init(py, MARKUP_MODULE, "MarkupAnnotation", slf, Some(kwargs))?;
    if vertices.len()? == 0 {
        return Err(PyValueError::new_err(
            "A polyline needs at least 1 vertex with two coordinates",
        ));
    }
    let mut coordinates: Vec<Bound<'_, PyAny>> = Vec::new();
    for vertex in vertices.try_iter()? {
        let vertex = vertex?;
        coordinates.push(number_object(py, &vertex.get_item(0)?)?);
        coordinates.push(number_object(py, &vertex.get_item(1)?)?);
    }
    let rect = bounding_rectangle(py, vertices)?;
    update_named(
        py,
        slf,
        &[
            ("/Subtype", name_object(py, "/PolyLine")?),
            ("/Vertices", array_object(py, coordinates)?),
            ("/Rect", rect),
        ],
    )
}

// @pypdf: pypdf.annotations._markup_annotations.Rectangle.__init__
#[pyfunction]
fn annotationsmod_rectangle_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    rect: &Bound<'_, PyAny>,
    interior_color: &Bound<'_, PyAny>,
    border_color: &Bound<'_, PyAny>,
    kwargs: &Bound<'_, PyDict>,
) -> PyResult<()> {
    call_parent_init(py, MARKUP_MODULE, "MarkupAnnotation", slf, Some(kwargs))?;
    update_named(
        py,
        slf,
        &[
            ("/Type", name_object(py, "/Annot")?),
            ("/Subtype", name_object(py, "/Square")?),
            ("/Rect", rectangle_object(py, rect)?),
        ],
    )?;
    if !border_color.is_none() {
        let colors = rgb_floats(py, border_color)?;
        set_name_item(py, slf, "/C", &array_object(py, colors)?)?;
    }
    if interior_color.is_truthy()? {
        let colors = rgb_floats(py, interior_color)?;
        set_name_item(py, slf, "/IC", &array_object(py, colors)?)?;
    }
    Ok(())
}

// @pypdf: pypdf.annotations._markup_annotations.Highlight.__init__
#[pyfunction]
fn annotationsmod_highlight_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    rect: &Bound<'_, PyAny>,
    quad_points: &Bound<'_, PyAny>,
    highlight_color: &Bound<'_, PyAny>,
    printing: &Bound<'_, PyAny>,
    kwargs: &Bound<'_, PyDict>,
) -> PyResult<()> {
    call_parent_init(py, MARKUP_MODULE, "MarkupAnnotation", slf, Some(kwargs))?;
    let colors = rgb_floats(py, highlight_color)?;
    update_named(
        py,
        slf,
        &[
            ("/Subtype", name_object(py, "/Highlight")?),
            ("/Rect", rectangle_object(py, rect)?),
            ("/QuadPoints", quad_points.clone()),
            ("/C", array_object(py, colors)?),
        ],
    )?;
    if printing.is_truthy()? {
        slf.setattr(
            "flags",
            constants_attr(py, "AnnotationFlag")?.getattr("PRINT")?,
        )?;
    }
    Ok(())
}

// @pypdf: pypdf.annotations._markup_annotations.Ellipse.__init__
#[pyfunction]
fn annotationsmod_ellipse_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    rect: &Bound<'_, PyAny>,
    interior_color: &Bound<'_, PyAny>,
    kwargs: &Bound<'_, PyDict>,
) -> PyResult<()> {
    call_parent_init(py, MARKUP_MODULE, "MarkupAnnotation", slf, Some(kwargs))?;
    update_named(
        py,
        slf,
        &[
            ("/Type", name_object(py, "/Annot")?),
            ("/Subtype", name_object(py, "/Circle")?),
            ("/Rect", rectangle_object(py, rect)?),
        ],
    )?;
    if interior_color.is_truthy()? {
        let colors = rgb_floats(py, interior_color)?;
        set_name_item(py, slf, "/IC", &array_object(py, colors)?)?;
    }
    Ok(())
}

// @pypdf: pypdf.annotations._markup_annotations.Polygon.__init__
#[pyfunction]
fn annotationsmod_polygon_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    vertices: &Bound<'_, PyAny>,
    kwargs: &Bound<'_, PyDict>,
) -> PyResult<()> {
    call_parent_init(py, MARKUP_MODULE, "MarkupAnnotation", slf, Some(kwargs))?;
    if vertices.len()? == 0 {
        return Err(PyValueError::new_err(
            "A polygon needs at least 1 vertex with two coordinates",
        ));
    }
    let mut coordinates: Vec<Bound<'_, PyAny>> = Vec::new();
    for vertex in vertices.try_iter()? {
        let vertex = vertex?;
        coordinates.push(number_object(py, &vertex.get_item(0)?)?);
        coordinates.push(number_object(py, &vertex.get_item(1)?)?);
    }
    let rect = bounding_rectangle(py, vertices)?;
    update_named(
        py,
        slf,
        &[
            ("/Type", name_object(py, "/Annot")?),
            ("/Subtype", name_object(py, "/Polygon")?),
            ("/Vertices", array_object(py, coordinates)?),
            ("/IT", name_object(py, "/PolygonCloud")?),
            ("/Rect", rect),
        ],
    )
}

// ---------------------------------------------------------------------------
// pypdf.annotations._non_markup_annotations
// ---------------------------------------------------------------------------

// @pypdf: pypdf.annotations._non_markup_annotations.Link.__init__
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn annotationsmod_link_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    rect: &Bound<'_, PyAny>,
    border: &Bound<'_, PyAny>,
    url: &Bound<'_, PyAny>,
    target_page_index: &Bound<'_, PyAny>,
    fit: &Bound<'_, PyAny>,
    kwargs: &Bound<'_, PyDict>,
) -> PyResult<()> {
    call_parent_init(py, BASE_MODULE, "AnnotationDictionary", slf, Some(kwargs))?;

    let is_external = !url.is_none();
    let is_internal = !target_page_index.is_none();
    if !is_external && !is_internal {
        return Err(PyValueError::new_err(
            "Either 'url' or 'target_page_index' have to be provided. Both were None.",
        ));
    }
    if is_external && is_internal {
        return Err(PyValueError::new_err(format!(
            "Either 'url' or 'target_page_index' have to be provided. url={}, target_page_index={}",
            pyutil::py_repr(url)?,
            pyutil::py_repr(target_page_index)?
        )));
    }

    let border_array = if !border.is_none() {
        // ``border_arr = [NumberObject(n) for n in border[:3]]`` then, for a
        // four element ``border``, ``border_arr.append(ArrayObject([...]))``.
        let head = border.get_item(py_slice(py, None, Some(3))?)?;
        let mut border_items = number_list(py, &head)?;
        if border.len()? == 4 {
            let dash_pattern = array_object(py, number_list(py, &border.get_item(3)?)?)?;
            border_items.push(dash_pattern);
        }
        array_object(py, border_items)?
    } else {
        let mut zeros: Vec<Bound<'_, PyAny>> = Vec::new();
        for _ in 0..3 {
            zeros.push(number_object(py, &0i64.into_pyobject(py)?.into_any())?);
        }
        array_object(py, zeros)?
    };

    update_named(
        py,
        slf,
        &[
            ("/Type", name_object(py, "/Annot")?),
            ("/Subtype", name_object(py, "/Link")?),
            ("/Rect", rectangle_object(py, rect)?),
            ("/Border", border_array),
        ],
    )?;

    if is_external {
        let action = PyDict::new(py);
        action.set_item(name_object(py, "/S")?, name_object(py, "/URI")?)?;
        action.set_item(name_object(py, "/Type")?, name_object(py, "/Action")?)?;
        action.set_item(name_object(py, "/URI")?, text_string(py, url)?)?;
        set_name_item(py, slf, "/A", &dictionary_object(py, &action)?)?;
    }
    if is_internal {
        // This needs to be updated later!
        let deferred = PyDict::new(py);
        deferred.set_item("target_page_index", number_object(py, target_page_index)?)?;
        deferred.set_item(
            "fit",
            name_object(py, &pyutil::py_str(&fit.getattr("fit_type")?)?)?,
        )?;
        deferred.set_item("fit_args", fit.getattr("fit_args")?)?;
        set_name_item(py, slf, "/Dest", &dictionary_object(py, &deferred)?)?;
    }
    Ok(())
}

// @pypdf: pypdf.annotations._non_markup_annotations.Popup.__init__
#[pyfunction]
fn annotationsmod_popup_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    rect: &Bound<'_, PyAny>,
    parent: &Bound<'_, PyAny>,
    open: &Bound<'_, PyAny>,
    kwargs: &Bound<'_, PyDict>,
) -> PyResult<()> {
    call_parent_init(py, BASE_MODULE, "AnnotationDictionary", slf, Some(kwargs))?;
    update_named(
        py,
        slf,
        &[
            ("/Subtype", name_object(py, "/Popup")?),
            ("/Rect", rectangle_object(py, rect)?),
            ("/Open", boolean_object(py, open)?),
        ],
    )?;
    if parent.is_truthy()? {
        // This needs to be an indirect object
        match parent.getattr("indirect_reference") {
            Ok(reference) => set_name_item(py, slf, "/Parent", &reference)?,
            Err(error) if error.is_instance_of::<PyAttributeError>(py) => {
                pyutil::logger_warning(
                    py,
                    NON_MARKUP_SOURCE,
                    "Unregistered Parent object : No Parent field set",
                    None,
                )?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Definitions that stay on the Python side
// ---------------------------------------------------------------------------
//
// The class objects and their base classes are *definitions*, not logic: the
// MRO (`DictionaryObject` subclass, `ABC`), the keyword-only `__init__`
// signatures and the class-level type alias must exist on the Python side so
// `isinstance()` results, subclassing and pickling match upstream.  Each of them
// is listed here for gate A6; every `__init__` body lives in this file.
//
// @pypdf: pypdf.annotations._base.AnnotationDictionary
// @pypdf: pypdf.annotations._markup_annotations.Vertex
// @pypdf: pypdf.annotations._markup_annotations.MarkupAnnotation
// @pypdf: pypdf.annotations._markup_annotations.Text
// @pypdf: pypdf.annotations._markup_annotations.FreeText
// @pypdf: pypdf.annotations._markup_annotations.Line
// @pypdf: pypdf.annotations._markup_annotations.PolyLine
// @pypdf: pypdf.annotations._markup_annotations.Rectangle
// @pypdf: pypdf.annotations._markup_annotations.Highlight
// @pypdf: pypdf.annotations._markup_annotations.Ellipse
// @pypdf: pypdf.annotations._markup_annotations.Polygon
// @pypdf: pypdf.annotations._non_markup_annotations.Link
// @pypdf: pypdf.annotations._non_markup_annotations.Popup

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(
        annotationsmod_annotation_dictionary_init,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        annotationsmod_annotation_dictionary_flags_get,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        annotationsmod_annotation_dictionary_flags_set,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(annotationsmod_no_flags, module)?)?;
    module.add_function(wrap_pyfunction!(
        annotationsmod_get_bounding_rectangle,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        annotationsmod_markup_annotation_init,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(annotationsmod_text_init, module)?)?;
    module.add_function(wrap_pyfunction!(annotationsmod_free_text_init, module)?)?;
    module.add_function(wrap_pyfunction!(annotationsmod_line_init, module)?)?;
    module.add_function(wrap_pyfunction!(annotationsmod_polyline_init, module)?)?;
    module.add_function(wrap_pyfunction!(annotationsmod_rectangle_init, module)?)?;
    module.add_function(wrap_pyfunction!(annotationsmod_highlight_init, module)?)?;
    module.add_function(wrap_pyfunction!(annotationsmod_ellipse_init, module)?)?;
    module.add_function(wrap_pyfunction!(annotationsmod_polygon_init, module)?)?;
    module.add_function(wrap_pyfunction!(annotationsmod_link_init, module)?)?;
    module.add_function(wrap_pyfunction!(annotationsmod_popup_init, module)?)?;
    Ok(())
}
