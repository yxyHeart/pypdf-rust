//! Rust port of `pypdf/_crypt_providers/` (upstream pypdf 7.1.1).
//!
//! Four upstream modules are covered:
//!
//! * `pypdf._crypt_providers._base` -- `CryptBase` / `CryptIdentity`
//! * `pypdf._crypt_providers._cryptography` -- the `cryptography` provider
//! * `pypdf._crypt_providers._fallback` -- the pure-Rust RC4 fallback provider
//! * `pypdf._crypt_providers._pycryptodome` -- the `pycryptodome` provider
//!
//! As everywhere else in the port the classes themselves stay Python (see
//! `docs/PORTING_GUIDE.md` §1): `python/pypdf/_crypt_providers/*.py` keeps the
//! upstream class definitions (including `CryptRC4._is_rc4_supported`, which the
//! official tests monkeypatch, and the `cipher` attribute the tests replace with
//! a rejecting stub) and every method body is a one-line delegation here.
//!
//! Dependency parity (`docs/FIDELITY_CONTRACT.md` §5): the `cryptography`
//! provider does **not** reimplement AES/RC4 in Rust.  It calls the very same
//! `cryptography` objects upstream calls (`ARC4`, `Cipher`, `PKCS7`, `CBC`,
//! `ECB`), through PyO3, so ciphertext bytes and error behaviour
//! (`UnsupportedAlgorithm`, `ValueError` from padding/unpadding, the invalid key
//! size `ValueError` from `ARC4(key)`, ...) are identical by construction.  The
//! same holds for `pycryptodome` (`Crypto.Cipher.AES/ARC4`,
//! `Crypto.Util.Padding`): the module keeps upstream's *lazy* `from Crypto import
//! __version__` behaviour, so it still raises `ImportError` when `Crypto` is
//! absent, while the implementation drives `Crypto` itself.
//!
//! Randomness (AES-CBC IVs) is routed through the unified PRNG entry point
//! documented in `docs/FIDELITY_CONTRACT.md` §3 -- see [`token_bytes`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use pyo3::exceptions::PyZeroDivisionError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyByteArray, PyBytes, PyDict, PyModule, PyTuple};

use pypdf_rs_core::prng::SplitMix64;

use crate::errors;
use crate::pyutil;

/// `pypdf/_crypt_providers/_cryptography.py`
const CRYPTOGRAPHY_MODULE: &str = "pypdf._crypt_providers._cryptography";
/// `pypdf/_crypt_providers/_fallback.py`
const FALLBACK_MODULE: &str = "pypdf._crypt_providers._fallback";
/// `pypdf/_crypt_providers/_pycryptodome.py`
const PYCROPTODOME_MODULE: &str = "pypdf._crypt_providers._pycryptodome";
/// `cryptography.exceptions`
const CRYPTOGRAPHY_EXCEPTIONS: &str = "cryptography.exceptions";

/// Upstream `_fallback._DEPENDENCY_ERROR_STR` (exported to Python as well).
const DEPENDENCY_ERROR_STR: &str = "cryptography>=3.1 is required for AES algorithm";

// Module-level constants and the class objects themselves stay in the Python
// shims (data / class attributes / `crypt_provider` tuples); their coverage
// annotations are recorded here (gate A6):
// @pypdf: pypdf._crypt_providers._cryptography.crypt_provider
// @pypdf: pypdf._crypt_providers._cryptography.CryptRC4
// @pypdf: pypdf._crypt_providers._cryptography.CryptRC4._is_rc4_supported
// @pypdf: pypdf._crypt_providers._cryptography.CryptAES
// @pypdf: pypdf._crypt_providers._fallback._DEPENDENCY_ERROR_STR
// @pypdf: pypdf._crypt_providers._fallback.crypt_provider
// @pypdf: pypdf._crypt_providers._fallback.CryptRC4
// @pypdf: pypdf._crypt_providers._fallback.CryptAES
// @pypdf: pypdf._crypt_providers._pycryptodome.crypt_provider
// @pypdf: pypdf._crypt_providers._pycryptodome.CryptRC4
// @pypdf: pypdf._crypt_providers._pycryptodome.CryptAES

/// Message of `CryptRC4._disable_rc4` (upstream: parenthesised string literal).
const RC4_FALLBACK_WARNING: &str = "RC4 is not supported by the current OpenSSL build; \
     falling back to the pure-Python RC4 implementation.";

/// Environment variable selecting the deterministic PRNG stream of the Rust
/// side for differential runs (`docs/FIDELITY_CONTRACT.md` §3).  Accepts a
/// decimal or `0x`-prefixed hexadecimal seed.
const PRNG_SEED_ENV: &str = "PYPDF_RS_PRNG_SEED";

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn module<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyModule>> {
    py.import(name)
}

/// A module-level global of one of the provider shims (upstream resolves these
/// module globals at call time; keeping the lookup dynamic preserves
/// monkeypatch behaviour of e.g. `Cipher` / `CryptRC4`).
fn shim_attr<'py>(py: Python<'py>, module_name: &str, name: &str) -> PyResult<Bound<'py, PyAny>> {
    module(py, module_name)?.getattr(name)
}

fn py_bytes<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, data).into_any()
}

/// `bytes(data)` for the bytes-like inputs the providers accept (`bytes`,
/// `bytearray`, ...).  The same `TypeError` as upstream's `int.from_bytes` /
/// cipher call is produced for anything the CPython buffer protocol rejects.
fn as_bytes(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    obj.extract::<Vec<u8>>()
}

