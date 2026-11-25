//! Rust port of `pypdf/generic/_appearance_stream.py` (upstream pypdf 7.1.1).
//!
//! # Design: Python classes, Rust logic
//!
//! The class skeletons (`BaseStreamConfig`, `BaseStreamAppearance`,
//! `TextAlignment`, `WidthWordGlyphs`, `TextStreamAppearance`) stay verbatim in
//! the shim `python/pypdf/generic/_appearance_stream.py` because they are
//! observable Python API (dataclass defaults, `IntEnum` identity, named-tuple
//! unpacking, inheritance from the Python `DecodedStreamObject`, `mock.patch`
//! targets).  Every method body delegates to a `#[pyfunction]` implemented
//! here; no algorithm lives in the shim.
//!
//! # Byte-exact content streams
//!
//! The generated appearance stream bytes are compared against the oracle
//! byte-for-byte, so every value that is formatted into the stream goes through
//! Python's own `str()`/`round()`/arithmetic rather than Rust formatting:
//! upstream's f-strings depend on whether an operand is an `int`, a `float` or
//! a `NumberObject`/`FloatObject` subclass (`12` vs `12.0`), and `round(x, 3)`
//! returns an int for int input and a float otherwise.  Mixed Python operands
//! are therefore evaluated through `PyAny`'s numeric protocol, never by
//! extracting to Rust numerics first.
//!
//! # Optional-dependency parity (`HAS_RTL_SUPPORT`)
//!
//! `arabic_reshaper` and `python-bidi` are installed in both the dev and the
//! oracle environments (`docs/FIDELITY_CONTRACT.md` §5); exactly like upstream,
//! the port calls them through Python (`arabic_reshaper.reshape` then
//! `bidi.algorithm.get_display(..., base_dir="L")`) instead of reimplementing
//! the shaping algorithm.  `HAS_RTL_SUPPORT` is re-read from the shim module on
//! every generation, so `mock.patch("pypdf.generic._appearance_stream.
//! HAS_RTL_SUPPORT", False)` observes the same behaviour as upstream (the test
//! suite patches it).  The detection semantics themselves (`ImportError` →
//! `False`) live verbatim in the shim.
//!
//! # Determinism
//!
//! Every mapping that reaches the output is a Python `dict`/`list` created and
//! iterated in upstream order; no Rust `HashMap`/`HashSet` participates.  The
//! one upstream set iteration (`test_encodings` in
//! `_find_annotation_font_resource`) is a Python `set` iterated through Python,
//! so its order is the same as upstream for the same `PYTHONHASHSEED`.
//!
//! # No panics
//!
//! All failures are propagated as `PyResult`; there is no `unwrap`/`expect` on
//! values that can be influenced from Python.  The only `expect`s are on
//! options that are proven set together with `has_rtl` at construction time.

use pyo3::prelude::*;
use pyo3::types::{
    PyAny, PyBytes, PyDict, PyFloat, PyInt, PyList, PyModule, PySet, PyString, PyTuple,
};

use crate::pyutil;

/// Upstream module name; used as `logger_warning(..., source=__name__)`.
const SOURCE: &str = "pypdf.generic._appearance_stream";

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn shim_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    crate::pyutil::cached_module(py, "pypdf.generic._appearance_stream")
}

/// Fetch a name from the shim module.
///
/// Upstream's methods read module globals (`Color`, `Font`, `DeviceGray`,
/// `HAS_RTL_SUPPORT`, `TEXT_SPACE_TO_GLYPH_SPACE_FACTOR`, the constants
/// classes, ...) and the official tests monkeypatch some of them, so they are
/// resolved by name on every call instead of being cached in Rust.
fn shim_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    shim_module(py)?.getattr(name)
}

fn builtin<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    crate::pyutil::cached_module(py, "builtins")?.getattr(name)
}

fn none<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    py.None().into_bound(py)
}

fn py_int<'py>(py: Python<'py>, value: i64) -> Bound<'py, PyAny> {
    PyInt::new(py, value).into_any()
}

fn py_float<'py>(py: Python<'py>, value: f64) -> Bound<'py, PyAny> {
    PyFloat::new(py, value).into_any()
}

fn py_str<'py>(py: Python<'py>, value: &str) -> Bound<'py, PyAny> {
    PyString::new(py, value).into_any()
}

fn py_bytes<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, data).into_any()
}

/// `str(obj)` – identical to an f-string's `{obj}` for the values formatted
/// into an appearance stream (floats/ints/strings and their pypdf subclasses).
fn py_string(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    pyutil::py_str(obj)
}

fn truthy(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    obj.is_truthy()
}

/// `max(a, b)` through Python (mixed int/float/PdfObject operands matter).
fn py_max2<'py>(
    py: Python<'py>,
    a: &Bound<'py, PyAny>,
    b: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    builtin(py, "max")?.call1((a, b))
}

/// `min(*values)` through Python.
fn py_min<'py>(py: Python<'py>, values: Vec<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
    builtin(py, "min")?.call(PyTuple::new(py, values)?, None)
}

/// `round(value, ndigits)` through Python (int stays int, float stays float).
fn py_round<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
    ndigits: i64,
) -> PyResult<Bound<'py, PyAny>> {
    builtin(py, "round")?.call1((value, py_int(py, ndigits)))
}

/// `getattr(obj, name, default)`.
fn getattr_or<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    name: &str,
    default: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    builtin(py, "getattr")?.call1((obj, py_str(py, name), default.clone()))
}

fn name_object<'py>(py: Python<'py>, value: &str) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, "NameObject")?.call1((py_str(py, value),))
}

fn name_object_of<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, "NameObject")?.call1((value,))
}

/// `DictionaryObject(mapping)` – a Python `dict` subclass whose `dict.__init__`
/// performs the insertion, exactly like upstream's `DictionaryObject({...})`.
fn dictionary_object<'py>(
    py: Python<'py>,
    mapping: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, "DictionaryObject")?.call1((mapping,))
}

fn array_object<'py>(py: Python<'py>, items: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, "ArrayObject")?.call1((items,))
}

/// `WidthWordGlyphs(width=..., word=..., glyphs=...)` (upstream passes keywords).
fn width_word_glyphs<'py>(
    py: Python<'py>,
    width: &Bound<'py, PyAny>,
    word: &Bound<'py, PyAny>,
    glyphs: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("width", width)?;
    kwargs.set_item("word", word)?;
    kwargs.set_item("glyphs", glyphs)?;
    shim_attr(py, "WidthWordGlyphs")?.call((), Some(&kwargs))
}

/// `super(<class_name>, self).__init__(*args)` – keeps the dynamic MRO
/// resolution upstream's zero-argument `super()` gets (subclasses, mixins).
fn call_super_init<'py>(
    py: Python<'py>,
    class_name: &str,
    self_: &Bound<'py, PyAny>,
    args: Vec<Bound<'py, PyAny>>,
) -> PyResult<()> {
    let class = shim_attr(py, class_name)?;
    let super_object = builtin(py, "super")?.call1((class, self_.clone()))?;
    let init = super_object.getattr("__init__")?;
    init.call(PyTuple::new(py, args)?, None)?;
    Ok(())
}

/// `bytes` payload of a Python `bytes` object.
fn bytes_of(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    Ok(obj.cast::<PyBytes>()?.as_bytes().to_vec())
}

