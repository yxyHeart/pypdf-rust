//! Byte-level scanner and parser for PDF content streams.
//!
//! This module is pure Rust (no PyO3) so the grammar and limit semantics can be
//! unit-tested and fuzzed independently.  It mirrors the parsing behaviour of
//! pypdf 7.1.1's `ContentStream._parse_content_stream` + `read_object` family
//! (`generic/_data_structures.py`, `generic/_base.py`, `generic/_utils.py`) so
//! the Python-facing layer can materialise identical objects.
//!
//! The parser never calls back into Python: names, strings, numbers and nested
//! containers are scanned here and returned as `Value`s; the PyO3 layer
//! materialises them once per value.  Cases the byte scanner deliberately does
//! not handle (a `stream` keyword following a dictionary inside a content
//! stream) report [`ScanError::Fallback`] so the caller can use the legacy
//! parser.

/// pypdf's `_utils.WHITESPACES`.
pub const WHITESPACES: [u8; 6] = [0x00, 0x09, 0x0a, 0x0c, 0x0d, 0x20];

/// `tok in WHITESPACES` from upstream.
#[inline]
pub fn is_whitespace(byte: u8) -> bool {
    WHITESPACES.contains(&byte)
}

/// Python `re` `\s` for bytes patterns (`[ \t\n\r\f\v]`).
#[inline]
pub fn is_regex_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// `NameObject.delimiter_pattern` / `WHITESPACES_AS_REGEXP` delimiter set.
#[inline]
pub fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

/// Stop predicate for `NameObject.delimiter_pattern`: `\s+|[\(\)<>\[\]{}/%]`.
#[inline]
pub fn is_name_delimiter(byte: u8) -> bool {
    is_regex_whitespace(byte) || is_delimiter(byte)
}

/// `NumberObject.NumberPattern` = `b"[^+-.0-9]"`.
#[inline]
pub fn is_number_delimiter(byte: u8) -> bool {
    !matches!(byte, b'+' | b'-' | b'.' | b'0'..=b'9')
}

/// Errors mirroring the upstream exception classes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanError {
    /// `LimitReachedError(message)`
    Limit(String),
    /// `PdfStreamError(message)`
    Stream(String),
    /// `PdfReadError(message)`
    Read(String),
    /// `ValueError`: negative seek on a BytesIO (upstream leaks this).
    NegativeSeek(i64),
    /// `AssertionError` (BI with pending operands).
    Assert,
    /// Not representable with the byte scanner: use the legacy Python path.
    Fallback,
}

/// Resource limits, mirroring `pypdf.get_configuration()` values.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub budget: i64,
    pub array_max: i64,
    pub dictionary_max: i64,
    pub operator_length: usize,
    pub name_length: usize,
    pub number_length: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            budget: 2_000_000,
            array_max: 100_000,
            dictionary_max: 100_000,
            operator_length: 128,
            name_length: 4096,
            number_length: 64,
        }
    }
}

/// A warning emitted at the same points as upstream's `logger_warning` calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningEvent {
    /// `generic/_utils.py`: "Unexpected escaped string: %(token)s"
    UnexpectedEscape(u8),
    /// `generic/_data_structures.py`: duplicate dictionary key
    MultipleDefinition { position: usize, key: Vec<u8> },
    /// `generic/_data_structures.py`: dictionary key is not a NameObject; the
    /// materialiser formats `Expecting a NameObject for key but found {key!r}`.
    NonNameKeyValue(Value),
    /// `generic/_data_structures.py`: `read_object` raised a `PdfReadError`
    /// while reading a dictionary key; payload is the exception message.
    KeyParseError(String),
    /// `generic/_data_structures.py`: value parse raised (non-limit) inside a
    /// dictionary, so a partial dictionary is returned; the materialiser logs
    /// `repr(exception)[:500]`.
    PartialDictionary(Box<ScanError>),
}

/// One parsed content-stream operand value.
///
/// Numbers keep their raw token bytes: upstream constructs `NumberObject` /
/// `FloatObject` from those bytes, which decides integer/float parsing and the
/// invalid-value fallbacks.  Names keep the raw bytes after the leading `/`
/// (still `#xx`-encoded); the materialiser applies `NameObject.unnumber`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Integer(Vec<u8>),
    Real(Vec<u8>),
    Name(Vec<u8>),
    LiteralString(Vec<u8>),
    HexString(Vec<u8>),
    Boolean(bool),
    Null,
    Array(Vec<Value>),
    Dictionary(Vec<(Value, Value)>),
    Indirect { idnum: i64, generation: i64 },
}

