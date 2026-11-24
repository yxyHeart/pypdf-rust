//! PyO3 bindings for the pypdf 7.1.1 Rust port.
//!
//! The Python-visible object model is Python-native (real `dict`/`list`
//! subclasses for containers, thin Python skeletons for scalar subclasses); this
//! extension module hosts the Rust implementations and is re-exported by the
//! `pypdf` package shims under `python/pypdf/`.
//!
//! Ported upstream modules live in `src/modules/`; `build.rs` compiles them and
//! wires up their `register()` functions, so adding a ported module never
//! requires editing this file.

use pyo3::prelude::*;

pub mod errors;
pub mod pyutil;

include!(concat!(env!("OUT_DIR"), "/modules_generated.rs"));

/// Version of the upstream release this port tracks.
pub const PYPDF_VERSION: &str = pypdf_rs_core::PYPDF_VERSION;

#[pymodule]
fn _pypdf(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", PYPDF_VERSION)?;
    register_all(module)?;
    Ok(())
}