// ---------------------------------------------------------------------------
// BaseStreamAppearance
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._appearance_stream.BaseStreamAppearance._add_matrix
#[pyfunction]
fn appearance_base_stream_appearance_add_matrix<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
    rotation: &Bound<'py, PyAny>,
) -> PyResult<()> {
    // matrix = Transformation().rotate(rotation)
    let matrix = shim_attr(py, "Transformation")?
        .call0()?
        .call_method1("rotate", (rotation,))?;
    let rectangle = self_.getattr("_layout")?.getattr("rectangle")?;
    let width = rectangle.getattr("width")?;
    let height = rectangle.getattr("height")?;
    let zero = py_float(py, 0.0);
    // bottom_right_corner = (width, 0.0); top_left_corner = (0.0, height);
    // top_right_corner = (width, height)
    let bottom_right_corner = PyTuple::new(py, [width.clone(), zero.clone()])?.into_any();
    let top_left_corner = PyTuple::new(py, [zero.clone(), height.clone()])?.into_any();
    let top_right_corner = PyTuple::new(py, [width, height])?.into_any();
    let rotated_bottom_right_corner = matrix.call_method1("apply_on", (bottom_right_corner,))?;
    let rotated_top_left_corner = matrix.call_method1("apply_on", (top_left_corner,))?;
    let rotated_top_right_corner = matrix.call_method1("apply_on", (top_right_corner,))?;
    // translation_x_offset = -min(0.0, rb[0], tl[0], tr[0])
    let translation_x_offset = py_min(
        py,
        vec![
            zero.clone(),
            rotated_bottom_right_corner.get_item(0)?,
            rotated_top_left_corner.get_item(0)?,
            rotated_top_right_corner.get_item(0)?,
        ],
    )?
    .neg()?;
    // translation_y_offset = -min(0.0, rb[1], tl[1], tr[1])
    let translation_y_offset = py_min(
        py,
        vec![
            zero,
            rotated_bottom_right_corner.get_item(1)?,
            rotated_top_left_corner.get_item(1)?,
            rotated_top_right_corner.get_item(1)?,
        ],
    )?
    .neg()?;
    let matrix = matrix.call_method1("translate", (translation_x_offset, translation_y_offset))?;
    // ArrayObject([FloatObject(round(i, 3)) for i in matrix.ctm])
    let rounded = PyList::empty(py);
    let float_class = shim_attr(py, "FloatObject")?;
    for value in matrix.getattr("ctm")?.try_iter()? {
        let value = value?;
        rounded.append(float_class.call1((py_round(py, &value, 3)?,))?)?;
    }
    self_.set_item(
        name_object(py, "/Matrix")?,
        array_object(py, &rounded.into_any())?,
    )?;
    Ok(())
}

// @pypdf: pypdf.generic._appearance_stream.BaseStreamAppearance.__init__
#[pyfunction]
fn appearance_base_stream_appearance_init<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
    layout: &Bound<'py, PyAny>,
) -> PyResult<()> {
    // super().__init__()
    call_super_init(py, "BaseStreamAppearance", self_, Vec::new())?;
    // self._layout = layout or BaseStreamConfig()
    let layout = if truthy(layout)? {
        layout.clone()
    } else {
        shim_attr(py, "BaseStreamConfig")?.call0()?
    };
    self_.setattr("_layout", &layout)?;
    self_.set_item(name_object(py, "/Type")?, name_object(py, "/XObject")?)?;
    self_.set_item(name_object(py, "/Subtype")?, name_object(py, "/Form")?)?;
    let rectangle = layout.getattr("rectangle")?;
    self_.set_item(name_object(py, "/BBox")?, rectangle.clone())?;

    // rotation = self._layout.rotation % 360
    let rotation = layout.getattr("rotation")?.rem(py_int(py, 360))?;
    if truthy(&rotation)? {
        self_.call_method1("_add_matrix", (rotation,))?;
    }

    let mut ap_stream_parts: Vec<String> = Vec::new();
    let color_class = shim_attr(py, "Color")?;
    let background_color = layout.getattr("background_color")?;
    if background_color.is_instance(&color_class)? {
        let width = rectangle.getattr("width")?;
        let height = rectangle.getattr("height")?;
        let operator = background_color.call_method0("as_operator")?;
        ap_stream_parts.push(format!(
            "0 0 {} {} re\n{}\nf\n",
            py_string(&width)?,
            py_string(&height)?,
            py_string(&operator)?,
        ));
    }
    let border_width = layout.getattr("border_width")?;
    if border_width.gt(py_int(py, 0))? {
        let border_color = layout.getattr("border_color")?;
        if border_color.is_instance(&color_class)? {
            let border_width_string = if border_width.ne(py_int(py, 1))? {
                format!("{} w\n", py_string(&border_width)?)
            } else {
                String::new()
            };
            let width = rectangle.getattr("width")?;
            let height = rectangle.getattr("height")?;
            // width - 2 * border_width / height - 2 * border_width
            let inner_width = width.sub(py_int(py, 2).mul(&border_width)?)?;
            let inner_height = height.sub(py_int(py, 2).mul(&border_width)?)?;
            let stroke_kwargs = PyDict::new(py);
            stroke_kwargs.set_item("stroke", true)?;
            let stroke_operator =
                border_color.call_method("as_operator", (), Some(&stroke_kwargs))?;
            ap_stream_parts.push(format!(
                "{} {} {} {} re\n{}{}\ns\n",
                py_string(&border_width)?,
                py_string(&border_width)?,
                py_string(&inner_width)?,
                py_string(&inner_height)?,
                border_width_string,
                py_string(&stroke_operator)?,
            ));
        } else {
            // If we aren't drawing a border, then set border_width to 0.
            layout.setattr("border_width", py_int(py, 0))?;
        }
    }
    if !ap_stream_parts.is_empty() {
        ap_stream_parts.insert(0, "q\n".to_string());
        ap_stream_parts.push("Q\n".to_string());
    }
    let joined = ap_stream_parts.concat();
    let encoded = py_str(py, &joined).call_method0("encode")?;
    self_.setattr("_ap_stream_data", encoded)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// TextStreamAppearance._scale_text
// ---------------------------------------------------------------------------

/// Append the "current line" `WidthWordGlyphs` to `wrapped_lines`.
fn push_wrapped_line<'py>(
    py: Python<'py>,
    wrapped_lines: &Bound<'py, PyList>,
    font: &Bound<'py, PyAny>,
    current_line_words: &Bound<'py, PyList>,
    current_line_width: &Bound<'py, PyAny>,
) -> PyResult<()> {
    // " ".join(map(attrgetter("word"), current_line_words))
    let words = PyList::empty(py);
    let glyphs = PyList::empty(py);
    for word_glyphs in current_line_words.try_iter()? {
        let word_glyphs = word_glyphs?;
        words.append(word_glyphs.getattr("word")?)?;
        glyphs.append(word_glyphs.getattr("glyphs")?)?;
    }
    let word = py_str(py, " ").call_method1("join", (words,))?;
    // font.space_char.join(map(attrgetter("glyphs"), current_line_words))
    let glyph_text = font
        .getattr("space_char")?
        .call_method1("join", (glyphs,))?;
    wrapped_lines.append(width_word_glyphs(
        py,
        current_line_width,
        &word,
        &glyph_text,
    )?)?;
    Ok(())
}

// @pypdf: pypdf.generic._appearance_stream.TextStreamAppearance._scale_text
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn appearance_text_stream_appearance_scale_text<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
    font: &Bound<'py, PyAny>,
    font_size: &Bound<'py, PyAny>,
    leading_factor: &Bound<'py, PyAny>,
    field_width: &Bound<'py, PyAny>,
    field_height: &Bound<'py, PyAny>,
    paragraphs: &Bound<'py, PyAny>,
    min_font_size: &Bound<'py, PyAny>,
    font_size_step: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let text_space_factor = shim_attr(py, "TEXT_SPACE_TO_GLYPH_SPACE_FACTOR")?;
    let wrapped_lines = PyList::empty(py);
    let mut current_line_words = PyList::empty(py);
    let mut current_line_width = py_int(py, 0);
    // space_width = font.space_width * font_size / TEXT_SPACE_TO_GLYPH_SPACE_FACTOR
    let space_width = font
        .getattr("space_width")?
        .mul(font_size)?
        .div(&text_space_factor)?;
    for paragraph in paragraphs.try_iter()? {
        let paragraph = paragraph?;
        for (index, item) in paragraph.try_iter()?.enumerate() {
            let word_glyphs = item?;
            // word_width = width_word_glyphs.width * font_size
            let word_width = word_glyphs.getattr("width")?.mul(font_size)?;
            // test_width = current_line_width + word_width + (space_width if i else 0)
            let extra = if index != 0 {
                space_width.clone()
            } else {
                py_int(py, 0)
            };
            let test_width = current_line_width.add(&word_width)?.add(&extra)?;
            if test_width.gt(field_width)? && !current_line_words.is_empty() {
                push_wrapped_line(
                    py,
                    &wrapped_lines,
                    font,
                    &current_line_words,
                    &current_line_width,
                )?;
                current_line_words = PyList::empty(py);
                current_line_words.append(word_glyphs)?;
                current_line_width = word_width;
            } else if current_line_words.is_empty() && word_width.gt(field_width)? {
                wrapped_lines.append(width_word_glyphs(
                    py,
                    &word_width,
                    &word_glyphs.getattr("word")?,
                    &word_glyphs.getattr("glyphs")?,
                )?)?;
                current_line_words = PyList::empty(py);
                current_line_width = py_int(py, 0);
            } else {
                if !current_line_words.is_empty() {
                    current_line_width = current_line_width.add(&space_width)?;
                }
                current_line_words.append(word_glyphs)?;
                current_line_width = current_line_width.add(&word_width)?;
            }
        }
        if !current_line_words.is_empty() {
            push_wrapped_line(
                py,
                &wrapped_lines,
                font,
                &current_line_words,
                &current_line_width,
            )?;
            current_line_words = PyList::empty(py);
            current_line_width = py_int(py, 0);
        }
    }
    // estimated_total_height = font_size + (len(wrapped_lines) - 1) * leading_factor * font_size
    let estimated_total_height = font_size.add(
        &py_int(py, wrapped_lines.len() as i64 - 1)
            .mul(leading_factor)?
            .mul(font_size)?,
    )?;
    if estimated_total_height.gt(field_height)? {
        // Text overflows height; Retry with smaller font size.
        let new_font_size = font_size.sub(font_size_step)?;
        if new_font_size.ge(min_font_size)? {
            return self_.call_method1(
                "_scale_text",
                (
                    font,
                    new_font_size,
                    leading_factor,
                    field_width,
                    field_height,
                    paragraphs,
                    min_font_size,
                    font_size_step,
                ),
            );
        }
    }
    Ok(PyTuple::new(py, [wrapped_lines.into_any(), py_round(py, font_size, 1)?])?.into_any())
}

