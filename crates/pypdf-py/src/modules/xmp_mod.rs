//! Rust port of `pypdf/xmp.py` (upstream pypdf 7.1.1).
//!
//! # Design
//!
//! `XmpInformation` and `_XmpBuilder` are *Python classes* defined in the
//! hand-written shim `python/pypdf/xmp.py`, exactly like upstream: the MRO,
//! the `@property` decorators, the class attributes and the `ExpatBuilderNS`
//! base class are all preserved.  Every method body delegates to a
//! `#[pyfunction]` implemented here, so all logic lives in Rust.
//!
//! The XML DOM work is *not* reimplemented: it uses the very same
//! `xml.dom.minidom` / `xml.dom.expatbuilder` / `xml.parsers.expat` objects
//! through PyO3 (`Document`, `Element`, `ProcessingInstruction`, `NodeList`,
//! `ExpatBuilderNS`, `ParserCreate`), so the serialized XMP bytes are produced
//! by libexpat + minidom exactly like upstream (`toxml(encoding="utf-8")` per
//! top-level node, no XML declaration).
//!
//! Cross-object work is done by calling the Python methods (e.g.
//! `self._get_text(...)`, `desc.getElementsByTagNameNS(...)`) rather than by
//! reimplementing them, so subclassing/monkeypatching observations stay equal.
//! Namespace URIs, the prefix map, the `iso8601` regex and `_MINIMAL_XMP` are
//! defined here and re-exported into `pypdf.xmp`, and call sites read them back
//! from the shim module (like upstream reads its module globals), which keeps
//! monkeypatching behaviour identical.

use pyo3::exceptions::{PyAttributeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyInt, PyList, PyModule, PyString};

use crate::errors;
use crate::pyutil;

// ---------------------------------------------------------------------------
// Upstream module constants
// ---------------------------------------------------------------------------

// @pypdf: pypdf.xmp.XMP_MAX_INPUT_LENGTH
/// DEPRECATED: Use `pypdf.Configuration`.
pub const XMP_MAX_INPUT_LENGTH: i64 = 5_000_000;
// @pypdf: pypdf.xmp.XMP_MAX_ELEMENT_COUNT
/// DEPRECATED: Use `pypdf.Configuration`.
pub const XMP_MAX_ELEMENT_COUNT: i64 = 100_000;

// @pypdf: pypdf.xmp.RDF_NAMESPACE
pub const RDF_NAMESPACE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
// @pypdf: pypdf.xmp.DC_NAMESPACE
pub const DC_NAMESPACE: &str = "http://purl.org/dc/elements/1.1/";
// @pypdf: pypdf.xmp.XMP_NAMESPACE
pub const XMP_NAMESPACE: &str = "http://ns.adobe.com/xap/1.0/";
// @pypdf: pypdf.xmp.PDF_NAMESPACE
pub const PDF_NAMESPACE: &str = "http://ns.adobe.com/pdf/1.3/";
// @pypdf: pypdf.xmp.XMPMM_NAMESPACE
pub const XMPMM_NAMESPACE: &str = "http://ns.adobe.com/xap/1.0/mm/";
// @pypdf: pypdf.xmp.PDFX_NAMESPACE
pub const PDFX_NAMESPACE: &str = "http://ns.adobe.com/pdfx/1.3/";
// @pypdf: pypdf.xmp.PDFAID_NAMESPACE
pub const PDFAID_NAMESPACE: &str = "http://www.aiim.org/pdfa/ns/id/";

/// The compiled `iso8601` regular expression (upstream defines it with
/// `re.compile(..., re.VERBOSE)`); the pattern text is byte-identical.
// @pypdf: pypdf.xmp.iso8601
pub const ISO8601_PATTERN: &str = r"
        (?P<year>[0-9]{4})
        (-
            (?P<month>[0-9]{2})
            (-
                (?P<day>[0-9]+)
                (T
                    (?P<hour>[0-9]{2}):
                    (?P<minute>[0-9]{2})
                    (:(?P<second>[0-9]{2}(.[0-9]+)?))?
                    (?P<tzd>Z|[-+][0-9]{2}:[0-9]{2})
                )?
            )?
        )?
        ";

/// Minimal XMP template (upstream `_MINIMAL_XMP`).
// @pypdf: pypdf.xmp._MINIMAL_XMP
pub fn minimal_xmp() -> String {
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"pypdf\">
    <rdf:RDF xmlns:rdf=\"{RDF_NAMESPACE}\">
        <rdf:Description rdf:about=\"\"
            xmlns:dc=\"{DC_NAMESPACE}\"
            xmlns:xmp=\"{XMP_NAMESPACE}\"
            xmlns:pdf=\"{PDF_NAMESPACE}\"
            xmlns:xmpMM=\"{XMPMM_NAMESPACE}\"
            xmlns:pdfaid=\"{PDFAID_NAMESPACE}\"
            xmlns:pdfx=\"{PDFX_NAMESPACE}\">
        </rdf:Description>
    </rdf:RDF>
</x:xmpmeta>
<?xpacket end=\"r\"?>"
    )
}

// @pypdf: pypdf.xmp._NAMESPACE_PREFIX_MAP
fn namespace_prefix_map(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let mapping = PyDict::new(py);
    mapping.set_item(DC_NAMESPACE, "dc")?;
    mapping.set_item(XMP_NAMESPACE, "xmp")?;
    mapping.set_item(PDF_NAMESPACE, "pdf")?;
    mapping.set_item(XMPMM_NAMESPACE, "xmpMM")?;
    mapping.set_item(PDFAID_NAMESPACE, "pdfaid")?;
    mapping.set_item(PDFX_NAMESPACE, "pdfx")?;
    Ok(mapping)
}

// @pypdf: pypdf.xmp.K
fn type_var_k(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    py.import("typing")?.getattr("TypeVar")?.call1(("K",))
}

fn compiled_iso8601(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let re = py.import("re")?;
    let flags = re.getattr("VERBOSE")?;
    re.call_method1("compile", (ISO8601_PATTERN, flags))
}

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn shim<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("pypdf.xmp")
}

/// Fetch a name from this module's shim (upstream references these as globals).
fn shim_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim(py)?.getattr(name)
}

fn shim_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, name)
}

fn configuration<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf._configuration")?
        .getattr("get_configuration")?
        .call0()
}

fn generic_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("pypdf.generic")?.getattr(name)
}

fn py_bytes<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, data).into_any()
}

fn py_none<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    py.None().into_bound(py)
}

fn py_str<'py>(py: Python<'py>, value: &str) -> Bound<'py, PyAny> {
    PyString::new(py, value).into_any()
}

/// `isinstance(error, <module>.<name>)` for a raised `PyErr`.
fn err_is(py: Python<'_>, error: &PyErr, module: &str, name: &str) -> bool {
    match py.import(module).and_then(|module| module.getattr(name)) {
        Ok(class) => error.value(py).is_instance(&class).unwrap_or(false),
        Err(_) => false,
    }
}

/// `raise <Exception>(...) from exc`: sets `__cause__` (and therefore
/// `__suppress_context__`) plus `__context__`, exactly like CPython's
/// `raise ... from ...` inside an `except` block.
fn raise_from(py: Python<'_>, error: PyErr, cause: PyErr) -> PyErr {
    error.set_cause(py, Some(cause.clone_ref(py)));
    error.set_context(py, Some(cause));
    error
}

/// `raise XmpDocumentError("XMP Document is None")`.
fn xmp_document_error(py: Python<'_>) -> PyErr {
    errors::raise_msg(py, "XmpDocumentError", "XMP Document is None")
}

/// `str(exc)` of a raised exception.
fn error_message(py: Python<'_>, error: &PyErr) -> PyResult<String> {
    pyutil::py_str(error.value(py))
}

