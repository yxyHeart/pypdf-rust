# Copyright (c) 2022, exiledkingcc
# All rights reserved.
#
# Redistribution and use in source and binary forms, with or without
# modification, are permitted provided that the following conditions are
# met:
#
# * Redistributions of source code must retain the above copyright notice,
# this list of conditions and the following disclaimer.
# * Redistributions in binary form must reproduce the above copyright notice,
# this list of conditions and the following disclaimer in the documentation
# and/or other materials provided with the distribution.
# * The name of the author may not be used to endorse or promote products
# derived from this software without specific prior written permission.
#
# THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
# AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
# IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
# ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE
# LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
# CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
# SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
# INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
# CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
# ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
# POSSIBILITY OF SUCH DAMAGE.

# Rust-backed module: the classes, enum members, constants and annotations are
# defined here exactly as upstream; every function/method body delegates to
# ``crates/pypdf-py/src/modules/encryption_mod.rs``.

import hashlib  # noqa: F401  (upstream module global; the Rust logic drives it)
import secrets  # noqa: F401  (upstream module global; randomness is Rust-side)
import stringprep
import struct  # noqa: F401  (upstream module global; the Rust logic drives it)
import unicodedata  # noqa: F401  (upstream module global; the Rust logic drives it)
from enum import Enum, IntEnum
from typing import Any

from pypdf._crypt_providers import (
    CryptAES,
    CryptBase,
    CryptIdentity,
    CryptRC4,
    aes_cbc_decrypt,
    aes_cbc_encrypt,
    aes_ecb_decrypt,
    aes_ecb_encrypt,
    rc4_decrypt,
    rc4_encrypt,
)
from pypdf._pypdf import (
    encryption_algv4_compute_key as _algv4_compute_key,
    encryption_algv4_compute_o_value as _algv4_compute_o_value,
    encryption_algv4_compute_o_value_key as _algv4_compute_o_value_key,
    encryption_algv4_compute_u_value as _algv4_compute_u_value,
    encryption_algv4_verify_owner_password as _algv4_verify_owner_password,
    encryption_algv4_verify_user_password as _algv4_verify_user_password,
    encryption_algv5_calculate_hash as _algv5_calculate_hash,
    encryption_algv5_compute_o_value as _algv5_compute_o_value,
    encryption_algv5_compute_perms_value as _algv5_compute_perms_value,
    encryption_algv5_compute_u_value as _algv5_compute_u_value,
    encryption_algv5_generate_values as _algv5_generate_values,
    encryption_algv5_verify_owner_password as _algv5_verify_owner_password,
    encryption_algv5_verify_perms as _algv5_verify_perms,
    encryption_algv5_verify_user_password as _algv5_verify_user_password,
    encryption_compute_values_v4 as _compute_values_v4,
    encryption_cryptfilter_decrypt_object as _cryptfilter_decrypt_object,
    encryption_cryptfilter_encrypt_object as _cryptfilter_encrypt_object,
    encryption_cryptfilter_init as _cryptfilter_init,
    encryption_decrypt_object as _decrypt_object,
    encryption_encode_password as _encode_password,
    encryption_encrypt_object as _encrypt_object,
    encryption_get_crypt as _get_crypt,
    encryption_get_crypt_filter as _get_crypt_filter_impl,
    encryption_get_entry as _get_entry_impl,
    encryption_init as _encryption_init,
    encryption_is_decrypted as _is_decrypted,
    encryption_is_encryption_object as _is_encryption_object,
    encryption_make as _make,
    encryption_make_crypt_filter as _make_crypt_filter,
    encryption_padding as _padding_impl,
    encryption_read as _read,
    encryption_saslprep as _saslprep_impl,
    encryption_verify as _verify,
    encryption_verify_v4 as _verify_v4,
    encryption_verify_v5 as _verify_v5,
    encryption_write_entry as _write_entry,
)
from ._utils import logger_warning  # noqa: F401  (upstream module global)
from .errors import PdfReadError  # noqa: F401  (upstream module global)
from .generic import (  # noqa: F401  (upstream module globals)
    ArrayObject,
    BooleanObject,
    ByteStringObject,
    DictionaryObject,
    NameObject,
    NullObject,
    NumberObject,
    PdfObject,
    StreamObject,
    TextStringObject,
    create_string_object,
)