// ---------------------------------------------------------------------------
// TextStreamAppearance._generate_appearance_stream_data
// ---------------------------------------------------------------------------

/// The two typographic maps plus the (dynamically read) RTL imports, used by the
/// `_unicode_to_glyph_id` / `_glyph_id_to_bytes` closures of
/// `_generate_appearance_stream_data`.
struct GlyphMaps<'py> {
    py: Python<'py>,
    reverse_cmap: Bound<'py, PyAny>,
    encoding_cmap: Bound<'py, PyAny>,
    has_rtl: bool,
    arabic_reshaper: Option<Bound<'py, PyAny>>,
    get_display: Option<Bound<'py, PyAny>>,
}

impl<'py> GlyphMaps<'py> {
    fn new(
        py: Python<'py>,
        reverse_cmap: Bound<'py, PyAny>,
        encoding_cmap: Bound<'py, PyAny>,
    ) -> PyResult<Self> {
        // Upstream reads the module-level `HAS_RTL_SUPPORT` inside the closure;
        // it is read once per generation here (no instrumented difference) so
        // that `mock.patch("...HAS_RTL_SUPPORT", False)` is observed.
        let has_rtl = truthy(&shim_attr(py, "HAS_RTL_SUPPORT")?)?;
        let (arabic_reshaper, get_display) = if has_rtl {
            (
                Some(shim_attr(py, "arabic_reshaper")?),
                Some(shim_attr(py, "get_display")?),
            )
        } else {
            (None, None)
        };
        Ok(Self {
            py,
            reverse_cmap,
            encoding_cmap,
            has_rtl,
            arabic_reshaper,
            get_display,
        })
    }

    /// `_unicode_to_glyph_id(text, reverse_cmap)`.
    fn unicode_to_glyph_id(&self, text: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let py = self.py;
        let mapped = if self.has_rtl {
            // These are Some whenever has_rtl is true (set together in `new`).
            let arabic_reshaper = self
                .arabic_reshaper
                .as_ref()
                .expect("HAS_RTL_SUPPORT implies arabic_reshaper was imported");
            let get_display = self
                .get_display
                .as_ref()
                .expect("HAS_RTL_SUPPORT implies get_display was imported");
            let reshaped_text = arabic_reshaper.call_method1("reshape", (text,))?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("base_dir", "L")?;
            get_display.call((reshaped_text,), Some(&kwargs))?
        } else {
            text.clone()
        };
        // "".join(reverse_cmap.get(char, char) for char in mapped)
        let characters = PyList::empty(py);
        for character in mapped.try_iter()? {
            let character = character?;
            characters.append(
                self.reverse_cmap
                    .call_method1("get", (&character, &character))?,
            )?;
        }
        py_str(py, "").call_method1("join", (characters,))
    }

    /// `_glyph_id_to_bytes(glyphs, encoding_cmap)`.
    fn glyph_id_to_bytes(&self, glyphs: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let py = self.py;
        let result = PyList::empty(py);
        let ord = builtin(py, "ord")?;
        for glyph_id in glyphs.try_iter()? {
            let glyph_id = glyph_id?;
            let code = ord.call1((&glyph_id,))?;
            // bytes((ord(glyph_id),)) if ord(glyph_id) < 256 else b"?"
            let default = if code.lt(256)? {
                PyBytes::new(py, &[code.extract::<i64>()? as u8]).into_any()
            } else {
                py_bytes(py, b"?")
            };
            result.append(
                self.encoding_cmap
                    .call_method1("get", (&glyph_id, default))?,
            )?;
        }
        Ok(result.into_any())
    }
}

