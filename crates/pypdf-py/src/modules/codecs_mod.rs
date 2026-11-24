//! Rust port of `pypdf/_codecs/_codecs.py` (upstream pypdf 7.1.1).
//!
//! `Codec` is the upstream abstract base class for all codecs.  `LzwCodec`
//! implements the adaptive Lempel-Ziv-Welch (LZW) codec described by the PDF
//! specification ("7.4.4.2 Details of LZW Encoding" and "7.4.4.4 Details of LZW
//! Decoding").
//!
//! The upstream state machine is ported literally, including its quirks (the
//! bit-width bookkeeping of `_pack_codes_into_bytes`, the re-initialisation
//! rules of the decoding table, the 20-bit `_next_data` truncation, ...), so
//! that the produced code streams are byte-identical to upstream.

use pyo3::exceptions::{PyAttributeError, PyIndexError, PyNotImplementedError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyList};

use crate::errors;
use crate::pyutil;

/// Upstream `LzwCodec._AND_TABLE`: the masks for 9, 10, 11 and 12 bit codes.
const AND_TABLE: [i64; 4] = [511, 1023, 2047, 4095];

/// The `source=__name__` value used by upstream `logger_warning` calls in
/// `pypdf/_codecs/_codecs.py`.
const LOGGER_SOURCE: &str = "pypdf._codecs._codecs";

/// Abstract base class for all codecs.
///
/// `Codec()` has no `#[new]`: like the upstream `ABC` it cannot be instantiated
/// (PyO3 installs `Py_TPFLAGS_DISALLOW_INSTANTIATION`); subclasses must provide
/// their own constructor.
// @pypdf: pypdf._codecs._codecs.Codec
#[pyclass(name = "_Codec", module = "pypdf._codecs._codecs", subclass, dict)]
pub struct Codec {}

#[pymethods]
impl Codec {
    /// Encode the input data.
    // @pypdf: pypdf._codecs._codecs.Codec.encode
    fn encode(&self, _data: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
        Err(abstract_method("encode"))
    }

    /// Decode the input data.
    // @pypdf: pypdf._codecs._codecs.Codec.decode
    fn decode(&self, _data: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
        Err(abstract_method("decode"))
    }
}

fn abstract_method(name: &str) -> PyErr {
    PyNotImplementedError::new_err(format!(
        "Codec.{name}() is an abstract method and must be implemented by subclasses"
    ))
}

/// Lempel-Ziv-Welch (LZW) adaptive compression codec.
// @pypdf: pypdf._codecs._codecs.LzwCodec
#[pyclass(
    name = "_LzwCodecImpl",
    module = "pypdf._codecs._codecs",
    extends = Codec,
    subclass,
    dict
)]
pub struct LzwCodec {
    /// Upstream instance attribute `max_output_length`.
    max_output_length: Py<PyAny>,
    /// Upstream instance attribute `_table_index`.
    table_index: i64,
    /// Upstream instance attribute `_next_bits`.
    next_bits: i64,
    /// Upstream instance attribute `_next_data`.
    next_data: i64,
    /// Upstream instance attribute `_byte_pointer`.
    byte_pointer: i64,
    /// Upstream instance attribute `decoding_table`; `None` mirrors the attribute
    /// not existing yet (upstream only creates it in `__init__`, so this is only
    /// `None` for objects created without running the constructor, e.g. via
    /// `__new__` from Rust).
    decoding_table: Option<Py<PyAny>>,
    /// Upstream instance attribute `encoding_table` (created on demand).
    encoding_table: Option<Py<PyAny>>,
    /// Upstream instance attribute `next_code` (created on demand).
    next_code: Option<i64>,
    /// Upstream instance attribute `bits_per_code` (created on demand).
    bits_per_code: Option<i64>,
    /// Upstream instance attribute `max_code_value` (created on demand).
    max_code_value: Option<i64>,
    /// Upstream instance attribute `_bits_to_get` (created on demand).
    bits_to_get: Option<i64>,
}

#[pymethods]
impl LzwCodec {
    // @pypdf: pypdf._codecs._codecs.LzwCodec.__init__
    #[new]
    #[pyo3(signature = (max_output_length = None))]
    fn new(
        py: Python<'_>,
        max_output_length: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let max_output_length = match max_output_length {
            Some(value) => value.clone().unbind(),
            None => 75_000_000_i64.into_pyobject(py)?.into_any().unbind(),
        };
        let decoding_table = PyList::empty(py).into_any().unbind();
        Ok(PyClassInitializer::from(Codec {}).add_subclass(Self {
            max_output_length,
            table_index: -1,
            next_bits: -1,
            next_data: -1,
            byte_pointer: -1,
            decoding_table: Some(decoding_table),
            encoding_table: None,
            next_code: None,
            bits_per_code: None,
            max_code_value: None,
            bits_to_get: None,
        }))
    }