/// `isinstance(error, <module>.<class>)` for a caught `PyErr`.
fn err_is_instance(
    py: Python<'_>,
    error: &PyErr,
    module_name: &str,
    class_name: &str,
) -> PyResult<bool> {
    let class = shim_attr(py, module_name, class_name)?;
    error.value(py).is_instance(&class)
}

/// `raise PdfStreamError(exception) from exception` (upstream padding errors).
fn stream_error_from(py: Python<'_>, cause: PyErr) -> PyErr {
    let value = cause.value(py).clone();
    let args = match PyTuple::new(py, [value]) {
        Ok(args) => args,
        Err(error) => return error,
    };
    let error = errors::raise(py, "PdfStreamError", args);
    error.set_cause(py, Some(cause));
    error
}

/// `logger_warning(..., source=__name__)` with the provider module name.
fn warn(
    py: Python<'_>,
    source: &str,
    message: &str,
    values: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    pyutil::logger_warning(py, source, message, values)
}

// ---------------------------------------------------------------------------
// Deterministic PRNG entry point (`docs/FIDELITY_CONTRACT.md` §3)
// ---------------------------------------------------------------------------

/// Deterministic splitmix64 stream, enabled through
/// `PYPDF_RS_PRNG_SEED` or `cp_prng_set_seed()`.
static PRNG: Mutex<Option<SplitMix64>> = Mutex::new(None);
/// Whether the environment variable has been consulted already.
static PRNG_ENV_CHECKED: AtomicBool = AtomicBool::new(false);

fn parse_seed(text: &str) -> Option<u64> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        text.parse::<u64>().ok()
    }
}

fn seeded_token_bytes(count: usize) -> Option<Vec<u8>> {
    let mut guard = match PRNG.lock() {
        Ok(guard) => guard,
        // A poisoned lock only means another thread panicked while holding it;
        // the PRNG state itself is still usable.
        Err(poisoned) => poisoned.into_inner(),
    };
    if guard.is_none() && !PRNG_ENV_CHECKED.swap(true, Ordering::SeqCst) {
        if let Ok(value) = std::env::var(PRNG_SEED_ENV) {
            if let Some(seed) = parse_seed(&value) {
                *guard = Some(SplitMix64::new(seed));
            }
        }
    }
    guard.as_mut().map(|rng| rng.token_bytes(count))
}

/// `secrets.token_bytes(n)` of the reference implementation.
///
/// In differential runs the Python side replaces `secrets.token_bytes` with the
/// deterministic splitmix64 stream (`tools/oracle/prng.py`), so delegating to
/// that same module reproduces the reference byte stream at the same call
/// sites.  Production runs get the system entropy of `secrets`.  When an
/// explicit seed is configured (`PYPDF_RS_PRNG_SEED` / `cp_prng_set_seed`), the
/// Rust `SplitMix64` of `pypdf_rs_core::prng` is consumed instead so the Rust
/// side is deterministic without depending on the Python patch.
pub fn token_bytes(py: Python<'_>, count: usize) -> PyResult<Vec<u8>> {
    if let Some(bytes) = seeded_token_bytes(count) {
        return Ok(bytes);
    }
    let secrets = py.import("secrets")?;
    let value = secrets.call_method1("token_bytes", (count,))?;
    as_bytes(&value)
}

/// Configure (or clear, with `None`) the deterministic splitmix64 stream.
// @pypdf-module: pypdf._crypt_providers._base
// @pypdf-module: pypdf._crypt_providers._cryptography
// @pypdf-module: pypdf._crypt_providers._fallback
// @pypdf-module: pypdf._crypt_providers._pycryptodome
#[pyfunction]
fn cp_prng_set_seed(seed: Option<u64>) {
    let mut guard = match PRNG.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = seed.map(SplitMix64::new);
    // An explicit configuration always wins over the environment variable,
    // including when it is cleared back to system entropy.
    PRNG_ENV_CHECKED.store(true, Ordering::SeqCst);
}

// ---------------------------------------------------------------------------
// pypdf._crypt_providers._base
// ---------------------------------------------------------------------------

// The two classes live in `python/pypdf/_crypt_providers/_base.py` (upstream
// `CryptBase`, `CryptIdentity(CryptBase)`); the methods below are their bodies.
// @pypdf: pypdf._crypt_providers._base.CryptBase
// @pypdf: pypdf._crypt_providers._base.CryptIdentity

/// `CryptBase.encrypt` -- identity, returns the very same object.
// @pypdf: pypdf._crypt_providers._base.CryptBase.encrypt
#[pyfunction]
fn cp_cryptbase_encrypt(data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(data.clone().unbind())
}

/// `CryptBase.decrypt` -- identity, returns the very same object.
// @pypdf: pypdf._crypt_providers._base.CryptBase.decrypt
#[pyfunction]
fn cp_cryptbase_decrypt(data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(data.clone().unbind())
}

// ---------------------------------------------------------------------------
// pypdf._crypt_providers._cryptography
// ---------------------------------------------------------------------------

/// `Cipher(ARC4(key), mode=None).encryptor()` for the cryptography provider.
fn arc4_encryptor<'py>(py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let cipher_class = shim_attr(py, CRYPTOGRAPHY_MODULE, "Cipher")?;
    let arc4 = shim_attr(py, CRYPTOGRAPHY_MODULE, "ARC4")?.call1((key,))?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("mode", py.None())?;
    let cipher = cipher_class.call((arc4,), Some(&kwargs))?;
    cipher.call_method0("encryptor")
}