/// `int(value)` as a Python object (upstream passes the result straight on).
fn int_of<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    py.import("builtins")?.getattr("int")?.call1((value,))
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

/// `re.Match.group(name)`.
fn match_group<'py>(matches: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    matches.call_method1("group", (name,))
}

/// `matches.group(name) or default`.
fn match_group_or<'py>(
    py: Python<'py>,
    matches: &Bound<'py, PyAny>,
    name: &str,
    default: &str,
) -> PyResult<Bound<'py, PyAny>> {
    let value = match_group(matches, name)?;
    if value.is_truthy()? {
        Ok(value)
    } else {
        Ok(py_str(py, default))
    }
}

// ---------------------------------------------------------------------------
// Module level functions
// ---------------------------------------------------------------------------

// @pypdf: pypdf.xmp._identity
#[pyfunction]
fn xmpmod_identity(value: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(value.clone().unbind())
}

// @pypdf: pypdf.xmp._converter_date
#[pyfunction]
fn xmpmod_converter_date<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let iso8601 = shim_attr(py, "iso8601")?;
    let matches = iso8601.call_method1("match", (value,))?;
    if matches.is_none() {
        return Err(PyValueError::new_err(format!(
            "Invalid date format: {}",
            pyutil::py_str(value)?
        )));
    }
    let year = int_of(py, &match_group(&matches, "year")?)?;
    let month = int_of(py, &match_group_or(py, &matches, "month", "1")?)?;
    let day = int_of(py, &match_group_or(py, &matches, "day", "1")?)?;
    let hour = int_of(py, &match_group_or(py, &matches, "hour", "0")?)?;
    let minute = int_of(py, &match_group_or(py, &matches, "minute", "0")?)?;
    let second_text = match_group_or(py, &matches, "second", "0")?;

    let decimal = py.import("decimal")?;
    let second = decimal.getattr("Decimal")?.call1((&second_text,))?;
    let seconds_dec = second.call_method1("to_integral", (decimal.getattr("ROUND_FLOOR")?,))?;
    let million = PyInt::new(py, 1_000_000).into_any();
    let milliseconds_dec = second.sub(&seconds_dec)?.mul(&million)?;
    let seconds = int_of(py, &seconds_dec)?;
    let milliseconds = int_of(py, &milliseconds_dec)?;

    let datetime_module = py.import("datetime")?;
    let tzd = match_group_or(py, &matches, "tzd", "Z")?;
    let mut dt = match datetime_module.getattr("datetime")?.call1((
        &year,
        &month,
        &day,
        &hour,
        &minute,
        &seconds,
        &milliseconds,
    )) {
        Ok(dt) => dt,
        Err(error) if error.is_instance_of::<PyValueError>(py) => {
            let rendered = pyutil::py_repr(value)?;
            let detail = error_message(py, &error)?;
            let wrapped = PyValueError::new_err(format!("Invalid date {rendered}: {detail}"));
            return Err(raise_from(py, wrapped, error));
        }
        Err(error) => return Err(error),
    };

    if !tzd.eq("Z")? {
        let tzd_text = pyutil::py_str(&tzd)?;
        let (tzd_hours, tzd_minutes) = match tzd_text[1..].split_once(':') {
            Some((hours, minutes)) => (hours, minutes),
            // The regex guarantees ``[-+]HH:MM``; keep a graceful fallback.
            None => ("0", "0"),
        };
        let hours: i64 = tzd_hours.parse().unwrap_or(0);
        let minutes: i64 = tzd_minutes.parse().unwrap_or(0);
        let kwargs = PyDict::new(py);
        kwargs.set_item("hours", hours)?;
        kwargs.set_item("minutes", minutes)?;
        let offset = datetime_module
            .getattr("timedelta")?
            .call((), Some(&kwargs))?;
        // ``dt = dt - offset if tzd.startswith("+") else dt + offset``
        dt = if tzd_text.starts_with('+') {
            dt.sub(&offset)?
        } else {
            dt.add(&offset)?
        };
    }
    Ok(dt.unbind())
}

// @pypdf: pypdf.xmp._format_datetime_utc
#[pyfunction]
fn xmpmod_format_datetime_utc<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let mut value = value.clone();
    if !value.getattr("tzinfo")?.is_none() {
        let utcoffset = value.call_method0("utcoffset")?;
        if !utcoffset.is_none() {
            let utc = py.import("datetime")?.getattr("timezone")?.getattr("utc")?;
            value = value.call_method1("astimezone", (utc,))?;
        }
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("tzinfo", py.None())?;
    let value = value.call_method("replace", (), Some(&kwargs))?;
    Ok(value
        .call_method1("strftime", ("%Y-%m-%dT%H:%M:%S.%fZ",))?
        .unbind())
}

// @pypdf: pypdf.xmp._generic_get
#[pyfunction]
fn xmpmod_generic_get<'py>(
    py: Python<'py>,
    element: &Bound<'py, PyAny>,
    slf: &Bound<'py, PyAny>,
    list_type: &Bound<'py, PyAny>,
    converter: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let rdf_namespace = shim_attr(py, "RDF_NAMESPACE")?;
    let containers = element.call_method1("getElementsByTagNameNS", (rdf_namespace, list_type))?;
    let retval = PyList::empty(py);
    if containers.len()? != 0 {
        for container in containers.try_iter()? {
            let container = container?;
            let items = container.call_method1(
                "getElementsByTagNameNS",
                (shim_attr(py, "RDF_NAMESPACE")?, "li"),
            )?;
            for item in items.try_iter()? {
                let item = item?;
                let value = slf.call_method1("_get_text", (&item,))?;
                let value = converter.call1((value,))?;
                retval.append(value)?;
            }
        }
        return Ok(retval.into_any().unbind());
    }
    Ok(py.None())
}

// ---------------------------------------------------------------------------
// _XmpBuilder
// ---------------------------------------------------------------------------

fn expatbuilder_class<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    py.import("xml.dom.expatbuilder")?.getattr("ExpatBuilderNS")
}

// @pypdf: pypdf.xmp._XmpBuilder.__init__
#[pyfunction]
fn xmpmod_xmpbuilder_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    options: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let base = expatbuilder_class(py)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("options", options)?;
    base.getattr("__init__")?.call((slf,), Some(&kwargs))?;
    slf.setattr("_element_count", 0_i64)?;
    slf.setattr("_configuration", configuration(py)?)?;
    Ok(())
}

// @pypdf: pypdf.xmp._XmpBuilder.custom_entity_declaration_handler
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn xmpmod_xmpbuilder_custom_entity_declaration_handler(
    py: Python<'_>,
    _slf: &Bound<'_, PyAny>,
    entity_name: &Bound<'_, PyAny>,
    _is_parameter_entity: &Bound<'_, PyAny>,
    _value: &Bound<'_, PyAny>,
    _base: &Bound<'_, PyAny>,
    _system_id: &Bound<'_, PyAny>,
    _public_id: &Bound<'_, PyAny>,
    _notation_name: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let message = format!("Forbidden entities: {}", pyutil::py_repr(entity_name)?);
    let exception = py
        .import("xml.parsers.expat")?
        .getattr("ExpatError")?
        .call1((message,))?;
    Err(PyErr::from_value(exception))
}

// @pypdf: pypdf.xmp._XmpBuilder.start_element_handler
#[pyfunction]
fn xmpmod_xmpbuilder_start_element_handler(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
    attributes: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let element_count = slf.getattr("_element_count")?.extract::<i64>()? + 1;
    slf.setattr("_element_count", element_count)?;
    let maximum = slf
        .getattr("_configuration")?
        .getattr("xmp_maximum_element_count")?
        .extract::<i64>()?;
    if element_count > maximum {
        return Err(errors::raise_msg(
            py,
            "LimitReachedError",
            &format!("XMP metadata exceeds limit of {maximum} elements."),
        ));
    }
    let base = expatbuilder_class(py)?.getattr("start_element_handler")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("name", name)?;
    kwargs.set_item("attributes", attributes)?;
    base.call((slf,), Some(&kwargs))?;
    Ok(())
}