// @pypdf: pypdf.generic._appearance_stream.TextStreamAppearance._generate_appearance_stream_data
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn appearance_text_stream_appearance_generate_appearance_stream_data<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
    text: &Bound<'py, PyAny>,
    selection: &Bound<'py, PyAny>,
    font: &Bound<'py, PyAny>,
    font_name: &Bound<'py, PyAny>,
    font_size: &Bound<'py, PyAny>,
    font_color: &Bound<'py, PyAny>,
    is_multiline: &Bound<'py, PyAny>,
    alignment: &Bound<'py, PyAny>,
    is_comb: &Bound<'py, PyAny>,
    max_length: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // font_color = font_color or DeviceGray()
    let font_color = if truthy(font_color)? {
        font_color.clone()
    } else {
        shim_attr(py, "DeviceGray")?.call0()?
    };
    let layout = self_.getattr("_layout")?;
    let rectangle = layout.getattr("rectangle")?;
    let font_descriptor = font.getattr("font_descriptor")?;
    let bbox = font_descriptor.getattr("bbox")?;
    let text_space_factor = shim_attr(py, "TEXT_SPACE_TO_GLYPH_SPACE_FACTOR")?;
    // leading_factor = (bbox[3] - bbox[1]) / TEXT_SPACE_TO_GLYPH_SPACE_FACTOR
    let leading_factor = bbox
        .get_item(3)?
        .sub(&bbox.get_item(1)?)?
        .div(&text_space_factor)?;

    // factor = 2 if border_style in {"/B", "/I"} else 1
    let border_style = layout.getattr("border_style")?;
    let border_styles = PySet::new(py, ["/B", "/I"])?;
    let factor = if border_styles.contains(&border_style)? {
        py_int(py, 2)
    } else {
        py_int(py, 1)
    };
    // margin = border_width * factor
    let margin = layout.getattr("border_width")?.mul(&factor)?;
    let margin_max = py_max2(py, &margin, &py_int(py, 1))?;
    // field_height = rectangle.height - 2 * margin
    let field_height = rectangle
        .getattr("height")?
        .sub(&py_int(py, 2).mul(&margin)?)?;
    // field_width = rectangle.width - 4 * max(margin, 1)
    let field_width = rectangle
        .getattr("width")?
        .sub(&py_int(py, 4).mul(&margin_max)?)?;

    // reverse_cmap, encoding_cmap = font._get_typographic_maps()
    let typographic_maps = font.call_method0("_get_typographic_maps")?;
    let maps = GlyphMaps::new(
        py,
        typographic_maps.get_item(0)?,
        typographic_maps.get_item(1)?,
    )?;

    let mut font_size = font_size.clone();
    // Upstream reassigns the `is_multiline` parameter when a selection is present;
    // that reassignment is visible to the y-offset computation below.
    let mut is_multiline = truthy(is_multiline)?;
    let lines: Bound<'py, PyAny>;
    if font_size.eq(py_int(py, 0))? {
        let min_font_size = py_float(py, 4.0);
        if truthy(selection)? {
            // Don't wrap text when dealing with a /Ch field, in order to prevent
            // problems with matching "selection" with "line" later on.
            is_multiline = false;
        }
        if is_multiline {
            font_size = shim_attr(py, "DEFAULT_FONT_SIZE_IN_MULTILINE")?;
            let paragraphs = PyList::empty(py);
            for line in text.call_method0("splitlines")?.try_iter()? {
                let line = line?;
                if !truthy(&line.call_method0("strip")?)? {
                    let empty_paragraph = PyList::empty(py);
                    empty_paragraph.append(width_word_glyphs(
                        py,
                        &py_float(py, 0.0),
                        &py_str(py, ""),
                        &py_str(py, ""),
                    )?)?;
                    paragraphs.append(empty_paragraph)?;
                    continue;
                }
                let line_by_widths_words_glyphs = PyList::empty(py);
                for word in line.call_method1("split", (py_str(py, " "),))?.try_iter()? {
                    let word = word?;
                    let glyph_word = maps.unicode_to_glyph_id(&word)?;
                    let width = font
                        .call_method1("get_text_width", (&word,))?
                        .div(&text_space_factor)?;
                    line_by_widths_words_glyphs.append(width_word_glyphs(
                        py,
                        &width,
                        &word,
                        &glyph_word,
                    )?)?;
                }
                paragraphs.append(line_by_widths_words_glyphs)?;
            }
            let scaled = self_.call_method1(
                "_scale_text",
                (
                    font,
                    font_size.clone(),
                    &leading_factor,
                    &field_width,
                    &field_height,
                    &paragraphs,
                    &min_font_size,
                ),
            )?;
            lines = scaled.get_item(0)?;
            font_size = scaled.get_item(1)?;
        } else {
            // max_vertical_size = field_height / leading_factor
            let max_vertical_size = field_height.div(&leading_factor)?;
            let glyphs = maps.unicode_to_glyph_id(text)?;
            // text_width_unscaled = font.get_text_width(glyphs) / 1000
            let text_width_unscaled = font
                .call_method1("get_text_width", (&glyphs,))?
                .div(&text_space_factor)?;
            let horizontal_denominator = if truthy(&text_width_unscaled)? {
                text_width_unscaled.clone()
            } else {
                py_int(py, 1)
            };
            let max_horizontal_size = field_width.div(&horizontal_denominator)?;
            // font_size = round(max(min(max_vertical_size, max_horizontal_size), min_font_size), 1)
            let smallest = py_min(py, vec![max_vertical_size, max_horizontal_size])?;
            let largest = py_max2(py, &smallest, &min_font_size)?;
            font_size = py_round(py, &largest, 1)?;
            let line = width_word_glyphs(py, &text_width_unscaled.mul(&font_size)?, text, &glyphs)?;
            lines = PyList::new(py, [line])?.into_any();
        }
    } else if truthy(is_comb)? {
        if truthy(max_length)? && py_int(py, text.len()? as i64).gt(max_length)? {
            let values = PyDict::new(py);
            values.set_item("text", text)?;
            values.set_item("max_length", max_length)?;
            pyutil::logger_warning(
                py,
                SOURCE,
                "Length of text %(text)s exceeds maximum length (%(max_length)d) \
                 of field, input truncated.",
                Some(&values),
            )?;
        }
        // We act as if each character is one line, because we draw it separately later on.
        let comb_lines = PyList::empty(py);
        let text_length = text.len()? as i64;
        for (index, character) in text.try_iter()?.enumerate() {
            let character = character?;
            let limit = if truthy(max_length)? {
                max_length.clone()
            } else {
                py_int(py, text_length)
            };
            if py_int(py, index as i64).lt(&limit)? {
                let glyphs = maps.unicode_to_glyph_id(&character)?;
                let width = font
                    .call_method1("get_text_width", (&glyphs,))?
                    .mul(&font_size)?
                    .div(&text_space_factor)?;
                comb_lines.append(width_word_glyphs(py, &width, &character, &glyphs)?)?;
            }
        }
        lines = comb_lines.into_any();
    } else {
        let plain_lines = PyList::empty(py);
        for line in text.call_method0("splitlines")?.try_iter()? {
            let line = line?;
            let glyphs = maps.unicode_to_glyph_id(&line)?;
            let width = font
                .call_method1("get_text_width", (&glyphs,))?
                .mul(&font_size)?
                .div(&text_space_factor)?;
            plain_lines.append(width_word_glyphs(py, &width, &line, &glyphs)?)?;
        }
        lines = plain_lines.into_any();
    }

    // Set the vertical offset.
    let y_offset = if is_multiline {
        // field_height + margin - bbox[3] * font_size / 1000
        field_height
            .add(&margin)?
            .sub(&bbox.get_item(3)?.mul(&font_size)?.div(&text_space_factor)?)?
    } else {
        // margin + ((field_height - ascent * font_size / 1000) / 2)
        let ascent = font_descriptor.getattr("ascent")?;
        margin.add(
            &field_height
                .sub(&ascent.mul(&font_size)?.div(&text_space_factor)?)?
                .div(py_int(py, 2))?,
        )?
    };
    // default_appearance = f"{font_name} {font_size} Tf {font_color.as_operator()}"
    let default_appearance = format!(
        "{} {} Tf {}",
        py_string(font_name)?,
        py_string(&font_size)?,
        py_string(&font_color.call_method0("as_operator")?)?,
    );

    // ap_stream = bytearray()
    let mut ap_stream: Vec<u8> = Vec::new();
    ap_stream.extend_from_slice(
        format!(
            "q\n/Tx BMC \nq\n{} {} {} {} re\nW\nBT\n{}\n",
            // 2 * max(margin, 1)
            py_string(&py_int(py, 2).mul(&margin_max)?)?,
            py_string(&margin)?,
            // round(field_width - 2 * max(margin, 1), 3)
            py_string(&py_round(
                py,
                &field_width.sub(&py_int(py, 2).mul(&margin_max)?)?,
                3,
            )?)?,
            // round(field_height - margin, 3)
            py_string(&py_round(py, &field_height.sub(&margin)?, 3)?)?,
            default_appearance,
        )
        .as_bytes(),
    );
    // current_x_pos: float = 0
    let mut current_x_pos = py_int(py, 0);

    // selection_glyphs = _unicode_to_glyph_id("".join(selection), reverse_cmap) if selection else ""
    let selection_glyphs = if truthy(selection)? {
        let joined = py_str(py, "").call_method1("join", (selection,))?;
        maps.unicode_to_glyph_id(&joined)?
    } else {
        py_str(py, "")
    };

    let is_char_rtl = shim_attr(py, "is_char_rtl")?;
    let text_alignment = shim_attr(py, "TextAlignment")?;
    let alignment_right = text_alignment.getattr("RIGHT")?;
    let alignment_center = text_alignment.getattr("CENTER")?;

    for (line_number, line_item) in lines.try_iter()?.enumerate() {
        let line_item = line_item?;
        let line_width = line_item.get_item(0)?;
        let original_text = line_item.get_item(1)?;
        let line = line_item.get_item(2)?;
        let line_number_obj = py_int(py, line_number as i64);

        if truthy(&selection_glyphs)? && selection_glyphs.contains(&line)? {
            // Might be improved, but cannot find how to get fill working => replaced with lined box
            ap_stream.extend_from_slice(
                format!(
                    "1 {} {} {} re\n0.5 0.5 0.5 rg s\n{}\n",
                    py_string(&py_round(
                        py,
                        &y_offset
                            .sub(&line_number_obj.mul(&font_size)?.mul(&leading_factor)?)?
                            .sub(py_int(py, 1))?,
                        3,
                    )?)?,
                    py_string(&rectangle.getattr("width")?.sub(py_int(py, 2))?)?,
                    py_string(&font_size.add(py_int(py, 2))?)?,
                    default_appearance,
                )
                .as_bytes(),
            );
        }

        // Calculate the desired absolute starting X for the current line.
        let desired_abs_x_start = if truthy(is_comb)? && truthy(max_length)? {
            // Calculate the width of a cell for one character
            let cell_width = rectangle.getattr("width")?.div(max_length)?;
            let centering_offset_in_cell = cell_width.sub(&line_width)?.div(py_int(py, 2))?;
            line_number_obj
                .mul(&cell_width)?
                .add(&centering_offset_in_cell)?
        } else if alignment.eq(&alignment_right)? {
            rectangle
                .getattr("width")?
                .sub(&py_int(py, 2).mul(&margin_max)?)?
                .sub(&line_width)?
        } else if alignment.eq(&alignment_center)? {
            rectangle
                .getattr("width")?
                .sub(&line_width)?
                .div(py_int(py, 2))?
        } else {
            py_int(py, 2).mul(&margin_max)?
        };
        // x_rel_offset = desired_abs_x_start - current_x_pos
        let x_rel_offset = desired_abs_x_start.sub(&current_x_pos)?;

        // y_rel_offset: float = 0
        let y_rel_offset = if line_number == 0 {
            y_offset.clone()
        } else if truthy(is_comb)? {
            py_float(py, 0.0)
        } else {
            // - font_size * leading_factor
            font_size.neg()?.mul(&leading_factor)?
        };

        ap_stream.extend_from_slice(
            format!(
                "{} {} Td\n",
                py_string(&py_round(py, &x_rel_offset, 3)?)?,
                py_string(&py_round(py, &y_rel_offset, 3)?)?,
            )
            .as_bytes(),
        );
        current_x_pos = desired_abs_x_start;

        let mut is_rtl = false;
        for character in line.try_iter()? {
            if truthy(&is_char_rtl.call1((character?,))?)? {
                is_rtl = true;
                break;
            }
        }

        let encoded_line = maps.glyph_id_to_bytes(&line)?;
        if is_rtl {
            // Encode input text as UTF-16BE with a BOM, so that the input text is
            // returned in the right direction when copying text from the PDF.
            let utf16 = original_text.call_method1("encode", (py_str(py, "utf-16-be"),))?;
            let bom_text = py_bytes(py, b"\xfe\xff").add(&utf16)?;
            let hex_original_text = bom_text.call_method0("hex")?.call_method0("upper")?;
            ap_stream.extend_from_slice(
                format!(
                    "/Span << /ActualText <{}> >> BDC\n",
                    py_string(&hex_original_text)?,
                )
                .as_bytes(),
            );
        }
        if font.getattr("sub_type")?.eq(py_str(py, "Type0"))? {
            // 16-bit font
            let hex = py_bytes(py, b"")
                .call_method1("join", (&encoded_line,))?
                .call_method0("hex")?
                .call_method0("encode")?;
            ap_stream.push(b'<');
            ap_stream.extend_from_slice(&bytes_of(&hex)?);
            ap_stream.extend_from_slice(b"> Tj\n");
        } else {
            // Simple font, 8-bit encoded. Escape parentheses (PDF 1.7 reference, table 3.2).
            let escaped = py_bytes(py, b"")
                .call_method1("join", (&encoded_line,))?
                .call_method1("replace", (py_bytes(py, b"\\"), py_bytes(py, b"\\\\")))?
                .call_method1("replace", (py_bytes(py, b"("), py_bytes(py, br"\(")))?
                .call_method1("replace", (py_bytes(py, b")"), py_bytes(py, br"\)")))?;
            ap_stream.push(b'(');
            ap_stream.extend_from_slice(&bytes_of(&escaped)?);
            ap_stream.extend_from_slice(b") Tj\n");
        }
        if is_rtl {
            ap_stream.extend_from_slice(b"EMC\n");
        }
    }
    ap_stream.extend_from_slice(b"ET\nQ\nEMC\nQ\n");
    Ok(py_bytes(py, &ap_stream))
}