/// One entry of `ContentStream._operations`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Operation {
        operands: Vec<Value>,
        operator: Vec<u8>,
    },
    /// `BI` was found: the caller must run the Python inline-image reader from
    /// this byte offset and then resume parsing at `resume`.
    InlineImage { start: usize },
}

#[derive(Debug, Clone)]
pub struct ParseOutput {
    pub items: Vec<Item>,
    pub warnings: Vec<WarningEvent>,
    /// Offset after the `BI` operator (where the inline image begins).
    pub inline_image: Option<usize>,
    /// Tokens scanned (for diagnostics/counters).
    pub token_count: u64,
}

/// A parse failure together with the state accumulated before the error.
#[derive(Debug, Clone)]
pub struct Failure {
    pub error: ScanError,
    pub items: Vec<Item>,
    pub warnings: Vec<WarningEvent>,
    pub budget_left: i64,
}

/// Result of parsing up to a single inline image.
#[derive(Debug, Clone)]
pub struct Segment {
    pub items: Vec<Item>,
    pub warnings: Vec<WarningEvent>,
    pub inline_image_start: Option<usize>,
    pub next_offset: usize,
    pub token_count: u64,
    pub budget_left: i64,
}

struct Parser<'a> {
    data: &'a [u8],
    pos: usize,
    limits: Limits,
    budget: i64,
    warnings: Vec<WarningEvent>,
    items: Vec<Item>,
    token_count: u64,
}

impl<'a> Parser<'a> {
    fn new(data: &'a [u8], limits: Limits, start: usize) -> Self {
        Self {
            data,
            pos: start,
            limits,
            budget: limits.budget,
            warnings: Vec::new(),
            items: Vec::new(),
            token_count: 0,
        }
    }

