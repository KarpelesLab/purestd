//! `std::collections`. The ordered containers come from `alloc`; `HashMap` and
//! `HashSet` are implemented here from scratch (open-addressing with linear
//! probing and tombstones) over [`crate::hash`], so no third-party crate is
//! needed.

pub use crate::alloc::collections::{
    btree_map, btree_set, BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque,
};

use crate::alloc::vec::Vec;
use crate::hash::{BuildHasher, Hash, Hasher, RandomState};
use core::borrow::Borrow;
use core::mem;

enum Slot<K, V> {
    Empty,
    Tombstone,
    Full(u64, K, V),
}

impl<K, V> Slot<K, V> {
    #[inline]
    fn is_empty(&self) -> bool {
        matches!(self, Slot::Empty)
    }
}

/// A hash map using open addressing with linear probing. Drop-in for
/// `std::collections::HashMap`.
pub struct HashMap<K, V, S = RandomState> {
    hash_builder: S,
    slots: Vec<Slot<K, V>>,
    len: usize,
    /// `len + tombstones`; growth is driven by this so probe chains stay short.
    used: usize,
}

#[inline]
fn hash_one<S: BuildHasher, Q: Hash + ?Sized>(builder: &S, key: &Q) -> u64 {
    let mut h = builder.build_hasher();
    key.hash(&mut h);
    h.finish()
}

impl<K, V> HashMap<K, V, RandomState> {
    pub fn new() -> HashMap<K, V, RandomState> {
        HashMap::with_hasher(RandomState::new())
    }
    pub fn with_capacity(cap: usize) -> HashMap<K, V, RandomState> {
        HashMap::with_capacity_and_hasher(cap, RandomState::new())
    }
}

