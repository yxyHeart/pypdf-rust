"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._crypt_providers``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

from pypdf._crypt_providers._base import CryptBase, CryptIdentity
try:
    from pypdf._crypt_providers._cryptography import CryptAES, CryptRC4, aes_cbc_decrypt, aes_cbc_encrypt, aes_ecb_decrypt, aes_ecb_encrypt, crypt_provider, rc4_decrypt, rc4_encrypt
except ImportError:
    try:
        from pypdf._crypt_providers._pycryptodome import CryptAES, CryptRC4, aes_cbc_decrypt, aes_cbc_encrypt, aes_ecb_decrypt, aes_ecb_encrypt, crypt_provider, rc4_decrypt, rc4_encrypt
    except ImportError:
        from pypdf._crypt_providers._fallback import CryptAES, CryptRC4, aes_cbc_decrypt, aes_cbc_encrypt, aes_ecb_decrypt, aes_ecb_encrypt, crypt_provider, rc4_decrypt, rc4_encrypt
__all__ = []
