//! Rust port of `pypdf/_encryption.py` (upstream pypdf 7.1.1).
//!
//! # Design
//!
//! As required by `docs/PORTING_GUIDE.md` §1 the classes stay Python
//! (`python/pypdf/_encryption.py` keeps upstream's class definitions, method
//! signatures, `IntEnum`/`tuple, Enum` members and the `EncryptionValues`
//! annotations) and every method body delegates here.  The constants
//! (`_PADDING`, `_SASLPREP_PROHIBITED_CHECKS`) and the `EncryptionValues`
//! annotations live in the shim as data; the Rust code reads them through the
//! module namespace exactly like upstream's global lookups.
//!
//! Everything cryptographic is delegated to the *Python* provider functions
//! (`pypdf._crypt_providers.rc4_encrypt`, `aes_cbc_encrypt`, ...) and to the
//! `hashlib` / `struct` / `stringprep` modules, so:
//!
//! * AES/RC4 outputs are byte-identical (the providers themselves drive
//!   `cryptography` through PyO3, see `crypt_providers_mod.rs`);
//! * error behaviour is identical (a monkeypatched `hashlib.md5` raises exactly
//!   where it does upstream -- AES-256/R5/R6 never calls MD5);
//! * `logger_warning(...)` is still the Python function of
//!   `pypdf._encryption`, with the upstream `source=__name__`;
//! * random bytes (R5/R6 salts, `/Perms` tail, the file key) go through the
//!   unified PRNG entry point documented in `docs/FIDELITY_CONTRACT.md` §3
//!   (`crypt_providers_mod::token_bytes`), at exactly the upstream call sites.

use pyo3::exceptions::{PyAssertionError, PyNotImplementedError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyBytes, PyDict, PyInt, PyModule, PyString, PyTuple};

use crate::crypt_providers_mod::token_bytes;
use crate::errors;
use crate::pyutil;

/// Upstream module name; used as `logger_warning(..., source=__name__)`.
const SOURCE: &str = "pypdf._encryption";

/// `b"\xff\xff\xff\xff"` (`b"\xff" * 4` in upstream literals).
const FOUR_FF: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

// ---------------------------------------------------------------------------
// Small helpers around Python semantics
// ---------------------------------------------------------------------------

fn module<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyModule>> {
    py.import(name)
}

/// A module-level global of `pypdf._encryption` (upstream resolves these names
/// in its own module namespace, so monkeypatching keeps working).
fn enc_attr<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    module(py, SOURCE)?.getattr(name)
}

/// A class of the `pypdf._encryption` shim.
fn enc_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    enc_attr(py, name)
}

fn builtins<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("builtins")
}

fn py_bytes<'py>(py: Python<'py>, data: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, data).into_any()
}

fn as_bytes(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    obj.extract::<Vec<u8>>()
}

fn int_obj<'py>(py: Python<'py>, value: i64) -> Bound<'py, PyAny> {
    // `PyInt::new` only panics when the allocation fails (like `Vec` growth).
    PyInt::new(py, value).into_any()
}

fn bool_obj<'py>(py: Python<'py>, value: bool) -> Bound<'py, PyAny> {
    // `PyBool::new` returns one of the two Python singletons; infallible.
    PyBool::new(py, value).to_owned().into_any()
}

fn as_i64(obj: &Bound<'_, PyAny>) -> PyResult<i64> {
    obj.extract::<i64>()
}

fn is_instance(obj: &Bound<'_, PyAny>, class: &Bound<'_, PyAny>) -> PyResult<bool> {
    obj.is_instance(class)
}

fn err_is_instance(
    py: Python<'_>,
    error: &PyErr,
    module_name: &str,
    class_name: &str,
) -> PyResult<bool> {
    let class = module(py, module_name)?.getattr(class_name)?;
    error.value(py).is_instance(&class)
}

/// `obj[start:stop]` with integer bounds (`None` means open).
fn slice_of<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
    start: Option<i64>,
    stop: Option<i64>,
) -> PyResult<Bound<'py, PyAny>> {
    let start = start.map(|value| int_obj(py, value));
    let stop = stop.map(|value| int_obj(py, value));
    let slice = pyutil::py_slice(py, start.as_ref(), stop.as_ref(), None)?;
    obj.get_item(&slice)
}

/// `a + b + c + ...` with Python's `bytes` concatenation semantics.
fn concat<'py>(py: Python<'py>, parts: &[Bound<'py, PyAny>]) -> PyResult<Bound<'py, PyAny>> {
    let mut iter = parts.iter();
    let Some(first) = iter.next() else {
        return Ok(py_bytes(py, b""));
    };
    let mut acc = first.clone();
    for part in iter {
        acc = pyutil::py_add(&acc, part)?;
    }
    Ok(acc)
}

/// `bytes(x ^ value for x in key)`.
fn xor_key<'py>(
    py: Python<'py>,
    key: &Bound<'py, PyAny>,
    value: u8,
) -> PyResult<Bound<'py, PyAny>> {
    let bytes = as_bytes(key)?;
    let xored: Vec<u8> = bytes.iter().map(|byte| byte ^ value).collect();
    Ok(py_bytes(py, &xored))
}

/// `bytes(0 for _ in range(16))`.
fn zero_iv<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
    py_bytes(py, &[0u8; 16])
}

/// `logger_warning(message, source=__name__, **values)`.
fn logger_warning(
    py: Python<'_>,
    message: &str,
    values: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("source", SOURCE)?;
    if let Some(values) = values {
        for (key, value) in values.iter() {
            kwargs.set_item(key, value)?;
        }
    }
    enc_attr(py, "logger_warning")?.call((message,), Some(&kwargs))?;
    Ok(())
}

/// `hashlib.md5(data)`.
fn md5<'py>(py: Python<'py>, data: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    module(py, "hashlib")?.call_method1("md5", (data,))
}

/// `hash.digest()`.
fn digest<'py>(hash: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    hash.call_method0("digest")
}

/// `struct.pack("<I", value)`.
fn pack_le_u32<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    module(py, "struct")?.call_method1("pack", ("<I", value))
}

/// `int(value)`.
fn to_int<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    builtins(py)?.getattr("int")?.call1((value,))
}

/// A tuple of builtin type objects, e.g. `(int, float)`.
fn builtin_types<'py>(py: Python<'py>, names: &[&str]) -> PyResult<Bound<'py, PyAny>> {
    let builtins = builtins(py)?;
    let mut items: Vec<Bound<'py, PyAny>> = Vec::with_capacity(names.len());
    for name in names {
        items.push(builtins.getattr(*name)?);
    }
    Ok(PyTuple::new(py, items)?.into_any())
}

/// A tuple of classes of the `pypdf._encryption` module, e.g. `(ByteStringObject, TextStringObject)`.
fn class_tuple<'py>(py: Python<'py>, names: &[&str]) -> PyResult<Bound<'py, PyAny>> {
    let mut items: Vec<Bound<'py, PyAny>> = Vec::with_capacity(names.len());
    for name in names {
        items.push(enc_class(py, name)?);
    }
    Ok(PyTuple::new(py, items)?.into_any())
}

/// `NullObject()`.
fn null_object<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    enc_class(py, "NullObject")?.call0()
}

/// The `AlgV4` class through the shim module (upstream calls `AlgV4.<method>`).
fn algv4<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    enc_class(py, "AlgV4")?.getattr(name)
}

/// The `AlgV5` class through the shim module (upstream calls `AlgV5.<method>`).
fn algv5<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    enc_class(py, "AlgV5")?.getattr(name)
}

// ---------------------------------------------------------------------------
// pypdf._encryption._PADDING / _SASLPREP_PROHIBITED_CHECKS
// ---------------------------------------------------------------------------

// The upstream constants `_PADDING` and `_SASLPREP_PROHIBITED_CHECKS` are data
// and stay in `python/pypdf/_encryption.py`; the logic that consumes them lives
// below.  Their coverage annotations are recorded here (gate A6):
// @pypdf: pypdf._encryption._PADDING
// @pypdf: pypdf._encryption._SASLPREP_PROHIBITED_CHECKS

// ---------------------------------------------------------------------------
// pypdf._encryption._padding
// ---------------------------------------------------------------------------

/// `(data + _PADDING)[:32]`.
// @pypdf: pypdf._encryption._padding
#[pyfunction]
fn encryption_padding(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let padding = enc_attr(py, "_PADDING")?;
    let combined = pyutil::py_add(data, &padding)?;
    Ok(slice_of(py, &combined, Some(0), Some(32))?.unbind())
}

// ---------------------------------------------------------------------------
// pypdf._encryption._saslprep
// ---------------------------------------------------------------------------