impl<K, V, S> HashMap<K, V, S> {
    pub fn with_hasher(hash_builder: S) -> HashMap<K, V, S> {
        HashMap {
            hash_builder,
            slots: Vec::new(),
            len: 0,
            used: 0,
        }
    }
    pub fn with_capacity_and_hasher(cap: usize, hash_builder: S) -> HashMap<K, V, S> {
        let mut m = HashMap::with_hasher(hash_builder);
        if cap > 0 {
            m.grow_to(capacity_for(cap));
        }
        m
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn capacity(&self) -> usize {
        self.slots.len() / 8 * 7
    }
    pub fn clear(&mut self) {
        for s in &mut self.slots {
            *s = Slot::Empty;
        }
        self.len = 0;
        self.used = 0;
    }
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            slots: &self.slots,
            idx: 0,
        }
    }
    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        IterMut {
            slots: self.slots.iter_mut(),
        }
    }
    pub fn keys(&self) -> Keys<'_, K, V> {
        Keys { inner: self.iter() }
    }
    pub fn values(&self) -> Values<'_, K, V> {
        Values { inner: self.iter() }
    }
    pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
        ValuesMut {
            inner: self.iter_mut(),
        }
    }

    #[inline]
    fn cap(&self) -> usize {
        self.slots.len()
    }

    fn grow_to(&mut self, new_cap: usize) {
        let mut slots: Vec<Slot<K, V>> = Vec::with_capacity(new_cap);
        for _ in 0..new_cap {
            slots.push(Slot::Empty);
        }
        let old = mem::replace(&mut self.slots, slots);
        self.used = self.len;
        let mask = new_cap - 1;
        for slot in old {
            if let Slot::Full(h, k, v) = slot {
                let mut i = (h as usize) & mask;
                loop {
                    if self.slots[i].is_empty() {
                        self.slots[i] = Slot::Full(h, k, v);
                        break;
                    }
                    i = (i + 1) & mask;
                }
            }
        }
    }

    fn ensure_capacity(&mut self) {
        // Keep load factor (used/cap) below 7/8, and never let the table fill.
        if self.cap() == 0 {
            self.grow_to(8);
        } else if (self.used + 1) * 8 >= self.cap() * 7 {
            self.grow_to(self.cap() * 2);
        }
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> HashMap<K, V, S> {
    /// Probe for `key`. Returns the index of a matching `Full` slot if present,
    /// and the index where a new entry would be inserted.
    fn probe<Q>(&self, hash: u64, key: &Q) -> (Option<usize>, usize)
    where
        K: Borrow<Q>,
        Q: Eq + ?Sized,
    {
        let mask = self.cap() - 1;
        let mut i = (hash as usize) & mask;
        let mut insert: Option<usize> = None;
        loop {
            match &self.slots[i] {
                Slot::Empty => return (None, insert.unwrap_or(i)),
                Slot::Tombstone => {
                    if insert.is_none() {
                        insert = Some(i);
                    }
                }
                Slot::Full(h, k, _) => {
                    if *h == hash && k.borrow() == key {
                        return (Some(i), i);
                    }
                }
            }
            i = (i + 1) & mask;
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.ensure_capacity();
        let hash = hash_one(&self.hash_builder, &key);
        let (found, ins) = self.probe(hash, &key);
        match found {
            Some(i) => {
                if let Slot::Full(_, _, v) = &mut self.slots[i] {
                    Some(mem::replace(v, value))
                } else {
                    unreachable!()
                }
            }
            None => {
                // Filling an Empty slot grows `used`; reusing a Tombstone doesn't.
                if self.slots[ins].is_empty() {
                    self.used += 1;
                }
                self.slots[ins] = Slot::Full(hash, key, value);
                self.len += 1;
                None
            }
        }
    }

    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        if self.cap() == 0 {
            return None;
        }
        let hash = hash_one(&self.hash_builder, key);
        match self.probe(hash, key).0 {
            Some(i) => match &self.slots[i] {
                Slot::Full(_, _, v) => Some(v),
                _ => None,
            },
            None => None,
        }
    }

    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        if self.cap() == 0 {
            return None;
        }
        let hash = hash_one(&self.hash_builder, key);
        match self.probe(hash, key).0 {
            Some(i) => match &mut self.slots[i] {
                Slot::Full(_, _, v) => Some(v),
                _ => None,
            },
            None => None,
        }
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get(key).is_some()
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        if self.cap() == 0 {
            return None;
        }
        let hash = hash_one(&self.hash_builder, key);
        if let Some(i) = self.probe(hash, key).0 {
            let old = mem::replace(&mut self.slots[i], Slot::Tombstone);
            if let Slot::Full(_, _, v) = old {
                self.len -= 1;
                // `used` stays: a tombstone still occupies the slot for probing.
                return Some(v);
            }
        }
        None
    }

    pub fn entry(&mut self, key: K) -> Entry<'_, K, V, S> {
        self.ensure_capacity();
        let hash = hash_one(&self.hash_builder, &key);
        let (found, ins) = self.probe(hash, &key);
        match found {
            Some(i) => Entry::Occupied(OccupiedEntry { map: self, idx: i }),
            None => Entry::Vacant(VacantEntry {
                map: self,
                idx: ins,
                hash,
                key,
            }),
        }
    }
}

fn capacity_for(items: usize) -> usize {
    // Smallest power-of-two table that keeps `items` under the 7/8 load factor.
    let mut cap = 8;
    while items * 8 >= cap * 7 {
        cap *= 2;
    }
    cap
}

// ---- trait impls ----

impl<K: Hash + Eq, V, S: BuildHasher + Default> Default for HashMap<K, V, S> {
    fn default() -> Self {
        HashMap::with_hasher(S::default())
    }
}

impl<K, Q, V, S> core::ops::Index<&Q> for HashMap<K, V, S>
where
    K: Hash + Eq + Borrow<Q>,
    Q: Hash + Eq + ?Sized,
    S: BuildHasher,
{
    type Output = V;
    fn index(&self, key: &Q) -> &V {
        self.get(key).expect("no entry found for key")
    }
}

