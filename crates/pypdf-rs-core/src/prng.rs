//! Deterministic PRNG shared with the Python oracle.
//!
//! The reference implementation draws randomness from ``secrets.token_bytes``
//! and ``uuid.uuid4``.  For byte-level differential comparison both sides must
//! consume the *same* stream in the same order; this module is the Rust half of
//! that contract (see ``docs/FIDELITY_CONTRACT.md`` §3 and
//! ``tools/oracle/prng.py``).
//!
//! splitmix64, big-endian words:
//!
//! ```text
//! state += 0x9E3779B97F4A7C15
//! z = state
//! z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
//! z = (z ^ (z >> 27)) * 0x94D049BB133111EB
//! out = z ^ (z >> 31)
//! token_bytes(n) = ceil(n/8) big-endian words, truncated to n bytes
//! ```

pub const DEFAULT_SEED: u64 = 0x00C0_FFEE_5EED_1234;
const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
const MIX1: u64 = 0xBF58_476D_1CE4_E5B9;
const MIX2: u64 = 0x94D0_49BB_1331_11EB;

/// Deterministic splitmix64 stream.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN_GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(MIX1);
        z = (z ^ (z >> 27)).wrapping_mul(MIX2);
        z ^ (z >> 31)
    }

    /// Equivalent of ``secrets.token_bytes(n)`` under the deterministic patch.
    pub fn token_bytes(&mut self, n: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(n.div_ceil(8) * 8);
        while out.len() < n {
            out.extend_from_slice(&self.next_u64().to_be_bytes());
        }
        out.truncate(n);
        out
    }

    /// Equivalent of ``secrets.randbits(k)``.
    pub fn randbits(&mut self, k: u32) -> u128 {
        let nbytes = k.div_ceil(8) as usize;
        let bytes = self.token_bytes(nbytes);
        let mut value: u128 = 0;
        for byte in bytes {
            value = (value << 8) | u128::from(byte);
        }
        value >> (nbytes as u32 * 8 - k)
    }

    /// Equivalent of the deterministic ``uuid.uuid4()`` patch: 16 bytes with
    /// version 4 and RFC 4122 variant bits applied by ``uuid.UUID(bytes=...,
    /// version=4)``.
    pub fn uuid4_bytes(&mut self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&self.token_bytes(16));
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        bytes
    }

    /// Formatted like ``str(uuid.uuid4())``.
    pub fn uuid4_string(&mut self) -> String {
        let b = self.uuid4_bytes();
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-\
             {:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12],
            b[13], b[14], b[15]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_matches_reference_vectors() {
        // Generated with tools/oracle/prng.py (same constants, same algorithm).
        let mut rng = SplitMix64::new(42);
        assert_eq!(rng.next_u64(), 0xbdd7_3226_2feb_6e95);
        assert_eq!(rng.next_u64(), 0x28ef_e333_b266_f103);
        assert_eq!(rng.next_u64(), 0x4752_6757_130f_9f52);
    }

    #[test]
    fn token_bytes_is_truncated_big_endian() {
        let mut rng = SplitMix64::new(DEFAULT_SEED);
        let bytes = rng.token_bytes(16);
        assert_eq!(bytes.len(), 16);
        let mut rng2 = SplitMix64::new(DEFAULT_SEED);
        let word = rng2.next_u64();
        assert_eq!(&bytes[..8], &word.to_be_bytes());
    }
}