    #[inline]
    fn remaining(&self) -> &'a [u8] {
        &self.data[self.pos.min(self.data.len())..]
    }

    #[inline]
    fn eof(&self) -> bool {
        self.pos >= self.data.len()
    }

    #[inline]
    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    /// `read_non_whitespace`: skip WHITESPACES, consume and return the next byte.
    fn read_non_whitespace(&mut self) -> Option<u8> {
        while let Some(byte) = self.peek() {
            if is_whitespace(byte) {
                self.pos += 1;
            } else {
                self.pos += 1;
                return Some(byte);
            }
        }
        None
    }

    /// Skip whitespace without consuming the following byte.
    fn skip_whitespace(&mut self) -> Option<u8> {
        while let Some(byte) = self.peek() {
            if is_whitespace(byte) {
                self.pos += 1;
            } else {
                return Some(byte);
            }
        }
        None
    }

    /// `_utils.read_until_regex` over the in-memory buffer.
    ///
    /// Replicates upstream's chunked reads (16 bytes doubling to 8192) because
    /// the length-limit error message depends on the accumulated chunk size.
    fn read_until_regex(
        &mut self,
        stop: fn(u8) -> bool,
        length: Option<usize>,
    ) -> Result<Vec<u8>, ScanError> {
        let start = self.pos;
        let mut total = 0usize;
        let mut chunk_size = 16usize;
        let mut tail_len = 0usize;
        loop {
            let available = self.data.len().saturating_sub(self.pos);
            let take = chunk_size.min(available);
            if take == 0 {
                return Ok(self.data[start..].to_vec());
            }
            let current_length = total + take;
            if let Some(limit) = length {
                if current_length >= limit {
                    return Err(ScanError::Limit(format!(
                        "Read stream length of {current_length} exceeds maximum allowed length of {limit}."
                    )));
                }
            }
            let window_start = self.pos - tail_len;
            let window = &self.data[window_start..self.pos + take];
            let search_offset = tail_len;
            if let Some(index) = window[search_offset..]
                .iter()
                .position(|byte| stop(*byte))
            {
                let actual_start = total - tail_len + search_offset + index;
                return Ok(self.data[start..start + actual_start].to_vec());
            }
            self.pos += take;
            total = current_length;
            tail_len = take.min(16);
            if chunk_size < 8192 {
                chunk_size <<= 1;
            }
        }
    }

    /// `read_until_regex` that also reports how many bytes were consumed.
    fn read_until_regex_bounded(
        &mut self,
        stop: fn(u8) -> bool,
        length: Option<usize>,
    ) -> Result<Vec<u8>, ScanError> {
        let before = self.pos;
        let token = self.read_until_regex(stop, length)?;
        // `read_until_regex` seeks the stream back to the match start.
        self.pos = before + token.len();
        Ok(token)
    }

    fn check_comment(&mut self) -> Result<(), ScanError> {
        // `skip_over_comment`: consume until CR/LF, error on EOF.
        while let Some(byte) = self.peek() {
            self.pos += 1;
            if byte == b'\n' || byte == b'\r' {
                return Ok(());
            }
        }
        Err(ScanError::Stream("File ended unexpectedly.".to_string()))
    }

    /// `skip_over_comments(limit=...)` starting at the `%` character.
    fn skip_over_comments(&mut self, limit: Option<usize>) -> Result<(), ScanError> {
        let mut count = 0usize;
        while self.peek() == Some(b'%') {
            if let Some(limit) = limit {
                if count >= limit {
                    return Err(ScanError::Limit(format!(
                        "Maximum number of leading comments reached: {limit}."
                    )));
                }
            }
            self.check_comment()?;
            // read_non_whitespace then seek back one byte.
            self.skip_whitespace();
            count += 1;
        }
        Ok(())
    }

    /// `ContentStream._check_limits` when called from `_parse_content_stream`
    /// raises a different message than the nested array/dictionary checks.
    fn decrement_budget(&mut self, amount: i64, per_stream: bool) -> Result<(), ScanError> {
        self.budget -= amount;
        if self.budget < 0 {
            return Err(ScanError::Limit(if per_stream {
                "Reached maximum number of tokens per stream.".to_string()
            } else {
                "Reached maximum number of tokens.".to_string()
            }));
        }
        Ok(())
    }

    /// Parse one object value (`read_object` semantics for content streams).
    fn parse_value(&mut self) -> Result<Value, ScanError> {
        self.token_count += 1;
        let byte = self.read_non_whitespace().ok_or_else(|| {
            ScanError::Read("Invalid Elementary Object starting with b''".to_string())
        })?;
        self.pos -= 1;
        match byte {
            b'%' => {
                self.skip_over_comments(Some(1000))?;
                // Upstream re-reads the next character after comments.
                self.parse_value()
            }
            b'/' => {
                self.pos += 1;
                let raw = self.read_until_regex_bounded(is_name_delimiter, Some(self.limits.name_length))?;
                Ok(Value::Name(raw))
            }
            b'<' => {
                if self.remaining().starts_with(b"<<") {
                    self.parse_dictionary()
                } else {
                    self.parse_hex_string()
                }
            }
            b'[' => self.parse_array(),
            b't' | b'f' => self.parse_boolean(),
            b'(' => self.parse_literal_string(),
            b'n' => {
                // `NullObject.read_from_stream` consumes exactly "null".
                if self.remaining().starts_with(b"null") {
                    self.pos += 4;
                    Ok(Value::Null)
                } else {
                    Err(ScanError::Read("Could not read Null object".to_string()))
                }
            }
            b'e' => {
                // `read_object` maps "endobj" to NullObject.
                if self.remaining().starts_with(b"endobj") {
                    self.pos += 6;
                    Ok(Value::Null)
                } else {
                    self.invalid_object(byte)
                }
            }
            _ => self.parse_number_or_indirect(byte),
        }
    }

    fn invalid_object(&mut self, byte: u8) -> Result<Value, ScanError> {
        let pos = self.pos;
        // Upstream does `stream.seek(-20, 1)` on a BytesIO, which clamps to 0
        // for negative positions, then reads 80 bytes of context.
        let start = pos.saturating_sub(20);
        let extract = &self.data[start..(pos + 60).min(self.data.len())];
        Err(ScanError::Read(format!(
            "Invalid Elementary Object starting with {} @{}: {}",
            python_bytes_repr(&[byte]),
            pos,
            python_bytes_repr(extract),
        )))
    }

    fn parse_boolean(&mut self) -> Result<Value, ScanError> {
        let rest = self.remaining();
        if rest.starts_with(b"true") {
            self.pos += 4;
            return Ok(Value::Boolean(true));
        }
        if rest.starts_with(b"fals") {
            self.pos += 5;
            return Ok(Value::Boolean(false));
        }
        Err(ScanError::Read("Could not read Boolean object".to_string()))
    }

    fn parse_number_or_indirect(&mut self, first: u8) -> Result<Value, ScanError> {
        if !matches!(first, b'+' | b'-' | b'.' | b'0'..=b'9') {
            return self.invalid_object(first);
        }
        let peek_len = (self.data.len() - self.pos).min(20);
        let peek = &self.data[self.pos..self.pos + peek_len];
        if let Some((end, idnum, generation)) = match_indirect(peek) {
            // `IndirectObject` case: the stream advances to just after the "R".
            self.pos += end - 1;
            return Ok(Value::Indirect { idnum, generation });
        }
        if let Some(stop_index) = peek.iter().position(|byte| is_number_delimiter(*byte)) {
            let raw = peek[..stop_index].to_vec();
            self.pos += stop_index;
            return Ok(number_value(raw));
        }
        // Number longer than the 20-byte peek: upstream falls back to
        // `NumberObject.read_from_stream` (chunked `read_until_regex`).
        let raw = self.read_until_regex_bounded(is_number_delimiter, Some(self.limits.number_length))?;
        Ok(number_value(raw))
    }

    fn parse_literal_string(&mut self) -> Result<Value, ScanError> {
        // `read_string_from_stream` (see generic/_utils.py).
        self.pos += 1; // '('
        let mut parens = 1i32;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let byte = match self.peek() {
                Some(byte) => byte,
                None => {
                    return Err(ScanError::Stream(
                        "Stream has ended unexpectedly".to_string(),
                    ))
                }
            };
            self.pos += 1;
            match byte {
                b'(' => {
                    parens += 1;
                    out.push(byte);
                }
                b')' => {
                    parens -= 1;
                    if parens == 0 {
                        return Ok(Value::LiteralString(out));
                    }
                    out.push(byte);
                }
                b'\\' => {
                    let escaped = match self.peek() {
                        Some(value) => value,
                        None => {
                            return Err(ScanError::Stream(
                                "Stream has ended unexpectedly".to_string(),
                            ))
                        }
                    };
                    self.pos += 1;
                    if let Some(mapped) = escape_value(escaped) {
                        out.push(mapped);
                        continue;
                    }
                    if (b'0'..=b'7').contains(&escaped) {
                        // Up to three octal digits; on overflow upstream restores
                        // the position to just after the backslash+first digit.
                        let sav = self.pos - 1;
                        let mut token = vec![escaped];
                        for _ in 0..2 {
                            match self.peek() {
                                Some(next) if (b'0'..=b'7').contains(&next) => {
                                    token.push(next);
                                    self.pos += 1;
                                }
                                Some(_) => break,
                                None => break,
                            }
                        }
                        let value = token.iter().fold(0u32, |acc, digit| {
                            acc * 8 + u32::from(digit - b'0')
                        });
                        if value > 255 {
                            out.push(b'\\');
                            self.pos = sav;
                        } else {
                            out.push(value as u8);
                        }
                        continue;
                    }
                    if escaped == b'\n' || escaped == b'\r' {
                        // Escaped line break: consume the second half of a CRLF.
                        if let Some(next) = self.peek() {
                            if next == b'\n' || next == b'\r' {
                                self.pos += 1;
                            }
                        }
                        continue;
                    }
                    self.warnings.push(WarningEvent::UnexpectedEscape(escaped));
                    out.push(b'\\');
                    out.push(escaped);
                }
                _ => out.push(byte),
            }
        }
    }

    fn parse_hex_string(&mut self) -> Result<Value, ScanError> {
        // `read_hex_string_from_stream`.
        self.pos += 1; // '<'
        let mut out: Vec<u8> = Vec::new();
        let mut nibble: Vec<u8> = Vec::new();
        loop {
            let byte = self.read_non_whitespace().ok_or_else(|| {
                ScanError::Stream("Stream has ended unexpectedly".to_string())
            })?;
            if byte == b'>' {
                break;
            }
            if !byte.is_ascii_hexdigit() {
                return Err(ScanError::Stream(format!(
                    "Invalid hexadecimal character {} in hex string",
                    python_bytes_repr(&[byte])
                )));
            }
            nibble.push(byte);
            if nibble.len() == 2 {
                out.push(hex_pair(&nibble));
                nibble.clear();
            }
        }
        if nibble.len() == 1 {
            nibble.push(b'0');
            out.push(hex_pair(&nibble));
        }
        Ok(Value::HexString(out))
    }

    fn parse_array(&mut self) -> Result<Value, ScanError> {
        // `ArrayObject.read_from_stream`.
        self.pos += 1; // '['
        let mut remaining = self.limits.array_max;
        let mut values: Vec<Value> = Vec::new();
        loop {
            let Some(byte) = self.skip_whitespace() else {
                break;
            };
            if byte == b'%' {
                self.check_comment()?;
                continue;
            }
            if byte == b']' {
                self.pos += 1;
                break;
            }
            remaining -= 1;
            if remaining < 0 {
                return Err(ScanError::Limit(
                    "Reached maximum array element count.".to_string(),
                ));
            }
            self.decrement_budget(1, false)?;
            if byte == b'n' && self.remaining().len() >= 4 && &self.remaining()[..4] == b"null" {
                self.pos += 4;
                values.push(Value::Null);
                continue;
            }
            values.push(self.parse_value()?);
        }
        Ok(Value::Array(values))
    }

    fn parse_dictionary(&mut self) -> Result<Value, ScanError> {
        // `DictionaryObject.read_from_stream` (pdf=None, non-strict).
        self.pos += 2; // '<<'
        let mut remaining = self.limits.dictionary_max;
        let mut entries: Vec<(Value, Value)> = Vec::new();
        loop {
            let Some(byte) = self.read_non_whitespace() else {
                return Err(ScanError::Stream(
                    "Stream has ended unexpectedly".to_string(),
                ));
            };
            if byte == b'%' {
                self.pos -= 1;
                self.check_comment()?;
                continue;
            }
            if byte == b'>' {
                self.pos += 1;
                break;
            }
            self.pos -= 1;
            remaining -= 1;
            if remaining < 0 {
                return Err(ScanError::Limit(
                    "Reached maximum number of dictionary entries.".to_string(),
                ));
            }
            self.decrement_budget(2, false)?;
            let key = match self.parse_value() {
                Ok(Value::Null) => break,
                Ok(value @ Value::Name(_)) => value,
                Ok(other) => {
                    self.warnings.push(WarningEvent::NonNameKeyValue(other));
                    continue;
                }
                Err(ScanError::Read(message)) => {
                    self.warnings.push(WarningEvent::KeyParseError(message));
                    continue;
                }
                Err(error) => return Err(error),
            };
            self.skip_whitespace();
            let value = match self.parse_value() {
                Ok(value) => value,
                Err(ScanError::Limit(_)) => return Err(ScanError::Limit(
                    "Reached maximum number of tokens.".to_string(),
                )),
                Err(error) => {
                    // Upstream logs the exception and returns the partial dict.
                    self.warnings
                        .push(WarningEvent::PartialDictionary(Box::new(error)));
                    return Ok(Value::Dictionary(entries));
                }
            };
            if entries.iter().any(|(existing, _)| *existing == key) {
                self.warnings.push(WarningEvent::MultipleDefinition {
                    position: self.pos,
                    key: name_raw_bytes(&key),
                });
            } else {
                entries.push((key, value));
            }
        }
        // A `stream` keyword after a dictionary inside a content stream is not
        // supported by the byte scanner; let the legacy path handle it.
        let save = self.pos;
        let mut probe = self.pos;
        while probe < self.data.len() && is_whitespace(self.data[probe]) {
            probe += 1;
        }
        if self.data[probe..].starts_with(b"stream") {
            self.pos = save;
            return Err(ScanError::Fallback);
        }
        Ok(Value::Dictionary(entries))
    }

    /// Parse one segment: operations up to EOF or the next inline image.
    fn parse_segment_inner(&mut self) -> Result<Segment, ScanError> {
        let mut operands: Vec<Value> = Vec::new();
        loop {
            let Some(peek) = self.skip_whitespace() else {
                break;
            };
            if byte_is_operator_start(peek) {
                let operator = self.read_until_regex_bounded(
                    is_name_delimiter,
                    Some(self.limits.operator_length),
                )?;
                self.token_count += 1;
                if operator == b"BI" {
                    if !operands.is_empty() {
                        return Err(ScanError::Assert);
                    }
                    self.decrement_budget(2, true)?;
                    return Ok(Segment {
                        items: std::mem::take(&mut self.items),
                        warnings: std::mem::take(&mut self.warnings),
                        inline_image_start: Some(self.pos),
                        next_offset: self.pos,
                        token_count: self.token_count,
                        budget_left: self.budget,
                    });
                }
                self.decrement_budget(1, true)?;
                self.items.push(Item::Operation {
                    operands: std::mem::take(&mut operands),
                    operator,
                });
            } else if peek == b'%' {
                self.check_comment()?;
            } else {
                self.decrement_budget(1, true)?;
                operands.push(self.parse_value()?);
            }
        }
        Ok(Segment {
            items: std::mem::take(&mut self.items),
            warnings: std::mem::take(&mut self.warnings),
            inline_image_start: None,
            next_offset: self.pos,
            token_count: self.token_count,
            budget_left: self.budget,
        })
    }

    fn parse_segment(&mut self) -> Result<Segment, Failure> {
        match self.parse_segment_inner() {
            Ok(segment) => Ok(segment),
            Err(error) => Err(Failure {
                error,
                items: std::mem::take(&mut self.items),
                warnings: std::mem::take(&mut self.warnings),
                budget_left: self.budget,
            }),
        }
    }
}

