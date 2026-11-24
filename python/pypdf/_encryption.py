"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._encryption``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import hashlib
import secrets
import stringprep
import struct
import unicodedata
from enum import Enum, IntEnum
from typing import Any
from pypdf._crypt_providers import CryptAES, CryptBase, CryptIdentity, CryptRC4, aes_cbc_decrypt, aes_cbc_encrypt, aes_ecb_decrypt, aes_ecb_encrypt, rc4_decrypt, rc4_encrypt
from ._utils import logger_warning
from .errors import PdfReadError
from .generic import ArrayObject, BooleanObject, ByteStringObject, DictionaryObject, NameObject, NullObject, NumberObject, PdfObject, StreamObject, TextStringObject, create_string_object
class CryptFilter:
    """Stub for pypdf._encryption.CryptFilter."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def encrypt_object(self, *args, **kwargs):
        return None  # inert stub
    def decrypt_object(self, *args, **kwargs):
        return None  # inert stub
_PADDING = b'(\xbfN^Nu\x8aAd\x00NV\xff\xfa\x01\x08..\x00\xb6\xd0h>\x80/\x0c\xa9\xfedSiz'
def _padding(*args, **kwargs):
    return None  # inert stub: pypdf._encryption._padding
_SASLPREP_PROHIBITED_CHECKS = ()
class AlgV4:
    """Stub for pypdf._encryption.AlgV4."""
    @staticmethod
    def compute_key(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def compute_O_value_key(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def compute_O_value(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def compute_U_value(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def verify_user_password(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def verify_owner_password(*args, **kwargs):
        return None  # inert stub
class AlgV5:
    """Stub for pypdf._encryption.AlgV5."""
    @staticmethod
    def verify_owner_password(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def verify_user_password(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def calculate_hash(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def verify_perms(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def generate_values(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def compute_U_value(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def compute_O_value(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def compute_Perms_value(*args, **kwargs):
        return None  # inert stub
class PasswordType:
    """Stub for pypdf._encryption.PasswordType."""
    NOT_DECRYPTED = 0
    USER_PASSWORD = 1
    OWNER_PASSWORD = 2
def _saslprep(*args, **kwargs):
    return None  # inert stub: pypdf._encryption._saslprep
class EncryptAlgorithm:
    """Stub for pypdf._encryption.EncryptAlgorithm."""
    RC4_40 = ()
    RC4_128 = ()
    AES_128 = ()
    AES_256_R5 = ()
    AES_256 = ()
class EncryptionValues:
    """Stub for pypdf._encryption.EncryptionValues."""
    O = None
    U = None
    OE = None
    UE = None
    Perms = None
def _get_entry(*args, **kwargs):
    return None  # inert stub: pypdf._encryption._get_entry
def _get_crypt_filter(*args, **kwargs):
    return None  # inert stub: pypdf._encryption._get_crypt_filter
class Encryption:
    """Stub for pypdf._encryption.Encryption."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def is_decrypted(self, *args, **kwargs):
        return None  # inert stub
    def encrypt_object(self, *args, **kwargs):
        return None  # inert stub
    def decrypt_object(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _is_encryption_object(*args, **kwargs):
        return None  # inert stub
    def _make_crypt_filter(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _get_crypt(*args, **kwargs):
        return None  # inert stub
    def _encode_password(self, *args, **kwargs):
        return None  # inert stub
    def verify(self, *args, **kwargs):
        return None  # inert stub
    def verify_v4(self, *args, **kwargs):
        return None  # inert stub
    def verify_v5(self, *args, **kwargs):
        return None  # inert stub
    def write_entry(self, *args, **kwargs):
        return None  # inert stub
    def compute_values_v4(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def read(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def make(*args, **kwargs):
        return None  # inert stub