/// `_saslprep`, the SASLprep profile (RFC 4013) of stringprep (RFC 3454).
///
/// The tables are the CPython `stringprep` module's functions (upstream calls
/// them directly), the prohibited-character list is the shim's
/// `_SASLPREP_PROHIBITED_CHECKS` constant, and the normalization is
/// `unicodedata.normalize("NFKC", ...)` -- so both results and the `ValueError`
/// texts are exactly upstream's.
// @pypdf: pypdf._encryption._saslprep
#[pyfunction]
fn encryption_saslprep(py: Python<'_>, password: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let stringprep = module(py, "stringprep")?;
    let in_table_b1 = stringprep.getattr("in_table_b1")?;
    let in_table_c12 = stringprep.getattr("in_table_c12")?;

    // Mapping (RFC 4013 §2.1): B.1 -> nothing, C.1.2 -> U+0020.
    let mut mapped: Vec<Bound<'_, PyAny>> = Vec::new();
    for character in password.try_iter()? {
        let character = character?;
        if in_table_b1.call1((&character,))?.is_truthy()? {
            continue;
        }
        if in_table_c12.call1((&character,))?.is_truthy()? {
            mapped.push(PyString::new(py, " ").into_any());
        } else {
            mapped.push(character);
        }
    }
    let joined = PyString::new(py, "").call_method1("join", (mapped,))?;

    // Normalization (RFC 4013 §2.2) -- Unicode NFKC.
    let normalized = module(py, "unicodedata")?.call_method1("normalize", ("NFKC", &joined))?;

    // Prohibited output (RFC 4013 §2.3): C.1.2, C.2.1, C.2.2, C.3-C.9, A.1.
    let checks = enc_attr(py, "_SASLPREP_PROHIBITED_CHECKS")?;
    for character in normalized.try_iter()? {
        let character = character?;
        for check in checks.try_iter()? {
            let check = check?;
            let function = check.get_item(0)?;
            let description = check.get_item(1)?;
            if function.call1((&character,))?.is_truthy()? {
                let code = builtins(py)?.call_method1("ord", (&character,))?;
                let message = format!(
                    "SASLprep: {} U+{:04X}",
                    pyutil::py_str(&description)?,
                    as_i64(&code)?
                );
                return Err(PyValueError::new_err(message));
            }
        }
    }

    // Bidirectional check (RFC 4013 §2.4, RFC 3454 §6).
    let in_table_d1 = stringprep.getattr("in_table_d1")?;
    let in_table_d2 = stringprep.getattr("in_table_d2")?;
    let mut has_r_and_al = false;
    let mut has_l = false;
    for character in normalized.try_iter()? {
        let character = character?;
        if in_table_d1.call1((&character,))?.is_truthy()? {
            has_r_and_al = true;
        }
        if in_table_d2.call1((&character,))?.is_truthy()? {
            has_l = true;
        }
    }
    if has_r_and_al {
        if has_l {
            return Err(PyValueError::new_err(
                "SASLprep: string with RandALCat characters must not contain LCat characters",
            ));
        }
        let first = normalized.get_item(0)?;
        let last = normalized.get_item(-1)?;
        if !in_table_d1.call1((&first,))?.is_truthy()?
            || !in_table_d1.call1((&last,))?.is_truthy()?
        {
            return Err(PyValueError::new_err(
                "SASLprep: string with RandALCat characters must start and end with RandALCat characters",
            ));
        }
    }
    Ok(normalized.unbind())
}

// ---------------------------------------------------------------------------
// pypdf._encryption._get_entry / _get_crypt_filter
// ---------------------------------------------------------------------------

/// Upstream `_get_entry`: fetch an entry of the encryption dictionary, treating
/// null as absent.
// @pypdf: pypdf._encryption._get_entry
#[pyfunction]
fn encryption_get_entry(
    py: Python<'_>,
    dictionary: &Bound<'_, PyAny>,
    key: &Bound<'_, PyAny>,
    expected_types: &Bound<'_, PyAny>,
    type_name: &Bound<'_, PyAny>,
    default: &Bound<'_, PyAny>,
    required: bool,
) -> PyResult<Py<PyAny>> {
    let null = null_object(py)?;
    let value = dictionary
        .call_method1("get", (key, &null))?
        .call_method0("get_object")?;
    // `isinstance(value, NullObject)` -- the *class*, not the instance.
    if is_instance(&value, &enc_class(py, "NullObject")?)? {
        if required {
            return Err(errors::pdf_read_error(
                py,
                &format!(
                    "{} entry of the encryption dictionary is missing",
                    pyutil::py_str(key)?
                ),
            ));
        }
        return Ok(default.clone().unbind());
    }
    if !value.is_instance(expected_types)? {
        return Err(errors::pdf_read_error(
            py,
            &format!(
                "{} entry of the encryption dictionary is not a {}",
                pyutil::py_str(key)?,
                pyutil::py_str(type_name)?
            ),
        ));
    }
    Ok(value.unbind())
}

/// Upstream `_get_crypt_filter`: look up a named crypt filter of `/CF`.
// @pypdf: pypdf._encryption._get_crypt_filter
#[pyfunction]
fn encryption_get_crypt_filter(
    py: Python<'_>,
    filters: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let null = null_object(py)?;
    let crypt_filter = filters
        .call_method1("get", (name, &null))?
        .call_method0("get_object")?;
    if !is_instance(&crypt_filter, &enc_class(py, "DictionaryObject")?)? {
        return Err(errors::pdf_read_error(
            py,
            &format!(
                "Crypt filter {} is missing from /CF or not a dictionary",
                pyutil::py_str(name)?
            ),
        ));
    }
    Ok(crypt_filter.unbind())
}

/// Call the shim's `_get_entry` (upstream calls the module-level function).
#[allow(clippy::too_many_arguments)]
fn call_get_entry<'py>(
    py: Python<'py>,
    dictionary: &Bound<'py, PyAny>,
    key: &str,
    expected_types: &Bound<'py, PyAny>,
    type_name: &str,
    default: &Bound<'py, PyAny>,
    required: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("expected_types", expected_types)?;
    kwargs.set_item("type_name", type_name)?;
    kwargs.set_item("default", default)?;
    kwargs.set_item("required", required)?;
    enc_attr(py, "_get_entry")?.call((dictionary, key), Some(&kwargs))
}

// ---------------------------------------------------------------------------
// pypdf._encryption.AlgV4
// ---------------------------------------------------------------------------

// Algorithms 2-7 of the PDF specification (revisions 2-4).
// The class lives in `python/pypdf/_encryption.py`; each method delegates here.
// @pypdf: pypdf._encryption.AlgV4

/// Algorithm 2: computing an encryption key.
// @pypdf: pypdf._encryption.AlgV4.compute_key
#[pyfunction]
#[allow(clippy::too_many_arguments, clippy::similar_names)]
fn encryption_algv4_compute_key(
    py: Python<'_>,
    password: &Bound<'_, PyAny>,
    rev: &Bound<'_, PyAny>,
    key_size: &Bound<'_, PyAny>,
    o_entry: &Bound<'_, PyAny>,
    p_entry: &Bound<'_, PyAny>,
    id1_entry: &Bound<'_, PyAny>,
    metadata_encrypted: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let padded = enc_attr(py, "_padding")?.call1((password,))?;
    let u_hash = md5(py, &padded)?;
    u_hash.call_method1("update", (o_entry,))?;
    let packed = pack_le_u32(py, p_entry)?;
    u_hash.call_method1("update", (&packed,))?;
    u_hash.call_method1("update", (id1_entry,))?;
    let rev_value = as_i64(rev)?;
    if rev_value >= 4 && !metadata_encrypted.is_truthy()? {
        u_hash.call_method1("update", (py_bytes(py, &FOUR_FF),))?;
    }
    let mut u_hash_digest = digest(&u_hash)?;
    let length = as_i64(key_size)? / 8;
    if rev_value >= 3 {
        for _ in 0..50 {
            u_hash_digest = digest(&md5(
                py,
                &slice_of(py, &u_hash_digest, Some(0), Some(length))?,
            )?)?;
        }
    }
    Ok(slice_of(py, &u_hash_digest, Some(0), Some(length))?.unbind())
}

/// Algorithm 3: computing the O (owner password) key.
// @pypdf: pypdf._encryption.AlgV4.compute_O_value_key
#[pyfunction]
#[allow(clippy::similar_names)]
fn encryption_algv4_compute_o_value_key(
    py: Python<'_>,
    owner_password: &Bound<'_, PyAny>,
    rev: &Bound<'_, PyAny>,
    key_size: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let padded = enc_attr(py, "_padding")?.call1((owner_password,))?;
    let mut o_hash_digest = digest(&md5(py, &padded)?)?;
    if as_i64(rev)? >= 3 {
        for _ in 0..50 {
            o_hash_digest = digest(&md5(py, &o_hash_digest)?)?;
        }
    }
    Ok(slice_of(py, &o_hash_digest, Some(0), Some(as_i64(key_size)? / 8))?.unbind())
}

/// Algorithm 3: computing the O (owner password) value.
// @pypdf: pypdf._encryption.AlgV4.compute_O_value
#[pyfunction]
fn encryption_algv4_compute_o_value(
    py: Python<'_>,
    rc4_key: &Bound<'_, PyAny>,
    user_password: &Bound<'_, PyAny>,
    rev: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let rc4_encrypt = enc_attr(py, "rc4_encrypt")?;
    let padded = enc_attr(py, "_padding")?.call1((user_password,))?;
    let mut rc4_enc = rc4_encrypt.call1((rc4_key, &padded))?;
    if as_i64(rev)? >= 3 {
        for i in 1..20u8 {
            let key = xor_key(py, rc4_key, i)?;
            rc4_enc = rc4_encrypt.call1((key, rc4_enc))?;
        }
    }
    Ok(rc4_enc.unbind())
}

