// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use num_traits::PrimInt;
use std::collections::{vec_deque, VecDeque};
use std::fmt::Debug;

/// The key value cache such that:
/// 1. Key must be numeric
/// 2. Keys must be sequential
#[derive(Clone)]
pub struct SequentialKeyCache<K, V> {
    increment: K,
    /// The underlying data
    data: VecDeque<(K, V)>,
}

/// The result enum for sequential cache insertion
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SequentialAppendError {
    AboveBound,
    /// The key has already been inserted
    AlreadyInserted,
    BelowBound,
}

impl<K: PrimInt + Debug, V> Default for SequentialKeyCache<K, V> {
    fn default() -> Self {
        Self::sequential()
    }
}

impl<K: PrimInt + Debug, V> SequentialKeyCache<K, V> {
    pub fn new(increment: K) -> Self {
        Self {
            increment,
            data: Default::default(),
        }
    }

    /// Create a cache with key increment 1
    pub fn sequential() -> Self {
        Self {
            increment: K::one(),
            data: Default::default(),
        }
    }

    pub fn increment(&self) -> K {
        self.increment
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn upper_bound(&self) -> Option<K> {
        self.data.back().map(|v| v.0)
    }

    pub fn lower_bound(&self) -> Option<K> {
        self.data.front().map(|v| v.0)
    }

    fn within_bound(&self, k: K) -> bool {
        match (self.lower_bound(), self.upper_bound()) {
            (Some(lower), Some(upper)) => lower <= k && k <= upper,
            (None, None) => false,
            // other states are not reachable, even if there is one entry, both upper and
            // lower bounds should be the same, both should be Some.
            _ => unreachable!(),
        }
    }

    pub fn get_value(&self, key: K) -> Option<&V> {
        if !self.within_bound(key) {
            return None;
        }

        let lower = self.lower_bound().unwrap();
        // safe to unwrap as index must be uint
        let index = ((key - lower) / self.increment).to_usize().unwrap();

        self.data.get(index).map(|entry| &entry.1)
    }

    pub fn values_from(&self, start: K) -> ValueIter<K, V> {
        if self.is_empty() {
            return ValueIter {
                i: self.data.iter(),
            };
        }

        let lower = self.lower_bound().unwrap();
        // safe to unwrap as index must be uint
        let index = ((start.max(lower) - lower) / self.increment)
            .to_usize()
            .unwrap();

        ValueIter {
            i: self.data.range(index..),
        }
    }

    pub fn values_within(&self, start: K, end: K) -> ValueIter<K, V> {
        if self.is_empty() {
            return ValueIter {
                i: self.data.iter(),
            };
        }

        let lower = self.lower_bound().unwrap();
        let upper = self.upper_bound().unwrap();
        // safe to unwrap as index must be uint
        let end_idx = ((end.min(upper) - lower) / self.increment)
            .to_usize()
            .unwrap();
        let start_idx = ((start.max(lower) - lower) / self.increment)
            .to_usize()
            .unwrap();

        ValueIter {
            i: self.data.range(start_idx..=end_idx),
        }
    }

    pub fn values(&self) -> ValueIter<K, V> {
        ValueIter {
            i: self.data.iter(),
        }
    }

    /// Removes the all the keys below the target value, exclusive.
    pub fn remove_key_below(&mut self, key: K) {
        while let Some((k, _)) = self.data.front() {
            if *k < key {
                self.data.pop_front();
                continue;
            }
            break;
        }
    }

    /// Removes the all the keys above the target value, exclusive.
    pub fn remove_key_above(&mut self, key: K) {
        while let Some((k, _)) = self.data.back() {
            if *k > key {
                self.data.pop_back();
                continue;
            }
            break;
        }
    }

    /// Insert the key and value pair only if the key is upper_bound + 1
    pub fn append(&mut self, key: K, val: V) -> Result<(), SequentialAppendError> {
        let expected_next_key = if let Some(upper) = self.upper_bound() {
            upper.add(self.increment)
        } else {
            // no upper bound means no data yet, push back directly
            self.data.push_back((key, val));
            return Ok(());
        };

        if expected_next_key == key {
            self.data.push_back((key, val));
            return Ok(());
        }

        if expected_next_key < key {
            return Err(SequentialAppendError::AboveBound);
        }

        // safe to unwrap as we must have lower bound at this stage
        let lower = self.lower_bound().unwrap();
        if key < lower {
            Err(SequentialAppendError::BelowBound)
        } else {
            Err(SequentialAppendError::AlreadyInserted)
        }
    }
}

pub struct ValueIter<'a, K, V> {
    i: vec_deque::Iter<'a, (K, V)>,
}

impl<'a, K, V> Iterator for ValueIter<'a, K, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        self.i.next().map(|entry| &entry.1)
    }
}

#[cfg(test)]
mod tests {
    use crate::cache::SequentialKeyCache;

    #[test]
    fn insert_works() {
        let mut cache = SequentialKeyCache::new(1);

        for k in 9..100 {
            cache.append(k, k).unwrap();
        }

        for i in 9..100 {
            assert_eq!(cache.get_value(i), Some(&i));
        }

        assert_eq!(cache.get_value(100), None);
        assert_eq!(cache.lower_bound(), Some(9));
        assert_eq!(cache.upper_bound(), Some(99));
    }

    #[test]
    fn range_works() {
        let mut cache = SequentialKeyCache::new(1);

        for k in 1..100 {
            cache.append(k, k).unwrap();
        }

        let range = cache.values_from(50);
        assert_eq!(
            range.into_iter().cloned().collect::<Vec<_>>(),
            (50..100).collect::<Vec<_>>()
        );

        let range = cache.values_from(0);
        assert_eq!(
            range.into_iter().cloned().collect::<Vec<_>>(),
            (1..100).collect::<Vec<_>>()
        );

        let range = cache.values_within(50, 60);
        assert_eq!(
            range.into_iter().cloned().collect::<Vec<_>>(),
            (50..=60).collect::<Vec<_>>()
        );
        let range = cache.values_within(0, 1000);
        assert_eq!(
            range.into_iter().cloned().collect::<Vec<_>>(),
            (1..100).collect::<Vec<_>>()
        );

        let values = cache.values();
        assert_eq!(
            values.cloned().collect::<Vec<_>>(),
            (1..100).collect::<Vec<_>>()
        );
    }

    #[test]
    fn remove_works() {
        let mut cache = SequentialKeyCache::new(1);

        for k in 0..100 {
            cache.append(k, k).unwrap();
        }

        cache.remove_key_below(10);
        cache.remove_key_above(50);

        let values = cache.values();
        assert_eq!(
            values.into_iter().cloned().collect::<Vec<_>>(),
            (10..51).collect::<Vec<_>>()
        );
    }

    #[test]
    fn diff_increment_works() {
        let incre = 101;
        let mut cache = SequentialKeyCache::new(101);

        for k in 0..100 {
            cache.append(k * incre, k).unwrap();
        }

        let values = cache.values_from(incre + 1);
        assert_eq!(
            values.into_iter().cloned().collect::<Vec<_>>(),
            (1..100).collect::<Vec<_>>()
        );
    }
}
