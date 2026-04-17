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
use alloc::string::String;

/// Hash a string with a seed
fn hash_str(s: &str, seed: u128) -> u128 {
    let mut hash = seed;

    for b in s.as_bytes() {
        hash ^= *b as u128;
        hash = hash.wrapping_mul(0x100000001b3);
    }

    hash
}


/// Entry in the hash map
enum Entry {
    Empty,
    Deleted, 
    Occupied(String, u32),
}


pub struct HashMap {
    buckets: Vec<Entry>,
    seed: u128,
    size: usize,
}

impl HashMap {
    /// Create a new hash map with the default capacity
    pub fn new() -> Self {
        Self::with_capacity(131_071)
    }

    pub fn with_capacity(cap: usize) -> Self {
        let seed = axhal::misc::random();

        let mut buckets = Vec::new();
        buckets.resize_with(cap, || Entry::Empty);

        Self {
            buckets,
            seed,
            size: 0,
        }
    }

    fn index(&self, key: &str) -> usize {
        (hash_str(key, self.seed) as usize) % self.buckets.len()
    }

    pub fn insert(&mut self, key: String, value: u32) {
        let mut idx = self.index(&key);
        let mut first_deleted = None;

        for _ in 0..self.buckets.len() {
            match &mut self.buckets[idx] {
                Entry::Empty => {
                    let insert_idx = first_deleted.unwrap_or(idx);
                    self.buckets[insert_idx] = Entry::Occupied(key, value);
                    self.size += 1;
                    return;
                }
                Entry::Deleted => {
                    if first_deleted.is_none() {
                        first_deleted = Some(idx);
                    }
                }
                Entry::Occupied(k, v) => {
                    if k == &key {
                        *v = value;
                        return;
                    }
                }
            }
            idx = (idx + 1) % self.buckets.len();
        }
    }

    pub fn get(&self, key: &str) -> Option<u32> {
        let mut idx = self.index(key);

        for _ in 0..self.buckets.len() {
            match &self.buckets[idx] {
                Entry::Empty => return None,
                Entry::Deleted => {}
                Entry::Occupied(k, v) => {
                    if k == key {
                        return Some(*v);
                    }
                }
            }
            idx = (idx + 1) % self.buckets.len();
        }

        None
    }

    pub fn remove(&mut self, key: &str) -> Option<u32> {
        let mut idx = self.index(key);

        for _ in 0..self.buckets.len() {
            match &self.buckets[idx] {
                Entry::Empty => return None,
                Entry::Deleted => {}
                Entry::Occupied(k, _) if k == key => {
                    let old = core::mem::replace(
                        &mut self.buckets[idx],
                        Entry::Deleted,
                    );

                    if let Entry::Occupied(_, v) = old {
                        self.size -= 1;
                        return Some(v);
                    }
                }
                _ => {}
            }
            idx = (idx + 1) % self.buckets.len();
        }

        None
    }

    pub fn iter(&self) -> Iter {
        Iter {
            buckets: &self.buckets,
            idx: 0,
        }
    }
}

pub struct Iter<'a> {
    buckets: &'a [Entry],
    idx: usize,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a String, &'a u32);

    fn next(&mut self) -> Option<Self::Item> {
        while self.idx < self.buckets.len() {
            if let Entry::Occupied(ref k, ref v) = self.buckets[self.idx] {
                self.idx += 1;
                return Some((k, v));
            }
            self.idx += 1;
        }
        None
    }
}