/// Algorithms 4/5: computing the U (user password) value.
// @pypdf: pypdf._encryption.AlgV4.compute_U_value
#[pyfunction]
#[allow(clippy::similar_names)]
fn encryption_algv4_compute_u_value(
    py: Python<'_>,
    key: &Bound<'_, PyAny>,
    rev: &Bound<'_, PyAny>,
    id1_entry: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let rc4_encrypt = enc_attr(py, "rc4_encrypt")?;
    let padding = enc_attr(py, "_PADDING")?;
    if as_i64(rev)? <= 2 {
        return Ok(rc4_encrypt.call1((key, &padding))?.unbind());
    }
    let u_hash = md5(py, &padding)?;
    u_hash.call_method1("update", (id1_entry,))?;
    let mut rc4_enc = rc4_encrypt.call1((key, digest(&u_hash)?))?;
    for i in 1..20u8 {
        let rc4_key = xor_key(py, key, i)?;
        rc4_enc = rc4_encrypt.call1((rc4_key, rc4_enc))?;
    }
    Ok(enc_attr(py, "_padding")?.call1((rc4_enc,))?.unbind())
}

/// Algorithm 6: authenticating the user password.
// @pypdf: pypdf._encryption.AlgV4.verify_user_password
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn encryption_algv4_verify_user_password(
    py: Python<'_>,
    user_password: &Bound<'_, PyAny>,
    rev: &Bound<'_, PyAny>,
    key_size: &Bound<'_, PyAny>,
    o_entry: &Bound<'_, PyAny>,
    u_entry: &Bound<'_, PyAny>,
    p_entry: &Bound<'_, PyAny>,
    id1_entry: &Bound<'_, PyAny>,
    metadata_encrypted: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let key = algv4(py, "compute_key")?.call1((
        user_password,
        rev,
        key_size,
        o_entry,
        p_entry,
        id1_entry,
        metadata_encrypted,
    ))?;
    let mut u_value = algv4(py, "compute_U_value")?.call1((&key, rev, id1_entry))?;
    let mut u_entry = u_entry.clone();
    if as_i64(rev)? >= 3 {
        u_value = slice_of(py, &u_value, Some(0), Some(16))?;
        u_entry = slice_of(py, &u_entry, Some(0), Some(16))?;
    }
    if !pyutil::py_eq(&u_value, &u_entry)? {
        return Ok(py_bytes(py, b"").unbind());
    }
    Ok(key.unbind())
}

/// Algorithm 7: authenticating the owner password.
// @pypdf: pypdf._encryption.AlgV4.verify_owner_password
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn encryption_algv4_verify_owner_password(
    py: Python<'_>,
    owner_password: &Bound<'_, PyAny>,
    rev: &Bound<'_, PyAny>,
    key_size: &Bound<'_, PyAny>,
    o_entry: &Bound<'_, PyAny>,
    u_entry: &Bound<'_, PyAny>,
    p_entry: &Bound<'_, PyAny>,
    id1_entry: &Bound<'_, PyAny>,
    metadata_encrypted: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let rc4_key = algv4(py, "compute_O_value_key")?.call1((owner_password, rev, key_size))?;
    let rc4_decrypt = enc_attr(py, "rc4_decrypt")?;
    let user_password = if as_i64(rev)? <= 2 {
        rc4_decrypt.call1((&rc4_key, o_entry))?
    } else {
        let mut user_password = o_entry.clone();
        for i in (0..=19u8).rev() {
            let key = xor_key(py, &rc4_key, i)?;
            user_password = rc4_decrypt.call1((key, user_password))?;
        }
        user_password
    };
    Ok(algv4(py, "verify_user_password")?
        .call1((
            user_password,
            rev,
            key_size,
            o_entry,
            u_entry,
            p_entry,
            id1_entry,
            metadata_encrypted,
        ))?
        .unbind())
}

// ---------------------------------------------------------------------------
// pypdf._encryption.AlgV5
// ---------------------------------------------------------------------------

// Algorithms 3.2a-3.10 of the PDF specification (revisions 5-6, AES-256).
// The class lives in `python/pypdf/_encryption.py`; each method delegates here.
// @pypdf: pypdf._encryption.AlgV5

/// Algorithm 3.2a: computing an encryption key (owner side).
// @pypdf: pypdf._encryption.AlgV5.verify_owner_password
#[pyfunction]
#[allow(clippy::similar_names)]
fn encryption_algv5_verify_owner_password(
    py: Python<'_>,
    rev: &Bound<'_, PyAny>,
    password: &Bound<'_, PyAny>,
    o_value: &Bound<'_, PyAny>,
    oe_value: &Bound<'_, PyAny>,
    u_value: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let password = slice_of(py, password, Some(0), Some(127))?;
    let calculated = algv5(py, "calculate_hash")?.call1((
        rev,
        &password,
        slice_of(py, o_value, Some(32), Some(40))?,
        slice_of(py, u_value, Some(0), Some(48))?,
    ))?;
    if !pyutil::py_eq(&calculated, &slice_of(py, o_value, Some(0), Some(32))?)? {
        return Ok(py_bytes(py, b"").unbind());
    }
    let iv = zero_iv(py);
    let tmp_key = algv5(py, "calculate_hash")?.call1((
        rev,
        &password,
        slice_of(py, o_value, Some(40), Some(48))?,
        slice_of(py, u_value, Some(0), Some(48))?,
    ))?;
    Ok(enc_attr(py, "aes_cbc_decrypt")?
        .call1((tmp_key, iv, oe_value))?
        .unbind())
}

/// Algorithm 3.2a: computing an encryption key (user side).
// @pypdf: pypdf._encryption.AlgV5.verify_user_password
#[pyfunction]
#[allow(clippy::similar_names)]
fn encryption_algv5_verify_user_password(
    py: Python<'_>,
    rev: &Bound<'_, PyAny>,
    password: &Bound<'_, PyAny>,
    u_value: &Bound<'_, PyAny>,
    ue_value: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let password = slice_of(py, password, Some(0), Some(127))?;
    let calculated = algv5(py, "calculate_hash")?.call1((
        rev,
        &password,
        slice_of(py, u_value, Some(32), Some(40))?,
        py_bytes(py, b""),
    ))?;
    if !pyutil::py_eq(&calculated, &slice_of(py, u_value, Some(0), Some(32))?)? {
        return Ok(py_bytes(py, b"").unbind());
    }
    let iv = zero_iv(py);
    let tmp_key = algv5(py, "calculate_hash")?.call1((
        rev,
        &password,
        slice_of(py, u_value, Some(40), Some(48))?,
        py_bytes(py, b""),
    ))?;
    Ok(enc_attr(py, "aes_cbc_decrypt")?
        .call1((tmp_key, iv, ue_value))?
        .unbind())
}

/// The R6 hash function of algorithm 3.2a (ported from qpdf).
// @pypdf: pypdf._encryption.AlgV5.calculate_hash
#[pyfunction]
#[allow(clippy::similar_names)]
fn encryption_algv5_calculate_hash(
    py: Python<'_>,
    rev: &Bound<'_, PyAny>,
    password: &Bound<'_, PyAny>,
    salt: &Bound<'_, PyAny>,
    udata: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let hashlib = module(py, "hashlib")?;
    let mut k = digest(&hashlib.call_method1(
        "sha256",
        (concat(
            py,
            &[password.clone(), salt.clone(), udata.clone()],
        )?,),
    )?)?;
    if as_i64(rev)? < 6 {
        return Ok(k.unbind());
    }
    let aes_cbc_encrypt = enc_attr(py, "aes_cbc_encrypt")?;
    let mut count: i64 = 0;
    loop {
        count += 1;
        let k1 = concat(py, &[password.clone(), k.clone(), udata.clone()])?;
        let k1_repeated = k1.call_method1("__mul__", (64,))?;
        let e = aes_cbc_encrypt.call1((
            slice_of(py, &k, Some(0), Some(16))?,
            slice_of(py, &k, Some(16), Some(32))?,
            k1_repeated,
        ))?;
        let hash_fn_name = ["sha256", "sha384", "sha512"]
            [(bytes_sum(&slice_of(py, &e, Some(0), Some(16))?)? % 3) as usize];
        k = digest(&hashlib.getattr(hash_fn_name)?.call1((&e,))?)?;
        let last = e.get_item(-1)?.extract::<i64>()?;
        if count >= 64 && last <= count - 32 {
            break;
        }
    }
    Ok(slice_of(py, &k, Some(0), Some(32))?.unbind())
}

/// `sum(e[:16])` -- the Python `sum()` of the first bytes of `e`.
fn bytes_sum(data: &Bound<'_, PyAny>) -> PyResult<u64> {
    Ok(as_bytes(data)?.iter().map(|byte| u64::from(*byte)).sum())
}

/// Algorithm 3.10: verify the `/Perms` value.
// @pypdf: pypdf._encryption.AlgV5.verify_perms
#[pyfunction]
fn encryption_algv5_verify_perms(
    py: Python<'_>,
    key: &Bound<'_, PyAny>,
    perms: &Bound<'_, PyAny>,
    p_entry: &Bound<'_, PyAny>,
    metadata_encrypted: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let b8 = if metadata_encrypted.is_truthy()? {
        py_bytes(py, b"T")
    } else {
        py_bytes(py, b"F")
    };
    let p1 = concat(
        py,
        &[
            pack_le_u32(py, p_entry)?,
            py_bytes(py, &FOUR_FF),
            b8,
            py_bytes(py, b"adb"),
        ],
    )?;
    let p2 = enc_attr(py, "aes_ecb_decrypt")?.call1((key, perms))?;
    pyutil::py_eq(&p1, &slice_of(py, &p2, Some(0), Some(12))?)
}