impl<K: Hash + Eq, V, S: BuildHasher + Default> FromIterator<(K, V)> for HashMap<K, V, S> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut m = HashMap::with_hasher(S::default());
        for (k, v) in iter {
            m.insert(k, v);
        }
        m
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> Extend<(K, V)> for HashMap<K, V, S> {
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

// ---- Entry API ----

/// A view into a single `HashMap` slot. Drop-in for `std::collections::hash_map::Entry`.
pub enum Entry<'a, K, V, S = RandomState> {
    Occupied(OccupiedEntry<'a, K, V, S>),
    Vacant(VacantEntry<'a, K, V, S>),
}

pub struct OccupiedEntry<'a, K, V, S = RandomState> {
    map: &'a mut HashMap<K, V, S>,
    idx: usize,
}

pub struct VacantEntry<'a, K, V, S = RandomState> {
    map: &'a mut HashMap<K, V, S>,
    idx: usize,
    hash: u64,
    key: K,
}

impl<'a, K, V, S> Entry<'a, K, V, S> {
    pub fn or_insert(self, default: V) -> &'a mut V {
        self.or_insert_with(|| default)
    }

    pub fn or_insert_with<F: FnOnce() -> V>(self, default: F) -> &'a mut V {
        match self {
            Entry::Occupied(e) => match &mut e.map.slots[e.idx] {
                Slot::Full(_, _, v) => v,
                _ => unreachable!(),
            },
            Entry::Vacant(e) => {
                if e.map.slots[e.idx].is_empty() {
                    e.map.used += 1;
                }
                e.map.slots[e.idx] = Slot::Full(e.hash, e.key, default());
                e.map.len += 1;
                match &mut e.map.slots[e.idx] {
                    Slot::Full(_, _, v) => v,
                    _ => unreachable!(),
                }
            }
        }
    }

    pub fn and_modify<F: FnOnce(&mut V)>(mut self, f: F) -> Self {
        if let Entry::Occupied(ref mut e) = self {
            if let Slot::Full(_, _, v) = &mut e.map.slots[e.idx] {
                f(v);
            }
        }
        self
    }
}

impl<'a, K, V: Default, S> Entry<'a, K, V, S> {
    pub fn or_default(self) -> &'a mut V {
        self.or_insert_with(V::default)
    }
}

// ---- iterators ----

pub struct Iter<'a, K, V> {
    slots: &'a [Slot<K, V>],
    idx: usize,
}
impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<(&'a K, &'a V)> {
        while self.idx < self.slots.len() {
            let i = self.idx;
            self.idx += 1;
            if let Slot::Full(_, k, v) = &self.slots[i] {
                return Some((k, v));
            }
        }
        None
    }
}

pub struct IterMut<'a, K, V> {
    slots: core::slice::IterMut<'a, Slot<K, V>>,
}
impl<'a, K, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);
    fn next(&mut self) -> Option<(&'a K, &'a mut V)> {
        for slot in self.slots.by_ref() {
            if let Slot::Full(_, k, v) = slot {
                return Some((&*k, v));
            }
        }
        None
    }
}

pub struct Keys<'a, K, V> {
    inner: Iter<'a, K, V>,
}
impl<'a, K, V> Iterator for Keys<'a, K, V> {
    type Item = &'a K;
    fn next(&mut self) -> Option<&'a K> {
        self.inner.next().map(|(k, _)| k)
    }
}

pub struct Values<'a, K, V> {
    inner: Iter<'a, K, V>,
}
impl<'a, K, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;
    fn next(&mut self) -> Option<&'a V> {
        self.inner.next().map(|(_, v)| v)
    }
}

pub struct ValuesMut<'a, K, V> {
    inner: IterMut<'a, K, V>,
}
impl<'a, K, V> Iterator for ValuesMut<'a, K, V> {
    type Item = &'a mut V;
    fn next(&mut self) -> Option<&'a mut V> {
        self.inner.next().map(|(_, v)| v)
    }
}

impl<'a, K, V, S> IntoIterator for &'a HashMap<K, V, S> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;
    fn into_iter(self) -> Iter<'a, K, V> {
        self.iter()
    }
}

// ---------------------------------------------------------------------------
// HashSet — a thin wrapper over HashMap<T, ()>.
// ---------------------------------------------------------------------------

/// A hash set. Drop-in for `std::collections::HashSet`.
pub struct HashSet<T, S = RandomState> {
    map: HashMap<T, (), S>,
}