/// `Cipher(ARC4(key), mode=None).decryptor()` for the cryptography provider.
fn arc4_decryptor<'py>(py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let cipher_class = shim_attr(py, CRYPTOGRAPHY_MODULE, "Cipher")?;
    let arc4 = shim_attr(py, CRYPTOGRAPHY_MODULE, "ARC4")?.call1((key,))?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("mode", py.None())?;
    let cipher = cipher_class.call((arc4,), Some(&kwargs))?;
    cipher.call_method0("decryptor")
}

/// `cryptography`'s `Cipher(<algorithm>, mode=<mode>)` -- the algorithm object
/// is used as-is (an `AES(key)` instance for the class API, a fresh `AES(key)`
/// for the module-level helpers).
fn cipher_with_algorithm<'py>(
    py: Python<'py>,
    algorithm: &Bound<'py, PyAny>,
    mode_name: &str,
    iv: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let cipher_class = shim_attr(py, CRYPTOGRAPHY_MODULE, "Cipher")?;
    let mode_class = shim_attr(py, CRYPTOGRAPHY_MODULE, mode_name)?;
    let mode = match iv {
        Some(iv) => mode_class.call1((iv,))?,
        None => mode_class.call0()?,
    };
    let kwargs = PyDict::new(py);
    kwargs.set_item("mode", mode)?;
    cipher_class.call((algorithm,), Some(&kwargs))
}

/// `cryptography`'s `Cipher(AES(key), mode=<mode>)`.
fn aes_cipher<'py>(
    py: Python<'py>,
    key: &Bound<'py, PyAny>,
    mode_name: &str,
    iv: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let aes = shim_attr(py, CRYPTOGRAPHY_MODULE, "AES")?.call1((key,))?;
    cipher_with_algorithm(py, &aes, mode_name, iv)
}

/// `encryptor.update(data) + encryptor.finalize()`.
fn update_finalize(encryptor: &Bound<'_, PyAny>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let updated = encryptor.call_method1("update", (data,))?;
    let finalized = encryptor.call_method0("finalize")?;
    Ok(pyutil::py_add(&updated, &finalized)?.unbind())
}

/// `CryptRC4.__init__` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.CryptRC4.__init__
#[pyfunction]
fn cp_cryptography_cryptrc4_init(
    instance: &Bound<'_, PyAny>,
    key: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let py = instance.py();
    instance.setattr("_key", key)?;
    instance.setattr("_fallback", py.None())?;
    // Upstream references the module global `CryptRC4`, so the class attribute
    // is consulted on `CryptRC4` itself (tests monkeypatch it there).
    let class = shim_attr(py, CRYPTOGRAPHY_MODULE, "CryptRC4")?;
    if class.getattr("_is_rc4_supported")?.is_truthy()? {
        let cipher_class = shim_attr(py, CRYPTOGRAPHY_MODULE, "Cipher")?;
        let arc4 = shim_attr(py, CRYPTOGRAPHY_MODULE, "ARC4")?.call1((key,))?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("mode", py.None())?;
        let cipher = cipher_class.call((arc4,), Some(&kwargs))?;
        instance.setattr("cipher", cipher)?;
    } else {
        // Upstream pre-creates the pure-Python fallback when RC4 is known to be
        // unsupported, so no failing cipher call is paid for this instance.
        let fallback = instance.call_method1("_pure_python_rc4", (key,))?;
        instance.setattr("_fallback", fallback)?;
    }
    Ok(())
}

/// `CryptRC4._pure_python_rc4` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.CryptRC4._pure_python_rc4
#[pyfunction]
fn cp_cryptography_cryptrc4_pure_python_rc4(key: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let fallback = module(py, FALLBACK_MODULE)?;
    Ok(fallback.getattr("CryptRC4")?.call1((key,))?.unbind())
}

/// `CryptRC4._disable_rc4` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.CryptRC4._disable_rc4
#[pyfunction]
fn cp_cryptography_cryptrc4_disable_rc4(
    class: &Bound<'_, PyAny>,
    key: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = class.py();
    if class.getattr("_is_rc4_supported")?.is_truthy()? {
        warn(py, CRYPTOGRAPHY_MODULE, RC4_FALLBACK_WARNING, None)?;
        class.setattr("_is_rc4_supported", false)?;
    }
    Ok(class.getattr("_pure_python_rc4")?.call1((key,))?.unbind())
}

/// `CryptRC4.encrypt` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.CryptRC4.encrypt
#[pyfunction]
fn cp_cryptography_cryptrc4_encrypt(
    instance: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = instance.py();
    let fallback = instance.getattr("_fallback")?;
    if !fallback.is_none() {
        return Ok(fallback.call_method1("encrypt", (data,))?.unbind());
    }
    let attempt = (|| -> PyResult<Py<PyAny>> {
        let encryptor = instance.getattr("cipher")?.call_method0("encryptor")?;
        update_finalize(&encryptor, data)
    })();
    match attempt {
        Ok(value) => Ok(value),
        Err(error) => {
            if err_is_instance(py, &error, CRYPTOGRAPHY_EXCEPTIONS, "UnsupportedAlgorithm")? {
                let key = instance.getattr("_key")?;
                let fallback = instance.call_method1("_disable_rc4", (key,))?;
                instance.setattr("_fallback", &fallback)?;
                Ok(fallback.call_method1("encrypt", (data,))?.unbind())
            } else {
                Err(error)
            }
        }
    }
}