/// Algorithms 3.8-3.10: generate the `/U`, `/UE`, `/O`, `/OE`, `/Perms` values.
// @pypdf: pypdf._encryption.AlgV5.generate_values
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn encryption_algv5_generate_values(
    py: Python<'_>,
    rev: &Bound<'_, PyAny>,
    user_password: &Bound<'_, PyAny>,
    owner_password: &Bound<'_, PyAny>,
    key: &Bound<'_, PyAny>,
    p_entry: &Bound<'_, PyAny>,
    metadata_encrypted: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let user_password = slice_of(py, user_password, Some(0), Some(127))?;
    let owner_password = slice_of(py, owner_password, Some(0), Some(127))?;
    let u_values = algv5(py, "compute_U_value")?.call1((rev, &user_password, key))?;
    let u_value = u_values.get_item(0)?;
    let ue_value = u_values.get_item(1)?;
    let o_values = algv5(py, "compute_O_value")?.call1((rev, &owner_password, key, &u_value))?;
    let o_value = o_values.get_item(0)?;
    let oe_value = o_values.get_item(1)?;
    let perms = algv5(py, "compute_Perms_value")?.call1((key, p_entry, metadata_encrypted))?;

    let values = PyDict::new(py);
    values.set_item("/U", &u_value)?;
    values.set_item("/UE", &ue_value)?;
    values.set_item("/O", &o_value)?;
    values.set_item("/OE", &oe_value)?;
    values.set_item("/Perms", &perms)?;
    Ok(values.into_any().unbind())
}

/// Algorithm 3.8: computing the U and UE values.
// @pypdf: pypdf._encryption.AlgV5.compute_U_value
#[pyfunction]
fn encryption_algv5_compute_u_value(
    py: Python<'_>,
    rev: &Bound<'_, PyAny>,
    password: &Bound<'_, PyAny>,
    key: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let random_bytes = py_bytes(py, &token_bytes(py, 16)?);
    let val_salt = slice_of(py, &random_bytes, Some(0), Some(8))?;
    let key_salt = slice_of(py, &random_bytes, Some(8), None)?;
    let u_value = concat(
        py,
        &[
            algv5(py, "calculate_hash")?.call1((
                rev,
                password,
                val_salt.clone(),
                py_bytes(py, b""),
            ))?,
            val_salt,
            key_salt.clone(),
        ],
    )?;
    let tmp_key =
        algv5(py, "calculate_hash")?.call1((rev, password, key_salt, py_bytes(py, b"")))?;
    let iv = zero_iv(py);
    let ue_value = enc_attr(py, "aes_cbc_encrypt")?.call1((tmp_key, iv, key))?;
    Ok(PyTuple::new(py, [u_value, ue_value])?.into_any().unbind())
}

/// Algorithm 3.9: computing the O and OE values.
// @pypdf: pypdf._encryption.AlgV5.compute_O_value
#[pyfunction]
fn encryption_algv5_compute_o_value(
    py: Python<'_>,
    rev: &Bound<'_, PyAny>,
    password: &Bound<'_, PyAny>,
    key: &Bound<'_, PyAny>,
    u_value: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let random_bytes = py_bytes(py, &token_bytes(py, 16)?);
    let val_salt = slice_of(py, &random_bytes, Some(0), Some(8))?;
    let key_salt = slice_of(py, &random_bytes, Some(8), None)?;
    let o_value = concat(
        py,
        &[
            algv5(py, "calculate_hash")?.call1((
                rev,
                password,
                val_salt.clone(),
                u_value.clone(),
            ))?,
            val_salt,
            key_salt.clone(),
        ],
    )?;
    let tmp_key = algv5(py, "calculate_hash")?.call1((
        rev,
        password,
        key_salt,
        slice_of(py, u_value, Some(0), Some(48))?,
    ))?;
    let iv = zero_iv(py);
    let oe_value = enc_attr(py, "aes_cbc_encrypt")?.call1((tmp_key, iv, key))?;
    Ok(PyTuple::new(py, [o_value, oe_value])?.into_any().unbind())
}

/// Algorithm 3.10: computing the `/Perms` value.
// @pypdf: pypdf._encryption.AlgV5.compute_Perms_value
#[pyfunction]
fn encryption_algv5_compute_perms_value(
    py: Python<'_>,
    key: &Bound<'_, PyAny>,
    p_entry: &Bound<'_, PyAny>,
    metadata_encrypted: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let b8 = if metadata_encrypted.is_truthy()? {
        py_bytes(py, b"T")
    } else {
        py_bytes(py, b"F")
    };
    let random_bytes = py_bytes(py, &token_bytes(py, 4)?);
    let data = concat(
        py,
        &[
            pack_le_u32(py, p_entry)?,
            py_bytes(py, &FOUR_FF),
            b8,
            py_bytes(py, b"adb"),
            random_bytes,
        ],
    )?;
    Ok(enc_attr(py, "aes_ecb_encrypt")?
        .call1((key, data))?
        .unbind())
}

// ---------------------------------------------------------------------------
// pypdf._encryption.PasswordType / EncryptAlgorithm / EncryptionValues
// ---------------------------------------------------------------------------

// `PasswordType(IntEnum)`, `EncryptAlgorithm(tuple, Enum)` and the
// `EncryptionValues` annotation holder are Python skeletons in
// `python/pypdf/_encryption.py`; their members/annotations are the public API
// and stay there (coverage annotations recorded here for gate A6).
// @pypdf: pypdf._encryption.PasswordType
// @pypdf: pypdf._encryption.PasswordType.NOT_DECRYPTED
// @pypdf: pypdf._encryption.PasswordType.USER_PASSWORD
// @pypdf: pypdf._encryption.PasswordType.OWNER_PASSWORD
// @pypdf: pypdf._encryption.EncryptAlgorithm
// @pypdf: pypdf._encryption.EncryptAlgorithm.RC4_40
// @pypdf: pypdf._encryption.EncryptAlgorithm.RC4_128
// @pypdf: pypdf._encryption.EncryptAlgorithm.AES_128
// @pypdf: pypdf._encryption.EncryptAlgorithm.AES_256_R5
// @pypdf: pypdf._encryption.EncryptAlgorithm.AES_256
// @pypdf: pypdf._encryption.EncryptionValues
// @pypdf: pypdf._encryption.EncryptionValues.O
// @pypdf: pypdf._encryption.EncryptionValues.U
// @pypdf: pypdf._encryption.EncryptionValues.OE
// @pypdf: pypdf._encryption.EncryptionValues.UE
// @pypdf: pypdf._encryption.EncryptionValues.Perms

// ---------------------------------------------------------------------------
// pypdf._encryption.CryptFilter
// ---------------------------------------------------------------------------

// Stream/string/embedded-file crypt filter collection.
// The class lives in `python/pypdf/_encryption.py`; the methods delegate here.
// @pypdf: pypdf._encryption.CryptFilter

/// `CryptFilter.__init__` -- stores the three crypt objects.
// @pypdf: pypdf._encryption.CryptFilter.__init__
#[pyfunction]
fn encryption_cryptfilter_init(
    instance: &Bound<'_, PyAny>,
    stm_crypt: &Bound<'_, PyAny>,
    str_crypt: &Bound<'_, PyAny>,
    ef_crypt: &Bound<'_, PyAny>,
) -> PyResult<()> {
    instance.setattr("stm_crypt", stm_crypt)?;
    instance.setattr("str_crypt", str_crypt)?;
    instance.setattr("ef_crypt", ef_crypt)
}

/// `CryptFilter.encrypt_object` -- recursively encrypt a PDF object.
// @pypdf: pypdf._encryption.CryptFilter.encrypt_object
#[pyfunction]
fn encryption_cryptfilter_encrypt_object(
    py: Python<'_>,
    instance: &Bound<'_, PyAny>,
    obj: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    if is_instance(obj, &enc_class(py, "ByteStringObject")?)? {
        let data = instance
            .getattr("str_crypt")?
            .call_method1("encrypt", (obj.getattr("original_bytes")?,))?;
        return Ok(enc_class(py, "ByteStringObject")?.call1((data,))?.unbind());
    }
    if is_instance(obj, &enc_class(py, "TextStringObject")?)? {
        let data = instance
            .getattr("str_crypt")?
            .call_method1("encrypt", (obj.call_method0("get_encoded_bytes")?,))?;
        return Ok(enc_class(py, "ByteStringObject")?.call1((data,))?.unbind());
    }
    if is_instance(obj, &enc_class(py, "StreamObject")?)? {
        let obj2 = enc_class(py, "StreamObject")?.call0()?;
        obj2.call_method1("update", (obj,))?;
        let data = instance
            .getattr("stm_crypt")?
            .call_method1("encrypt", (obj.getattr("_data")?,))?;
        obj2.call_method1("set_data", (data,))?;
        // Don't forget the Stream dict.
        for item in obj.call_method0("items")?.try_iter()? {
            let (key, value) = item?.extract::<(Bound<'_, PyAny>, Bound<'_, PyAny>)>()?;
            let encrypted = instance.call_method1("encrypt_object", (value,))?;
            obj2.set_item(key, encrypted)?;
        }
        return Ok(obj2.unbind());
    }
    if is_instance(obj, &enc_class(py, "DictionaryObject")?)? {
        let obj2 = enc_class(py, "DictionaryObject")?.call0()?;
        for item in obj.call_method0("items")?.try_iter()? {
            let (key, value) = item?.extract::<(Bound<'_, PyAny>, Bound<'_, PyAny>)>()?;
            let encrypted = instance.call_method1("encrypt_object", (value,))?;
            obj2.set_item(key, encrypted)?;
        }
        return Ok(obj2.unbind());
    }
    if is_instance(obj, &enc_class(py, "ArrayObject")?)? {
        let mut items: Vec<Bound<'_, PyAny>> = Vec::new();
        for value in obj.try_iter()? {
            items.push(instance.call_method1("encrypt_object", (value?,))?);
        }
        return Ok(enc_class(py, "ArrayObject")?.call1((items,))?.unbind());
    }
    Ok(obj.clone().unbind())
}

