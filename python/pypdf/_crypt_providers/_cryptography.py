# Copyright (c) 2023, exiledkingcc
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

# Rust-backed module: the classes are defined here exactly as upstream; every
# method body delegates to ``crates/pypdf-py/src/modules/crypt_providers_mod.rs``.
# The ``cryptography`` objects below are the ones the Rust logic drives through
# PyO3, so ciphertext bytes and exceptions are those of the installed package.

import secrets  # noqa: F401  (upstream module global; Rust reads `secrets.token_bytes`)

from cryptography import __version__
from cryptography.exceptions import UnsupportedAlgorithm
from cryptography.hazmat.decrepit.ciphers.algorithms import ARC4
from cryptography.hazmat.primitives.ciphers.algorithms import AES
from cryptography.hazmat.primitives.ciphers.base import Cipher
from cryptography.hazmat.primitives.ciphers.modes import CBC, ECB
from cryptography.hazmat.primitives.padding import PKCS7

from pypdf._crypt_providers._base import CryptBase
from pypdf._pypdf import (
    cp_cryptography_aes_cbc_decrypt as _aes_cbc_decrypt,
    cp_cryptography_aes_cbc_encrypt as _aes_cbc_encrypt,
    cp_cryptography_aes_ecb_decrypt as _aes_ecb_decrypt,
    cp_cryptography_aes_ecb_encrypt as _aes_ecb_encrypt,
    cp_cryptography_cryptaes_decrypt as _cryptaes_decrypt,
    cp_cryptography_cryptaes_encrypt as _cryptaes_encrypt,
    cp_cryptography_cryptaes_init as _cryptaes_init,
    cp_cryptography_cryptrc4_decrypt as _cryptrc4_decrypt,
    cp_cryptography_cryptrc4_disable_rc4 as _cryptrc4_disable_rc4,
    cp_cryptography_cryptrc4_encrypt as _cryptrc4_encrypt,
    cp_cryptography_cryptrc4_init as _cryptrc4_init,
    cp_cryptography_cryptrc4_pure_python_rc4 as _cryptrc4_pure_python_rc4,
    cp_cryptography_rc4_decrypt as _rc4_decrypt,
    cp_cryptography_rc4_encrypt as _rc4_encrypt,
)
from pypdf._utils import logger_warning  # noqa: F401  (upstream module global)
from pypdf.errors import PdfStreamError  # noqa: F401  (upstream module global)

crypt_provider = ("cryptography", __version__)


class CryptRC4(CryptBase):
    # Assume OpenSSL provides RC4; flipped to False the first time it rejects
    # the cipher (e.g. legacy provider disabled), so the failing attempt is
    # paid once.
    _is_rc4_supported = True

    def __init__(self, key: bytes) -> None:
        _cryptrc4_init(self, key)

    @staticmethod
    def _pure_python_rc4(key: bytes) -> CryptBase:
        return _cryptrc4_pure_python_rc4(key)

    @classmethod
    def _disable_rc4(cls, key: bytes) -> CryptBase:
        return _cryptrc4_disable_rc4(cls, key)

    def encrypt(self, data: bytes) -> bytes:
        return _cryptrc4_encrypt(self, data)

    def decrypt(self, data: bytes, *, strict: bool = True) -> bytes:
        return _cryptrc4_decrypt(self, data, strict=strict)


class CryptAES(CryptBase):
    def __init__(self, key: bytes) -> None:
        _cryptaes_init(self, key)

    def encrypt(self, data: bytes) -> bytes:
        return _cryptaes_encrypt(self, data)

    def decrypt(self, data: bytes, *, strict: bool = True) -> bytes:
        return _cryptaes_decrypt(self, data, strict=strict)


def rc4_encrypt(key: bytes, data: bytes) -> bytes:
    return _rc4_encrypt(key, data)


def rc4_decrypt(key: bytes, data: bytes) -> bytes:
    return _rc4_decrypt(key, data)


def aes_ecb_encrypt(key: bytes, data: bytes) -> bytes:
    return _aes_ecb_encrypt(key, data)


def aes_ecb_decrypt(key: bytes, data: bytes) -> bytes:
    return _aes_ecb_decrypt(key, data)


def aes_cbc_encrypt(key: bytes, iv: bytes, data: bytes) -> bytes:
    return _aes_cbc_encrypt(key, iv, data)


def aes_cbc_decrypt(key: bytes, iv: bytes, data: bytes) -> bytes:
    return _aes_cbc_decrypt(key, iv, data)