/// `CryptRC4.decrypt` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.CryptRC4.decrypt
#[pyfunction]
fn cp_cryptography_cryptrc4_decrypt(
    instance: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let py = instance.py();
    let fallback = instance.getattr("_fallback")?;
    if !fallback.is_none() {
        let kwargs = PyDict::new(py);
        kwargs.set_item("strict", strict)?;
        return Ok(fallback
            .call_method("decrypt", (data,), Some(&kwargs))?
            .unbind());
    }
    let attempt = (|| -> PyResult<Py<PyAny>> {
        let decryptor = instance.getattr("cipher")?.call_method0("decryptor")?;
        update_finalize(&decryptor, data)
    })();
    match attempt {
        Ok(value) => Ok(value),
        Err(error) => {
            if err_is_instance(py, &error, CRYPTOGRAPHY_EXCEPTIONS, "UnsupportedAlgorithm")? {
                let key = instance.getattr("_key")?;
                let fallback = instance.call_method1("_disable_rc4", (key,))?;
                instance.setattr("_fallback", &fallback)?;
                let kwargs = PyDict::new(py);
                kwargs.set_item("strict", strict)?;
                Ok(fallback
                    .call_method("decrypt", (data,), Some(&kwargs))?
                    .unbind())
            } else {
                Err(error)
            }
        }
    }
}

/// `CryptAES.__init__` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.CryptAES.__init__
#[pyfunction]
fn cp_cryptography_cryptaes_init(
    instance: &Bound<'_, PyAny>,
    key: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let py = instance.py();
    let alg = shim_attr(py, CRYPTOGRAPHY_MODULE, "AES")?.call1((key,))?;
    instance.setattr("alg", alg)
}

/// `PKCS7(128).padder()`.
fn pkcs7_padder(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    shim_attr(py, CRYPTOGRAPHY_MODULE, "PKCS7")?
        .call1((128,))?
        .call_method0("padder")
}

/// `PKCS7(128).unpadder()`.
fn pkcs7_unpadder(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    shim_attr(py, CRYPTOGRAPHY_MODULE, "PKCS7")?
        .call1((128,))?
        .call_method0("unpadder")
}

/// `CryptAES.encrypt` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.CryptAES.encrypt
#[pyfunction]
fn cp_cryptography_cryptaes_encrypt(
    instance: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = instance.py();
    let iv = py_bytes(py, &token_bytes(py, 16)?);
    let padder = pkcs7_padder(py)?;
    let padded = update_with(&padder, data)?;

    let alg = instance.getattr("alg")?;
    let cipher = cipher_with_algorithm(py, &alg, "CBC", Some(&iv))?;
    let encryptor = cipher.call_method0("encryptor")?;
    let encrypted = update_with(&encryptor, &padded)?;
    Ok(pyutil::py_add(&iv, &encrypted)?.unbind())
}

/// `padder.update(data) + padder.finalize()` (padding objects, not ciphers).
fn update_with<'py>(
    padder: &Bound<'py, PyAny>,
    data: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let updated = padder.call_method1("update", (data,))?;
    let finalized = padder.call_method0("finalize")?;
    pyutil::py_add(&updated, &finalized)
}

/// `CryptAES.decrypt` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.CryptAES.decrypt
#[pyfunction]
fn cp_cryptography_cryptaes_decrypt(
    instance: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let py = instance.py();
    let sixteen = 16i64.into_pyobject(py)?.into_any();
    let iv = data.get_item(pyutil::py_slice(py, None, Some(&sixteen), None)?)?;
    let mut body = data.get_item(pyutil::py_slice(py, Some(&sixteen), None, None)?)?;
    // for empty encrypted data
    if body.len()? == 0 {
        return Ok(body.unbind());
    }
    if !strict && body.len()? % 16 != 0 {
        warn(py, CRYPTOGRAPHY_MODULE, "Adding missing padding.", None)?;
        let padder = pkcs7_padder(py)?;
        body = update_with(&padder, &body)?;
    }

    let alg = instance.getattr("alg")?;
    let cipher = cipher_with_algorithm(py, &alg, "CBC", Some(&iv))?;
    let decryptor = cipher.call_method0("decryptor")?;
    let padded_data = match update_with(&decryptor, &body) {
        Ok(value) => value,
        Err(error) => {
            if err_is_instance(py, &error, "builtins", "ValueError")? {
                return Err(stream_error_from(py, error));
            }
            return Err(error);
        }
    };

    let unpadder = pkcs7_unpadder(py)?;
    match update_with(&unpadder, &padded_data) {
        Ok(value) => Ok(value.unbind()),
        Err(error) => {
            if err_is_instance(py, &error, "builtins", "ValueError")? {
                if strict {
                    return Err(stream_error_from(py, error));
                }
                let values = PyDict::new(py);
                values.set_item("exception", error.value(py))?;
                warn(
                    py,
                    CRYPTOGRAPHY_MODULE,
                    "Ignoring padding error: %(exception)s",
                    Some(&values),
                )?;
                let raw = as_bytes(&padded_data)?;
                // `padded_data[: -padded_data[-1]]` -- note that Python's
                // `x[:-0]` is the empty slice, not the whole buffer.
                let last = raw.last().copied().unwrap_or(0);
                let keep = if last == 0 {
                    0
                } else {
                    raw.len().saturating_sub(usize::from(last))
                };
                return Ok(py_bytes(py, &raw[..keep]).unbind());
            }
            Err(error)
        }
    }
}