/// `CryptFilter.decrypt_object` -- recursively decrypt a PDF object in place.
// @pypdf: pypdf._encryption.CryptFilter.decrypt_object
#[pyfunction]
fn encryption_cryptfilter_decrypt_object(
    py: Python<'_>,
    instance: &Bound<'_, PyAny>,
    obj: &Bound<'_, PyAny>,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let string_types = class_tuple(py, &["ByteStringObject", "TextStringObject"])?;
    let strict_kwargs = PyDict::new(py);
    strict_kwargs.set_item("strict", strict)?;
    if obj.is_instance(&string_types)? {
        let data = instance.getattr("str_crypt")?.call_method(
            "decrypt",
            (obj.getattr("original_bytes")?,),
            Some(&strict_kwargs),
        )?;
        return Ok(enc_attr(py, "create_string_object")?
            .call1((data,))?
            .unbind());
    }
    if is_instance(obj, &enc_class(py, "StreamObject")?)? {
        let data = instance.getattr("stm_crypt")?.call_method(
            "decrypt",
            (obj.getattr("_data")?,),
            Some(&strict_kwargs),
        )?;
        obj.setattr("_data", data)?;
        for item in obj.call_method0("items")?.try_iter()? {
            let (key, value) = item?.extract::<(Bound<'_, PyAny>, Bound<'_, PyAny>)>()?;
            let decrypted =
                instance.call_method("decrypt_object", (value,), Some(&strict_kwargs))?;
            obj.set_item(key, decrypted)?;
        }
        return Ok(obj.clone().unbind());
    }
    if is_instance(obj, &enc_class(py, "DictionaryObject")?)? {
        for item in obj.call_method0("items")?.try_iter()? {
            let (key, value) = item?.extract::<(Bound<'_, PyAny>, Bound<'_, PyAny>)>()?;
            let decrypted =
                instance.call_method("decrypt_object", (value,), Some(&strict_kwargs))?;
            obj.set_item(key, decrypted)?;
        }
        return Ok(obj.clone().unbind());
    }
    if is_instance(obj, &enc_class(py, "ArrayObject")?)? {
        for index in 0..obj.len()? {
            let value = obj.get_item(index)?;
            let decrypted =
                instance.call_method("decrypt_object", (value,), Some(&strict_kwargs))?;
            obj.set_item(index, decrypted)?;
        }
        return Ok(obj.clone().unbind());
    }
    Ok(obj.clone().unbind())
}

// ---------------------------------------------------------------------------
// pypdf._encryption.Encryption
// ---------------------------------------------------------------------------

// The encryption parameters of a document (header + verification + writing).
// The class lives in `python/pypdf/_encryption.py`; the methods delegate here.
// @pypdf: pypdf._encryption.Encryption

/// `Encryption.__init__` -- spell out the encryption dictionary entries.
// @pypdf: pypdf._encryption.Encryption.__init__
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn encryption_init(
    py: Python<'_>,
    instance: &Bound<'_, PyAny>,
    v_entry: &Bound<'_, PyAny>,
    r_entry: &Bound<'_, PyAny>,
    length_entry: &Bound<'_, PyAny>,
    p_entry: &Bound<'_, PyAny>,
    _entry: &Bound<'_, PyAny>,
    encrypt_metadata: &Bound<'_, PyAny>,
    first_id_entry: &Bound<'_, PyAny>,
    stm_filter: &Bound<'_, PyAny>,
    str_filter: &Bound<'_, PyAny>,
    ef_filter: &Bound<'_, PyAny>,
    values: &Bound<'_, PyAny>,
) -> PyResult<()> {
    instance.setattr("V", v_entry)?;
    instance.setattr("R", r_entry)?;
    instance.setattr("Length", length_entry)?;
    // `(P + 0x100000000) % 0x100000000` -- maybe P < 0.
    let modulus = int_obj(py, 0x1_0000_0000);
    let shifted = pyutil::py_add(p_entry, &modulus)?;
    let normalized = shifted.call_method1("__mod__", (&modulus,))?;
    instance.setattr("P", normalized)?;
    instance.setattr("EncryptMetadata", encrypt_metadata)?;
    instance.setattr("id1_entry", first_id_entry)?;
    instance.setattr("StmF", stm_filter)?;
    instance.setattr("StrF", str_filter)?;
    instance.setattr("EFF", ef_filter)?;
    let values = if values.is_truthy()? {
        values.clone()
    } else {
        enc_class(py, "EncryptionValues")?.call0()?
    };
    instance.setattr("values", values)?;
    instance.setattr(
        "_password_type",
        enc_class(py, "PasswordType")?.getattr("NOT_DECRYPTED")?,
    )?;
    instance.setattr("_key", py.None())?;
    instance.setattr("_are_permissions_valid", true)?;
    Ok(())
}

/// `Encryption.is_decrypted`.
// @pypdf: pypdf._encryption.Encryption.is_decrypted
#[pyfunction]
fn encryption_is_decrypted(py: Python<'_>, instance: &Bound<'_, PyAny>) -> PyResult<bool> {
    let password_type = instance.getattr("_password_type")?;
    let not_decrypted = enc_class(py, "PasswordType")?.getattr("NOT_DECRYPTED")?;
    Ok(!pyutil::py_eq(&password_type, &not_decrypted)?)
}

/// `Encryption.encrypt_object`.
// @pypdf: pypdf._encryption.Encryption.encrypt_object
#[pyfunction]
fn encryption_encrypt_object(
    instance: &Bound<'_, PyAny>,
    obj: &Bound<'_, PyAny>,
    idnum: &Bound<'_, PyAny>,
    generation: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    if !instance
        .call_method1("_is_encryption_object", (obj,))?
        .is_truthy()?
    {
        return Ok(obj.clone().unbind());
    }
    let crypt_filter = instance.call_method1("_make_crypt_filter", (idnum, generation))?;
    Ok(crypt_filter
        .call_method1("encrypt_object", (obj,))?
        .unbind())
}

/// `Encryption.decrypt_object`.
// @pypdf: pypdf._encryption.Encryption.decrypt_object
#[pyfunction]
#[pyo3(signature = (instance, obj, idnum, generation, strict = true))]
fn encryption_decrypt_object(
    instance: &Bound<'_, PyAny>,
    obj: &Bound<'_, PyAny>,
    idnum: &Bound<'_, PyAny>,
    generation: &Bound<'_, PyAny>,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    if !instance
        .call_method1("_is_encryption_object", (obj,))?
        .is_truthy()?
    {
        return Ok(obj.clone().unbind());
    }
    let crypt_filter = instance.call_method1("_make_crypt_filter", (idnum, generation))?;
    let kwargs = PyDict::new(instance.py());
    kwargs.set_item("strict", strict)?;
    Ok(crypt_filter
        .call_method("decrypt_object", (obj,), Some(&kwargs))?
        .unbind())
}

/// `Encryption._is_encryption_object`.
// @pypdf: pypdf._encryption.Encryption._is_encryption_object
#[pyfunction]
fn encryption_is_encryption_object(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    let types = class_tuple(
        py,
        &[
            "ByteStringObject",
            "TextStringObject",
            "StreamObject",
            "ArrayObject",
            "DictionaryObject",
        ],
    )?;
    obj.is_instance(&types)
}