    #[getter]
    fn max_output_length(&self, py: Python<'_>) -> Py<PyAny> {
        self.max_output_length.clone_ref(py)
    }

    #[setter]
    fn set_max_output_length(&mut self, value: Bound<'_, PyAny>) {
        self.max_output_length = value.unbind();
    }

    // @pypdf: pypdf._codecs._codecs.LzwCodec.CLEAR_TABLE_MARKER
    #[classattr]
    const CLEAR_TABLE_MARKER: i64 = 256;

    // @pypdf: pypdf._codecs._codecs.LzwCodec.EOD_MARKER
    #[classattr]
    const EOD_MARKER: i64 = 257;

    // @pypdf: pypdf._codecs._codecs.LzwCodec.INITIAL_BITS_PER_CODE
    #[classattr]
    const INITIAL_BITS_PER_CODE: i64 = 9;

    // @pypdf: pypdf._codecs._codecs.LzwCodec.MAX_BITS_PER_CODE
    #[classattr]
    const MAX_BITS_PER_CODE: i64 = 12;

    // @pypdf: pypdf._codecs._codecs.LzwCodec._AND_TABLE
    #[classattr]
    #[allow(non_snake_case)]
    fn _AND_TABLE(py: Python<'_>) -> PyResult<Py<PyList>> {
        Ok(PyList::new(py, AND_TABLE)?.unbind())
    }

    // @pypdf: pypdf._codecs._codecs.LzwCodec._initialize_encoding_table
    fn _initialize_encoding_table(&mut self, py: Python<'_>) -> PyResult<()> {
        self.initialize_encoding_table_impl(py)
    }

    // @pypdf: pypdf._codecs._codecs.LzwCodec._increase_next_code
    fn _increase_next_code(&mut self) -> PyResult<()> {
        self.increase_next_code_impl()
    }

    // @pypdf: pypdf._codecs._codecs.LzwCodec.encode
    fn encode(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
        let input = extract_bytes(data)?;
        self.encode_impl(py, &input)
    }

