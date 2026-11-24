//! Dump Python-compatible formatting for the f64 bit patterns given on stdin.
//!
//! Used by tools/probe/pyfmt_crosscheck.py to validate far more values than the
//! committed golden file contains.

use std::io::{self, BufRead, Write};

use pypdf_rs_core::pyfmt::{float_myrepr, py_float_fixed, py_float_repr};

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    for line in stdin.lock().lines() {
        let line = line?;
        let hex = line.trim();
        if hex.is_empty() {
            continue;
        }
        let bits = u64::from_str_radix(hex, 16).expect("hex bits");
        let value = f64::from_bits(bits);
        let myrepr = match float_myrepr(value) {
            Ok(text) => text,
            Err(pypdf_rs_core::pyfmt::MyReprError::Overflow) => "!OverflowError".to_string(),
            Err(pypdf_rs_core::pyfmt::MyReprError::ValueError) => "!ValueError".to_string(),
        };
        writeln!(
            out,
            "{hex}\t{}\t{}\t{}\t{}",
            py_float_repr(value),
            py_float_fixed(value, 8),
            py_float_fixed(value, 2),
            myrepr
        )?;
    }
    Ok(())
}