// ---------------------------------------------------------------------------
// TextStreamAppearance.__init__
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._appearance_stream.TextStreamAppearance.__init__
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn appearance_text_stream_appearance_init<'py>(
    py: Python<'py>,
    self_: &Bound<'py, PyAny>,
    layout: &Bound<'py, PyAny>,
    text: &Bound<'py, PyAny>,
    selection: &Bound<'py, PyAny>,
    font: &Bound<'py, PyAny>,
    font_resource: &Bound<'py, PyAny>,
    font_name: &Bound<'py, PyAny>,
    font_size: &Bound<'py, PyAny>,
    font_color: &Bound<'py, PyAny>,
    is_multiline: &Bound<'py, PyAny>,
    alignment: &Bound<'py, PyAny>,
    is_comb: &Bound<'py, PyAny>,
    max_length: &Bound<'py, PyAny>,
) -> PyResult<()> {
    // super().__init__(layout)
    call_super_init(py, "TextStreamAppearance", self_, vec![layout.clone()])?;

    let mut font_name = font_name.clone();
    let mut font = font.clone();
    let mut font_resource = font_resource.clone();
    // if not font or not font_resource:
    if !truthy(&font)? || !truthy(&font_resource)? {
        font_name = py_str(py, "/Helv");
        font = shim_attr(py, "Font")?.call_method0("from_core_font_name")?;
        font_resource = font.call_method0("as_font_resource")?;
    }
    // font_color or DeviceGray()
    let font_color = if truthy(font_color)? {
        font_color.clone()
    } else {
        shim_attr(py, "DeviceGray")?.call0()?
    };

    let kwargs = PyDict::new(py);
    kwargs.set_item("font_name", &font_name)?;
    kwargs.set_item("font_size", font_size)?;
    kwargs.set_item("font_color", &font_color)?;
    kwargs.set_item("is_multiline", is_multiline)?;
    kwargs.set_item("alignment", alignment)?;
    kwargs.set_item("is_comb", is_comb)?;
    kwargs.set_item("max_length", max_length)?;
    let generated = self_.call_method(
        "_generate_appearance_stream_data",
        (text, selection, &font),
        Some(&kwargs),
    )?;

    // self._ap_stream_data += generated
    let ap_stream_data = self_.getattr("_ap_stream_data")?.add(&generated)?;
    self_.setattr("_ap_stream_data", &ap_stream_data)?;

    // self.set_data(ByteStringObject(self._ap_stream_data))
    let byte_string = shim_attr(py, "ByteStringObject")?.call1((ap_stream_data.clone(),))?;
    self_.call_method1("set_data", (byte_string,))?;
    // self[NameObject("/Length")] = NumberObject(len(self._ap_stream_data))
    let length = ap_stream_data.len()?;
    self_.set_item(
        name_object(py, "/Length")?,
        shim_attr(py, "NumberObject")?.call1((py_int(py, length as i64),))?,
    )?;
    // Update Resources with font information.
    let font_mapping = PyDict::new(py);
    font_mapping.set_item(
        name_object_of(py, &font_name)?,
        getattr_or(py, &font_resource, "indirect_reference", &font_resource)?,
    )?;
    let fonts = dictionary_object(py, &font_mapping)?;
    let resources_mapping = PyDict::new(py);
    resources_mapping.set_item(name_object(py, "/Font")?, fonts)?;
    self_.set_item(
        name_object(py, "/Resources")?,
        dictionary_object(py, &resources_mapping)?,
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// TextStreamAppearance static helpers
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._appearance_stream.TextStreamAppearance._find_annotation_font_resource
#[pyfunction]
fn appearance_text_stream_appearance_find_annotation_font_resource<'py>(
    py: Python<'py>,
    font_name: &Bound<'py, PyAny>,
    annotation: &Bound<'py, PyAny>,
    acro_form: &Bound<'py, PyAny>,
    text: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut font_name = font_name.clone();
    // acro_form_resources = annotation.get_inherited("/DR", acro_form.get("/DR", DictionaryObject()))
    let acro_form_resources = annotation.call_method1(
        "get_inherited",
        (
            py_str(py, "/DR"),
            acro_form.call_method1(
                "get",
                (
                    py_str(py, "/DR"),
                    shim_attr(py, "DictionaryObject")?.call0()?,
                ),
            )?,
        ),
    )?;
    let font = {
        // acro_form_font_resources = acro_form_resources.get("/Font", DictionaryObject())
        let acro_form_font_resources = acro_form_resources.call_method1(
            "get",
            (
                py_str(py, "/Font"),
                shim_attr(py, "DictionaryObject")?.call0()?,
            ),
        )?;
        let font_resource = acro_form_font_resources.call_method1("get", (&font_name, none(py)))?;
        if truthy(&font_resource)? {
            shim_attr(py, "Font")?.call_method1(
                "from_font_resource",
                (font_resource.call_method0("get_object")?,),
            )?
        } else {
            // Normally, we should have found a font resource by now. However, when a user
            // has provided a specific font name, we may not have found the associated font
            // resource among the AcroForm resources. Also, in case of the 14 Adobe Core
            // fonts, we may be expected to construct a font resource ourselves.
            let core_font_metrics = shim_attr(py, "CORE_FONT_METRICS")?;
            let without_slash = font_name.call_method1("removeprefix", (py_str(py, "/"),))?;
            if !core_font_metrics.contains(&without_slash)? {
                let values = PyDict::new(py);
                values.set_item("font_name", &font_name)?;
                pyutil::logger_warning(
                    py,
                    SOURCE,
                    "Font dictionary for %(font_name)s not found; defaulting to Helvetica.",
                    Some(&values),
                )?;
                font_name = py_str(py, "/Helvetica");
            }
            shim_attr(py, "Font")?.call_method1("from_core_font_name", (&font_name,))?
        }
    };
    let font_descriptor = font.getattr("font_descriptor")?;
    let font_file = font_descriptor.getattr("font_file")?;
    let core_font_metrics = shim_attr(py, "CORE_FONT_METRICS")?;
    let mut encodable = font.call_method1("can_encode", (text,))?.is_truthy()?;
    let mut font = font;
    if !encodable {
        // If we have a font file, we can try to produce a new font resource with an
        // encoding that does include the necessary characters.
        if truthy(&font_file)? && font.getattr("sub_type")?.eq(py_str(py, "TrueType"))? {
            let attempt: PyResult<(Bound<'py, PyAny>, bool)> = (|| {
                let data = font_file.call_method0("get_data")?;
                let stream = shim_attr(py, "BytesIO")?.call1((data,))?;
                let new_font = font.call_method1("from_truetype_font_file", (stream,))?;
                let can = new_font.call_method1("can_encode", (text,))?.is_truthy()?;
                Ok((new_font, can))
            })();
            match attempt {
                Ok((new_font, can)) => {
                    font = new_font;
                    font_name = py_str(py, "/PYPDF1");
                    encodable = can;
                }
                Err(error) => {
                    let import_error = builtin(py, "ImportError")?;
                    let pdf_read_error = shim_attr(py, "PdfReadError")?;
                    if error.is_instance(py, &import_error)
                        || error.is_instance(py, &pdf_read_error)
                    {
                        let values = PyDict::new(py);
                        values.set_item("exception", error.value(py).as_any())?;
                        pyutil::logger_warning(
                            py,
                            SOURCE,
                            "Unable to use embedded font for encoding: %(exception)s",
                            Some(&values),
                        )?;
                    } else {
                        return Err(error);
                    }
                }
            }
        // If it's one of the unembedded 14 Adobe Core Fonts, we can test other
        // supported encodings.
        } else if font.getattr("sub_type")?.eq(py_str(py, "Type1"))?
            && core_font_metrics.contains(font.getattr("name")?)?
        {
            let core_font_metrics_entry = core_font_metrics.get_item(font.getattr("name")?)?;
            let test_encodings =
                PySet::new(py, ["cp1250", "cp1252", "cp1254", "cp1257", "iso8859_15"])?;
            let copy_module = shim_attr(py, "copy")?;
            let encoding_function = shim_attr(py, "encoding_dict_from_named_encoding")?;
            for encoding in test_encodings.try_iter()? {
                let encoding = encoding?;
                // test_font = copy.copy(font)
                let test_font = copy_module.call_method1("copy", (&font,))?;
                test_font.setattr("encoding", encoding_function.call1((&encoding,))?)?;
                encodable = test_font.call_method1("can_encode", (text,))?.is_truthy()?;
                if encodable {
                    font = test_font;
                    let character_widths = font.getattr("character_widths")?;
                    character_widths.call_method0("clear")?;
                    let metrics_character_widths =
                        core_font_metrics_entry.getattr("character_widths")?;
                    for code_character in font
                        .getattr("encoding")?
                        .call_method0("items")?
                        .try_iter()?
                    {
                        let code_character = code_character?;
                        let code = code_character.get_item(0)?;
                        let character = code_character.get_item(1)?;
                        if metrics_character_widths.contains(&character)? {
                            character_widths.set_item(
                                builtin(py, "chr")?.call1((code,))?,
                                metrics_character_widths.get_item(&character)?,
                            )?;
                        }
                    }
                    character_widths
                        .set_item("default", metrics_character_widths.get_item("default")?)?;
                    font_name = py_str(py, "/PYPDF1").add(&encoding)?;
                    break;
                }
            }
        }
        if !encodable {
            let values = PyDict::new(py);
            values.set_item("text", text)?;
            pyutil::logger_warning(
                py,
                SOURCE,
                "Text string '%(text)s' contains characters not supported by font encoding. \
                 This may result in text corruption. \
                 Consider calling writer.update_page_form_field_values with auto_regenerate=True.",
                Some(&values),
            )?;
        }
    }
    Ok(PyTuple::new(py, [font_name, font])?.into_any())
}

