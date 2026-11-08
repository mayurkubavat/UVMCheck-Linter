//! A fast, non-cryptographic hasher for internal maps whose keys no input
//! controls the distribution of (node ids, names, cache keys).

use std::hash::{BuildHasherDefault, Hasher};

/// The rustc "Fx" hash: a multiply and rotate per word.
#[derive(Default, Clone, Copy)]
pub struct FxHasher(u64);

const K: u64 = 0xf135_7aea_2e62_a9c5;

impl FxHasher {
    #[inline]
    fn add(&mut self, w: u64) {
        self.0 = (self.0.rotate_left(26) ^ w).wrapping_mul(K);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut b = bytes;
        while b.len() >= 8 {
            self.add(u64::from_le_bytes(b[..8].try_into().unwrap()));
            b = &b[8..];
        }
        if b.len() >= 4 {
            self.add(u32::from_le_bytes(b[..4].try_into().unwrap()) as u64);
            b = &b[4..];
        }
        for &x in b {
            self.add(x as u64);
        }
    }
    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }
}

/// A `HashMap` with [`FxHasher`].
pub type FxMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<FxHasher>>;