// @pypdf: pypdf.xmp._XmpBuilder.install
#[pyfunction]
fn xmpmod_xmpbuilder_install(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    parser: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let base = expatbuilder_class(py)?.getattr("install")?;
    base.call1((slf, parser))?;
    parser.setattr(
        "EntityDeclHandler",
        slf.getattr("custom_entity_declaration_handler")?,
    )?;
    parser.setattr("StartElementHandler", slf.getattr("start_element_handler")?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// XmpInformation
// ---------------------------------------------------------------------------

// @pypdf: pypdf.xmp.XmpInformation.__init__
#[pyfunction]
fn xmpmod_xmpinformation_init(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    stream: &Bound<'_, PyAny>,
) -> PyResult<()> {
    slf.setattr("stream", stream)?;
    let parsed = (|| -> PyResult<Bound<'_, PyAny>> {
        let data = stream.call_method0("get_data")?;
        let configuration = configuration(py)?;
        let maximum_input_length = configuration.getattr("xmp_maximum_input_length")?;
        let length = data.len()?;
        if PyInt::new(py, length as i64)
            .into_any()
            .gt(&maximum_input_length)?
        {
            return Err(errors::raise_msg(
                py,
                "LimitReachedError",
                &format!(
                    "XMP stream size {length} exceeds limit of {}.",
                    pyutil::py_str(&maximum_input_length)?
                ),
            ));
        }
        let builder = shim_class(py, "_XmpBuilder")?.call0()?;
        builder.call_method1("parseString", (data,))
    })();

    let doc_root = match parsed {
        Ok(doc_root) => doc_root,
        Err(error) => {
            // ``except (AttributeError, ExpatError) as exception``
            if error.is_instance_of::<PyAttributeError>(py)
                || err_is(py, &error, "xml.parsers.expat", "ExpatError")
            {
                let message = format!(
                    "XML in XmpInformation was invalid: {}",
                    error_message(py, &error)?
                );
                return Err(raise_from(py, errors::pdf_read_error(py, &message), error));
            }
            return Err(error);
        }
    };

    let rdf_roots = doc_root.call_method1(
        "getElementsByTagNameNS",
        (shim_attr(py, "RDF_NAMESPACE")?, "RDF"),
    )?;
    if rdf_roots.len()? == 0 {
        return Err(errors::pdf_read_error(
            py,
            "XML in XmpInformation was invalid: Missing rdf:RDF root element",
        ));
    }
    slf.setattr("rdf_root", rdf_roots.get_item(0)?)?;
    slf.setattr("cache", PyDict::new(py))?;
    Ok(())
}

// @pypdf: pypdf.xmp.XmpInformation.create
#[pyfunction]
fn xmpmod_xmpinformation_create(py: Python<'_>, cls: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let stream = generic_class(py, "ContentStream")?.call1((py.None(), py.None()))?;
    stream.call_method1("set_data", (py_bytes(py, minimal_xmp().as_bytes()),))?;
    Ok(cls.call1((stream,))?.unbind())
}

// @pypdf: pypdf.xmp.XmpInformation.write_to_stream
#[pyfunction]
fn xmpmod_xmpinformation_write_to_stream(
    py: Python<'_>,
    _slf: &Bound<'_, PyAny>,
    _stream: &Bound<'_, PyAny>,
) -> PyResult<()> {
    shim_attr(py, "deprecation_with_replacement")?.call1((
        "XmpInformation.write_to_stream",
        "PdfWriter.xmp_metadata",
        "6.0.0",
    ))?;
    Ok(())
}

/// Items yielded by `XmpInformation.get_element` (upstream is a generator).
// @pypdf: pypdf.xmp.XmpInformation.get_element
#[pyfunction]
fn xmpmod_xmpinformation_get_element_items<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    about_uri: &Bound<'py, PyAny>,
    namespace: &Bound<'py, PyAny>,
    name: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let rdf_namespace = shim_attr(py, "RDF_NAMESPACE")?;
    let rdf_root = slf.getattr("rdf_root")?;
    let descriptions = rdf_root.call_method1(
        "getElementsByTagNameNS",
        (rdf_namespace.clone(), "Description"),
    )?;
    let items = PyList::empty(py);
    for desc in descriptions.try_iter()? {
        let desc = desc?;
        let about = desc.call_method1("getAttributeNS", (rdf_namespace.clone(), "about"))?;
        if !about.eq(about_uri)? {
            continue;
        }
        let attr = desc.call_method1("getAttributeNodeNS", (namespace, name))?;
        if !attr.is_none() {
            items.append(attr)?;
        }
        let elements = desc.call_method1("getElementsByTagNameNS", (namespace, name))?;
        for element in elements.try_iter()? {
            items.append(element?)?;
        }
    }
    Ok(items.into_any().unbind())
}

/// Items yielded by `XmpInformation.get_nodes_in_namespace`.
// @pypdf: pypdf.xmp.XmpInformation.get_nodes_in_namespace
#[pyfunction]
fn xmpmod_xmpinformation_get_nodes_in_namespace_items<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    about_uri: &Bound<'py, PyAny>,
    namespace: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let rdf_namespace = shim_attr(py, "RDF_NAMESPACE")?;
    let rdf_root = slf.getattr("rdf_root")?;
    let descriptions = rdf_root.call_method1(
        "getElementsByTagNameNS",
        (rdf_namespace.clone(), "Description"),
    )?;
    let items = PyList::empty(py);
    for desc in descriptions.try_iter()? {
        let desc = desc?;
        let about = desc.call_method1("getAttributeNS", (rdf_namespace.clone(), "about"))?;
        if !about.eq(about_uri)? {
            continue;
        }
        let attributes = desc.getattr("attributes")?;
        let count = attributes.getattr("length")?.extract::<usize>()?;
        for index in 0..count {
            let attr = attributes.call_method1("item", (index,))?;
            if !attr.is_none() && attr.getattr("namespaceURI")?.eq(namespace)? {
                items.append(attr)?;
            }
        }
        for child in desc.getattr("childNodes")?.try_iter()? {
            let child = child?;
            if child.getattr("namespaceURI")?.eq(namespace)? {
                items.append(child)?;
            }
        }
    }
    Ok(items.into_any().unbind())
}

// @pypdf: pypdf.xmp.XmpInformation._get_text
#[pyfunction]
fn xmpmod_xmpinformation_get_text<'py>(
    py: Python<'py>,
    _slf: &Bound<'py, PyAny>,
    element: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let mut text = String::new();
    for child in element.getattr("childNodes")?.try_iter()? {
        let child = child?;
        if child.getattr("nodeType")?.eq(child.getattr("TEXT_NODE")?)? {
            text.push_str(&pyutil::py_str(&child.getattr("data")?)?);
        }
    }
    Ok(PyString::new(py, &text).into_any().unbind())
}

// @pypdf: pypdf.xmp.XmpInformation._get_single_value
#[pyfunction]
fn xmpmod_xmpinformation_get_single_value<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    namespace: &Bound<'py, PyAny>,
    name: &Bound<'py, PyAny>,
    converter: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let cache = slf.getattr("cache")?;
    let cached = cache
        .call_method1("get", (namespace, PyDict::new(py)))?
        .call_method1("get", (name,))?;
    if cached.is_truthy()? {
        return Ok(cached.unbind());
    }
    let mut value = py_none(py);
    // Upstream keeps only the first element (``for element in ...: ...; break``).
    if let Some(element) = slf
        .call_method1("get_element", ("", namespace, name))?
        .try_iter()?
        .next()
    {
        let element = element?;
        if element
            .getattr("nodeType")?
            .eq(element.getattr("ATTRIBUTE_NODE")?)?
        {
            value = element.getattr("nodeValue")?;
        } else {
            value = slf.call_method1("_get_text", (&element,))?;
        }
    }
    if !value.is_none() {
        value = converter.call1((value,))?;
    }
    let ns_cache = cache.call_method1("setdefault", (namespace, PyDict::new(py)))?;
    ns_cache.set_item(name, &value)?;
    Ok(value.unbind())
}