/// Algorithm 1 / 3.1a: build the crypt filters for one object.
// @pypdf: pypdf._encryption.Encryption._make_crypt_filter
#[pyfunction]
#[allow(clippy::similar_names)]
fn encryption_make_crypt_filter(
    py: Python<'_>,
    instance: &Bound<'_, PyAny>,
    idnum: &Bound<'_, PyAny>,
    generation: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let pack1 = idnum
        .call_method1("__and__", (int_obj(py, 0xFF_FFFF),))?
        .call_method1("to_bytes", (3, "little"))?;
    let pack2 = generation
        .call_method1("__and__", (int_obj(py, 0xFFFF),))?
        .call_method1("to_bytes", (2, "little"))?;

    let key = instance.getattr("_key")?;
    if !key.is_truthy()? {
        // `assert self._key` -- bare AssertionError, exactly like upstream.
        return Err(PyAssertionError::new_err(()));
    }

    let v_entry = instance.getattr("V")?;
    let mut rc4_key = py_bytes(py, b"");
    let mut aes128_key = py_bytes(py, b"");
    if v_entry.le(4)? {
        let n = if v_entry.eq(1)? {
            5
        } else {
            as_i64(&instance.getattr("Length")?)? / 8
        };
        let key_data = concat(py, &[slice_of(py, &key, Some(0), Some(n))?, pack1, pack2])?;
        let key_hash = md5(py, &key_data)?;
        let rc4_digest = digest(&key_hash)?;
        rc4_key = slice_of(py, &rc4_digest, Some(0), Some((n + 5).min(16)))?;
        key_hash.call_method1("update", (py_bytes(py, b"sAlT"),))?;
        let aes_digest = digest(&key_hash)?;
        aes128_key = slice_of(py, &aes_digest, Some(0), Some((n + 5).min(16)))?;
    }
    let aes256_key = key;

    // Streams, strings and embedded files usually share one crypt filter.
    let crypts = PyDict::new(py);
    for method in [
        instance.getattr("StmF")?,
        instance.getattr("StrF")?,
        instance.getattr("EFF")?,
    ] {
        if !crypts.contains(&method)? {
            let crypt = instance
                .call_method1("_get_crypt", (&method, &rc4_key, &aes128_key, &aes256_key))?;
            crypts.set_item(&method, crypt)?;
        }
    }
    let stm = crypts.get_item(instance.getattr("StmF")?)?;
    let str_crypt = crypts.get_item(instance.getattr("StrF")?)?;
    let ef = crypts.get_item(instance.getattr("EFF")?)?;
    Ok(enc_class(py, "CryptFilter")?
        .call1((stm, str_crypt, ef))?
        .unbind())
}

/// `Encryption._get_crypt` -- pick the crypt implementation for a `/CFM` name.
// @pypdf: pypdf._encryption.Encryption._get_crypt
#[pyfunction]
fn encryption_get_crypt(
    py: Python<'_>,
    method: &Bound<'_, PyAny>,
    rc4_key: &Bound<'_, PyAny>,
    aes128_key: &Bound<'_, PyAny>,
    aes256_key: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let aesv2 = PyString::new(py, "/AESV2").into_any();
    if pyutil::py_eq(method, &aesv2)? {
        return Ok(enc_class(py, "CryptAES")?.call1((aes128_key,))?.unbind());
    }
    let aesv3 = PyString::new(py, "/AESV3").into_any();
    if pyutil::py_eq(method, &aesv3)? {
        return Ok(enc_class(py, "CryptAES")?.call1((aes256_key,))?.unbind());
    }
    let identity = PyString::new(py, "/Identity").into_any();
    if pyutil::py_eq(method, &identity)? {
        return Ok(enc_class(py, "CryptIdentity")?.call0()?.unbind());
    }
    Ok(enc_class(py, "CryptRC4")?.call1((rc4_key,))?.unbind())
}

/// `Encryption._encode_password` -- string -> bytes, with SASLprep for V >= 5.
// @pypdf: pypdf._encryption.Encryption._encode_password
#[pyfunction]
#[pyo3(signature = (instance, password, strict = false))]
fn encryption_encode_password(
    instance: &Bound<'_, PyAny>,
    password: &Bound<'_, PyAny>,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let py = instance.py();
    let str_type = builtins(py)?.getattr("str")?;
    if !password.is_instance(&str_type)? {
        return Ok(password.clone().unbind());
    }
    if instance.getattr("V")?.ge(5)? {
        // PDF 2.0 §7.6.4.3.3: AES-256 (R5/R6) requires SASLprep normalization
        // followed by UTF-8 encoding.
        match enc_attr(py, "_saslprep")?.call1((password,)) {
            Ok(prepared) => {
                return Ok(prepared.call_method1("encode", ("utf-8",))?.unbind());
            }
            Err(error) => {
                let is_saslprep_error = err_is_instance(py, &error, "builtins", "ValueError")?
                    || err_is_instance(py, &error, "builtins", "IndexError")?;
                if !is_saslprep_error {
                    return Err(error);
                }
                if strict {
                    let message = format!(
                        "Password SASLprep normalization failed: {}",
                        pyutil::py_str(error.value(py))?
                    );
                    let strict_error = PyValueError::new_err(message);
                    strict_error.set_cause(py, Some(error));
                    return Err(strict_error);
                }
                let values = PyDict::new(py);
                values.set_item("err", pyutil::py_str(error.value(py))?)?;
                logger_warning(
                    py,
                    "SASLprep normalization failed, using plain UTF-8: %(err)s",
                    Some(&values),
                )?;
            }
        }
        return Ok(password.call_method1("encode", ("utf-8",))?.unbind());
    }
    match password.call_method1("encode", ("latin-1",)) {
        Ok(encoded) => Ok(encoded.unbind()),
        Err(error) => {
            if err_is_instance(py, &error, "builtins", "UnicodeEncodeError")? {
                Ok(password.call_method1("encode", ("utf-8",))?.unbind())
            } else {
                Err(error)
            }
        }
    }
}

/// `Encryption.verify` -- check a password and, on success, remember the key.
// @pypdf: pypdf._encryption.Encryption.verify
#[pyfunction]
#[pyo3(signature = (instance, password, strict = false))]
fn encryption_verify(
    instance: &Bound<'_, PyAny>,
    password: &Bound<'_, PyAny>,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let py = instance.py();
    let kwargs = PyDict::new(py);
    kwargs.set_item("strict", strict)?;
    let pwd = instance.call_method("_encode_password", (password,), Some(&kwargs))?;
    let pair = if instance.getattr("V")?.le(4)? {
        instance.call_method1("verify_v4", (&pwd,))?
    } else {
        instance.call_method1("verify_v5", (&pwd,))?
    };
    let key = pair.get_item(0)?;
    let rc = pair.get_item(1)?;
    let not_decrypted = enc_class(py, "PasswordType")?.getattr("NOT_DECRYPTED")?;
    if !pyutil::py_eq(&rc, &not_decrypted)? {
        instance.setattr("_password_type", &rc)?;
        instance.setattr("_key", &key)?;
    }
    Ok(rc.unbind())
}

/// `Encryption.verify_v4` -- owner password first, then user password.
// @pypdf: pypdf._encryption.Encryption.verify_v4
#[pyfunction]
fn encryption_verify_v4(
    py: Python<'_>,
    instance: &Bound<'_, PyAny>,
    password: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let values = instance.getattr("values")?;
    let key = algv4(py, "verify_owner_password")?.call1((
        password,
        instance.getattr("R")?,
        instance.getattr("Length")?,
        values.getattr("O")?,
        values.getattr("U")?,
        instance.getattr("P")?,
        instance.getattr("id1_entry")?,
        instance.getattr("EncryptMetadata")?,
    ))?;
    let password_type = enc_class(py, "PasswordType")?;
    if key.is_truthy()? {
        return Ok(
            PyTuple::new(py, [key, password_type.getattr("OWNER_PASSWORD")?])?
                .into_any()
                .unbind(),
        );
    }
    let key = algv4(py, "verify_user_password")?.call1((
        password,
        instance.getattr("R")?,
        instance.getattr("Length")?,
        values.getattr("O")?,
        values.getattr("U")?,
        instance.getattr("P")?,
        instance.getattr("id1_entry")?,
        instance.getattr("EncryptMetadata")?,
    ))?;
    if key.is_truthy()? {
        return Ok(
            PyTuple::new(py, [key, password_type.getattr("USER_PASSWORD")?])?
                .into_any()
                .unbind(),
        );
    }
    Ok(PyTuple::new(
        py,
        [py_bytes(py, b""), password_type.getattr("NOT_DECRYPTED")?],
    )?
    .into_any()
    .unbind())
}

/// `Encryption.verify_v5` -- owner password, then user password, then `/Perms`.
// @pypdf: pypdf._encryption.Encryption.verify_v5
#[pyfunction]
fn encryption_verify_v5(
    py: Python<'_>,
    instance: &Bound<'_, PyAny>,
    password: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let values = instance.getattr("values")?;
    let password_type = enc_class(py, "PasswordType")?;
    let mut key = algv5(py, "verify_owner_password")?.call1((
        instance.getattr("R")?,
        password,
        values.getattr("O")?,
        values.getattr("OE")?,
        values.getattr("U")?,
    ))?;
    let mut rc = password_type.getattr("OWNER_PASSWORD")?;
    if !key.is_truthy()? {
        key = algv5(py, "verify_user_password")?.call1((
            instance.getattr("R")?,
            password,
            values.getattr("U")?,
            values.getattr("UE")?,
        ))?;
        rc = password_type.getattr("USER_PASSWORD")?;
    }
    if !key.is_truthy()? {
        return Ok(PyTuple::new(
            py,
            [py_bytes(py, b""), password_type.getattr("NOT_DECRYPTED")?],
        )?
        .into_any()
        .unbind());
    }

    // verify Perms
    let perms_valid = algv5(py, "verify_perms")?.call1((
        &key,
        values.getattr("Perms")?,
        instance.getattr("P")?,
        instance.getattr("EncryptMetadata")?,
    ))?;
    instance.setattr("_are_permissions_valid", &perms_valid)?;
    if !perms_valid.is_truthy()? {
        logger_warning(py, "ignore '/Perms' verify failed", None)?;
    }
    Ok(PyTuple::new(py, [key, rc])?.into_any().unbind())
}

