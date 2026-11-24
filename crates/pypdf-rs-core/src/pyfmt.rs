//! CPython-compatible formatting primitives.
//!
//! Writer output is byte-compared against pypdf, and pypdf writes floats with
//! Python's formatting rules (`FloatObject.myrepr`, `f"{value:.{n}f}"`).  This
//! module reproduces them exactly; `tests/golden.rs` checks thousands of values
//! generated from CPython.

/// Python `repr(float)` / `str(float)`.
///
/// CPython uses the shortest decimal representation that round-trips, printed
/// in fixed notation when the decimal point position `decpt` satisfies
/// `-4 < decpt <= 16`, and in scientific notation otherwise.
#[must_use]
pub fn py_float_repr(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "inf"
        } else {
            "-inf"
        }
        .to_string();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_string();
    }

    let (negative, digits, decpt) = shortest_digits(value);
    let digits = digits.as_str();
    let decpt: i32 = decpt;
    let len = digits.len() as i32;
    let mut out = String::with_capacity(digits.len() + 8);
    if negative {
        out.push('-');
    }

    // CPython renders in scientific notation when decpt <= -4 or decpt > 16,
    // where decpt is the position of the decimal point relative to the first
    // digit (1.5 -> decpt 1, 0.5 -> decpt 0).
    if decpt <= -4 || decpt > 16 {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let exponent: i32 = decpt - 1;
        out.push('e');
        out.push(if exponent < 0 { '-' } else { '+' });
        let abs = exponent.unsigned_abs();
        if abs < 10 {
            out.push('0');
        }
        out.push_str(&abs.to_string());
        return out;
    }

    if decpt <= 0 {
        out.push_str("0.");
        for _ in 0..(-decpt) {
            out.push('0');
        }
        out.push_str(digits);
    } else if decpt >= len {
        out.push_str(digits);
        for _ in 0..(decpt - len) {
            out.push('0');
        }
        out.push_str(".0");
    } else {
        let split = decpt as usize;
        out.push_str(&digits[..split]);
        out.push('.');
        out.push_str(&digits[split..]);
    }
    out
}

/// Shortest round-tripping decimal digits, matching CPython's `dtoa` mode 0.
///
/// `ryu` is used instead of the standard library formatter because the latter
/// breaks ties differently: for `0x431545a586150045` Rust's `{:?}` prints
/// `1496887933091857.3` while both CPython and `ryu` print `...57.2`.
///
/// Returns `(negative, digits, decpt)`: `digits` has no leading/trailing zeros
/// and `decpt` is the position of the decimal point (value = 0.digits * 10^decpt).
fn shortest_digits(value: f64) -> (bool, String, i32) {
    let negative = value.is_sign_negative();
    let mut buffer = ryu::Buffer::new();
    let text = buffer.format(value.abs()).to_string();
    let (mantissa, exponent) = match text.split_once('e') {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().expect("exponent")),
        None => (text.as_str(), 0),
    };
    let (integer, fraction) = match mantissa.split_once('.') {
        Some((integer, fraction)) => (integer, fraction),
        None => (mantissa, ""),
    };

    if integer != "0" {
        let mut digits = format!("{integer}{fraction}");
        while digits.ends_with('0') {
            digits.pop();
        }
        let decpt = integer.len() as i32 + exponent;
        (negative, digits, decpt)
    } else {
        // 0.<fraction>: locate the first significant digit.
        match fraction.find(|c: char| c != '0') {
            Some(index) => {
                let mut digits = fraction[index..].to_string();
                while digits.ends_with('0') {
                    digits.pop();
                }
                let decpt = exponent - index as i32;
                (negative, digits, decpt)
            }
            None => (negative, "0".to_string(), 1),
        }
    }
}

/// Python `f"{value:.{precision}f}"`.
///
/// Rust's formatter performs correct (round-half-even) decimal conversion of the
/// exact binary value, which is what CPython does as well.
#[must_use]
pub fn py_float_fixed(value: f64, precision: usize) -> String {
    // Rust renders non-finite values as "NaN"/"inf"; CPython always uses
    // lowercase "nan" (sign dropped) and "inf"/"-inf".
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "inf"
        } else {
            "-inf"
        }
        .to_string();
    }
    format!("{value:.precision$}")
}

/// Failure modes of `FloatObject.myrepr()` for non-finite values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MyReprError {
    /// `int(log10(inf))` raises `OverflowError` in CPython.
    Overflow,
    /// `int(log10(nan))` raises `ValueError` in CPython.
    ValueError,
}

/// pypdf's `FloatObject.myrepr()`.
///
/// ```text
/// if self == 0: return "0.0"
/// nb = FLOAT_WRITE_PRECISION - int(log10(abs(self)))   # FLOAT_WRITE_PRECISION = 8
/// return f"{self:.{max(1, nb)}f}".rstrip("0").rstrip(".")
/// ```
pub fn float_myrepr(value: f64) -> Result<String, MyReprError> {
    const FLOAT_WRITE_PRECISION: i32 = 8;

    if value == 0.0 {
        return Ok("0.0".to_string());
    }
    let magnitude = value.abs();
    if magnitude.is_infinite() {
        return Err(MyReprError::Overflow);
    }
    if magnitude.is_nan() {
        return Err(MyReprError::ValueError);
    }
    // CPython's int() truncates towards zero, same as an `as i32` cast.
    let nb = FLOAT_WRITE_PRECISION - (magnitude.log10() as i32);
    let precision = nb.max(1) as usize;
    let text = py_float_fixed(value, precision);
    let text = text.trim_end_matches('0');
    let text = text.trim_end_matches('.');
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repr_special_values() {
        assert_eq!(py_float_repr(0.0), "0.0");
        assert_eq!(py_float_repr(-0.0), "-0.0");
        assert_eq!(py_float_repr(f64::INFINITY), "inf");
        assert_eq!(py_float_repr(f64::NEG_INFINITY), "-inf");
        assert_eq!(py_float_repr(f64::NAN), "nan");
    }

    #[test]
    fn repr_scientific_threshold() {
        assert_eq!(py_float_repr(1e15), "1000000000000000.0");
        assert_eq!(py_float_repr(1e16), "1e+16");
        assert_eq!(py_float_repr(1e-4), "0.0001");
        assert_eq!(py_float_repr(1e-5), "1e-05");
        assert_eq!(py_float_repr(1.5e16), "1.5e+16");
    }

    #[test]
    fn myrepr_matches_pypdf() {
        assert_eq!(float_myrepr(0.0).unwrap(), "0.0");
        assert_eq!(float_myrepr(-0.0).unwrap(), "0.0");
        assert_eq!(float_myrepr(1.0).unwrap(), "1");
        assert_eq!(float_myrepr(100.0).unwrap(), "100");
        assert_eq!(float_myrepr(0.5).unwrap(), "0.5");
        assert_eq!(float_myrepr(1.25).unwrap(), "1.25");
        assert_eq!(float_myrepr(595.2755905511812).unwrap(), "595.275591");
        assert_eq!(float_myrepr(f64::INFINITY), Err(MyReprError::Overflow));
        assert_eq!(float_myrepr(f64::NAN), Err(MyReprError::ValueError));
    }
}