// @pypdf: pypdf.xmp.XmpInformation._getter_bag
#[pyfunction]
fn xmpmod_xmpinformation_getter_bag(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    namespace: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let cache = slf.getattr("cache")?;
    let cached = cache
        .call_method1("get", (namespace, PyDict::new(py)))?
        .call_method1("get", (name,))?;
    if cached.is_truthy()? {
        return Ok(cached.unbind());
    }
    let generic_get = shim_attr(py, "_generic_get")?;
    let retval = PyList::empty(py);
    for element in slf
        .call_method1("get_element", ("", namespace, name))?
        .try_iter()?
    {
        let element = element?;
        let bags = generic_get.call1((&element, slf, "Bag"))?;
        if bags.is_none() {
            let value = slf.call_method1("_get_text", (&element,))?;
            retval.append(value)?;
        } else {
            retval.call_method1("extend", (bags,))?;
        }
    }
    let ns_cache = cache.call_method1("setdefault", (namespace, PyDict::new(py)))?;
    ns_cache.set_item(name, &retval)?;
    Ok(retval.into_any().unbind())
}

// @pypdf: pypdf.xmp.XmpInformation._get_seq_values
#[pyfunction]
fn xmpmod_xmpinformation_get_seq_values<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    namespace: &Bound<'py, PyAny>,
    name: &Bound<'py, PyAny>,
    converter: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let cache = slf.getattr("cache")?;
    let cached = cache
        .call_method1("get", (namespace, PyDict::new(py)))?
        .call_method1("get", (name,))?;
    if cached.is_truthy()? {
        return Ok(cached.unbind());
    }
    let generic_get = shim_attr(py, "_generic_get")?;
    let retval = PyList::empty(py);
    for element in slf
        .call_method1("get_element", ("", namespace, name))?
        .try_iter()?
    {
        let element = element?;
        let seqs = generic_get.call1((&element, slf, "Seq", converter))?;
        if !seqs.is_none() {
            retval.call_method1("extend", (seqs,))?;
        } else {
            // See issue at https://github.com/py-pdf/pypdf/issues/3324: some
            // applications store `dc:creator` as an unordered array (bag).
            let bags = generic_get.call1((&element, slf, "Bag"))?;
            if !bags.is_none() {
                retval.call_method1("extend", (bags,))?;
            } else {
                let text = slf.call_method1("_get_text", (&element,))?;
                let value = converter.call1((text,))?;
                retval.append(value)?;
            }
        }
    }
    let ns_cache = cache.call_method1("setdefault", (namespace, PyDict::new(py)))?;
    ns_cache.set_item(name, &retval)?;
    Ok(retval.into_any().unbind())
}

// @pypdf: pypdf.xmp.XmpInformation._get_langalt_values
#[pyfunction]
fn xmpmod_xmpinformation_get_langalt_values(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    namespace: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let cache = slf.getattr("cache")?;
    let cached = cache
        .call_method1("get", (namespace, PyDict::new(py)))?
        .call_method1("get", (name,))?;
    if cached.is_truthy()? {
        return Ok(cached.unbind());
    }
    let rdf_namespace = shim_attr(py, "RDF_NAMESPACE")?;
    let retval = PyDict::new(py);
    for element in slf
        .call_method1("get_element", ("", namespace, name))?
        .try_iter()?
    {
        let element = element?;
        let alts =
            element.call_method1("getElementsByTagNameNS", (rdf_namespace.clone(), "Alt"))?;
        if alts.len()? != 0 {
            for alt in alts.try_iter()? {
                let alt = alt?;
                let items =
                    alt.call_method1("getElementsByTagNameNS", (rdf_namespace.clone(), "li"))?;
                for item in items.try_iter()? {
                    let item = item?;
                    let value = slf.call_method1("_get_text", (&item,))?;
                    let lang = item.call_method1("getAttribute", ("xml:lang",))?;
                    retval.set_item(lang, value)?;
                }
            }
        } else {
            retval.set_item("x-default", slf.call_method1("_get_text", (&element,))?)?;
        }
    }
    let ns_cache = cache.call_method1("setdefault", (namespace, PyDict::new(py)))?;
    ns_cache.set_item(name, &retval)?;
    Ok(retval.into_any().unbind())
}

// @pypdf: pypdf.xmp.XmpInformation.custom_properties
#[pyfunction]
fn xmpmod_xmpinformation_custom_properties(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    if !slf.hasattr("_custom_properties")? {
        let properties = PyDict::new(py);
        slf.setattr("_custom_properties", &properties)?;
        let hexdigits = shim_attr(py, "hexdigits")?;
        let nodes = slf.call_method1(
            "get_nodes_in_namespace",
            ("", shim_attr(py, "PDFX_NAMESPACE")?),
        )?;
        for node in nodes.try_iter()? {
            let node = node?;
            let mut key = node.getattr("localName")?;
            let mut start: i64 = 0;
            loop {
                // see documentation about PDFX_NAMESPACE in pypdf/xmp.py
                let marker = py_str(py, "\u{2182}");
                let index = key.call_method1("find", (&marker, start))?;
                let index = index.extract::<i64>()?;
                if index == -1 {
                    break;
                }
                let hex_id = key.get_item(py_slice(py, Some(index + 1), Some(index + 5))?)?;
                let mut valid = hex_id.len()? == 4;
                if valid {
                    for character in hex_id.try_iter()? {
                        if !hexdigits.contains(character?)? {
                            valid = false;
                            break;
                        }
                    }
                }
                if !valid {
                    // Not a well-formed escape; leave the marker untouched and
                    // continue past it instead of crashing.
                    start = index + 1;
                    continue;
                }
                let kwargs = PyDict::new(py);
                kwargs.set_item("base", 16)?;
                let code = py
                    .import("builtins")?
                    .getattr("int")?
                    .call((&hex_id,), Some(&kwargs))?;
                let replacement = py.import("builtins")?.getattr("chr")?.call1((code,))?;
                let head = key.get_item(py_slice(py, None, Some(index))?)?;
                let tail = key.get_item(py_slice(py, Some(index + 5), None)?)?;
                let joined = pyutil::py_add(&head, &replacement)?;
                key = pyutil::py_add(&joined, &tail)?;
                start = index + 1;
            }
            if node
                .getattr("nodeType")?
                .eq(node.getattr("ATTRIBUTE_NODE")?)?
            {
                properties.set_item(&key, node.getattr("nodeValue")?)?;
            } else {
                let value = slf.call_method1("_get_text", (&node,))?;
                properties.set_item(&key, value)?;
            }
        }
    }
    Ok(slf.getattr("_custom_properties")?.unbind())
}

// -- private helpers shared by the value setters ---------------------------

/// `self._clear_cache_entry(namespace, name)`.
fn call_clear_cache_entry(
    slf: &Bound<'_, PyAny>,
    namespace: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
) -> PyResult<()> {
    slf.call_method1("_clear_cache_entry", (namespace, name))?;
    Ok(())
}

/// `self._update_stream()`.
fn call_update_stream(slf: &Bound<'_, PyAny>) -> PyResult<()> {
    slf.call_method0("_update_stream")?;
    Ok(())
}