fn name_raw_bytes(value: &Value) -> Vec<u8> {
    match value {
        Value::Name(raw) => raw.clone(),
        _ => Vec::new(),
    }
}

fn number_value(raw: Vec<u8>) -> Value {
    if raw.contains(&b'.') {
        Value::Real(raw)
    } else {
        Value::Integer(raw)
    }
}

fn byte_is_operator_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'\'' || byte == b'"'
}

fn escape_value(byte: u8) -> Option<u8> {
    Some(match byte {
        b'n' => b'\n',
        b'r' => b'\r',
        b't' => b'\t',
        b'b' => 0x08,
        b'f' => 0x0c,
        b'(' => b'(',
        b')' => b')',
        b'/' => b'/',
        b'\\' => b'\\',
        b' ' => b' ',
        b'%' => b'%',
        b'<' => b'<',
        b'>' => b'>',
        b'[' => b'[',
        b']' => b']',
        b'#' => b'#',
        b'_' => b'_',
        b'&' => b'&',
        b'$' => b'$',
        _ => return None,
    })
}

fn hex_pair(digits: &[u8]) -> u8 {
    let high = (digits[0] as char).to_digit(16).unwrap_or(0) as u8;
    let low = (digits[1] as char).to_digit(16).unwrap_or(0) as u8;
    high * 16 + low
}

