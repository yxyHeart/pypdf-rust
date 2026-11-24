//! Differential golden tests against CPython.
//!
//! Vectors are produced by `tools/probe/gen_pyfmt_golden.py` from the oracle
//! Python (CPython 3.14) and must be regenerated whenever the generator changes.

use pypdf_rs_core::prng::SplitMix64;
use pypdf_rs_core::pyfmt::{
    float_myrepr, py_float_fixed, py_float_repr, MyReprError,
};

const PYFMT_GOLDEN: &str = include_str!("data/pyfmt_golden.tsv");
const PRNG_GOLDEN: &str = include_str!("data/prng_golden.tsv");

fn bits_to_float(hex: &str) -> f64 {
    let raw = u64::from_str_radix(hex, 16).expect("hex bits");
    f64::from_bits(raw)
}

#[test]
fn float_formatting_matches_cpython() {
    let mut checked = 0usize;
    for (index, line) in PYFMT_GOLDEN.lines().enumerate() {
        if index == 0 || line.trim().is_empty() {
            continue; // header
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 9, "malformed golden line {index}: {line}");
        let value = bits_to_float(fields[0]);
        let context = format!("bits={} ({line})", fields[0]);

        assert_eq!(py_float_repr(value), fields[1], "repr mismatch {context}");
        assert_eq!(py_float_repr(value), fields[2], "str mismatch {context}");
        assert_eq!(py_float_repr(value), fields[1], "repr/str disagree {context}");

        match float_myrepr(value) {
            Ok(text) => assert_eq!(text, fields[3], "myrepr mismatch {context}"),
            Err(MyReprError::Overflow) => {
                assert_eq!(fields[3], "!OverflowError", "myrepr error mismatch {context}")
            }
            Err(MyReprError::ValueError) => {
                assert_eq!(fields[3], "!ValueError", "myrepr error mismatch {context}")
            }
        }

        for (column, precision) in [(4usize, 1usize), (5, 2), (6, 4), (7, 6), (8, 8)] {
            assert_eq!(
                py_float_fixed(value, precision),
                fields[column],
                "fixed({precision}) mismatch {context}"
            );
        }
        checked += 1;
    }
    assert!(checked > 6000, "expected thousands of vectors, got {checked}");
}

#[test]
fn prng_matches_python_stream() {
    let mut checked = 0usize;
    for (index, line) in PRNG_GOLDEN.lines().enumerate() {
        if index == 0 || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 7, "malformed prng golden line: {line}");
        let seed: u64 = fields[0].parse().expect("seed");

        let mut stream = SplitMix64::new(seed);
        for (column, text) in fields.iter().enumerate().skip(1).take(4) {
            let expected: u64 = text.parse().expect("u64");
            assert_eq!(stream.next_u64(), expected, "seed {seed} word {column}");
        }

        let mut tokens = SplitMix64::new(seed);
        assert_eq!(
            tokens.token_bytes(16),
            hex_to_bytes(fields[5]),
            "seed {seed} token_bytes(16)"
        );

        let mut uuids = SplitMix64::new(seed);
        assert_eq!(uuids.uuid4_string(), fields[6], "seed {seed} uuid4");
        checked += 1;
    }
    assert!(checked >= 6, "expected prng vectors, got {checked}");
}

fn hex_to_bytes(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}
