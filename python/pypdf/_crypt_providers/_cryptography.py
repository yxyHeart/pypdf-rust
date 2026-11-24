"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._crypt_providers._cryptography``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import secrets
from cryptography import __version__
from cryptography.exceptions import UnsupportedAlgorithm
from cryptography.hazmat.decrepit.ciphers.algorithms import ARC4
from cryptography.hazmat.primitives.ciphers.algorithms import AES
from cryptography.hazmat.primitives.ciphers.base import Cipher
from cryptography.hazmat.primitives.ciphers.modes import CBC, ECB
from cryptography.hazmat.primitives.padding import PKCS7
from pypdf._crypt_providers._base import CryptBase
from pypdf._utils import logger_warning
from pypdf.errors import PdfStreamError
crypt_provider = ()
class CryptRC4(CryptBase):
    """Stub for pypdf._crypt_providers._cryptography.CryptRC4."""
    _is_rc4_supported = True
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    @staticmethod
    def _pure_python_rc4(*args, **kwargs):
        return None  # inert stub
    @classmethod
    def _disable_rc4(cls, *args, **kwargs):
        return None  # inert stub
    def encrypt(self, *args, **kwargs):
        return None  # inert stub
    def decrypt(self, *args, **kwargs):
        return None  # inert stub
class CryptAES(CryptBase):
    """Stub for pypdf._crypt_providers._cryptography.CryptAES."""
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
    return None  # inert stub: pypdf._crypt_providers._cryptography.rc4_encrypt
def rc4_decrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._cryptography.rc4_decrypt
def aes_ecb_encrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._cryptography.aes_ecb_encrypt
def aes_ecb_decrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._cryptography.aes_ecb_decrypt
def aes_cbc_encrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._cryptography.aes_cbc_encrypt
def aes_cbc_decrypt(*args, **kwargs):
    return None  # inert stub: pypdf._crypt_providers._cryptography.aes_cbc_decrypt