/// Match `IndirectPattern = rb"[+-]?(\d+)\s+(\d+)\s+R[^a-zA-Z]"` at the start.
fn match_indirect(peek: &[u8]) -> Option<(usize, i64, i64)> {
    let mut index = 0usize;
    let mut sign = 1i64;
    if peek.first().is_some_and(|byte| *byte == b'+' || *byte == b'-') {
        sign = if peek[0] == b'-' { -1 } else { 1 };
        index += 1;
    }
    let idnum_start = index;
    while index < peek.len() && peek[index].is_ascii_digit() {
        index += 1;
    }
    if index == idnum_start {
        return None;
    }
    let idnum: i64 = std::str::from_utf8(&peek[idnum_start..index]).ok()?.parse::<i64>().ok()? * sign;
    let ws_start = index;
    while index < peek.len() && is_regex_whitespace(peek[index]) {
        index += 1;
    }
    if index == ws_start {
        return None;
    }
    let generation_start = index;
    while index < peek.len() && peek[index].is_ascii_digit() {
        index += 1;
    }
    if index == generation_start {
        return None;
    }
    let generation: i64 = std::str::from_utf8(&peek[generation_start..index]).ok()?.parse::<i64>().ok()?;
    let ws_start = index;
    while index < peek.len() && is_regex_whitespace(peek[index]) {
        index += 1;
    }
    if index == ws_start || index >= peek.len() || peek[index] != b'R' {
        return None;
    }
    index += 1;
    // `[^a-zA-Z]` must follow (consumed by the regex, hence `end - 1` upstream).
    if index >= peek.len() || peek[index].is_ascii_alphabetic() {
        return None;
    }
    Some((index + 1, idnum, generation))
}

