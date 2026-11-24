"""Deterministic PRNG shared by the Python oracle and the Rust implementation.

The reference implementation (pypdf 7.1.1) draws randomness from
``secrets.token_bytes`` in five places:

* ``pypdf/_writer.py``  -> ``PdfWriter.generate_file_identifiers`` (/ID entries)
* ``pypdf/_encryption.py`` -> AlgV5 salts (/Perms, /U, /O) and the file key

To compare writer output byte-for-byte we replace ``secrets.token_bytes`` with a
seeded splitmix64 stream on *both* sides.  The stream definition below is the
single source of truth and is mirrored in ``crates/pypdf-rs-core/src/prng.rs``.

Stream layout (big endian):

    state := seed
    next() -> state += 0x9E3779B97F4A7C15 (mod 2**64)
              z = state
              z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9 (mod 2**64)
              z = (z ^ (z >> 27)) * 0x94D049BB133111EB (mod 2**64)
              return z ^ (z >> 31)

    token_bytes(n) consumes ceil(n / 8) words, concatenates them big endian and
    returns the first n bytes.
"""

from __future__ import annotations

MASK = (1 << 64) - 1
GOLDEN_GAMMA = 0x9E3779B97F4A7C15
MIX1 = 0xBF58476D1CE4E5B9
MIX2 = 0x94D049BB133111EB

DEFAULT_SEED = 0xC0FFEE_5EED_1234


class SplitMix64:
    def __init__(self, seed: int = DEFAULT_SEED) -> None:
        self.state = seed & MASK

    def next_u64(self) -> int:
        self.state = (self.state + GOLDEN_GAMMA) & MASK
        z = self.state
        z = ((z ^ (z >> 30)) * MIX1) & MASK
        z = ((z ^ (z >> 27)) * MIX2) & MASK
        return (z ^ (z >> 31)) & MASK

    def token_bytes(self, n: int) -> bytes:
        out = bytearray()
        while len(out) < n:
            out += self.next_u64().to_bytes(8, "big")
        return bytes(out[:n])

    def randbits(self, k: int) -> int:
        nbytes = (k + 7) // 8
        value = int.from_bytes(self.token_bytes(nbytes), "big")
        return value >> (nbytes * 8 - k)


def install_deterministic_secrets(seed: int = DEFAULT_SEED):
    """Patch every randomness source pypdf uses so output becomes reproducible.

    pypdf draws entropy from two modules:

    * ``secrets.token_bytes`` -- /ID entries, encryption salts, file keys
    * ``uuid.uuid4()``        -- JavaScript action names, annotation /NM names

    Both share one SplitMix64 stream so that a single deterministic sequence is
    consumed in call order.  The Rust implementation mirrors this exactly (see
    ``crates/pypdf-rs-core/src/prng.rs``).

    Returns the ``SplitMix64`` instance that drives the stream.
    """
    import secrets
    import uuid

    rng = SplitMix64(seed)
    secrets.token_bytes = rng.token_bytes  # type: ignore[assignment]
    secrets.token_hex = lambda n=None: rng.token_bytes(  # type: ignore[assignment]
        32 if n is None else n
    ).hex()
    secrets.randbits = rng.randbits  # type: ignore[assignment]
    secrets.choice = lambda seq: seq[rng.randbits(32) % len(seq)]  # type: ignore[assignment]

    def deterministic_uuid4():
        return uuid.UUID(bytes=rng.token_bytes(16), version=4)

    uuid.uuid4 = deterministic_uuid4  # type: ignore[assignment]
    return rng
