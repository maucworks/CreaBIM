//! A deterministic 64-bit hasher (FNV-1a) for content-addressed caches.
//!
//! `std`'s `DefaultHasher` is randomly seeded per instance, so two
//! hashes of identical content never match across instances: any cache
//! keyed by one silently never hits (and, worse, an identity check
//! built on one always reports "changed"). Use this instead wherever
//! a key must survive the call that made it.
use std::hash::Hasher;

/// FNV-1a over whatever is written; fixed seeds, stable across runs.
#[derive(Clone, Debug, Default)]
pub struct StableHasher {
    hash: u64,
}

impl StableHasher {
    pub fn new() -> Self {
        Self {
            hash: 0xcbf29ce484222325,
        }
    }
}

impl Hasher for StableHasher {
    fn write(&mut self, bytes: &[u8]) {
        const PRIME: u64 = 0x100000001b3;
        for &b in bytes {
            self.hash ^= b as u64;
            self.hash = self.hash.wrapping_mul(PRIME);
        }
    }

    fn finish(&self) -> u64 {
        self.hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_input_same_key_across_instances() {
        let keyed = |s: &str| {
            let mut h = StableHasher::new();
            use std::hash::Hash;
            s.hash(&mut h);
            h.finish()
        };
        assert_eq!(keyed("walls"), keyed("walls"));
        assert_ne!(keyed("walls"), keyed("slabs"));
    }
}
