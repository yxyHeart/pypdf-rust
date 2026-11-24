//! PyO3 bindings for the pypdf 7.1.1 Rust port.
//!
//! The Python-visible object model is Python-native (real `dict`/`list`
//! subclasses for containers, thin Python skeletons for scalar subclasses); this
//! extension module hosts the Rust implementations.

use pyo3::prelude::*;

/// Version of the upstream release this port tracks.
pub const PYPDF_VERSION: &str = "7.1.1";

#[pymodule]
fn _pypdf(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", PYPDF_VERSION)?;
    Ok(())
}