/// `Encryption.write_entry` -- compute the values and build the `/Encrypt` dict.
// @pypdf: pypdf._encryption.Encryption.write_entry
#[pyfunction]
#[pyo3(signature = (instance, user_password, owner_password, strict = false))]
fn encryption_write_entry(
    py: Python<'_>,
    instance: &Bound<'_, PyAny>,
    user_password: &Bound<'_, PyAny>,
    owner_password: &Bound<'_, PyAny>,
    strict: bool,
) -> PyResult<Py<PyAny>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("strict", strict)?;
    let user_pwd = instance.call_method("_encode_password", (user_password,), Some(&kwargs))?;
    let owner_pwd = if owner_password.is_truthy()? {
        instance.call_method("_encode_password", (owner_password,), Some(&kwargs))?
    } else {
        py.None().into_bound(py)
    };
    let owner_pwd = if owner_pwd.is_none() {
        user_pwd.clone()
    } else {
        owner_pwd
    };

    if instance.getattr("V")?.le(4)? {
        instance.call_method1("compute_values_v4", (&user_pwd, &owner_pwd))?;
    } else {
        let length = as_i64(&instance.getattr("Length")?)? / 8;
        let key = py_bytes(py, &token_bytes(py, length as usize)?);
        instance.setattr("_key", &key)?;
        let values = algv5(py, "generate_values")?.call1((
            instance.getattr("R")?,
            &user_pwd,
            &owner_pwd,
            &key,
            instance.getattr("P")?,
            instance.getattr("EncryptMetadata")?,
        ))?;
        let stored_values = instance.getattr("values")?;
        for name in ["/O", "/U", "/OE", "/UE", "/Perms"] {
            stored_values.setattr(&name[1..], values.get_item(name)?)?;
        }
    }

    let dict_obj = enc_class(py, "DictionaryObject")?.call0()?;
    let set_entry = |name: &str, value: Bound<'_, PyAny>| -> PyResult<()> {
        let key = enc_class(py, "NameObject")?.call1((name,))?;
        dict_obj.set_item(key, value)
    };
    set_entry(
        "/V",
        enc_class(py, "NumberObject")?.call1((instance.getattr("V")?,))?,
    )?;
    set_entry(
        "/R",
        enc_class(py, "NumberObject")?.call1((instance.getattr("R")?,))?,
    )?;
    set_entry(
        "/Length",
        enc_class(py, "NumberObject")?.call1((instance.getattr("Length")?,))?,
    )?;
    set_entry(
        "/P",
        enc_class(py, "NumberObject")?.call1((instance.getattr("P")?,))?,
    )?;
    set_entry(
        "/Filter",
        enc_class(py, "NameObject")?.call1(("/Standard",))?,
    )?;
    // ignore /EncryptMetadata

    let stored_values = instance.getattr("values")?;
    set_entry(
        "/O",
        enc_class(py, "ByteStringObject")?.call1((stored_values.getattr("O")?,))?,
    )?;
    set_entry(
        "/U",
        enc_class(py, "ByteStringObject")?.call1((stored_values.getattr("U")?,))?,
    )?;

    if instance.getattr("V")?.ge(4)? {
        // TODO: allow different method
        let std_cf = enc_class(py, "DictionaryObject")?.call0()?;
        let name_object = |name: &str| enc_class(py, "NameObject")?.call1((name,));
        std_cf.set_item(name_object("/AuthEvent")?, name_object("/DocOpen")?)?;
        // `NameObject(self.StmF)` -- `/CFM` holds the *method* name.
        let cfm = name_object(&pyutil::py_str(&instance.getattr("StmF")?)?)?;
        std_cf.set_item(name_object("/CFM")?, cfm)?;
        let length = as_i64(&instance.getattr("Length")?)? / 8;
        std_cf.set_item(
            name_object("/Length")?,
            enc_class(py, "NumberObject")?.call1((length,))?,
        )?;
        let cf = enc_class(py, "DictionaryObject")?.call0()?;
        cf.set_item(name_object("/StdCF")?, std_cf)?;
        set_entry("/CF", cf)?;
        set_entry("/StmF", name_object("/StdCF")?)?;
        set_entry("/StrF", name_object("/StdCF")?)?;
        // ignore EFF
        // dict_obj[NameObject("/EFF")] = NameObject("/StdCF")
    }

    if instance.getattr("V")?.ge(5)? {
        set_entry(
            "/OE",
            enc_class(py, "ByteStringObject")?.call1((stored_values.getattr("OE")?,))?,
        )?;
        set_entry(
            "/UE",
            enc_class(py, "ByteStringObject")?.call1((stored_values.getattr("UE")?,))?,
        )?;
        set_entry(
            "/Perms",
            enc_class(py, "ByteStringObject")?.call1((stored_values.getattr("Perms")?,))?,
        )?;
    }
    Ok(dict_obj.unbind())
}

/// `Encryption.compute_values_v4` -- algorithm 2-5 for revisions 2-4.
// @pypdf: pypdf._encryption.Encryption.compute_values_v4
#[pyfunction]
fn encryption_compute_values_v4(
    py: Python<'_>,
    instance: &Bound<'_, PyAny>,
    user_password: &Bound<'_, PyAny>,
    owner_password: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let rc4_key = algv4(py, "compute_O_value_key")?.call1((
        owner_password,
        instance.getattr("R")?,
        instance.getattr("Length")?,
    ))?;
    let o_value =
        algv4(py, "compute_O_value")?.call1((&rc4_key, user_password, instance.getattr("R")?))?;
    let key = algv4(py, "compute_key")?.call1((
        user_password,
        instance.getattr("R")?,
        instance.getattr("Length")?,
        &o_value,
        instance.getattr("P")?,
        instance.getattr("id1_entry")?,
        instance.getattr("EncryptMetadata")?,
    ))?;
    let u_value = algv4(py, "compute_U_value")?.call1((
        &key,
        instance.getattr("R")?,
        instance.getattr("id1_entry")?,
    ))?;

    instance.setattr("_key", &key)?;
    let values = instance.getattr("values")?;
    values.setattr("O", o_value)?;
    values.setattr("U", u_value)?;
    Ok(())
}