// @pypdf: pypdf.generic._appearance_stream.TextStreamAppearance._sync_appearance_stream_font_resources
#[pyfunction]
fn appearance_text_stream_appearance_sync_appearance_stream_font_resources<'py>(
    py: Python<'py>,
    writer: &Bound<'py, PyAny>,
    font_name: &Bound<'py, PyAny>,
    font: &Bound<'py, PyAny>,
    target_resource_dict: &Bound<'py, PyAny>,
    page: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // target_fonts = target_resource_dict.setdefault(NameObject("/Font"), DictionaryObject()).get_object()
    let target_fonts = target_resource_dict
        .call_method1(
            "setdefault",
            (
                name_object(py, "/Font")?,
                shim_attr(py, "DictionaryObject")?.call0()?,
            ),
        )?
        .call_method0("get_object")?;
    let font_resource_reference = if !target_fonts.contains(font_name)? {
        font.call_method1(
            "_add_to_writer",
            (writer, &target_fonts, name_object_of(py, font_name)?),
        )?
    } else {
        target_fonts.get_item(font_name)?
    };
    if truthy(page)? {
        // page_fonts_resource = page[PageAttributes.RESOURCES].setdefault("/Font", DictionaryObject()).get_object()
        let page_attributes = shim_attr(py, "PageAttributes")?;
        let page_fonts_resource = page
            .get_item(page_attributes.getattr("RESOURCES")?)?
            .call_method1(
                "setdefault",
                (
                    name_object(py, "/Font")?,
                    shim_attr(py, "DictionaryObject")?.call0()?,
                ),
            )?
            .call_method0("get_object")?;
        if !page_fonts_resource.contains(font_name)? {
            page_fonts_resource.set_item(
                name_object_of(py, font_name)?,
                getattr_or(
                    py,
                    &font_resource_reference,
                    "indirect_reference",
                    &font_resource_reference,
                )?,
            )?;
        }
    }
    Ok(font_resource_reference)
}

// @pypdf: pypdf.generic._appearance_stream.TextStreamAppearance._parse_default_appearance
#[pyfunction]
fn appearance_text_stream_appearance_parse_default_appearance<'py>(
    py: Python<'py>,
    default_appearance: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // stream = ContentStream(None, None)
    let stream = shim_attr(py, "ContentStream")?.call1((none(py), none(py)))?;
    // stream.set_data(default_appearance.original_bytes)
    stream.call_method1("set_data", (default_appearance.getattr("original_bytes")?,))?;
    let operations = match stream.getattr("operations") {
        Ok(operations) => operations,
        Err(error) => {
            // Ignore a malformed default appearance altogether.
            if error.is_instance(py, &shim_attr(py, "PdfReadError")?) {
                return Ok(PyTuple::new(py, [none(py), py_float(py, 0.0), none(py)])?.into_any());
            }
            return Err(error);
        }
    };
    let name_object_class = shim_attr(py, "NameObject")?;
    let int_class = builtin(py, "int")?;
    let float_class = builtin(py, "float")?;
    let color_class = shim_attr(py, "Color")?;
    let mut font_name = none(py);
    let mut font_size = py_float(py, 0.0);
    let mut font_color = none(py);
    for operation in operations.try_iter()? {
        let operation = operation?;
        let operands = operation.get_item(0)?;
        let operator = operation.get_item(1)?;
        if operator.eq(py_bytes(py, b"Tf"))? {
            if operands.len()? == 2
                && operands.get_item(0)?.is_instance(&name_object_class)?
                && (operands.get_item(1)?.is_instance(&int_class)?
                    || operands.get_item(1)?.is_instance(&float_class)?)
            {
                font_name = py_str(py, &pyutil::py_str(&operands.get_item(0)?)?);
                font_size = float_class.call1((operands.get_item(1)?,))?;
            }
        } else {
            let color = color_class.call_method1(
                "from_normalized_values",
                (
                    operands,
                    operator.call_method1("decode", (py_str(py, "latin-1"),))?,
                ),
            )?;
            if !color.is_none() {
                font_color = color;
            }
        }
    }
    Ok(PyTuple::new(py, [font_name, font_size, font_color])?.into_any())
}