class CryptFilter:
    def __init__(
        self,
        stm_crypt: CryptBase,
        str_crypt: CryptBase,
        ef_crypt: CryptBase,
    ) -> None:
        _cryptfilter_init(self, stm_crypt, str_crypt, ef_crypt)

    def encrypt_object(self, obj: PdfObject) -> PdfObject:
        return _cryptfilter_encrypt_object(self, obj)

    def decrypt_object(self, obj: PdfObject, *, strict: bool = True) -> PdfObject:
        return _cryptfilter_decrypt_object(self, obj, strict=strict)


_PADDING = (
    b"\x28\xbf\x4e\x5e\x4e\x75\x8a\x41\x64\x00\x4e\x56\xff\xfa\x01\x08"
    b"\x2e\x2e\x00\xb6\xd0\x68\x3e\x80\x2f\x0c\xa9\xfe\x64\x53\x69\x7a"
)


def _padding(data: bytes) -> bytes:
    return _padding_impl(data)


# Module-level constant for SASLprep prohibited character checks (RFC 4013 §2.3)
_SASLPREP_PROHIBITED_CHECKS = (
    (stringprep.in_table_c12, "non-ASCII space character"),
    (stringprep.in_table_c21, "ASCII control character"),
    (stringprep.in_table_c22, "non-ASCII control character"),
    (stringprep.in_table_c3, "private use character"),
    (stringprep.in_table_c4, "non-character code point"),
    (stringprep.in_table_c5, "surrogate code point"),
    (stringprep.in_table_c6, "inappropriate for plain text"),
    (stringprep.in_table_c7, "inappropriate for canonical representation"),
    (stringprep.in_table_c8, "change display properties/deprecated"),
    (stringprep.in_table_c9, "tagging character"),
    (stringprep.in_table_a1, "unassigned code point"),
)


class AlgV4:
    @staticmethod
    def compute_key(
        password: bytes,
        rev: int,
        key_size: int,
        o_entry: bytes,
        P: int,
        id1_entry: bytes,
        metadata_encrypted: bool,
    ) -> bytes:
        """Algorithm 2: Computing an encryption key."""
        return _algv4_compute_key(
            password, rev, key_size, o_entry, P, id1_entry, metadata_encrypted
        )

    @staticmethod
    def compute_O_value_key(owner_password: bytes, rev: int, key_size: int) -> bytes:
        """Algorithm 3: The O (owner password) value key."""
        return _algv4_compute_o_value_key(owner_password, rev, key_size)

    @staticmethod
    def compute_O_value(rc4_key: bytes, user_password: bytes, rev: int) -> bytes:
        """Algorithm 3: The O (owner password) value."""
        return _algv4_compute_o_value(rc4_key, user_password, rev)

    @staticmethod
    def compute_U_value(key: bytes, rev: int, id1_entry: bytes) -> bytes:
        """Algorithm 4/5: The U (user password) value."""
        return _algv4_compute_u_value(key, rev, id1_entry)

    @staticmethod
    def verify_user_password(
        user_password: bytes,
        rev: int,
        key_size: int,
        o_entry: bytes,
        u_entry: bytes,
        P: int,
        id1_entry: bytes,
        metadata_encrypted: bool,
    ) -> bytes:
        """Algorithm 6: Authenticating the user password."""
        return _algv4_verify_user_password(
            user_password,
            rev,
            key_size,
            o_entry,
            u_entry,
            P,
            id1_entry,
            metadata_encrypted,
        )

    @staticmethod
    def verify_owner_password(
        owner_password: bytes,
        rev: int,
        key_size: int,
        o_entry: bytes,
        u_entry: bytes,
        P: int,
        id1_entry: bytes,
        metadata_encrypted: bool,
    ) -> bytes:
        """Algorithm 7: Authenticating the owner password."""
        return _algv4_verify_owner_password(
            owner_password,
            rev,
            key_size,
            o_entry,
            u_entry,
            P,
            id1_entry,
            metadata_encrypted,
        )


