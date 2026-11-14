//! Small, stable hashing helpers.
//!
//! The standard library's hasher is randomised per process, which is right for hash maps and wrong for anything
//! stored or compared across runs (control fingerprints) or anything that must be reproducible (typing rhythm).

/// FNV-1a, 64-bit. Stable across runs, platforms and versions.
#[derive(Clone, Copy, Debug)]
pub struct Fnv64(u64);

impl Default for Fnv64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Fnv64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    pub const fn new() -> Self {
        Self(Self::OFFSET)
    }

    pub fn write(&mut self, bytes: &[u8]) -> &mut Self {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
        self
    }

    /// Writes a string followed by a separator, so ("ab", "c") and ("a", "bc") hash differently.
    pub fn write_str(&mut self, s: &str) -> &mut Self {
        self.write(s.as_bytes()).write(&[0xff])
    }

    pub fn write_i64(&mut self, v: i64) -> &mut Self {
        self.write(&v.to_le_bytes())
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

/// SplitMix64 finaliser: turns a seed into a well-mixed 64-bit value.
pub fn mix64(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A deterministic number in [0, 1) derived from a seed.
pub fn unit(seed: u64) -> f64 {
    (mix64(seed) >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_is_stable_and_separates_fields() {
        let a = Fnv64::new().write_str("ab").write_str("c").finish();
        let b = Fnv64::new().write_str("a").write_str("bc").finish();
        assert_ne!(a, b);
        assert_eq!(Fnv64::new().write(b"").finish(), 0xcbf2_9ce4_8422_2325);
    }

    #[test]
    fn unit_is_in_range_and_deterministic() {
        for seed in 0..1000 {
            let u = unit(seed);
            assert!((0.0..1.0).contains(&u));
            assert_eq!(u, unit(seed));
        }
    }
}
