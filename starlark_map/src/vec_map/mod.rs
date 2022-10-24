/*
 * Copyright 2019 The Starlark in Rust Authors.
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

mod iter;

use std::hash::Hash;
use std::hash::Hasher;
use std::mem;

use gazebo::prelude::*;

use crate::equivalent::Equivalent;
use crate::hash_value::StarlarkHashValue;
use crate::hashed::Hashed;
pub use crate::vec_map::iter::IntoIter;
pub use crate::vec_map::iter::Iter;
pub use crate::vec_map::iter::IterMut;
use crate::vec_map::iter::VMIntoIterHash;
use crate::vec_map::iter::VMIterHash;
use crate::vec_map::iter::VMKeys;
use crate::vec_map::iter::VMValues;
use crate::vec_map::iter::VMValuesMut;

/// Bucket in [`VecMap`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct Bucket<K, V> {
    hash: StarlarkHashValue,
    key: K,
    value: V,
}

#[allow(clippy::derive_hash_xor_eq)]
impl<K: Hash, V: Hash> Hash for Bucket<K, V> {
    fn hash<S: Hasher>(&self, state: &mut S) {
        self.hash.hash(state);
        // Ignore the key, because `hash` is already the hash of the key,
        // although maybe not as good hash as what is requested.
        self.value.hash(state);
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Default_)]
pub struct VecMap<K, V> {
    buckets: Vec<Bucket<K, V>>,
}

impl<K, V> VecMap<K, V> {
    #[inline]
    pub const fn new() -> Self {
        VecMap {
            buckets: Vec::new(),
        }
    }

    #[inline]
    pub fn with_capacity(n: usize) -> Self {
        VecMap {
            buckets: Vec::with_capacity(n),
        }
    }

    pub(crate) fn reserve(&mut self, additional: usize) {
        self.buckets.reserve(additional);
    }

    #[inline]
    pub(crate) fn capacity(&self) -> usize {
        self.buckets.capacity()
    }

    pub(crate) fn extra_memory(&self) -> usize {
        self.buckets.capacity() * mem::size_of::<Bucket<K, V>>()
    }

    #[inline]
    pub(crate) fn get_full<Q>(&self, key: Hashed<&Q>) -> Option<(usize, &K, &V)>
    where
        Q: ?Sized + Equivalent<K>,
    {
        let mut i = 0;
        #[allow(clippy::explicit_counter_loop)] // we are paranoid about performance
        for b in &self.buckets {
            if b.hash == key.hash() && key.key().equivalent(&b.key) {
                return Some((i, &b.key, &b.value));
            }
            i += 1;
        }
        None
    }

    #[inline]
    pub(crate) fn get_index_of_hashed<Q>(&self, key: Hashed<&Q>) -> Option<usize>
    where
        Q: ?Sized + Equivalent<K>,
    {
        self.get_full(key).map(|(i, _, _)| i)
    }

    #[inline]
    pub(crate) fn get_index(&self, index: usize) -> Option<(&K, &V)> {
        self.buckets.get(index).map(|x| (&x.key, &x.value))
    }

    #[inline]
    pub(crate) unsafe fn get_unchecked(&self, index: usize) -> (Hashed<&K>, &V) {
        debug_assert!(index < self.buckets.len());
        let Bucket { hash, key, value } = self.buckets.get_unchecked(index);
        (Hashed::new_unchecked(*hash, key), value)
    }

    #[inline]
    pub(crate) unsafe fn get_unchecked_mut(&mut self, index: usize) -> (Hashed<&K>, &mut V) {
        debug_assert!(index < self.buckets.len());
        let Bucket { hash, key, value } = self.buckets.get_unchecked_mut(index);
        (Hashed::new_unchecked(*hash, key), value)
    }

    #[inline]
    pub fn insert_unique_unchecked(&mut self, key: Hashed<K>, value: V) {
        self.buckets.push(Bucket {
            hash: key.hash(),
            key: key.into_key(),
            value,
        });
    }

    pub(crate) fn remove_hashed_entry<Q>(&mut self, key: Hashed<&Q>) -> Option<(K, V)>
    where
        Q: ?Sized + Equivalent<K>,
    {
        let len = self.buckets.len();
        if len == 0 {
            return None;
        }

        for i in 0..len {
            if self.buckets[i].hash == key.hash() && key.key().equivalent(&self.buckets[i].key) {
                let b = self.buckets.remove(i);
                return Some((b.key, b.value));
            }
        }
        None
    }

    #[inline]
    pub(crate) fn remove(&mut self, index: usize) -> (Hashed<K>, V) {
        let Bucket { hash, key, value } = self.buckets.remove(index);
        (Hashed::new_unchecked(hash, key), value)
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Option<(Hashed<K>, V)> {
        let Bucket { hash, key, value } = self.buckets.pop()?;
        Some((Hashed::new_unchecked(hash, key), value))
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.buckets.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.buckets.clear();
    }

    #[inline]
    pub(crate) fn values(&self) -> VMValues<K, V> {
        VMValues {
            iter: self.buckets.iter(),
        }
    }

    #[inline]
    pub(crate) fn values_mut(&mut self) -> VMValuesMut<K, V> {
        VMValuesMut {
            iter: self.buckets.iter_mut(),
        }
    }

    #[inline]
    pub(crate) fn keys(&self) -> VMKeys<K, V> {
        VMKeys {
            iter: self.buckets.iter(),
        }
    }

    #[inline]
    pub(crate) fn into_iter(self) -> IntoIter<K, V> {
        IntoIter {
            iter: self.buckets.into_iter(),
        }
    }

    #[inline]
    pub(crate) fn iter(&self) -> Iter<K, V> {
        Iter {
            iter: self.buckets.iter(),
        }
    }

    #[inline]
    pub(crate) fn iter_hashed(&self) -> VMIterHash<K, V> {
        VMIterHash {
            // Values go first since they terminate first and we can short-circuit
            iter: self.buckets.iter(),
        }
    }

    #[inline]
    pub fn into_iter_hashed(self) -> VMIntoIterHash<K, V> {
        // See the comments on VMIntoIterHash for why this one looks different
        VMIntoIterHash {
            iter: self.buckets.into_iter(),
        }
    }

    #[inline]
    pub(crate) fn iter_mut(&mut self) -> IterMut<K, V> {
        IterMut {
            iter: self.buckets.iter_mut(),
        }
    }

    pub(crate) fn sort_keys(&mut self)
    where
        K: Ord,
    {
        self.buckets.sort_by(|a, b| a.key.cmp(&b.key));
    }

    /// Equal if entries are equal in the iterator order.
    pub(crate) fn eq_ordered(&self, other: &Self) -> bool
    where
        K: PartialEq,
        V: PartialEq,
    {
        self.buckets.eq(&other.buckets)
    }

    /// Hash entries in the iterator order.
    ///
    /// Note, keys are not hashed, but previously computed hashes are hashed instead.
    pub(crate) fn hash_ordered<H: Hasher>(&self, state: &mut H)
    where
        K: Hash,
        V: Hash,
    {
        self.buckets.hash(state);
    }
}