impl<T: Hash + Eq> HashSet<T, RandomState> {
    pub fn new() -> HashSet<T, RandomState> {
        HashSet {
            map: HashMap::new(),
        }
    }
    pub fn with_capacity(cap: usize) -> HashSet<T, RandomState> {
        HashSet {
            map: HashMap::with_capacity(cap),
        }
    }
}

impl<T, S> HashSet<T, S> {
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    pub fn clear(&mut self) {
        self.map.clear()
    }
    pub fn iter(&self) -> Keys<'_, T, ()> {
        self.map.keys()
    }
}

impl<T: Hash + Eq, S: BuildHasher> HashSet<T, S> {
    pub fn insert(&mut self, value: T) -> bool {
        self.map.insert(value, ()).is_none()
    }
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.contains_key(value)
    }
    pub fn remove<Q>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.remove(value).is_some()
    }
}

impl<T: Hash + Eq, S: BuildHasher + Default> Default for HashSet<T, S> {
    fn default() -> Self {
        HashSet {
            map: HashMap::with_hasher(S::default()),
        }
    }
}

impl<T: Hash + Eq, S: BuildHasher + Default> FromIterator<T> for HashSet<T, S> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut s = HashSet {
            map: HashMap::with_hasher(S::default()),
        };
        for v in iter {
            s.insert(v);
        }
        s
    }
}

impl<T: Hash + Eq, S: BuildHasher> Extend<T> for HashSet<T, S> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for v in iter {
            self.insert(v);
        }
    }
}

/// `std::collections::hash_map` — submodule shape for drop-in imports.
pub mod hash_map {
    pub use super::{Entry, HashMap, Iter, IterMut, Keys, OccupiedEntry, VacantEntry, Values, ValuesMut};
    pub use crate::hash::{DefaultHasher, RandomState};
}

/// `std::collections::hash_set` — submodule shape for drop-in imports.
pub mod hash_set {
    pub use super::HashSet;
}

impl<K: Hash + Eq, V, S: BuildHasher> HashMap<K, V, S> {
    /// Retain only the entries for which `f(&k, &mut v)` returns true.
    pub fn retain<F: FnMut(&K, &mut V) -> bool>(&mut self, mut f: F) {
        for i in 0..self.slots.len() {
            let keep = match &mut self.slots[i] {
                Slot::Full(_, k, v) => f(k, v),
                _ => true,
            };
            if !keep {
                self.slots[i] = Slot::Tombstone;
                self.len -= 1;
            }
        }
    }
}

impl<T: Hash + Eq, S: BuildHasher> HashSet<T, S> {
    pub fn get<Q>(&self, value: &Q) -> Option<&T>
    where
        T: core::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        // The key is stored in the underlying map; surface it.
        if self.map.contains_key(value) {
            self.map.keys().find(|k| (*k).borrow() == value)
        } else {
            None
        }
    }
    pub fn is_disjoint(&self, other: &HashSet<T, S>) -> bool {
        self.iter().all(|v| !other.contains(v))
    }
    pub fn is_subset(&self, other: &HashSet<T, S>) -> bool {
        self.iter().all(|v| other.contains(v))
    }
    pub fn is_superset(&self, other: &HashSet<T, S>) -> bool {
        other.is_subset(self)
    }
    pub fn intersection<'a>(&'a self, other: &'a HashSet<T, S>) -> impl Iterator<Item = &'a T> {
        self.iter().filter(move |v| other.contains(*v))
    }
    pub fn difference<'a>(&'a self, other: &'a HashSet<T, S>) -> impl Iterator<Item = &'a T> {
        self.iter().filter(move |v| !other.contains(*v))
    }
    pub fn union<'a>(&'a self, other: &'a HashSet<T, S>) -> impl Iterator<Item = &'a T> {
        self.iter().chain(other.iter().filter(move |v| !self.contains(*v)))
    }
    pub fn retain<F: FnMut(&T) -> bool>(&mut self, mut f: F) {
        self.map.retain(|k, _| f(k));
    }
}

impl<'a, T, S> IntoIterator for &'a HashSet<T, S> {
    type Item = &'a T;
    type IntoIter = Keys<'a, T, ()>;
    fn into_iter(self) -> Keys<'a, T, ()> {
        self.iter()
    }
}