// ---------------------------------------------------------------------------
// TextStreamAppearance.from_text_annotation
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._appearance_stream.TextStreamAppearance.from_text_annotation
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn appearance_text_stream_appearance_from_text_annotation<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyAny>,
    writer: &Bound<'py, PyAny>,
    page: &Bound<'py, PyAny>,
    flatten: &Bound<'py, PyAny>,
    acro_form: &Bound<'py, PyAny>,
    field: &Bound<'py, PyAny>,
    annotation: &Bound<'py, PyAny>,
    user_font_name: &Bound<'py, PyAny>,
    user_font_size: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let annotation_dictionary_attributes = shim_attr(py, "AnnotationDictionaryAttributes")?;
    let field_dictionary_attributes = shim_attr(py, "FieldDictionaryAttributes")?;

    // Calculate rectangle dimensions, normalizing and applying page rotation.
    let rect_key = annotation_dictionary_attributes.getattr("Rect")?;
    let raw_rectangle = annotation.get_item(&rect_key)?;
    let rotated = PySet::new(py, [90i64, 270])?
        .contains(page.call_method1("get_inherited", (py_str(py, "/Rotate"),))?)?;
    let rectangle_values = if rotated {
        vec![
            py_int(py, 0),
            py_int(py, 0),
            raw_rectangle
                .get_item(3)?
                .sub(&raw_rectangle.get_item(1)?)?
                .abs()?,
            raw_rectangle
                .get_item(2)?
                .sub(&raw_rectangle.get_item(0)?)?
                .abs()?,
        ]
    } else {
        vec![
            py_int(py, 0),
            py_int(py, 0),
            raw_rectangle
                .get_item(2)?
                .sub(&raw_rectangle.get_item(0)?)?
                .abs()?,
            raw_rectangle
                .get_item(3)?
                .sub(&raw_rectangle.get_item(1)?)?
                .abs()?,
        ]
    };
    let rectangle =
        shim_attr(py, "RectangleObject")?.call1((PyTuple::new(py, rectangle_values)?,))?;

    // Get default appearance dictionary from annotation.
    let da_key = annotation_dictionary_attributes.getattr("DA")?;
    let default_appearance = annotation.call_method1(
        "get_inherited",
        (&da_key, acro_form.call_method1("get", (&da_key, none(py)))?),
    )?;
    let default_appearance = if truthy(&default_appearance)? {
        default_appearance.call_method0("get_object")?
    } else {
        // Create a default appearance if none was found in the annotation.
        shim_attr(py, "TextStringObject")?.call1((py_str(py, "/Helv 0 Tf 0 g"),))?
    };

    // Retrieve field text and selected values.
    let ff_key = field_dictionary_attributes.getattr("Ff")?;
    let ft_key = field_dictionary_attributes.getattr("FT")?;
    let ff_bits = field_dictionary_attributes.getattr("FfBits")?;
    let opt_key = field_dictionary_attributes.getattr("Opt")?;
    let field_flags = field.call_method1("get", (&ff_key, py_int(py, 0)))?;
    let (text, mut selection);
    let is_choice = field
        .call_method1("get", (&ft_key, py_str(py, "/Tx")))?
        .eq(py_str(py, "/Ch"))?;
    if is_choice
        && field_flags
            .bitand(ff_bits.getattr("Combo")?)?
            .eq(py_int(py, 0))?
    {
        let options = annotation.call_method1("get_inherited", (&opt_key, PyList::empty(py)))?;
        text = py_str(py, "\n").call_method1("join", (options,))?;
        selection = field.call_method1("get", (py_str(py, "/V"), PyList::empty(py)))?;
        if !selection.is_instance(&builtin(py, "list")?)? {
            let wrapped = PyList::empty(py);
            wrapped.append(selection)?;
            selection = wrapped.into_any();
        }
    } else {
        // /Tx
        text = field.call_method1("get", (py_str(py, "/V"), py_str(py, "")))?;
        selection = PyList::empty(py).into_any();
    }

    // Derive font name, size and color from the default appearance.
    let parsed = cls.call_method1("_parse_default_appearance", (default_appearance.clone(),))?;
    let mut da_font_name = parsed.get_item(0)?;
    let mut font_size = parsed.get_item(1)?;
    let font_color = parsed.get_item(2)?;
    if da_font_name.is_none() {
        let values = PyDict::new(py);
        values.set_item(
            "default_appearance",
            py_str(py, &pyutil::py_str(&default_appearance)?),
        )?;
        pyutil::logger_warning(
            py,
            SOURCE,
            "Could not read a complete Tf operator from the default appearance %(default_appearance)r; \
             defaulting to /Helv 0 Tf.",
            Some(&values),
        )?;
        da_font_name = py_str(py, "/Helv");
        font_size = py_float(py, 0.0);
    }
    // Determine the font name / size to use, prioritizing the user's input.
    let mut font_name = if truthy(user_font_name)? {
        user_font_name.clone()
    } else {
        da_font_name.clone()
    };
    if user_font_size.gt(py_int(py, 0))? {
        font_size = user_font_size.clone();
    }

    let found = cls.call_method1(
        "_find_annotation_font_resource",
        (
            font_name.clone(),
            annotation.clone(),
            acro_form.clone(),
            text.clone(),
        ),
    )?;
    font_name = found.get_item(0)?;
    let font = found.get_item(1)?;

    // Change the /DA information if we changed the font name.
    if font_name.ne(&da_font_name)? {
        let replaced = default_appearance.call_method1("replace", (&da_font_name, &font_name))?;
        let new_default_appearance = shim_attr(py, "TextStringObject")?.call1((replaced,))?;
        annotation.set_item(name_object(py, "/DA")?, new_default_appearance)?;
    }

    // Synchronise font resources.
    let font_resource_reference = cls.call_method1(
        "_sync_appearance_stream_font_resources",
        (
            writer.clone(),
            font_name.clone(),
            font.clone(),
            acro_form.call_method1(
                "setdefault",
                (
                    name_object(py, "/DR")?,
                    shim_attr(py, "DictionaryObject")?.call0()?,
                ),
            )?,
            if truthy(flatten)? {
                page.clone()
            } else {
                none(py)
            },
        ),
    )?;

    // Retrieve formatting information.
    let mut is_comb = false;
    let mut max_length = none(py);
    if field_flags.bitand(ff_bits.getattr("Comb")?)?.is_truthy()? {
        is_comb = true;
        max_length = annotation.call_method1("get", (py_str(py, "/MaxLen"),))?;
    }
    let mut is_multiline = false;
    if field_flags
        .bitand(ff_bits.getattr("Multiline")?)?
        .is_truthy()?
    {
        is_multiline = true;
    }
    let alignment = field.call_method1(
        "get",
        (
            py_str(py, "/Q"),
            shim_attr(py, "TextAlignment")?.getattr("LEFT")?,
        ),
    )?;
    let mut border_width = py_int(py, 1);
    let mut border_style = shim_attr(py, "BorderStyles")?.getattr("SOLID")?;
    if field.contains(py_str(py, "/BS"))? {
        let border_style_dictionary = field.get_item(py_str(py, "/BS"))?;
        border_width =
            border_style_dictionary.call_method1("get", (py_str(py, "/W"), border_width))?;
        border_style =
            border_style_dictionary.call_method1("get", (py_str(py, "/S"), border_style))?;
    }

    let mut rotation = py_int(py, 0);
    let mut border_color = none(py);
    let mut background_color = none(py);
    let appearance_characteristics =
        field.call_method1("get_inherited", (py_str(py, "/MK"), none(py)))?;
    if appearance_characteristics.is_instance(&shim_attr(py, "DictionaryObject")?)? {
        rotation = builtin(py, "int")?
            .call1((appearance_characteristics
                .call_method1("get", (py_str(py, "/R"), py_int(py, 0)))?,))?;
        // Color.from_normalized_values([]) results in None for a "/BC" or "/BG" value of [].
        border_color = shim_attr(py, "Color")?.call_method1(
            "from_normalized_values",
            (appearance_characteristics.call_method1("get", (py_str(py, "/BC"),))?,),
        )?;
        background_color = shim_attr(py, "Color")?.call_method1(
            "from_normalized_values",
            (appearance_characteristics.call_method1("get", (py_str(py, "/BG"),))?,),
        )?;
    }

    // Create the TextStreamAppearance instance.
    let layout_kwargs = PyDict::new(py);
    layout_kwargs.set_item("rectangle", rectangle)?;
    layout_kwargs.set_item("border_width", border_width)?;
    layout_kwargs.set_item("border_style", border_style)?;
    layout_kwargs.set_item("border_color", border_color)?;
    layout_kwargs.set_item("background_color", background_color)?;
    layout_kwargs.set_item("rotation", rotation)?;
    let layout = shim_attr(py, "BaseStreamConfig")?.call((), Some(&layout_kwargs))?;

    let appearance_kwargs = PyDict::new(py);
    appearance_kwargs.set_item("font_name", &font_name)?;
    appearance_kwargs.set_item("font_size", &font_size)?;
    appearance_kwargs.set_item("font_color", &font_color)?;
    appearance_kwargs.set_item("is_multiline", is_multiline)?;
    appearance_kwargs.set_item("alignment", &alignment)?;
    appearance_kwargs.set_item("is_comb", is_comb)?;
    appearance_kwargs.set_item("max_length", &max_length)?;
    let new_appearance_stream = cls.call(
        PyTuple::new(
            py,
            [
                layout,
                text,
                selection,
                font.clone(),
                font_resource_reference.clone(),
            ],
        )?,
        Some(&appearance_kwargs),
    )?;

    if annotation.contains(annotation_dictionary_attributes.getattr("AP")?)? {
        let appearance_dictionary =
            annotation.get_item(annotation_dictionary_attributes.getattr("AP")?)?;
        let normal_appearance =
            appearance_dictionary.call_method1("get", (py_str(py, "/N"), PyDict::new(py)))?;
        let skipped_keys = PySet::new(py, ["/BBox", "/Length", "/Subtype", "/Type", "/Filter"])?;
        let resources_key = py_str(py, "/Resources");
        for pair in normal_appearance.call_method0("items")?.try_iter()? {
            let pair = pair?;
            let key = pair.get_item(0)?;
            let value = pair.get_item(1)?;
            if skipped_keys.contains(&key)? {
                continue;
            }
            if key.eq(&resources_key)? {
                // Don't overwrite font resources added by TextAppearanceStream.__init__.
                if !value.contains(py_str(py, "/Font"))? {
                    value.call_method0("get_object")?.set_item(
                        name_object(py, "/Font")?,
                        shim_attr(py, "DictionaryObject")?.call0()?,
                    )?;
                }
                value
                    .get_item(py_str(py, "/Font"))?
                    .call_method0("get_object")?
                    .set_item(
                        name_object_of(py, &font_name)?,
                        getattr_or(
                            py,
                            &font_resource_reference,
                            "indirect_reference",
                            &font_resource_reference,
                        )?,
                    )?;
            } else {
                new_appearance_stream.set_item(key, value)?;
            }
        }
    }
    Ok(new_appearance_stream)
}

