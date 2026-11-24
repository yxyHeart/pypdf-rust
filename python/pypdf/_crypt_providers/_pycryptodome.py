"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._crypt_providers._pycryptodome``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import secrets
from Crypto import __version__
from Crypto.Cipher import AES, ARC4
from Crypto.Util.Padding import pad, unpad
from pypdf._crypt_providers._base import CryptBase
from pypdf._utils import logger_warning
from pypdf.errors import PdfStreamError
crypt_provider = ()
class CryptRC4(CryptBase):
    """Stub for pypdf._crypt_providers._pycryptodome.CryptRC4."""
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
    """Stub for pypdf._crypt_providers._pycryptodome.CryptAES."""
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
    return None  # inert stub: pypdf._crypt_providers._pycryptodome.rc4_encrypt
def rc4_decrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._pycryptodome.rc4_decrypt
def aes_ecb_encrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._pycryptodome.aes_ecb_encrypt
def aes_ecb_decrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._pycryptodome.aes_ecb_decrypt
def aes_cbc_encrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._pycryptodome.aes_cbc_encrypt
def aes_cbc_decrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._pycryptodome.aes_cbc_decrypt