class AlgV5:
    @staticmethod
    def verify_owner_password(
        R: int, password: bytes, o_value: bytes, oe_value: bytes, u_value: bytes
    ) -> bytes:
        """Algorithm 3.2a: Computing an encryption key (owner password)."""
        return _algv5_verify_owner_password(R, password, o_value, oe_value, u_value)

    @staticmethod
    def verify_user_password(
        R: int, password: bytes, u_value: bytes, ue_value: bytes
    ) -> bytes:
        """Algorithm 3.2a: Computing an encryption key (user password)."""
        return _algv5_verify_user_password(R, password, u_value, ue_value)

    @staticmethod
    def calculate_hash(R: int, password: bytes, salt: bytes, udata: bytes) -> bytes:
        return _algv5_calculate_hash(R, password, salt, udata)

    @staticmethod
    def verify_perms(
        key: bytes, perms: bytes, p: int, metadata_encrypted: bool
    ) -> bool:
        return _algv5_verify_perms(key, perms, p, metadata_encrypted)

    @staticmethod
    def generate_values(
        R: int,
        user_password: bytes,
        owner_password: bytes,
        key: bytes,
        p: int,
        metadata_encrypted: bool,
    ) -> dict[Any, Any]:
        return _algv5_generate_values(
            R, user_password, owner_password, key, p, metadata_encrypted
        )

    @staticmethod
    def compute_U_value(R: int, password: bytes, key: bytes) -> tuple[bytes, bytes]:
        """Algorithm 3.8: The U and UE values."""
        return _algv5_compute_u_value(R, password, key)

    @staticmethod
    def compute_O_value(
        R: int, password: bytes, key: bytes, u_value: bytes
    ) -> tuple[bytes, bytes]:
        """Algorithm 3.9: The O and OE values."""
        return _algv5_compute_o_value(R, password, key, u_value)

    @staticmethod
    def compute_Perms_value(key: bytes, p: int, metadata_encrypted: bool) -> bytes:
        """Algorithm 3.10: The Perms value."""
        return _algv5_compute_perms_value(key, p, metadata_encrypted)


class PasswordType(IntEnum):
    NOT_DECRYPTED = 0
    USER_PASSWORD = 1
    OWNER_PASSWORD = 2


def _saslprep(password: str) -> str:
    """
    Apply the SASLprep profile (RFC 4013) of stringprep (RFC 3454).

    This normalizes Unicode passwords for PDF 2.0 (AES-256, revision 5/6)
    encryption as required by the PDF specification.

    Args:
        password: The Unicode password string to normalize.

    Returns:
        The SASLprep-normalized string.

    Raises:
        ValueError: If the password contains prohibited characters
            or fails the bidirectional character check.

    """
    return _saslprep_impl(password)


class EncryptAlgorithm(tuple, Enum):  # type: ignore[type-arg] # noqa: SLOT001
    # V, R, Length
    RC4_40 = (1, 2, 40)
    RC4_128 = (2, 3, 128)
    AES_128 = (4, 4, 128)
    AES_256_R5 = (5, 5, 256)
    AES_256 = (5, 6, 256)


class EncryptionValues:
    O: bytes  # noqa: E741
    U: bytes
    OE: bytes
    UE: bytes
    Perms: bytes


def _get_entry(
    dictionary: DictionaryObject,
    key: str,
    *,
    expected_types: tuple[type, ...],
    type_name: str,
    default: Any | None = None,
    required: bool = False,
) -> Any:
    """Fetch an entry of the encryption dictionary, treating null as absent."""
    return _get_entry_impl(
        dictionary, key, expected_types, type_name, default, required
    )