/// `repr(bytes)` exactly like CPython (quote selection + escapes).
pub fn python_bytes_repr(bytes: &[u8]) -> String {
    let has_single = bytes.contains(&b'\'');
    let has_double = bytes.contains(&b'"');
    let quote = if has_single && !has_double { b'"' } else { b'\'' };
    let mut out = String::from("b");
    out.push(quote as char);
    for &byte in bytes {
        match byte {
            b'\\' => out.push_str("\\\\"),
            b'\t' => out.push_str("\\t"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            _ if byte == quote => {
                out.push('\\');
                out.push(byte as char);
            }
            0x20..=0x7e => out.push(byte as char),
            _ => out.push_str(&format!("\\x{byte:02x}")),
        }
    }
    out.push(quote as char);
    out
}

/// Result of parsing a whole content stream (no inline images supported).
pub fn parse(data: &[u8], limits: Limits) -> Result<(Vec<Item>, Vec<WarningEvent>), Failure> {
    let mut parser = Parser::new(data, limits, 0);
    let segment = parser.parse_segment()?;
    if segment.inline_image_start.is_some() {
        return Err(Failure {
            error: ScanError::Fallback,
            items: segment.items,
            warnings: segment.warnings,
            budget_left: segment.budget_left,
        });
    }
    Ok((segment.items, segment.warnings))
}

/// Parse one segment starting at `offset`.  When `inline_image_start` is set the
/// caller must handle the inline image and call again with `next_offset` set to
/// the position after the image data.
pub fn parse_segment(data: &[u8], limits: Limits, offset: usize) -> Result<Segment, Failure> {
    let mut parser = Parser::new(data, limits, offset);
    parser.parse_segment()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(input: &[u8]) -> Vec<Item> {
        parse(input, Limits::default()).expect("parse").0
    }

    #[test]
    fn simple_operators_and_numbers() {
        let items = ops(b"0 0 612 792 re\nBT /F1 12 Tf (Hello) Tj ET");
        assert_eq!(items.len(), 5);
        match &items[0] {
            Item::Operation { operands, operator } => {
                assert_eq!(operator, b"re");
                assert_eq!(operands.len(), 4);
                assert_eq!(operands[0], Value::Integer(b"0".to_vec()));
            }
            other => panic!("{other:?}"),
        }
        match &items[1] {
            Item::Operation { operands, operator } => {
                assert_eq!(operator, b"BT");
                assert!(operands.is_empty());
            }
            other => panic!("{other:?}"),
        }
        match &items[2] {
            Item::Operation { operands, operator } => {
                assert_eq!(operator, b"Tf");
                assert_eq!(operands[0], Value::Name(b"F1".to_vec()));
                assert_eq!(operands[1], Value::Integer(b"12".to_vec()));
            }
            other => panic!("{other:?}"),
        }
        match &items[3] {
            Item::Operation { operands, operator } => {
                assert_eq!(operator, b"Tj");
                assert_eq!(operands[0], Value::LiteralString(b"Hello".to_vec()));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn empty_and_whitespace_only() {
        assert!(ops(b"").is_empty());
        assert!(ops(b"  \r\n\t\x00\x0c").is_empty());
        assert!(ops(b"% only a comment\n").is_empty());
    }

    #[test]
    fn literal_string_escapes() {
        let items = ops(b"(a\\nb\\101c) Tj");
        match &items[0] {
            Item::Operation { operands, .. } => {
                assert_eq!(operands[0], Value::LiteralString(b"a\nbAc".to_vec()));
            }
            other => panic!("{other:?}"),
        }
        // Nested parentheses stay part of the string.
        let items = ops(b"(a(b)c) Tj");
        match &items[0] {
            Item::Operation { operands, .. } => {
                assert_eq!(operands[0], Value::LiteralString(b"a(b)c".to_vec()));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn strings_like_upstream() {
        // Escape dict and line continuation.
        let items = ops(b"(a\\\r\nb) Tj");
        match &items[0] {
            Item::Operation { operands, .. } => {
                assert_eq!(operands[0], Value::LiteralString(b"ab".to_vec()));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn hex_strings() {
        let items = ops(b"<616263> Tj <61 62  63> Tj");
        for item in &items {
            match item {
                Item::Operation { operands, .. } => {
                    assert_eq!(operands[0], Value::HexString(b"abc".to_vec()));
                }
                other => panic!("{other:?}"),
            }
        }
        // Odd number of nibbles is padded with a trailing zero.
        let items = ops(b"<616> Tj");
        match &items[0] {
            Item::Operation { operands, .. } => {
                assert_eq!(operands[0], Value::HexString(vec![0x61, 0x60]));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn arrays_and_dictionaries() {
        let items = ops(b"[1 2.5 /N (s)] TJ");
        match &items[0] {
            Item::Operation { operands, .. } => {
                assert_eq!(
                    operands[0],
                    Value::Array(vec![
                        Value::Integer(b"1".to_vec()),
                        Value::Real(b"2.5".to_vec()),
                        Value::Name(b"N".to_vec()),
                        Value::LiteralString(b"s".to_vec()),
                    ])
                );
            }
            other => panic!("{other:?}"),
        }
        let items = ops(b"<< /Type /OC /MCID 0 >> BDC");
        match &items[0] {
            Item::Operation { operands, .. } => {
                assert_eq!(
                    operands[0],
                    Value::Dictionary(vec![
                        (Value::Name(b"Type".to_vec()), Value::Name(b"OC".to_vec())),
                        (Value::Name(b"MCID".to_vec()), Value::Integer(b"0".to_vec())),
                    ])
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn numbers_and_indirect() {
        // Without a trailing non-letter character the `IndirectPattern` does not
        // match, so upstream parses `R` as an operator (kept for parity).
        let items = ops(b"12 0 R");
        assert_eq!(items.len(), 1);
        match &items[0] {
            Item::Operation { operands, operator } => {
                assert_eq!(operator, b"R");
                assert_eq!(operands.len(), 2);
            }
            other => panic!("{other:?}"),
        }
        let items = ops(b"1.5 0.5 12 m");
        match &items[0] {
            Item::Operation { operands, operator } => {
                assert_eq!(operator, b"m");
                assert_eq!(operands[0], Value::Real(b"1.5".to_vec()));
                assert_eq!(operands[1], Value::Real(b"0.5".to_vec()));
                assert_eq!(operands[2], Value::Integer(b"12".to_vec()));
            }
            other => panic!("{other:?}"),
        }
        // Indirect reference as an operand (with a trailing delimiter).
        let items = ops(b"1 0 R Do");
        match &items[0] {
            Item::Operation { operands, operator } => {
                assert_eq!(operator, b"Do");
                assert_eq!(
                    operands[0],
                    Value::Indirect {
                        idnum: 1,
                        generation: 0
                    }
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn booleans_and_null_inside_array() {
        // In content streams, a leading letter makes the token an operator, so
        // `true`/`false`/`null` reach the value parser only inside arrays.
        let items = ops(b"[true false null] TJ");
        match &items[0] {
            Item::Operation { operands, .. } => {
                assert_eq!(
                    operands[0],
                    Value::Array(vec![
                        Value::Boolean(true),
                        Value::Boolean(false),
                        Value::Null,
                    ])
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn inline_image_reports_position() {
        let segment = parse_segment(b"q BI /W 1 /H 1 /CS /G /BPC 8 ID \x00 EI Q", Limits::default(), 0)
            .expect("segment");
        assert!(segment.inline_image_start.is_some());
        assert_eq!(segment.items.len(), 1); // q
    }

    #[test]
    fn operator_length_limit() {
        let long = vec![b'A'; 200];
        let error = parse(&long, Limits::default()).unwrap_err().error;
        match error {
            ScanError::Limit(message) => assert!(message.contains("exceeds maximum allowed length of 128")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn token_budget_limit() {
        let mut limits = Limits::default();
        limits.budget = 3;
        let error = parse(b"1 2 3 4 5 m", limits).unwrap_err().error;
        assert_eq!(
            error,
            ScanError::Limit("Reached maximum number of tokens per stream.".to_string())
        );
    }

    #[test]
    fn fallback_only_for_stream_keyword() {
        assert!(parse(b"<< /A 1 >> BDC", Limits::default()).is_ok());
        assert_eq!(
            parse(b"<< /A 1 >> stream x", Limits::default()).map(|_| ()).unwrap_err().error,
            ScanError::Fallback
        );
    }

    #[test]
    fn fuzz_never_panics() {
        // Deterministic pseudo-random byte soup; the parser must never panic.
        let mut state = 0x1234_5678_9abc_def0u64;
        for length in [0usize, 1, 2, 3, 5, 8, 13, 21, 64, 256, 1024] {
            let mut data = Vec::with_capacity(length);
            for _ in 0..length {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                data.push((state >> 33) as u8);
            }
            let _ = parse(&data, Limits::default());
            let _ = parse_segment(&data, Limits::default(), 0);
        }
    }
}