/// `Encryption.read` -- build an `Encryption` from an `/Encrypt` dictionary.
// @pypdf: pypdf._encryption.Encryption.read
#[pyfunction]
fn encryption_read(
    py: Python<'_>,
    encryption_entry: &Bound<'_, PyAny>,
    first_id_entry: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    if !pyutil::py_eq(
        &encryption_entry.call_method1("get", ("/Filter",))?,
        &PyString::new(py, "/Standard").into_any(),
    )? {
        return Err(PyNotImplementedError::new_err(
            "only Standard PDF encryption handler is available",
        ));
    }
    if encryption_entry.contains("/SubFilter")? {
        return Err(PyNotImplementedError::new_err("/SubFilter NOT supported"));
    }

    let mut stm_filter = PyString::new(py, "/V2").into_any();
    let mut str_filter = PyString::new(py, "/V2").into_any();
    let mut ef_filter = PyString::new(py, "/V2").into_any();

    let alg_ver = encryption_entry.call_method1("get", ("/V", int_obj(py, 0)))?;
    let allowed_versions = PyTuple::new(py, [1i64, 2, 3, 4, 5])?;
    if !allowed_versions.contains(&alg_ver)? {
        return Err(PyNotImplementedError::new_err(format!(
            "Encryption V={} NOT supported",
            pyutil::py_str(&alg_ver)?
        )));
    }
    let mut stm_crypt_filter: Option<Bound<'_, PyAny>> = None;
    if alg_ver.ge(4)? {
        // /CF is only needed to look up named crypt filters;
        // /StmF, /StrF and /EFF default to /Identity.
        let filters = encryption_entry
            .call_method1("get", ("/CF", enc_class(py, "DictionaryObject")?.call0()?))?
            .call_method0("get_object")?;
        if !is_instance(&filters, &enc_class(py, "DictionaryObject")?)? {
            return Err(errors::pdf_read_error(
                py,
                "/CF entry of the encryption dictionary is not a dictionary",
            ));
        }

        let name_types = builtin_types(py, &["str"])?;
        stm_filter = call_get_entry(
            py,
            encryption_entry,
            "/StmF",
            &name_types,
            "name",
            &PyString::new(py, "/Identity").into_any(),
            false,
        )?;
        str_filter = call_get_entry(
            py,
            encryption_entry,
            "/StrF",
            &name_types,
            "name",
            &PyString::new(py, "/Identity").into_any(),
            false,
        )?;
        ef_filter = call_get_entry(
            py,
            encryption_entry,
            "/EFF",
            &name_types,
            "name",
            &stm_filter,
            false,
        )?;

        if !pyutil::py_eq(&stm_filter, &PyString::new(py, "/Identity").into_any())? {
            let crypt_filter = enc_attr(py, "_get_crypt_filter")?.call1((&filters, &stm_filter))?;
            stm_filter = call_get_entry(
                py,
                &crypt_filter,
                "/CFM",
                &name_types,
                "name",
                &PyString::new(py, "/None").into_any(),
                false,
            )?;
            stm_crypt_filter = Some(crypt_filter);
        }
        if !pyutil::py_eq(&str_filter, &PyString::new(py, "/Identity").into_any())? {
            let crypt_filter = enc_attr(py, "_get_crypt_filter")?.call1((&filters, &str_filter))?;
            str_filter = call_get_entry(
                py,
                &crypt_filter,
                "/CFM",
                &name_types,
                "name",
                &PyString::new(py, "/None").into_any(),
                false,
            )?;
        }
        if !pyutil::py_eq(&ef_filter, &PyString::new(py, "/Identity").into_any())? {
            let crypt_filter = enc_attr(py, "_get_crypt_filter")?.call1((&filters, &ef_filter))?;
            ef_filter = call_get_entry(
                py,
                &crypt_filter,
                "/CFM",
                &name_types,
                "name",
                &PyString::new(py, "/None").into_any(),
                false,
            )?;
        }

        let allowed_methods = PyTuple::new(
            py,
            [
                PyString::new(py, "/Identity").into_any(),
                PyString::new(py, "/V2").into_any(),
                PyString::new(py, "/AESV2").into_any(),
                PyString::new(py, "/AESV3").into_any(),
            ],
        )?;
        if !allowed_methods.contains(&stm_filter)? {
            return Err(PyNotImplementedError::new_err(format!(
                "StmF Method {} NOT supported!",
                pyutil::py_str(&stm_filter)?
            )));
        }
        if !allowed_methods.contains(&str_filter)? {
            return Err(PyNotImplementedError::new_err(format!(
                "StrF Method {} NOT supported!",
                pyutil::py_str(&str_filter)?
            )));
        }
        if !allowed_methods.contains(&ef_filter)? {
            return Err(PyNotImplementedError::new_err(format!(
                "EFF Method {} NOT supported!",
                pyutil::py_str(&ef_filter)?
            )));
        }
    }

    let number_types = builtin_types(py, &["int", "float"])?;
    let alg_rev = to_int(
        py,
        &call_get_entry(
            py,
            encryption_entry,
            "/R",
            &number_types,
            "number",
            &py.None().into_bound(py),
            true,
        )?,
    )?;
    let perm_flags = to_int(
        py,
        &call_get_entry(
            py,
            encryption_entry,
            "/P",
            &number_types,
            "number",
            &py.None().into_bound(py),
            true,
        )?,
    )?;
    let mut key_bits = to_int(
        py,
        &call_get_entry(
            py,
            encryption_entry,
            "/Length",
            &number_types,
            "number",
            &int_obj(py, 40),
            false,
        )?,
    )?;
    if alg_ver.eq(4)? && pyutil::py_eq(&stm_filter, &PyString::new(py, "/AESV2").into_any())? {
        if let Some(crypt_filter) = &stm_crypt_filter {
            // CF /Length is in bytes (default 16 for AES-128), convert to bits.
            let filter_length = to_int(
                py,
                &call_get_entry(
                    py,
                    crypt_filter,
                    "/Length",
                    &number_types,
                    "number",
                    &int_obj(py, 16),
                    false,
                )?,
            )?;
            key_bits = filter_length.call_method1("__mul__", (int_obj(py, 8),))?;
        }
    }
    let encrypt_metadata = encryption_entry
        .call_method1("get", ("/EncryptMetadata", null_object(py)?))?
        .call_method0("get_object")?;
    let encrypt_metadata = if is_instance(&encrypt_metadata, &enc_class(py, "BooleanObject")?)? {
        encrypt_metadata.getattr("value")?
    } else {
        if !is_instance(&encrypt_metadata, &enc_class(py, "NullObject")?)? {
            logger_warning(
                py,
                "Ignoring non-boolean /EncryptMetadata entry of the encryption dictionary",
                None,
            )?;
        }
        bool_obj(py, true)
    };
    let string_types = class_tuple(py, &["ByteStringObject", "TextStringObject"])?;
    let empty_string = enc_class(py, "ByteStringObject")?.call0()?;
    let values = enc_class(py, "EncryptionValues")?.call0()?;
    let mut stored = Vec::with_capacity(5);
    for (key, default, required) in [
        ("/O", None, true),
        ("/U", None, true),
        ("/OE", Some(empty_string.clone()), false),
        ("/UE", Some(empty_string.clone()), false),
        ("/Perms", Some(empty_string.clone()), false),
    ] {
        let entry = call_get_entry(
            py,
            encryption_entry,
            key,
            &string_types,
            "string",
            &default.unwrap_or_else(|| py.None().into_bound(py)),
            required,
        )?;
        stored.push(entry.getattr("original_bytes")?);
    }
    values.setattr("O", stored[0].clone())?;
    values.setattr("U", stored[1].clone())?;
    values.setattr("OE", stored[2].clone())?;
    values.setattr("UE", stored[3].clone())?;
    values.setattr("Perms", stored[4].clone())?;

    let kwargs = PyDict::new(py);
    kwargs.set_item("V", &alg_ver)?;
    kwargs.set_item("R", &alg_rev)?;
    kwargs.set_item("Length", &key_bits)?;
    kwargs.set_item("P", &perm_flags)?;
    kwargs.set_item("EncryptMetadata", &encrypt_metadata)?;
    kwargs.set_item("first_id_entry", first_id_entry)?;
    kwargs.set_item("values", &values)?;
    kwargs.set_item("StrF", &str_filter)?;
    kwargs.set_item("StmF", &stm_filter)?;
    kwargs.set_item("EFF", &ef_filter)?;
    kwargs.set_item("entry", encryption_entry)?;
    Ok(enc_class(py, "Encryption")?
        .call((), Some(&kwargs))?
        .unbind())
}

/// `Encryption.make` -- build an `Encryption` for writing.
// @pypdf: pypdf._encryption.Encryption.make
#[pyfunction]
fn encryption_make(
    py: Python<'_>,
    alg: &Bound<'_, PyAny>,
    permissions: &Bound<'_, PyAny>,
    first_id_entry: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let items: Vec<Bound<'_, PyAny>> = alg.try_iter()?.collect::<PyResult<Vec<_>>>()?;
    let mut items = items.into_iter();
    let (Some(alg_ver), Some(alg_rev), Some(key_bits)) = (items.next(), items.next(), items.next())
    else {
        return Err(PyValueError::new_err(
            "not enough values to unpack (expected 3)",
        ));
    };
    if items.next().is_some() {
        return Err(PyValueError::new_err(
            "too many values to unpack (expected 3)",
        ));
    }

    let mut stm_filter = PyString::new(py, "/V2").into_any();
    let mut str_filter = PyString::new(py, "/V2").into_any();
    let mut ef_filter = PyString::new(py, "/V2").into_any();

    let encrypt_algorithm = enc_class(py, "EncryptAlgorithm")?;
    if pyutil::py_eq(alg, &encrypt_algorithm.getattr("AES_128")?)? {
        stm_filter = PyString::new(py, "/AESV2").into_any();
        str_filter = PyString::new(py, "/AESV2").into_any();
        ef_filter = PyString::new(py, "/AESV2").into_any();
    } else if pyutil::py_eq(alg, &encrypt_algorithm.getattr("AES_256_R5")?)?
        || pyutil::py_eq(alg, &encrypt_algorithm.getattr("AES_256")?)?
    {
        stm_filter = PyString::new(py, "/AESV3").into_any();
        str_filter = PyString::new(py, "/AESV3").into_any();
        ef_filter = PyString::new(py, "/AESV3").into_any();
    }

    let kwargs = PyDict::new(py);
    kwargs.set_item("V", &alg_ver)?;
    kwargs.set_item("R", &alg_rev)?;
    kwargs.set_item("Length", &key_bits)?;
    kwargs.set_item("P", permissions)?;
    kwargs.set_item("EncryptMetadata", true)?;
    kwargs.set_item("first_id_entry", first_id_entry)?;
    kwargs.set_item("values", py.None())?;
    kwargs.set_item("StrF", &str_filter)?;
    kwargs.set_item("StmF", &stm_filter)?;
    kwargs.set_item("EFF", &ef_filter)?;
    kwargs.set_item("entry", enc_class(py, "DictionaryObject")?.call0()?)?;
    Ok(enc_class(py, "Encryption")?
        .call((), Some(&kwargs))?
        .unbind())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(encryption_padding, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_saslprep, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_get_entry, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_get_crypt_filter, module)?)?;

    module.add_function(wrap_pyfunction!(encryption_algv4_compute_key, module)?)?;
    module.add_function(wrap_pyfunction!(
        encryption_algv4_compute_o_value_key,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(encryption_algv4_compute_o_value, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_algv4_compute_u_value, module)?)?;
    module.add_function(wrap_pyfunction!(
        encryption_algv4_verify_user_password,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        encryption_algv4_verify_owner_password,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(
        encryption_algv5_verify_owner_password,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        encryption_algv5_verify_user_password,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(encryption_algv5_calculate_hash, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_algv5_verify_perms, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_algv5_generate_values, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_algv5_compute_u_value, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_algv5_compute_o_value, module)?)?;
    module.add_function(wrap_pyfunction!(
        encryption_algv5_compute_perms_value,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(encryption_cryptfilter_init, module)?)?;
    module.add_function(wrap_pyfunction!(
        encryption_cryptfilter_encrypt_object,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        encryption_cryptfilter_decrypt_object,
        module
    )?)?;

    module.add_function(wrap_pyfunction!(encryption_init, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_is_decrypted, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_encrypt_object, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_decrypt_object, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_is_encryption_object, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_make_crypt_filter, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_get_crypt, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_encode_password, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_verify, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_verify_v4, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_verify_v5, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_write_entry, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_compute_values_v4, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_read, module)?)?;
    module.add_function(wrap_pyfunction!(encryption_make, module)?)?;
    Ok(())
}