/// `desc.removeChild(elem)` for every element returned by
/// `desc.getElementsByTagNameNS(namespace, name)`.
fn remove_existing_elements(
    desc: &Bound<'_, PyAny>,
    namespace: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let existing = desc.call_method1("getElementsByTagNameNS", (namespace, name))?;
    for element in existing.try_iter()? {
        desc.call_method1("removeChild", (element?,))?;
    }
    Ok(())
}

/// `doc = self.rdf_root.ownerDocument`; `if doc is None: raise XmpDocumentError`.
fn owner_document<'py>(py: Python<'py>, slf: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let doc = slf.getattr("rdf_root")?.getattr("ownerDocument")?;
    if doc.is_none() {
        return Err(xmp_document_error(py));
    }
    Ok(doc)
}

/// `elem = doc.createElementNS(namespace, f"{prefix}:{name}")`.
fn create_element<'py>(
    py: Python<'py>,
    doc: &Bound<'py, PyAny>,
    namespace: &Bound<'py, PyAny>,
    name: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let prefix = slf_get_namespace_prefix(py, namespace)?;
    let name_text = pyutil::py_str(name)?;
    doc.call_method1(
        "createElementNS",
        (
            namespace,
            format!("{}:{name_text}", pyutil::py_str(&prefix)?),
        ),
    )
}

/// `self._get_namespace_prefix(namespace)` (module level helper for setters).
fn slf_get_namespace_prefix<'py>(
    py: Python<'py>,
    namespace: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, "_NAMESPACE_PREFIX_MAP")?.call_method1("get", (namespace, "unknown"))
}

// @pypdf: pypdf.xmp.XmpInformation._get_or_create_description
#[pyfunction]
fn xmpmod_xmpinformation_get_or_create_description<'py>(
    py: Python<'py>,
    slf: &Bound<'py, PyAny>,
    about_uri: &Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let rdf_namespace = shim_attr(py, "RDF_NAMESPACE")?;
    let rdf_root = slf.getattr("rdf_root")?;
    let descriptions = rdf_root.call_method1(
        "getElementsByTagNameNS",
        (rdf_namespace.clone(), "Description"),
    )?;
    for desc in descriptions.try_iter()? {
        let desc = desc?;
        let about = desc.call_method1("getAttributeNS", (rdf_namespace.clone(), "about"))?;
        if about.eq(about_uri)? {
            return Ok(desc.unbind());
        }
    }
    let doc = rdf_root.getattr("ownerDocument")?;
    if doc.is_none() {
        return Err(xmp_document_error(py));
    }
    let desc = doc.call_method1(
        "createElementNS",
        (rdf_namespace.clone(), "rdf:Description"),
    )?;
    desc.call_method1("setAttributeNS", (rdf_namespace, "rdf:about", about_uri))?;
    rdf_root.call_method1("appendChild", (&desc,))?;
    Ok(desc.unbind())
}

// @pypdf: pypdf.xmp.XmpInformation._clear_cache_entry
#[pyfunction]
fn xmpmod_xmpinformation_clear_cache_entry(
    _py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    namespace: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let cache = slf.getattr("cache")?;
    let ns_cache = cache.call_method1("get", (namespace,))?;
    if ns_cache.is_truthy()? && ns_cache.contains(name)? {
        ns_cache.del_item(name)?;
    }
    Ok(())
}

// @pypdf: pypdf.xmp.XmpInformation._set_single_value
#[pyfunction]
fn xmpmod_xmpinformation_set_single_value(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    namespace: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    call_clear_cache_entry(slf, namespace, name)?;
    let desc = slf.call_method1("_get_or_create_description", ("",))?;
    remove_existing_elements(&desc, namespace, name)?;
    let existing_attr = desc.call_method1("getAttributeNodeNS", (namespace, name))?;
    if !existing_attr.is_none() {
        desc.call_method1("removeAttributeNode", (existing_attr,))?;
    }
    if !value.is_none() {
        let doc = owner_document(py, slf)?;
        let elem = create_element(py, &doc, namespace, name)?;
        let text_node = doc.call_method1("createTextNode", (value.str()?,))?;
        elem.call_method1("appendChild", (&text_node,))?;
        desc.call_method1("appendChild", (&elem,))?;
    }
    call_update_stream(slf)
}

// @pypdf: pypdf.xmp.XmpInformation._set_bag_values
#[pyfunction]
fn xmpmod_xmpinformation_set_bag_values(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    namespace: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
    values: &Bound<'_, PyAny>,
) -> PyResult<()> {
    call_clear_cache_entry(slf, namespace, name)?;
    let desc = slf.call_method1("_get_or_create_description", ("",))?;
    remove_existing_elements(&desc, namespace, name)?;
    if values.is_truthy()? {
        let doc = owner_document(py, slf)?;
        let rdf_namespace = shim_attr(py, "RDF_NAMESPACE")?;
        let elem = create_element(py, &doc, namespace, name)?;
        let bag = doc.call_method1("createElementNS", (rdf_namespace.clone(), "rdf:Bag"))?;
        for value in values.try_iter()? {
            let value = value?;
            let li = doc.call_method1("createElementNS", (rdf_namespace.clone(), "rdf:li"))?;
            let text_node = doc.call_method1("createTextNode", (value.str()?,))?;
            li.call_method1("appendChild", (&text_node,))?;
            bag.call_method1("appendChild", (&li,))?;
        }
        elem.call_method1("appendChild", (&bag,))?;
        desc.call_method1("appendChild", (&elem,))?;
    }
    call_update_stream(slf)
}

// @pypdf: pypdf.xmp.XmpInformation._set_seq_values
#[pyfunction]
fn xmpmod_xmpinformation_set_seq_values(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    namespace: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
    values: &Bound<'_, PyAny>,
) -> PyResult<()> {
    call_clear_cache_entry(slf, namespace, name)?;
    let desc = slf.call_method1("_get_or_create_description", ("",))?;
    remove_existing_elements(&desc, namespace, name)?;
    if values.is_truthy()? {
        let doc = owner_document(py, slf)?;
        let rdf_namespace = shim_attr(py, "RDF_NAMESPACE")?;
        let elem = create_element(py, &doc, namespace, name)?;
        let seq = doc.call_method1("createElementNS", (rdf_namespace.clone(), "rdf:Seq"))?;
        for value in values.try_iter()? {
            let value = value?;
            let li = doc.call_method1("createElementNS", (rdf_namespace.clone(), "rdf:li"))?;
            let text_node = doc.call_method1("createTextNode", (value.str()?,))?;
            li.call_method1("appendChild", (&text_node,))?;
            seq.call_method1("appendChild", (&li,))?;
        }
        elem.call_method1("appendChild", (&seq,))?;
        desc.call_method1("appendChild", (&elem,))?;
    }
    call_update_stream(slf)
}

// @pypdf: pypdf.xmp.XmpInformation._set_langalt_values
#[pyfunction]
fn xmpmod_xmpinformation_set_langalt_values(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    namespace: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
    values: &Bound<'_, PyAny>,
) -> PyResult<()> {
    call_clear_cache_entry(slf, namespace, name)?;
    let desc = slf.call_method1("_get_or_create_description", ("",))?;
    remove_existing_elements(&desc, namespace, name)?;
    if values.is_truthy()? {
        let doc = owner_document(py, slf)?;
        let rdf_namespace = shim_attr(py, "RDF_NAMESPACE")?;
        let elem = create_element(py, &doc, namespace, name)?;
        let alt = doc.call_method1("createElementNS", (rdf_namespace.clone(), "rdf:Alt"))?;
        for item in values.call_method0("items")?.try_iter()? {
            let item = item?;
            let lang = item.get_item(0)?;
            let value = item.get_item(1)?;
            let li = doc.call_method1("createElementNS", (rdf_namespace.clone(), "rdf:li"))?;
            li.call_method1("setAttribute", ("xml:lang", lang))?;
            let text_node = doc.call_method1("createTextNode", (value.str()?,))?;
            li.call_method1("appendChild", (&text_node,))?;
            alt.call_method1("appendChild", (&li,))?;
        }
        elem.call_method1("appendChild", (&alt,))?;
        desc.call_method1("appendChild", (&elem,))?;
    }
    call_update_stream(slf)
}