/// `rc4_encrypt` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.rc4_encrypt
#[pyfunction]
fn cp_cryptography_rc4_encrypt(
    key: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let class = shim_attr(py, CRYPTOGRAPHY_MODULE, "CryptRC4")?;
    if class.getattr("_is_rc4_supported")?.is_truthy()? {
        match arc4_encryptor(py, key).and_then(|encryptor| update_finalize(&encryptor, data)) {
            Ok(value) => Ok(value),
            Err(error) => {
                if err_is_instance(py, &error, CRYPTOGRAPHY_EXCEPTIONS, "UnsupportedAlgorithm")? {
                    Ok(class
                        .call_method1("_disable_rc4", (key,))?
                        .call_method1("encrypt", (data,))?
                        .unbind())
                } else {
                    Err(error)
                }
            }
        }
    } else {
        Ok(class
            .call_method1("_pure_python_rc4", (key,))?
            .call_method1("encrypt", (data,))?
            .unbind())
    }
}

/// `rc4_decrypt` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.rc4_decrypt
#[pyfunction]
fn cp_cryptography_rc4_decrypt(
    key: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let class = shim_attr(py, CRYPTOGRAPHY_MODULE, "CryptRC4")?;
    if class.getattr("_is_rc4_supported")?.is_truthy()? {
        match arc4_decryptor(py, key).and_then(|decryptor| update_finalize(&decryptor, data)) {
            Ok(value) => Ok(value),
            Err(error) => {
                if err_is_instance(py, &error, CRYPTOGRAPHY_EXCEPTIONS, "UnsupportedAlgorithm")? {
                    Ok(class
                        .call_method1("_disable_rc4", (key,))?
                        .call_method1("decrypt", (data,))?
                        .unbind())
                } else {
                    Err(error)
                }
            }
        }
    } else {
        Ok(class
            .call_method1("_pure_python_rc4", (key,))?
            .call_method1("decrypt", (data,))?
            .unbind())
    }
}

/// `aes_ecb_encrypt` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.aes_ecb_encrypt
#[pyfunction]
fn cp_cryptography_aes_ecb_encrypt(
    key: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let cipher = aes_cipher(py, key, "ECB", None)?;
    let encryptor = cipher.call_method0("encryptor")?;
    update_finalize(&encryptor, data)
}

/// `aes_ecb_decrypt` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.aes_ecb_decrypt
#[pyfunction]
fn cp_cryptography_aes_ecb_decrypt(
    key: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let cipher = aes_cipher(py, key, "ECB", None)?;
    let decryptor = cipher.call_method0("decryptor")?;
    update_finalize(&decryptor, data)
}

/// `aes_cbc_encrypt` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.aes_cbc_encrypt
#[pyfunction]
fn cp_cryptography_aes_cbc_encrypt(
    key: &Bound<'_, PyAny>,
    iv: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let cipher = aes_cipher(py, key, "CBC", Some(iv))?;
    let encryptor = cipher.call_method0("encryptor")?;
    update_finalize(&encryptor, data)
}

/// `aes_cbc_decrypt` of the cryptography provider.
// @pypdf: pypdf._crypt_providers._cryptography.aes_cbc_decrypt
#[pyfunction]
fn cp_cryptography_aes_cbc_decrypt(
    key: &Bound<'_, PyAny>,
    iv: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let cipher = aes_cipher(py, key, "CBC", Some(iv))?;
    let decryptor = cipher.call_method0("decryptor")?;
    update_finalize(&decryptor, data)
}

// ---------------------------------------------------------------------------
// pypdf._crypt_providers._fallback
// ---------------------------------------------------------------------------

/// `CryptRC4.__init__` of the pure-Python-compatible fallback provider.
///
/// The RC4 key schedule of upstream's pure Python implementation, computed in
/// Rust; the resulting 256-byte permutation is stored on `self.s` exactly like
/// upstream (so the observable instance state is identical).
// @pypdf: pypdf._crypt_providers._fallback.CryptRC4.__init__
#[pyfunction]
fn cp_fallback_cryptrc4_init(instance: &Bound<'_, PyAny>, key: &Bound<'_, PyAny>) -> PyResult<()> {
    let py = instance.py();
    let key = as_bytes(key)?;
    let key_length = key.len();
    if key_length == 0 {
        // `key[i % key_length]` raises ZeroDivisionError upstream.
        return Err(PyZeroDivisionError::new_err("integer modulo by zero"));
    }
    let mut s: Vec<u8> = (0..=255u8).collect();
    let mut j: usize = 0;
    for i in 0..256usize {
        let s_i = s[i];
        j = (j + usize::from(s_i) + usize::from(key[i % key_length])) & 0xFF;
        s[i] = s[j];
        s[j] = s_i;
    }
    instance.setattr("s", PyByteArray::new(py, &s))
}

