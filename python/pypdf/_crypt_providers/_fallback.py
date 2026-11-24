"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._crypt_providers._fallback``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

from pypdf._crypt_providers._base import CryptBase
from pypdf.errors import DependencyError
_DEPENDENCY_ERROR_STR = 'cryptography>=3.1 is required for AES algorithm'
crypt_provider = ()
class CryptRC4(CryptBase):
    """Stub for pypdf._crypt_providers._fallback.CryptRC4."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def encrypt(self, *args, **kwargs):
        return None  # inert stub
    def decrypt(self, *args, **kwargs):
        return None  # inert stub
class CryptAES(CryptBase):
    """Stub for pypdf._crypt_providers._fallback.CryptAES."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def encrypt(self, *args, **kwargs):
        return None  # inert stub
    def decrypt(self, *args, **kwargs):
        return None  # inert stub
def rc4_encrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._fallback.rc4_encrypt
def rc4_decrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._fallback.rc4_decrypt
def aes_ecb_encrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._fallback.aes_ecb_encrypt
def aes_ecb_decrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._fallback.aes_ecb_decrypt
def aes_cbc_encrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._fallback.aes_cbc_encrypt
def aes_cbc_decrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._fallback.aes_cbc_decrypt