// @pypdf: pypdf.xmp.XmpInformation._get_namespace_prefix
#[pyfunction]
fn xmpmod_xmpinformation_get_namespace_prefix(
    py: Python<'_>,
    _slf: &Bound<'_, PyAny>,
    namespace: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(slf_get_namespace_prefix(py, namespace)?.unbind())
}

// @pypdf: pypdf.xmp.XmpInformation._update_stream
#[pyfunction]
fn xmpmod_xmpinformation_update_stream(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<()> {
    let doc = owner_document(py, slf)?;
    let parts = PyList::empty(py);
    let processing_instruction = py
        .import("xml.dom.minidom")?
        .getattr("ProcessingInstruction")?;
    let re = py.import("re")?;
    let pattern = "end\\s*=\\s*(['\"])[rw]\\1\\s*";
    for node in doc.getattr("childNodes")?.try_iter()? {
        let node = node?;
        if node.is_instance(&processing_instruction)?
            && node.getattr("target")?.eq("xpacket")?
            && !re
                .call_method1("fullmatch", (pattern, node.getattr("data")?))?
                .is_none()
        {
            // No padding is written, so the packet cannot be updated in place.
            node.setattr("data", "end=\"r\"")?;
        }
        let kwargs = PyDict::new(py);
        kwargs.set_item("encoding", "utf-8")?;
        parts.append(node.call_method("toxml", (), Some(&kwargs))?)?;
    }
    let joined = py_bytes(py, b"").call_method1("join", (&parts,))?;
    slf.getattr("stream")?.call_method1("set_data", (joined,))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Properties (upstream bodies are one-line delegations; the pattern in
// `pypdf/xmp.py` decides getter/setter flavour, namespace and name)
// ---------------------------------------------------------------------------

macro_rules! single_property {
    ($getter:ident, $setter:ident, $namespace:literal, $name:literal) => {
        #[pyfunction]
        fn $getter(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
            let namespace = shim_attr(py, $namespace)?;
            Ok(slf
                .call_method1("_get_single_value", (namespace, $name))?
                .unbind())
        }

        #[pyfunction]
        fn $setter(
            py: Python<'_>,
            slf: &Bound<'_, PyAny>,
            value: &Bound<'_, PyAny>,
        ) -> PyResult<()> {
            let namespace = shim_attr(py, $namespace)?;
            slf.call_method1("_set_single_value", (namespace, $name, value))?;
            Ok(())
        }
    };
}

macro_rules! seq_property_with_converter {
    ($getter:ident, $namespace:literal, $name:literal) => {
        #[pyfunction]
        fn $getter(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
            let namespace = shim_attr(py, $namespace)?;
            let converter = shim_attr(py, "_converter_date")?;
            Ok(slf
                .call_method1("_get_seq_values", (namespace, $name, converter))?
                .unbind())
        }
    };
}

macro_rules! single_property_with_converter {
    ($getter:ident, $namespace:literal, $name:literal) => {
        #[pyfunction]
        fn $getter(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
            let namespace = shim_attr(py, $namespace)?;
            let converter = shim_attr(py, "_converter_date")?;
            Ok(slf
                .call_method1("_get_single_value", (namespace, $name, converter))?
                .unbind())
        }
    };
}

macro_rules! bag_property {
    ($getter:ident, $setter:ident, $namespace:literal, $name:literal) => {
        #[pyfunction]
        fn $getter(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
            let namespace = shim_attr(py, $namespace)?;
            Ok(slf
                .call_method1("_getter_bag", (namespace, $name))?
                .unbind())
        }

        #[pyfunction]
        fn $setter(
            py: Python<'_>,
            slf: &Bound<'_, PyAny>,
            values: &Bound<'_, PyAny>,
        ) -> PyResult<()> {
            let namespace = shim_attr(py, $namespace)?;
            slf.call_method1("_set_bag_values", (namespace, $name, values))?;
            Ok(())
        }
    };
}

macro_rules! seq_property {
    ($getter:ident, $setter:ident, $namespace:literal, $name:literal) => {
        #[pyfunction]
        fn $getter(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
            let namespace = shim_attr(py, $namespace)?;
            Ok(slf
                .call_method1("_get_seq_values", (namespace, $name))?
                .unbind())
        }

        #[pyfunction]
        fn $setter(
            py: Python<'_>,
            slf: &Bound<'_, PyAny>,
            values: &Bound<'_, PyAny>,
        ) -> PyResult<()> {
            let namespace = shim_attr(py, $namespace)?;
            slf.call_method1("_set_seq_values", (namespace, $name, values))?;
            Ok(())
        }
    };
}

macro_rules! langalt_property {
    ($getter:ident, $setter:ident, $namespace:literal, $name:literal) => {
        #[pyfunction]
        fn $getter(py: Python<'_>, slf: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
            let namespace = shim_attr(py, $namespace)?;
            Ok(slf
                .call_method1("_get_langalt_values", (namespace, $name))?
                .unbind())
        }

        #[pyfunction]
        fn $setter(
            py: Python<'_>,
            slf: &Bound<'_, PyAny>,
            values: &Bound<'_, PyAny>,
        ) -> PyResult<()> {
            let namespace = shim_attr(py, $namespace)?;
            slf.call_method1("_set_langalt_values", (namespace, $name, values))?;
            Ok(())
        }
    };
}

// ``dc_contributor``: Contributors to the resource (other than the authors).
// @pypdf: pypdf.xmp.XmpInformation.dc_contributor
// @pypdf: pypdf.xmp.XmpInformation.dc_contributor.setter
bag_property!(
    xmpmod_dc_contributor_get,
    xmpmod_dc_contributor_set,
    "DC_NAMESPACE",
    "contributor"
);

// ``dc_coverage``: Text describing the extent or scope of the resource.
// @pypdf: pypdf.xmp.XmpInformation.dc_coverage
// @pypdf: pypdf.xmp.XmpInformation.dc_coverage.setter
single_property!(
    xmpmod_dc_coverage_get,
    xmpmod_dc_coverage_set,
    "DC_NAMESPACE",
    "coverage"
);

// ``dc_creator``: A sorted array of names of the authors of the resource.
// @pypdf: pypdf.xmp.XmpInformation.dc_creator
// @pypdf: pypdf.xmp.XmpInformation.dc_creator.setter
seq_property!(
    xmpmod_dc_creator_get,
    xmpmod_dc_creator_set,
    "DC_NAMESPACE",
    "creator"
);

// ``dc_date``: A sorted array of dates of significance to the resource.
// @pypdf: pypdf.xmp.XmpInformation.dc_date
seq_property_with_converter!(xmpmod_dc_date_get, "DC_NAMESPACE", "date");

// @pypdf: pypdf.xmp.XmpInformation.dc_date.setter
#[pyfunction]
fn xmpmod_dc_date_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    values: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let namespace = shim_attr(py, "DC_NAMESPACE")?;
    if values.is_none() {
        slf.call_method1("_set_seq_values", (namespace, "date", py.None()))?;
        return Ok(());
    }
    let datetime_class = py.import("datetime")?.getattr("datetime")?;
    let format_datetime_utc = shim_attr(py, "_format_datetime_utc")?;
    let date_strings = PyList::empty(py);
    for value in values.try_iter()? {
        let value = value?;
        if value.is_instance(&datetime_class)? {
            date_strings.append(format_datetime_utc.call1((&value,))?)?;
        } else {
            date_strings.append(value.str()?)?;
        }
    }
    slf.call_method1("_set_seq_values", (namespace, "date", date_strings))?;
    Ok(())
}

// ``dc_description``: A language-keyed dictionary of textual descriptions.
// @pypdf: pypdf.xmp.XmpInformation.dc_description
// @pypdf: pypdf.xmp.XmpInformation.dc_description.setter
langalt_property!(
    xmpmod_dc_description_get,
    xmpmod_dc_description_set,
    "DC_NAMESPACE",
    "description"
);

// ``dc_format``: The mime-type of the resource.
// @pypdf: pypdf.xmp.XmpInformation.dc_format
// @pypdf: pypdf.xmp.XmpInformation.dc_format.setter
single_property!(
    xmpmod_dc_format_get,
    xmpmod_dc_format_set,
    "DC_NAMESPACE",
    "format"
);

// ``dc_identifier``: Unique identifier of the resource.
// @pypdf: pypdf.xmp.XmpInformation.dc_identifier
// @pypdf: pypdf.xmp.XmpInformation.dc_identifier.setter
single_property!(
    xmpmod_dc_identifier_get,
    xmpmod_dc_identifier_set,
    "DC_NAMESPACE",
    "identifier"
);

// ``dc_language``: An unordered array specifying the languages used.
// @pypdf: pypdf.xmp.XmpInformation.dc_language
// @pypdf: pypdf.xmp.XmpInformation.dc_language.setter
bag_property!(
    xmpmod_dc_language_get,
    xmpmod_dc_language_set,
    "DC_NAMESPACE",
    "language"
);

// ``dc_publisher``: An unordered array of publisher names.
// @pypdf: pypdf.xmp.XmpInformation.dc_publisher
// @pypdf: pypdf.xmp.XmpInformation.dc_publisher.setter
bag_property!(
    xmpmod_dc_publisher_get,
    xmpmod_dc_publisher_set,
    "DC_NAMESPACE",
    "publisher"
);

// ``dc_relation``: An unordered array of relationships to other documents.
// @pypdf: pypdf.xmp.XmpInformation.dc_relation
// @pypdf: pypdf.xmp.XmpInformation.dc_relation.setter
bag_property!(
    xmpmod_dc_relation_get,
    xmpmod_dc_relation_set,
    "DC_NAMESPACE",
    "relation"
);

// ``dc_rights``: A language-keyed dictionary of the rights the user has.
// @pypdf: pypdf.xmp.XmpInformation.dc_rights
// @pypdf: pypdf.xmp.XmpInformation.dc_rights.setter
langalt_property!(
    xmpmod_dc_rights_get,
    xmpmod_dc_rights_set,
    "DC_NAMESPACE",
    "rights"
);

// ``dc_source``: Unique identifier of the work this resource derives from.
// @pypdf: pypdf.xmp.XmpInformation.dc_source
// @pypdf: pypdf.xmp.XmpInformation.dc_source.setter
single_property!(
    xmpmod_dc_source_get,
    xmpmod_dc_source_set,
    "DC_NAMESPACE",
    "source"
);

// ``dc_subject``: An unordered array of descriptive phrases or keywords.
// @pypdf: pypdf.xmp.XmpInformation.dc_subject
// @pypdf: pypdf.xmp.XmpInformation.dc_subject.setter
bag_property!(
    xmpmod_dc_subject_get,
    xmpmod_dc_subject_set,
    "DC_NAMESPACE",
    "subject"
);

// ``dc_title``: A language-keyed dictionary of the title of the resource.
// @pypdf: pypdf.xmp.XmpInformation.dc_title
// @pypdf: pypdf.xmp.XmpInformation.dc_title.setter
langalt_property!(
    xmpmod_dc_title_get,
    xmpmod_dc_title_set,
    "DC_NAMESPACE",
    "title"
);

// ``dc_type``: An unordered array of textual descriptions of the document type.
// @pypdf: pypdf.xmp.XmpInformation.dc_type
// @pypdf: pypdf.xmp.XmpInformation.dc_type.setter
bag_property!(
    xmpmod_dc_type_get,
    xmpmod_dc_type_set,
    "DC_NAMESPACE",
    "type"
);

// ``pdf_keywords``: An unformatted text string representing document keywords.
// @pypdf: pypdf.xmp.XmpInformation.pdf_keywords
// @pypdf: pypdf.xmp.XmpInformation.pdf_keywords.setter
single_property!(
    xmpmod_pdf_keywords_get,
    xmpmod_pdf_keywords_set,
    "PDF_NAMESPACE",
    "Keywords"
);

// ``pdf_pdfversion``: The PDF file version, for example 1.0 or 1.3.
// @pypdf: pypdf.xmp.XmpInformation.pdf_pdfversion
// @pypdf: pypdf.xmp.XmpInformation.pdf_pdfversion.setter
single_property!(
    xmpmod_pdf_pdfversion_get,
    xmpmod_pdf_pdfversion_set,
    "PDF_NAMESPACE",
    "PDFVersion"
);

// ``pdf_producer``: The name of the tool that saved the document as a PDF.
// @pypdf: pypdf.xmp.XmpInformation.pdf_producer
// @pypdf: pypdf.xmp.XmpInformation.pdf_producer.setter
single_property!(
    xmpmod_pdf_producer_get,
    xmpmod_pdf_producer_set,
    "PDF_NAMESPACE",
    "Producer"
);

// ``xmp_create_date``: The date and time the resource was originally created.
// @pypdf: pypdf.xmp.XmpInformation.xmp_create_date
single_property_with_converter!(xmpmod_xmp_create_date_get, "XMP_NAMESPACE", "CreateDate");

// @pypdf: pypdf.xmp.XmpInformation.xmp_create_date.setter
#[pyfunction]
fn xmpmod_xmp_create_date_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    set_utc_date_value(py, slf, "XMP_NAMESPACE", "CreateDate", value)
}

// ``xmp_modify_date``: The date and time the resource was last modified.
// @pypdf: pypdf.xmp.XmpInformation.xmp_modify_date
single_property_with_converter!(xmpmod_xmp_modify_date_get, "XMP_NAMESPACE", "ModifyDate");

// @pypdf: pypdf.xmp.XmpInformation.xmp_modify_date.setter
#[pyfunction]
fn xmpmod_xmp_modify_date_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    set_utc_date_value(py, slf, "XMP_NAMESPACE", "ModifyDate", value)
}