/// The RC4 keystream of upstream's pure Python fallback, as a byte vector.
fn fallback_keystream(state: &[u8], length: usize) -> Vec<u8> {
    let mut s = state.to_vec();
    let mut keystream = vec![0u8; length];
    let mut i: usize = 0;
    let mut j: usize = 0;
    for slot in keystream.iter_mut() {
        i = (i + 1) & 0xFF;
        let s_i = s[i];
        j = (j + usize::from(s_i)) & 0xFF;
        let s_j = s[j];
        s[i] = s_j;
        s[j] = s_i;
        *slot = s[(usize::from(s_i) + usize::from(s_j)) & 0xFF];
    }
    keystream
}

/// `self.s` of a fallback `CryptRC4` instance as bytes.
fn fallback_state(instance: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    as_bytes(&instance.getattr("s")?)
}

/// `CryptRC4.encrypt` of the fallback provider.
///
/// Upstream XORs the whole keystream with the data at once through big
/// integers; that is exactly a byte-wise XOR (the explicit output length keeps
/// leading zero bytes), which is what the loop below computes.
// @pypdf: pypdf._crypt_providers._fallback.CryptRC4.encrypt
#[pyfunction]
fn cp_fallback_cryptrc4_encrypt(
    instance: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = instance.py();
    let state = fallback_state(instance)?;
    let data = as_bytes(data)?;
    let keystream = fallback_keystream(&state, data.len());
    let xored: Vec<u8> = data
        .iter()
        .zip(keystream.iter())
        .map(|(byte, stream)| byte ^ stream)
        .collect();
    Ok(py_bytes(py, &xored).unbind())
}

/// `CryptRC4.decrypt` of the fallback provider (RC4 is symmetric).
// @pypdf: pypdf._crypt_providers._fallback.CryptRC4.decrypt
#[pyfunction]
fn cp_fallback_cryptrc4_decrypt(
    instance: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let _ = strict;
    cp_fallback_cryptrc4_encrypt(instance, data)
}

/// `CryptAES.__init__` of the fallback provider (upstream body is `pass`).
// @pypdf: pypdf._crypt_providers._fallback.CryptAES.__init__
#[pyfunction]
fn cp_fallback_cryptaes_init(instance: &Bound<'_, PyAny>, key: &Bound<'_, PyAny>) -> PyResult<()> {
    let _ = (instance, key);
    Ok(())
}

/// `CryptAES.encrypt` of the fallback provider.
// @pypdf: pypdf._crypt_providers._fallback.CryptAES.encrypt
#[pyfunction]
fn cp_fallback_cryptaes_encrypt(
    instance: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let _ = (instance, data);
    Err(errors::dependency_error(
        instance.py(),
        DEPENDENCY_ERROR_STR,
    ))
}

/// `CryptAES.decrypt` of the fallback provider.
// @pypdf: pypdf._crypt_providers._fallback.CryptAES.decrypt
#[pyfunction]
fn cp_fallback_cryptaes_decrypt(
    instance: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let _ = (instance, data, strict);
    Err(errors::dependency_error(
        instance.py(),
        DEPENDENCY_ERROR_STR,
    ))
}

/// `rc4_encrypt` of the fallback provider.
// @pypdf: pypdf._crypt_providers._fallback.rc4_encrypt
#[pyfunction]
fn cp_fallback_rc4_encrypt(key: &Bound<'_, PyAny>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let instance = shim_attr(py, FALLBACK_MODULE, "CryptRC4")?.call1((key,))?;
    Ok(instance.call_method1("encrypt", (data,))?.unbind())
}

/// `rc4_decrypt` of the fallback provider.
// @pypdf: pypdf._crypt_providers._fallback.rc4_decrypt
#[pyfunction]
fn cp_fallback_rc4_decrypt(key: &Bound<'_, PyAny>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let instance = shim_attr(py, FALLBACK_MODULE, "CryptRC4")?.call1((key,))?;
    Ok(instance.call_method1("decrypt", (data,))?.unbind())
}

/// `aes_ecb_encrypt` of the fallback provider.
// @pypdf: pypdf._crypt_providers._fallback.aes_ecb_encrypt
#[pyfunction]
fn cp_fallback_aes_ecb_encrypt(
    key: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let _ = (key, data);
    Err(errors::dependency_error(key.py(), DEPENDENCY_ERROR_STR))
}

/// `aes_ecb_decrypt` of the fallback provider.
// @pypdf: pypdf._crypt_providers._fallback.aes_ecb_decrypt
#[pyfunction]
fn cp_fallback_aes_ecb_decrypt(
    key: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let _ = (key, data);
    Err(errors::dependency_error(key.py(), DEPENDENCY_ERROR_STR))
}

/// `aes_cbc_encrypt` of the fallback provider.
// @pypdf: pypdf._crypt_providers._fallback.aes_cbc_encrypt
#[pyfunction]
fn cp_fallback_aes_cbc_encrypt(
    key: &Bound<'_, PyAny>,
    iv: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let _ = (key, iv, data);
    Err(errors::dependency_error(key.py(), DEPENDENCY_ERROR_STR))
}

/// `aes_cbc_decrypt` of the fallback provider.
// @pypdf: pypdf._crypt_providers._fallback.aes_cbc_decrypt
#[pyfunction]
fn cp_fallback_aes_cbc_decrypt(
    key: &Bound<'_, PyAny>,
    iv: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let _ = (key, iv, data);
    Err(errors::dependency_error(key.py(), DEPENDENCY_ERROR_STR))
}

// ---------------------------------------------------------------------------
// pypdf._crypt_providers._pycryptodome
// ---------------------------------------------------------------------------