    // @pypdf: pypdf._codecs._codecs.LzwCodec._pack_codes_into_bytes
    fn _pack_codes_into_bytes(
        &mut self,
        py: Python<'_>,
        codes: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyBytes>> {
        let codes: Vec<i64> = codes.extract()?;
        self.pack_codes_into_bytes_impl(py, &codes)
    }

    // @pypdf: pypdf._codecs._codecs.LzwCodec._initialize_decoding_table
    fn _initialize_decoding_table(&mut self, py: Python<'_>) -> PyResult<()> {
        self.initialize_decoding_table_impl(py)
    }

    // @pypdf: pypdf._codecs._codecs.LzwCodec._next_code_decode
    fn _next_code_decode(&mut self, data: &Bound<'_, PyAny>) -> PyResult<i64> {
        let input = extract_bytes(data)?;
        self.next_code_decode_impl(&input)
    }

    // @pypdf: pypdf._codecs._codecs.LzwCodec.decode
    fn decode(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
        let input = extract_bytes(data)?;
        self.decode_impl(py, &input)
    }

    // @pypdf: pypdf._codecs._codecs.LzwCodec._add_entry_decode
    fn _add_entry_decode(
        &mut self,
        py: Python<'_>,
        old_string: &Bound<'_, PyAny>,
        new_char: i64,
    ) -> PyResult<()> {
        let old_string = extract_bytes(old_string)?;
        let new_char = byte_from_int(new_char)?;
        self.add_entry_decode_impl(py, &old_string, new_char)
    }

    // --- plain instance attributes (upstream stores them in ``__dict__``) ---

    #[getter(_table_index)]
    fn get_table_index(&self) -> i64 {
        self.table_index
    }

    #[setter(_table_index)]
    fn set_table_index(&mut self, value: i64) {
        self.table_index = value;
    }

    #[getter(_next_bits)]
    fn get_next_bits(&self) -> i64 {
        self.next_bits
    }

    #[setter(_next_bits)]
    fn set_next_bits(&mut self, value: i64) {
        self.next_bits = value;
    }

    #[getter(_next_data)]
    fn get_next_data(&self) -> i64 {
        self.next_data
    }

    #[setter(_next_data)]
    fn set_next_data(&mut self, value: i64) {
        self.next_data = value;
    }

    #[getter(_byte_pointer)]
    fn get_byte_pointer(&self) -> i64 {
        self.byte_pointer
    }

    #[setter(_byte_pointer)]
    fn set_byte_pointer(&mut self, value: i64) {
        self.byte_pointer = value;
    }

    #[getter]
    fn decoding_table(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match &self.decoding_table {
            Some(table) => Ok(table.clone_ref(py)),
            None => Err(Self::missing_attribute("decoding_table")),
        }
    }

    #[setter]
    fn set_decoding_table(&mut self, value: Bound<'_, PyAny>) {
        self.decoding_table = Some(value.unbind());
    }

    #[getter]
    fn encoding_table(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match &self.encoding_table {
            Some(table) => Ok(table.clone_ref(py)),
            None => Err(Self::missing_attribute("encoding_table")),
        }
    }

    #[setter]
    fn set_encoding_table(&mut self, value: Bound<'_, PyAny>) {
        self.encoding_table = Some(value.unbind());
    }

    #[getter]
    fn next_code(&self) -> PyResult<i64> {
        self.next_code
            .ok_or_else(|| Self::missing_attribute("next_code"))
    }

    #[setter]
    fn set_next_code(&mut self, value: i64) {
        self.next_code = Some(value);
    }

    #[getter]
    fn bits_per_code(&self) -> PyResult<i64> {
        self.bits_per_code
            .ok_or_else(|| Self::missing_attribute("bits_per_code"))
    }

    #[setter]
    fn set_bits_per_code(&mut self, value: i64) {
        self.bits_per_code = Some(value);
    }

    #[getter]
    fn max_code_value(&self) -> PyResult<i64> {
        self.max_code_value
            .ok_or_else(|| Self::missing_attribute("max_code_value"))
    }

    #[setter]
    fn set_max_code_value(&mut self, value: i64) {
        self.max_code_value = Some(value);
    }

    #[getter(_bits_to_get)]
    fn get_bits_to_get(&self) -> PyResult<i64> {
        self.bits_to_get
            .ok_or_else(|| Self::missing_attribute("_bits_to_get"))
    }

    #[setter(_bits_to_get)]
    fn set_bits_to_get(&mut self, value: i64) {
        self.bits_to_get = Some(value);
    }
}

impl LzwCodec {
    fn missing_attribute(name: &str) -> PyErr {
        PyAttributeError::new_err(format!("'LzwCodec' object has no attribute '{name}'"))
    }

    /// Upstream `self.encoding_table` lookup, raising `AttributeError` when the
    /// attribute has not been created yet (identical to Python attribute access).
    fn encoding_table_bound(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match &self.encoding_table {
            Some(table) => Ok(table.clone_ref(py)),
            None => Err(Self::missing_attribute("encoding_table")),
        }
    }

    /// Upstream `self.decoding_table` lookup.
    fn decoding_table_bound(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match &self.decoding_table {
            Some(table) => Ok(table.clone_ref(py)),
            None => Err(Self::missing_attribute("decoding_table")),
        }
    }

    /// Upstream `_initialize_encoding_table`.
    fn initialize_encoding_table_impl(&mut self, py: Python<'_>) -> PyResult<()> {
        let table = PyDict::new(py);
        for value in 0..Self::CLEAR_TABLE_MARKER {
            table.set_item(PyBytes::new(py, &[value as u8]), value)?;
        }
        self.encoding_table = Some(table.into_any().unbind());
        self.next_code = Some(Self::EOD_MARKER + 1);
        self.bits_per_code = Some(Self::INITIAL_BITS_PER_CODE);
        self.max_code_value = Some((1_i64 << Self::INITIAL_BITS_PER_CODE) - 1);
        Ok(())
    }

    /// Upstream `_increase_next_code`.
    ///
    /// Attribute read order follows upstream (`next_code`, then `max_code_value`,
    /// then `bits_per_code`) so that an uninitialised codec raises the same
    /// `AttributeError`.
    fn increase_next_code_impl(&mut self) -> PyResult<()> {
        let next_code = self
            .next_code
            .ok_or_else(|| Self::missing_attribute("next_code"))?
            + 1;
        let max_code_value = self
            .max_code_value
            .ok_or_else(|| Self::missing_attribute("max_code_value"))?;
        self.next_code = Some(next_code);
        if next_code > max_code_value {
            let bits_per_code = self
                .bits_per_code
                .ok_or_else(|| Self::missing_attribute("bits_per_code"))?;
            if bits_per_code < Self::MAX_BITS_PER_CODE {
                let bits_per_code = bits_per_code + 1;
                self.bits_per_code = Some(bits_per_code);
                self.max_code_value = Some((1_i64 << bits_per_code) - 1);
            }
        }
        Ok(())
    }