// ``xmp_metadata_date``: The date and time any metadata was last changed.
// @pypdf: pypdf.xmp.XmpInformation.xmp_metadata_date
single_property_with_converter!(
    xmpmod_xmp_metadata_date_get,
    "XMP_NAMESPACE",
    "MetadataDate"
);

// @pypdf: pypdf.xmp.XmpInformation.xmp_metadata_date.setter
#[pyfunction]
fn xmpmod_xmp_metadata_date_set(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    set_utc_date_value(py, slf, "XMP_NAMESPACE", "MetadataDate", value)
}

/// Shared body of the `xmp_*_date` setters:
/// `_set_single_value(ns, name, _format_datetime_utc(value) if value else None)`.
fn set_utc_date_value(
    py: Python<'_>,
    slf: &Bound<'_, PyAny>,
    namespace: &str,
    name: &str,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let namespace = shim_attr(py, namespace)?;
    if value.is_truthy()? {
        let date_str = shim_attr(py, "_format_datetime_utc")?.call1((value,))?;
        slf.call_method1("_set_single_value", (namespace, name, date_str))?;
    } else {
        slf.call_method1("_set_single_value", (namespace, name, py.None()))?;
    }
    Ok(())
}

// ``xmp_creator_tool``: The name of the first known tool used to create the
// resource.
// @pypdf: pypdf.xmp.XmpInformation.xmp_creator_tool
// @pypdf: pypdf.xmp.XmpInformation.xmp_creator_tool.setter
single_property!(
    xmpmod_xmp_creator_tool_get,
    xmpmod_xmp_creator_tool_set,
    "XMP_NAMESPACE",
    "CreatorTool"
);