/// `ARC4.ARC4Cipher(key)` of `Crypto.Cipher.ARC4`.
fn pycryptodome_arc4<'py>(py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, PYCROPTODOME_MODULE, "ARC4")?
        .getattr("ARC4Cipher")?
        .call1((key,))
}

/// `AES.new(...)` of `Crypto.Cipher.AES` with a positional mode (and IV).
fn pycryptodome_aes<'py>(
    py: Python<'py>,
    key: &Bound<'py, PyAny>,
    mode_name: &str,
    iv: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let aes = shim_attr(py, PYCROPTODOME_MODULE, "AES")?;
    let mode = aes.getattr(mode_name)?;
    match iv {
        Some(iv) => aes.call_method1("new", (key, mode, iv)),
        None => aes.call_method1("new", (key, mode)),
    }
}

/// `Crypto.Util.Padding.pad(data, 16)`.
fn pycryptodome_pad<'py>(py: Python<'py>, data: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, PYCROPTODOME_MODULE, "pad")?.call1((data, 16))
}

/// `Crypto.Util.Padding.unpad(padded_data, 16)`.
fn pycryptodome_unpad<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    shim_attr(py, PYCROPTODOME_MODULE, "unpad")?.call1((data, 16))
}

/// `CryptRC4.__init__` of the pycryptodome provider.
// @pypdf: pypdf._crypt_providers._pycryptodome.CryptRC4.__init__
#[pyfunction]
fn cp_pycryptodome_cryptrc4_init(
    instance: &Bound<'_, PyAny>,
    key: &Bound<'_, PyAny>,
) -> PyResult<()> {
    instance.setattr("key", key)
}

/// `CryptRC4.encrypt` of the pycryptodome provider.
// @pypdf: pypdf._crypt_providers._pycryptodome.CryptRC4.encrypt
#[pyfunction]
fn cp_pycryptodome_cryptrc4_encrypt(
    instance: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = instance.py();
    let key = instance.getattr("key")?;
    let cipher = pycryptodome_arc4(py, &key)?;
    Ok(cipher.call_method1("encrypt", (data,))?.unbind())
}

/// `CryptRC4.decrypt` of the pycryptodome provider.
// @pypdf: pypdf._crypt_providers._pycryptodome.CryptRC4.decrypt
#[pyfunction]
fn cp_pycryptodome_cryptrc4_decrypt(
    instance: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let _ = strict;
    let py = instance.py();
    let key = instance.getattr("key")?;
    let cipher = pycryptodome_arc4(py, &key)?;
    Ok(cipher.call_method1("decrypt", (data,))?.unbind())
}

/// `CryptAES.__init__` of the pycryptodome provider.
// @pypdf: pypdf._crypt_providers._pycryptodome.CryptAES.__init__
#[pyfunction]
fn cp_pycryptodome_cryptaes_init(
    instance: &Bound<'_, PyAny>,
    key: &Bound<'_, PyAny>,
) -> PyResult<()> {
    instance.setattr("key", key)
}

/// `CryptAES.encrypt` of the pycryptodome provider.
// @pypdf: pypdf._crypt_providers._pycryptodome.CryptAES.encrypt
#[pyfunction]
fn cp_pycryptodome_cryptaes_encrypt(
    instance: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = instance.py();
    let iv = py_bytes(py, &token_bytes(py, 16)?);
    let padded = pycryptodome_pad(py, data)?;
    let key = instance.getattr("key")?;
    let aes = pycryptodome_aes(py, &key, "MODE_CBC", Some(&iv))?;
    let encrypted = aes.call_method1("encrypt", (&padded,))?;
    Ok(pyutil::py_add(&iv, &encrypted)?.unbind())
}

/// `CryptAES.decrypt` of the pycryptodome provider.
// @pypdf: pypdf._crypt_providers._pycryptodome.CryptAES.decrypt
#[pyfunction]
fn cp_pycryptodome_cryptaes_decrypt(
    instance: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let py = instance.py();
    let sixteen = 16i64.into_pyobject(py)?.into_any();
    let iv = data.get_item(pyutil::py_slice(py, None, Some(&sixteen), None)?)?;
    let mut body = data.get_item(pyutil::py_slice(py, Some(&sixteen), None, None)?)?;
    // for empty encrypted data
    if body.len()? == 0 {
        return Ok(body.unbind());
    }
    if !strict && body.len()? % 16 != 0 {
        warn(py, PYCROPTODOME_MODULE, "Adding missing padding.", None)?;
        body = pycryptodome_pad(py, &body)?;
    }

    let key = instance.getattr("key")?;
    let aes = pycryptodome_aes(py, &key, "MODE_CBC", Some(&iv))?;
    let padded_data = match aes.call_method1("decrypt", (&body,)) {
        Ok(value) => value,
        Err(error) => {
            if err_is_instance(py, &error, "builtins", "ValueError")? {
                return Err(stream_error_from(py, error));
            }
            return Err(error);
        }
    };

    match pycryptodome_unpad(py, &padded_data) {
        Ok(value) => Ok(value.unbind()),
        Err(error) => {
            if err_is_instance(py, &error, "builtins", "ValueError")? {
                if strict {
                    return Err(stream_error_from(py, error));
                }
                let values = PyDict::new(py);
                values.set_item("exception", error.value(py))?;
                warn(
                    py,
                    PYCROPTODOME_MODULE,
                    "Ignoring padding error: %(exception)s",
                    Some(&values),
                )?;
                let raw = as_bytes(&padded_data)?;
                let last = raw.last().copied().unwrap_or(0);
                let keep = if last == 0 {
                    0
                } else {
                    raw.len().saturating_sub(usize::from(last))
                };
                return Ok(py_bytes(py, &raw[..keep]).unbind());
            }
            Err(error)
        }
    }
}