def _get_crypt_filter(filters: DictionaryObject, name: str) -> DictionaryObject:
    return _get_crypt_filter_impl(filters, name)


class Encryption:
    """
    Collects and manages parameters for PDF document encryption and decryption.

    Args:
        V: A code specifying the algorithm to be used in encrypting and
           decrypting the document.
        R: The revision of the standard security handler.
        Length: The length of the encryption key in bits.
        P: A set of flags specifying which operations shall be permitted
           when the document is opened with user access
        entry: The encryption dictionary object.
        EncryptMetadata: Whether to encrypt metadata in the document.
        first_id_entry: The first 16 bytes of the file's original ID.
        StmF: The name of the crypt filter that shall be used by default
              when decrypting streams.
        StrF: The name of the crypt filter that shall be used when decrypting
              all strings in the document.
        EFF: The name of the crypt filter that shall be used when
             encrypting embedded file streams that do not have their own
             crypt filter specifier.
        values: Additional encryption parameters.

    """

    def __init__(
        self,
        V: int,
        R: int,
        Length: int,
        P: int,
        entry: DictionaryObject,
        EncryptMetadata: bool,
        first_id_entry: bytes,
        StmF: str,
        StrF: str,
        EFF: str,
        values: EncryptionValues | None,
    ) -> None:
        _encryption_init(
            self,
            V,
            R,
            Length,
            P,
            entry,
            EncryptMetadata,
            first_id_entry,
            StmF,
            StrF,
            EFF,
            values,
        )

    def is_decrypted(self) -> bool:
        return _is_decrypted(self)

    def encrypt_object(self, obj: PdfObject, idnum: int, generation: int) -> PdfObject:
        return _encrypt_object(self, obj, idnum, generation)

    def decrypt_object(
        self, obj: PdfObject, idnum: int, generation: int, *, strict: bool = True
    ) -> PdfObject:
        return _decrypt_object(self, obj, idnum, generation, strict=strict)

    @staticmethod
    def _is_encryption_object(obj: PdfObject) -> bool:
        return _is_encryption_object(obj)

    def _make_crypt_filter(self, idnum: int, generation: int) -> CryptFilter:
        """Algorithm 1 / 3.1a: Encryption of data using RC4, AES or AES-256."""
        return _make_crypt_filter(self, idnum, generation)

    @staticmethod
    def _get_crypt(
        method: str, rc4_key: bytes, aes128_key: bytes, aes256_key: bytes
    ) -> CryptBase:
        return _get_crypt(method, rc4_key, aes128_key, aes256_key)

    def _encode_password(
        self, password: bytes | str, *, strict: bool = False
    ) -> bytes:
        return _encode_password(self, password, strict=strict)

    def verify(
        self, password: bytes | str, *, strict: bool = False
    ) -> PasswordType:
        return _verify(self, password, strict=strict)

    def verify_v4(self, password: bytes) -> tuple[bytes, PasswordType]:
        return _verify_v4(self, password)

    def verify_v5(self, password: bytes) -> tuple[bytes, PasswordType]:
        return _verify_v5(self, password)

    def write_entry(
        self,
        user_password: str,
        owner_password: str | None,
        *,
        strict: bool = False,
    ) -> DictionaryObject:
        return _write_entry(self, user_password, owner_password, strict=strict)

    def compute_values_v4(self, user_password: bytes, owner_password: bytes) -> None:
        _compute_values_v4(self, user_password, owner_password)

    @staticmethod
    def read(encryption_entry: DictionaryObject, first_id_entry: bytes) -> "Encryption":
        return _read(encryption_entry, first_id_entry)

    @staticmethod
    def make(
        alg: EncryptAlgorithm, permissions: int, first_id_entry: bytes
    ) -> "Encryption":
        return _make(alg, permissions, first_id_entry)

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