    /// Upstream `encode`.
    fn encode_impl(&mut self, py: Python<'_>, input: &[u8]) -> PyResult<Py<PyBytes>> {
        let mut result_codes: Vec<i64> = Vec::with_capacity(input.len().saturating_mul(2) + 3);

        // The encoder shall begin by issuing a clear-table code.
        result_codes.push(Self::CLEAR_TABLE_MARKER);
        self.initialize_encoding_table_impl(py)?;

        let mut current_sequence: Vec<u8> = Vec::new();
        for &byte in input {
            let mut next_sequence = current_sequence.clone();
            next_sequence.push(byte);

            let next_key = PyBytes::new(py, &next_sequence);
            let in_table = {
                let table = self.encoding_table_bound(py)?.into_bound(py);
                table.contains(&next_key)?
            };
            if in_table {
                // Extend current sequence if already in the table.
                current_sequence = next_sequence;
                continue;
            }

            // Output code for the current sequence.
            let current_code: i64 = {
                let table = self.encoding_table_bound(py)?.into_bound(py);
                let current_key = PyBytes::new(py, &current_sequence);
                table.get_item(&current_key)?.extract()?
            };
            result_codes.push(current_code);

            // Add the new sequence to the table if there's room
            // (upstream: ``self.next_code <= (1 << MAX_BITS_PER_CODE) - 1``).
            let next_code = self.next_code.unwrap_or(Self::EOD_MARKER + 1);
            if next_code < (1_i64 << Self::MAX_BITS_PER_CODE) {
                {
                    let table = self.encoding_table_bound(py)?.into_bound(py);
                    table.set_item(next_key, next_code)?;
                }
                self.increase_next_code_impl()?;
            } else {
                // If the table is full, emit a clear-table command.
                result_codes.push(Self::CLEAR_TABLE_MARKER);
                self.initialize_encoding_table_impl(py)?;
            }

            // Start new sequence.
            current_sequence = vec![byte];
        }

        // Ensure everything actually is encoded.
        if !current_sequence.is_empty() {
            let table = self.encoding_table_bound(py)?.into_bound(py);
            let current_key = PyBytes::new(py, &current_sequence);
            let current_code: i64 = table.get_item(&current_key)?.extract()?;
            result_codes.push(current_code);
        }
        result_codes.push(Self::EOD_MARKER);

        self.pack_codes_into_bytes_impl(py, &result_codes)
    }

    /// Upstream `_pack_codes_into_bytes`.
    ///
    /// Codes are packed into a continuous bit stream, high-order bit first; the
    /// stream is then divided into bytes, high-order bit first.  The bit width
    /// starts at 9 bits and expands as the table grows.
    fn pack_codes_into_bytes_impl(
        &mut self,
        py: Python<'_>,
        codes: &[i64],
    ) -> PyResult<Py<PyBytes>> {
        self.initialize_encoding_table_impl(py)?;
        let mut buffer: u64 = 0;
        let mut bits_in_buffer: u32 = 0;
        let mut output: Vec<u8> = Vec::with_capacity(codes.len());

        for &code in codes {
            let bits_per_code = self.bits_per_code.unwrap_or(Self::INITIAL_BITS_PER_CODE) as u32;
            // Upstream keeps `buffer` as an arbitrary precision int; only the
            // lowest `bits_in_buffer + bits_per_code` bits are ever observable
            // (they are the only bits that get extracted), so a 20-bit mask is
            // applied after each code, exactly like the decoder does.
            buffer = buffer.wrapping_shl(bits_per_code) | (code as u64);
            bits_in_buffer += bits_per_code;

            while bits_in_buffer >= 8 {
                bits_in_buffer -= 8;
                output.push((buffer.wrapping_shr(bits_in_buffer) & 0xFF) as u8);
            }
            buffer &= 0xFFFFF;

            if code == Self::CLEAR_TABLE_MARKER {
                self.initialize_encoding_table_impl(py)?;
            } else if code == Self::EOD_MARKER {
                continue;
            } else {
                self.increase_next_code_impl()?;
            }
        }

        // Flush any remaining bits in the buffer.
        if bits_in_buffer > 0 {
            output.push((buffer.wrapping_shl(8 - bits_in_buffer) & 0xFF) as u8);
        }

        Ok(PyBytes::new(py, &output).unbind())
    }