/// `rc4_encrypt` of the pycryptodome provider.
// @pypdf: pypdf._crypt_providers._pycryptodome.rc4_encrypt
#[pyfunction]
fn cp_pycryptodome_rc4_encrypt(
    key: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let cipher = pycryptodome_arc4(py, key)?;
    Ok(cipher.call_method1("encrypt", (data,))?.unbind())
}

/// `rc4_decrypt` of the pycryptodome provider.
// @pypdf: pypdf._crypt_providers._pycryptodome.rc4_decrypt
#[pyfunction]
fn cp_pycryptodome_rc4_decrypt(
    key: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let cipher = pycryptodome_arc4(py, key)?;
    Ok(cipher.call_method1("decrypt", (data,))?.unbind())
}

/// `aes_ecb_encrypt` of the pycryptodome provider.
// @pypdf: pypdf._crypt_providers._pycryptodome.aes_ecb_encrypt
#[pyfunction]
fn cp_pycryptodome_aes_ecb_encrypt(
    key: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let cipher = pycryptodome_aes(py, key, "MODE_ECB", None)?;
    Ok(cipher.call_method1("encrypt", (data,))?.unbind())
}

/// `aes_ecb_decrypt` of the pycryptodome provider.
// @pypdf: pypdf._crypt_providers._pycryptodome.aes_ecb_decrypt
#[pyfunction]
fn cp_pycryptodome_aes_ecb_decrypt(
    key: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let cipher = pycryptodome_aes(py, key, "MODE_ECB", None)?;
    Ok(cipher.call_method1("decrypt", (data,))?.unbind())
}

/// `aes_cbc_encrypt` of the pycryptodome provider.
// @pypdf: pypdf._crypt_providers._pycryptodome.aes_cbc_encrypt
#[pyfunction]
fn cp_pycryptodome_aes_cbc_encrypt(
    key: &Bound<'_, PyAny>,
    iv: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let cipher = pycryptodome_aes(py, key, "MODE_CBC", Some(iv))?;
    Ok(cipher.call_method1("encrypt", (data,))?.unbind())
}

/// `aes_cbc_decrypt` of the pycryptodome provider.
// @pypdf: pypdf._crypt_providers._pycryptodome.aes_cbc_decrypt
#[pyfunction]
fn cp_pycryptodome_aes_cbc_decrypt(
    key: &Bound<'_, PyAny>,
    iv: &Bound<'_, PyAny>,
    data: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = key.py();
    let cipher = pycryptodome_aes(py, key, "MODE_CBC", Some(iv))?;
    Ok(cipher.call_method1("decrypt", (data,))?.unbind())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("CRYPT_DEPENDENCY_ERROR_STR", DEPENDENCY_ERROR_STR)?;
    module.add_function(wrap_pyfunction!(cp_prng_set_seed, module)?)?;

    module.add_function(wrap_pyfunction!(cp_cryptbase_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_cryptbase_decrypt, module)?)?;

    module.add_function(wrap_pyfunction!(cp_cryptography_cryptrc4_init, module)?)?;
    module.add_function(wrap_pyfunction!(
        cp_cryptography_cryptrc4_pure_python_rc4,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        cp_cryptography_cryptrc4_disable_rc4,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(cp_cryptography_cryptrc4_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_cryptography_cryptrc4_decrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_cryptography_cryptaes_init, module)?)?;
    module.add_function(wrap_pyfunction!(cp_cryptography_cryptaes_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_cryptography_cryptaes_decrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_cryptography_rc4_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_cryptography_rc4_decrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_cryptography_aes_ecb_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_cryptography_aes_ecb_decrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_cryptography_aes_cbc_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_cryptography_aes_cbc_decrypt, module)?)?;

    module.add_function(wrap_pyfunction!(cp_fallback_cryptrc4_init, module)?)?;
    module.add_function(wrap_pyfunction!(cp_fallback_cryptrc4_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_fallback_cryptrc4_decrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_fallback_cryptaes_init, module)?)?;
    module.add_function(wrap_pyfunction!(cp_fallback_cryptaes_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_fallback_cryptaes_decrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_fallback_rc4_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_fallback_rc4_decrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_fallback_aes_ecb_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_fallback_aes_ecb_decrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_fallback_aes_cbc_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_fallback_aes_cbc_decrypt, module)?)?;

    module.add_function(wrap_pyfunction!(cp_pycryptodome_cryptrc4_init, module)?)?;
    module.add_function(wrap_pyfunction!(cp_pycryptodome_cryptrc4_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_pycryptodome_cryptrc4_decrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_pycryptodome_cryptaes_init, module)?)?;
    module.add_function(wrap_pyfunction!(cp_pycryptodome_cryptaes_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_pycryptodome_cryptaes_decrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_pycryptodome_rc4_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_pycryptodome_rc4_decrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_pycryptodome_aes_ecb_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_pycryptodome_aes_ecb_decrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_pycryptodome_aes_cbc_encrypt, module)?)?;
    module.add_function(wrap_pyfunction!(cp_pycryptodome_aes_cbc_decrypt, module)?)?;
    Ok(())
}