// ---------------------------------------------------------------------------
// transform_annotation_appearance
// ---------------------------------------------------------------------------

// @pypdf: pypdf.generic._appearance_stream.transform_annotation_appearance
#[pyfunction]
fn appearance_transform_annotation_appearance<'py>(
    py: Python<'py>,
    annotation_obj: &Bound<'py, PyAny>,
    transformation: &Bound<'py, PyAny>,
) -> PyResult<()> {
    if !annotation_obj.contains(py_str(py, "/AP"))? {
        return Ok(());
    }
    let ap = annotation_obj.get_item(py_str(py, "/AP"))?;
    if !ap.contains(py_str(py, "/N"))? {
        return Ok(());
    }
    // __getitem__ already resolves indirect references, so this is never None.
    let normal_ap = ap.get_item(py_str(py, "/N"))?;
    let dictionary_object_class = shim_attr(py, "DictionaryObject")?;
    let stream_object_class = shim_attr(py, "StreamObject")?;
    // /N is a single appearance stream for most annotations, but for widgets with
    // multiple states (checkboxes, radio buttons) it is a dict of named sub-streams.
    let state_appearances: Bound<'py, PyAny> = if normal_ap.is_instance(&dictionary_object_class)?
        && !normal_ap.is_instance(&stream_object_class)?
    {
        normal_ap.call_method0("values")?
    } else {
        PyList::new(py, [normal_ap])?.into_any()
    };
    let default_matrix = PyTuple::new(py, [1i64, 0, 0, 1, 0, 0])?;
    let transformation_class = shim_attr(py, "Transformation")?;
    let float_object_class = shim_attr(py, "FloatObject")?;
    for state_appearance in state_appearances.try_iter()? {
        let state_appearance = state_appearance?;
        // get_object() is always safe to call: PdfObject.get_object() returns self.
        let state_object = state_appearance.call_method0("get_object")?;
        if !state_object.is_instance(&stream_object_class)? {
            continue;
        }
        // old_matrix = tuple(state_obj.get("/Matrix", (1, 0, 0, 1, 0, 0)))
        let old_matrix = builtin(py, "tuple")?
            .call1((state_object
                .call_method1("get", (py_str(py, "/Matrix"), default_matrix.clone()))?,))?;
        let ctm = transformation_class
            .call1((old_matrix,))?
            .call_method1("transform", (transformation,))?
            .getattr("ctm")?;
        // ArrayObject(FloatObject(x) for x in Transformation(old_matrix).transform(...).ctm)
        let floats = PyList::empty(py);
        for value in ctm.try_iter()? {
            floats.append(float_object_class.call1((value?,))?)?;
        }
        state_object.set_item(
            name_object(py, "/Matrix")?,
            array_object(py, &floats.into_any())?,
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Symbols implemented in the hand-written shim
/// (`python/pypdf/generic/_appearance_stream.py`): the module constants,
/// `HAS_RTL_SUPPORT`, the `BaseStreamConfig` dataclass, the `TextAlignment`
/// `IntEnum`, the `WidthWordGlyphs` named tuple, the class skeletons and the
/// re-exported helpers (`copy`, `BytesIO`, `Transformation`, `is_char_rtl`,
/// `logger_warning`, `CORE_FONT_METRICS`, `encoding_dict_from_named_encoding`,
/// `BorderStyles`, `AnnotationDictionaryAttributes`,
/// `FieldDictionaryAttributes`, `PageAttributes`, `PdfReadError`, the generic
/// classes, `Color`/`DeviceGray` and `Font`).
///
// @pypdf: pypdf.generic._appearance_stream.HAS_RTL_SUPPORT
// @pypdf: pypdf.generic._appearance_stream.DEFAULT_FONT_SIZE_IN_MULTILINE
// @pypdf: pypdf.generic._appearance_stream.TEXT_SPACE_TO_GLYPH_SPACE_FACTOR
// @pypdf: pypdf.generic._appearance_stream.BaseStreamConfig
// @pypdf: pypdf.generic._appearance_stream.BaseStreamConfig.rectangle
// @pypdf: pypdf.generic._appearance_stream.BaseStreamConfig.border_width
// @pypdf: pypdf.generic._appearance_stream.BaseStreamConfig.border_style
// @pypdf: pypdf.generic._appearance_stream.BaseStreamConfig.border_color
// @pypdf: pypdf.generic._appearance_stream.BaseStreamConfig.background_color
// @pypdf: pypdf.generic._appearance_stream.BaseStreamConfig.rotation
// @pypdf: pypdf.generic._appearance_stream.BaseStreamAppearance
// @pypdf: pypdf.generic._appearance_stream.TextAlignment
// @pypdf: pypdf.generic._appearance_stream.TextAlignment.LEFT
// @pypdf: pypdf.generic._appearance_stream.TextAlignment.CENTER
// @pypdf: pypdf.generic._appearance_stream.TextAlignment.RIGHT
// @pypdf: pypdf.generic._appearance_stream.WidthWordGlyphs
// @pypdf: pypdf.generic._appearance_stream.WidthWordGlyphs.width
// @pypdf: pypdf.generic._appearance_stream.WidthWordGlyphs.word
// @pypdf: pypdf.generic._appearance_stream.WidthWordGlyphs.glyphs
// @pypdf: pypdf.generic._appearance_stream.TextStreamAppearance
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(
        appearance_base_stream_appearance_add_matrix,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        appearance_base_stream_appearance_init,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        appearance_text_stream_appearance_scale_text,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        appearance_text_stream_appearance_generate_appearance_stream_data,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        appearance_text_stream_appearance_init,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        appearance_text_stream_appearance_find_annotation_font_resource,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        appearance_text_stream_appearance_sync_appearance_stream_font_resources,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        appearance_text_stream_appearance_parse_default_appearance,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        appearance_text_stream_appearance_from_text_annotation,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        appearance_transform_annotation_appearance,
        module
    )?)?;
    Ok(())
}