    /// Upstream `_initialize_decoding_table`.
    fn initialize_decoding_table_impl(&mut self, py: Python<'_>) -> PyResult<()> {
        let max_code_value = (1_i64 << Self::MAX_BITS_PER_CODE) - 1;
        self.max_code_value = Some(max_code_value);

        let empty = PyBytes::new(py, b"");
        let existing = self.decoding_table.take();
        let reuse_existing = match &existing {
            Some(table) => table.bind(py).is_truthy()?,
            None => false,
        };
        let table = if reuse_existing {
            let table = existing.expect("truthiness was checked above");
            {
                let bound = table.bind(py);
                for index in (Self::EOD_MARKER + 1)..self.table_index {
                    bound.set_item(index, &empty)?;
                }
            }
            table
        } else {
            let list = PyList::empty(py);
            for value in 0..Self::CLEAR_TABLE_MARKER {
                list.append(PyBytes::new(py, &[value as u8]))?;
            }
            for _ in Self::CLEAR_TABLE_MARKER..=max_code_value {
                list.append(&empty)?;
            }
            list.into_any().unbind()
        };
        self.decoding_table = Some(table);

        self.table_index = Self::EOD_MARKER + 1;
        self.bits_to_get = Some(Self::INITIAL_BITS_PER_CODE);
        Ok(())
    }

    /// Upstream `_next_code_decode`; returns `EOD_MARKER` on `IndexError`.
    fn next_code_decode_impl(&mut self, data: &[u8]) -> PyResult<i64> {
        let bits_to_get = self
            .bits_to_get
            .ok_or_else(|| Self::missing_attribute("_bits_to_get"))?;
        while self.next_bits < bits_to_get {
            let Some(byte) = byte_at(data, self.byte_pointer) else {
                return Ok(Self::EOD_MARKER);
            };
            self.next_data = self.next_data.wrapping_shl(8) | i64::from(byte);
            self.byte_pointer += 1;
            self.next_bits += 8;
        }

        let mask = and_table_value(bits_to_get)?;
        let code = (self.next_data >> (self.next_bits - bits_to_get)) & mask;
        self.next_bits -= bits_to_get;

        // Reduce data to get rid of the overhead, which increases performance
        // on large streams significantly (upstream behaviour).
        self.next_data &= 0xFFFFF;
        Ok(code)
    }

    /// Upstream `decode`.
    fn decode_impl(&mut self, py: Python<'_>, input: &[u8]) -> PyResult<Py<PyBytes>> {
        self.table_index = 0;
        self.byte_pointer = 0;
        self.next_data = 0;
        self.next_bits = 0;

        let mut output: Vec<u8> = Vec::new();
        let mut output_length: usize = 0;

        self.initialize_decoding_table_impl(py)?;
        let mut old_code: i64 = Self::CLEAR_TABLE_MARKER;

        loop {
            let mut code = self.next_code_decode_impl(input)?;
            if code == Self::EOD_MARKER {
                break;
            }

            let decoded: Vec<u8> = if code == Self::CLEAR_TABLE_MARKER {
                self.initialize_decoding_table_impl(py)?;
                code = self.next_code_decode_impl(input)?;
                if code == Self::EOD_MARKER {
                    break;
                }
                let entry = self.decoding_entry(py, code)?;
                output.extend_from_slice(&entry);
                old_code = code;
                entry
            } else if code < self.table_index {
                let entry = self.decoding_entry(py, code)?;
                output.extend_from_slice(&entry);
                if old_code != Self::CLEAR_TABLE_MARKER {
                    let old_string = self.decoding_entry(py, old_code)?;
                    let new_char = first_byte(&entry)?;
                    self.add_entry_decode_impl(py, &old_string, new_char)?;
                }
                old_code = code;
                entry
            } else {
                // The code is not in the table and not one of the special codes.
                let base = self.decoding_entry(py, old_code)?;
                if base.is_empty() {
                    let message = format!(
                        "LZW code {code} out of range with empty base at table index {}.",
                        self.table_index
                    );
                    return Err(errors::pdf_stream_error(py, &message));
                }
                let mut entry = Vec::with_capacity(base.len() + 1);
                entry.extend_from_slice(&base);
                entry.push(base[0]);
                output.extend_from_slice(&entry);
                let new_char = base[0];
                self.add_entry_decode_impl(py, &base, new_char)?;
                old_code = code;
                entry
            };

            output_length += decoded.len();
            if self.output_limit_reached(py, output_length)? {
                let message = format!(
                    "Limit reached while decompressing: {output_length} > {}",
                    self.max_output_length_text(py)?
                );
                return Err(errors::limit_reached_error(py, &message));
            }
        }

        Ok(PyBytes::new(py, &output).unbind())
    }