// ``xmpmm_document_id``: The common identifier for all versions.
// @pypdf: pypdf.xmp.XmpInformation.xmpmm_document_id
// @pypdf: pypdf.xmp.XmpInformation.xmpmm_document_id.setter
single_property!(
    xmpmod_xmpmm_document_id_get,
    xmpmod_xmpmm_document_id_set,
    "XMPMM_NAMESPACE",
    "DocumentID"
);

// ``xmpmm_instance_id``: An identifier for a specific incarnation of a document.
// @pypdf: pypdf.xmp.XmpInformation.xmpmm_instance_id
// @pypdf: pypdf.xmp.XmpInformation.xmpmm_instance_id.setter
single_property!(
    xmpmod_xmpmm_instance_id_get,
    xmpmod_xmpmm_instance_id_set,
    "XMPMM_NAMESPACE",
    "InstanceID"
);

// ``pdfaid_part``: The part of the PDF/A standard the document conforms to.
// @pypdf: pypdf.xmp.XmpInformation.pdfaid_part
// @pypdf: pypdf.xmp.XmpInformation.pdfaid_part.setter
single_property!(
    xmpmod_pdfaid_part_get,
    xmpmod_pdfaid_part_set,
    "PDFAID_NAMESPACE",
    "part"
);

// ``pdfaid_conformance``: The conformance level within the PDF/A standard.
// @pypdf: pypdf.xmp.XmpInformation.pdfaid_conformance
// @pypdf: pypdf.xmp.XmpInformation.pdfaid_conformance.setter
single_property!(
    xmpmod_pdfaid_conformance_get,
    xmpmod_pdfaid_conformance_set,
    "PDFAID_NAMESPACE",
    "conformance"
);

// ``custom_properties`` is implemented in `xmpmod_xmpinformation_custom_properties`.

// ---------------------------------------------------------------------------
// Definitions that stay on the Python side
// ---------------------------------------------------------------------------
//
// The class objects, their base classes and the `@property` decorators are
// *definitions*, not logic: they must exist on the Python side so the MRO,
// `isinstance()` results and the traced Python call graph match upstream.
// Each of them is listed here for gate A6; every method body lives in this file.
//
// @pypdf: pypdf.xmp.XmpInformation
// @pypdf: pypdf.xmp._XmpBuilder

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! add_functions {
    ($module:expr; $($function:ident),* $(,)?) => {
        $( $module.add_function(wrap_pyfunction!($function, $module)?)?; )*
    };
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = module.py();

    // Module constants (upstream defines these at module scope).
    module.add("xmpmod_XMP_MAX_INPUT_LENGTH", XMP_MAX_INPUT_LENGTH)?;
    module.add("xmpmod_XMP_MAX_ELEMENT_COUNT", XMP_MAX_ELEMENT_COUNT)?;
    module.add("xmpmod_RDF_NAMESPACE", RDF_NAMESPACE)?;
    module.add("xmpmod_DC_NAMESPACE", DC_NAMESPACE)?;
    module.add("xmpmod_XMP_NAMESPACE", XMP_NAMESPACE)?;
    module.add("xmpmod_PDF_NAMESPACE", PDF_NAMESPACE)?;
    module.add("xmpmod_XMPMM_NAMESPACE", XMPMM_NAMESPACE)?;
    module.add("xmpmod_PDFX_NAMESPACE", PDFX_NAMESPACE)?;
    module.add("xmpmod_PDFAID_NAMESPACE", PDFAID_NAMESPACE)?;
    module.add("xmpmod_iso8601", compiled_iso8601(py)?)?;
    module.add("xmpmod_K", type_var_k(py)?)?;
    module.add("xmpmod_MINIMAL_XMP", minimal_xmp())?;
    module.add("xmpmod_NAMESPACE_PREFIX_MAP", namespace_prefix_map(py)?)?;

    add_functions!(
        module;
        xmpmod_identity,
        xmpmod_converter_date,
        xmpmod_format_datetime_utc,
        xmpmod_generic_get,
        xmpmod_xmpbuilder_init,
        xmpmod_xmpbuilder_custom_entity_declaration_handler,
        xmpmod_xmpbuilder_start_element_handler,
        xmpmod_xmpbuilder_install,
        xmpmod_xmpinformation_init,
        xmpmod_xmpinformation_create,
        xmpmod_xmpinformation_write_to_stream,
        xmpmod_xmpinformation_get_element_items,
        xmpmod_xmpinformation_get_nodes_in_namespace_items,
        xmpmod_xmpinformation_get_text,
        xmpmod_xmpinformation_get_single_value,
        xmpmod_xmpinformation_getter_bag,
        xmpmod_xmpinformation_get_seq_values,
        xmpmod_xmpinformation_get_langalt_values,
        xmpmod_xmpinformation_custom_properties,
        xmpmod_xmpinformation_get_or_create_description,
        xmpmod_xmpinformation_clear_cache_entry,
        xmpmod_xmpinformation_set_single_value,
        xmpmod_xmpinformation_set_bag_values,
        xmpmod_xmpinformation_set_seq_values,
        xmpmod_xmpinformation_set_langalt_values,
        xmpmod_xmpinformation_get_namespace_prefix,
        xmpmod_xmpinformation_update_stream,
        xmpmod_dc_contributor_get,
        xmpmod_dc_contributor_set,
        xmpmod_dc_coverage_get,
        xmpmod_dc_coverage_set,
        xmpmod_dc_creator_get,
        xmpmod_dc_creator_set,
        xmpmod_dc_date_get,
        xmpmod_dc_date_set,
        xmpmod_dc_description_get,
        xmpmod_dc_description_set,
        xmpmod_dc_format_get,
        xmpmod_dc_format_set,
        xmpmod_dc_identifier_get,
        xmpmod_dc_identifier_set,
        xmpmod_dc_language_get,
        xmpmod_dc_language_set,
        xmpmod_dc_publisher_get,
        xmpmod_dc_publisher_set,
        xmpmod_dc_relation_get,
        xmpmod_dc_relation_set,
        xmpmod_dc_rights_get,
        xmpmod_dc_rights_set,
        xmpmod_dc_source_get,
        xmpmod_dc_source_set,
        xmpmod_dc_subject_get,
        xmpmod_dc_subject_set,
        xmpmod_dc_title_get,
        xmpmod_dc_title_set,
        xmpmod_dc_type_get,
        xmpmod_dc_type_set,
        xmpmod_pdf_keywords_get,
        xmpmod_pdf_keywords_set,
        xmpmod_pdf_pdfversion_get,
        xmpmod_pdf_pdfversion_set,
        xmpmod_pdf_producer_get,
        xmpmod_pdf_producer_set,
        xmpmod_xmp_create_date_get,
        xmpmod_xmp_create_date_set,
        xmpmod_xmp_modify_date_get,
        xmpmod_xmp_modify_date_set,
        xmpmod_xmp_metadata_date_get,
        xmpmod_xmp_metadata_date_set,
        xmpmod_xmp_creator_tool_get,
        xmpmod_xmp_creator_tool_set,
        xmpmod_xmpmm_document_id_get,
        xmpmod_xmpmm_document_id_set,
        xmpmod_xmpmm_instance_id_get,
        xmpmod_xmpmm_instance_id_set,
        xmpmod_pdfaid_part_get,
        xmpmod_pdfaid_part_set,
        xmpmod_pdfaid_conformance_get,
        xmpmod_pdfaid_conformance_set,
    );

    Ok(())
}
