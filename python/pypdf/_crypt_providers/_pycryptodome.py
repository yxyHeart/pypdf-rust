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
#
# The module-level ``from Crypto import __version__`` is deliberately kept: it is
# what makes this provider unavailable (`ImportError`) when pycryptodome is not
# installed, exactly like upstream.  When it *is* installed, the implementation
# drives the same `Crypto` objects upstream drives (through PyO3), so ciphertext
# bytes and exceptions match.

import secrets  # noqa: F401  (upstream module global; Rust reads `secrets.token_bytes`)

from Crypto import __version__
from Crypto.Cipher import AES, ARC4
from Crypto.Util.Padding import pad, unpad

from pypdf._crypt_providers._base import CryptBase
from pypdf._pypdf import (
    cp_pycryptodome_aes_cbc_decrypt as _aes_cbc_decrypt,
    cp_pycryptodome_aes_cbc_encrypt as _aes_cbc_encrypt,
    cp_pycryptodome_aes_ecb_decrypt as _aes_ecb_decrypt,
    cp_pycryptodome_aes_ecb_encrypt as _aes_ecb_encrypt,
    cp_pycryptodome_cryptaes_decrypt as _cryptaes_decrypt,
    cp_pycryptodome_cryptaes_encrypt as _cryptaes_encrypt,
    cp_pycryptodome_cryptaes_init as _cryptaes_init,
    cp_pycryptodome_cryptrc4_decrypt as _cryptrc4_decrypt,
    cp_pycryptodome_cryptrc4_encrypt as _cryptrc4_encrypt,
    cp_pycryptodome_cryptrc4_init as _cryptrc4_init,
    cp_pycryptodome_rc4_decrypt as _rc4_decrypt,
    cp_pycryptodome_rc4_encrypt as _rc4_encrypt,
)
from pypdf._utils import logger_warning  # noqa: F401  (upstream module global)
from pypdf.errors import PdfStreamError  # noqa: F401  (upstream module global)

crypt_provider = ("pycryptodome", __version__)


class CryptRC4(CryptBase):
    def __init__(self, key: bytes) -> None:
        _cryptrc4_init(self, key)

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

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
