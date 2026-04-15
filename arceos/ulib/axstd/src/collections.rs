//! Collection types for axstd.
//!
//! This module keeps all collections from `alloc::collections` and additionally
//! provides a local `HashMap` implementation that does not rely on external
//! hash table crates.

pub use alloc::collections::*;
/// Original collection types from `alloc::collections`.
///
/// This alias keeps the full upstream implementation available unchanged.
pub mod original {
    pub use alloc::collections::*;
}

use alloc::vec::Vec;
use arceos_api::modules::axhal;
use core::hash::{BuildHasher, Hash, Hasher};

#[derive(Debug)]
enum Bucket<K, V> {
    Empty,
    Occupied(K, V),
}

/// Default state for [`HashMap`], seeded by `axhal::misc::random()`.
#[derive(Clone)]
pub struct RandomState {
    seed0: u64,
    seed1: u64,
}

impl RandomState {
    /// Creates a randomized hash state.
    pub fn new() -> Self {
        let mut seed = axhal::misc::random();
        if seed == 0 {
            seed = 0x9e37_79b9_7f4a_7c15_d1b5_4a32_d192_ed03_u128;
        }
        Self {
            seed0: seed as u64,
            seed1: (seed >> 64) as u64,
        }
    }
}

impl Default for RandomState {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildHasher for RandomState {
    type Hasher = AxHasher;

    fn build_hasher(&self) -> Self::Hasher {
        AxHasher::new(self.seed0, self.seed1)
    }
}

/// A simple keyed hasher used by [`RandomState`].
pub struct AxHasher {
    state: u64,
    key: u64,
}

impl AxHasher {
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

    fn new(seed0: u64, seed1: u64) -> Self {
        Self {
            state: Self::OFFSET ^ seed0,
            key: seed1.rotate_left(17) ^ 0x517c_c1b7_2722_0a95,
        }
    }
}

impl Hasher for AxHasher {
    fn finish(&self) -> u64 {
        self.state ^ self.key.rotate_left(7)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state ^= (b as u64) ^ self.key;
            self.state = self.state.wrapping_mul(Self::PRIME);
            self.state ^= self.state >> 33;
        }
    }
}

/// A simple open-addressing hash map.
pub struct HashMap<K, V, S = RandomState> {
    buckets: Vec<Bucket<K, V>>,
    len: usize,
    hash_builder: S,
}

impl<K, V> HashMap<K, V, RandomState>
where
    K: Eq + Hash,
{
    pub fn new() -> Self {
        Self::with_hasher(RandomState::new())
    }
}

impl<K, V, S> Default for HashMap<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher + Clone + Default,
{
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}

impl<K, V, S> HashMap<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher + Clone,
{
    const INITIAL_CAPACITY: usize = 8;

    pub fn with_hasher(hash_builder: S) -> Self {
        Self {
            buckets: Self::empty_buckets(Self::INITIAL_CAPACITY),
            len: 0,
            hash_builder,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.ensure_capacity();
        self.insert_no_grow(key, value)
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            buckets: &self.buckets,
            index: 0,
        }
    }

    fn ensure_capacity(&mut self) {
        let cap = self.buckets.len();
        if self.len * 10 >= cap * 7 {
            self.rehash(cap * 2);
        }
    }

    fn rehash(&mut self, new_cap: usize) {
        let mut new_map = HashMap {
            buckets: Self::empty_buckets(new_cap.max(Self::INITIAL_CAPACITY).next_power_of_two()),
            len: 0,
            hash_builder: self.hash_builder.clone(),
        };

        for bucket in self.buckets.drain(..) {
            if let Bucket::Occupied(k, v) = bucket {
                let _ = new_map.insert_no_grow(k, v);
            }
        }

        *self = new_map;
    }

    fn insert_no_grow(&mut self, key: K, value: V) -> Option<V> {
        let hash = self.hash_key(&key);
        let mut idx = self.bucket_index(hash);

        loop {
            match &mut self.buckets[idx] {
                Bucket::Occupied(existing, v) if existing == &key => {
                    return Some(core::mem::replace(v, value))
                }
                Bucket::Occupied(_, _) => {
                    idx = (idx + 1) & (self.buckets.len() - 1);
                }
                Bucket::Empty => {
                    self.buckets[idx] = Bucket::Occupied(key, value);
                    self.len += 1;
                    return None;
                }
            }
        }
    }

    fn hash_key(&self, key: &K) -> u64 {
        let mut hasher = self.hash_builder.build_hasher();
        key.hash(&mut hasher);
        hasher.finish()
    }

    fn bucket_index(&self, hash: u64) -> usize {
        (hash as usize) & (self.buckets.len() - 1)
    }

    fn empty_buckets(cap: usize) -> Vec<Bucket<K, V>> {
        let mut buckets = Vec::with_capacity(cap);
        buckets.resize_with(cap, || Bucket::Empty);
        buckets
    }
}

pub struct Iter<'a, K, V> {
    buckets: &'a [Bucket<K, V>],
    index: usize,
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.buckets.len() {
            let idx = self.index;
            self.index += 1;
            if let Bucket::Occupied(k, v) = &self.buckets[idx] {
                return Some((k, v));
            }
        }
        None
    }
}
