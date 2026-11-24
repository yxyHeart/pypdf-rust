//! Pure-Rust algorithms used by the pypdf port.
//!
//! Everything in this crate is deterministic and free of Python dependencies so
//! it can be unit-tested and differentially verified against CPython directly.
//! The Python-facing object model lives in `pypdf-py`.

/// Upstream pypdf release this port tracks.
pub const PYPDF_VERSION: &str = "7.1.1";

pub mod prng;
pub mod pyfmt;