    /// Upstream `_add_entry_decode`.
    fn add_entry_decode_impl(
        &mut self,
        py: Python<'_>,
        old_string: &[u8],
        new_char: u8,
    ) -> PyResult<()> {
        let mut new_string = Vec::with_capacity(old_string.len() + 1);
        new_string.extend_from_slice(old_string);
        new_string.push(new_char);

        let max_code_value = self
            .max_code_value
            .ok_or_else(|| Self::missing_attribute("max_code_value"))?;
        if self.table_index > max_code_value {
            pyutil::logger_warning(
                py,
                LOGGER_SOURCE,
                "Ignoring too large LZW table index.",
                None,
            )?;
            return Ok(());
        }
        {
            let table = self.decoding_table_bound(py)?.into_bound(py);
            table.set_item(self.table_index, PyBytes::new(py, &new_string))?;
        }
        self.table_index += 1;

        // Update the number of bits to get based on the table index.
        match self.table_index {
            511 => self.bits_to_get = Some(10),
            1023 => self.bits_to_get = Some(11),
            2047 => self.bits_to_get = Some(12),
            _ => {}
        }
        Ok(())
    }

    /// Upstream `self.decoding_table[code]`.
    fn decoding_entry(&self, py: Python<'_>, code: i64) -> PyResult<Vec<u8>> {
        let table = self.decoding_table_bound(py)?.into_bound(py);
        table.get_item(code)?.extract()
    }

    /// Upstream `output_length > self.max_output_length`.
    ///
    /// `max_output_length` is a Python object upstream (compared with Python
    /// semantics); the common `int` case is handled without re-entering Python.
    fn output_limit_reached(&self, py: Python<'_>, output_length: usize) -> PyResult<bool> {
        let limit = self.max_output_length.bind(py);
        match limit.extract::<i128>() {
            Ok(limit) => Ok((output_length as i128) > limit),
            Err(_) => output_length.into_pyobject(py)?.into_any().gt(limit),
        }
    }

    /// `str(self.max_output_length)` for the `LimitReachedError` message.
    fn max_output_length_text(&self, py: Python<'_>) -> PyResult<String> {
        pyutil::py_str(self.max_output_length.bind(py))
    }
}

/// Interpret a Python object as the bytes-like payload handled by the codec.
fn extract_bytes(data: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    match data.cast::<PyBytes>() {
        Ok(bytes) => Ok(bytes.as_bytes().to_vec()),
        Err(_) => data.extract::<Vec<u8>>(),
    }
}

/// `data[index]` for a Python `bytes` object (negative indices index from the
/// end); `None` mirrors the upstream `IndexError` caught in `_next_code_decode`.
fn byte_at(data: &[u8], index: i64) -> Option<u8> {
    let length = data.len() as i64;
    let index = if index < 0 { index + length } else { index };
    if (0..length).contains(&index) {
        data.get(index as usize).copied()
    } else {
        None
    }
}

/// Python `bytes[0]`, raising `IndexError` for an empty `bytes`.
fn first_byte(decoded: &[u8]) -> PyResult<u8> {
    decoded
        .first()
        .copied()
        .ok_or_else(|| PyIndexError::new_err("index out of range"))
}

/// `bytes([new_char])` value range check.
fn byte_from_int(value: i64) -> PyResult<u8> {
    u8::try_from(value).map_err(|_| PyValueError::new_err("bytes must be in range(0, 256)"))
}

/// Upstream `self._AND_TABLE[self._bits_to_get - 9]` (a Python list supports
/// negative indices, so the same normalisation is applied here).
fn and_table_value(bits_to_get: i64) -> PyResult<i64> {
    let raw_index = bits_to_get - 9;
    let length = AND_TABLE.len() as i64;
    let index = if raw_index < 0 {
        raw_index + length
    } else {
        raw_index
    };
    if (0..length).contains(&index) {
        Ok(AND_TABLE[index as usize])
    } else {
        Err(PyIndexError::new_err("list index out of range"))
    }
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Codec>()?;
    module.add_class::<LzwCodec>()?;
    Ok(())
}
