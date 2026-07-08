use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::iter::Sum;
use std::ops::{Add, Deref, Index, IndexMut};

pub mod my_visitor;
use imbl::shared_ptr::DefaultSharedPtr;
use imbl::Vector;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[macro_export]
macro_rules! inOMap {
    () => { $crate::in_order_map::InOMap::new() };

    ( $( $key:expr => $value:expr ),* ) => {{
        let mut map = $crate::in_order_map::InOMap::new();
        $({
            map.insert($key, $value);
        })*;
        map
    }};

    ( $( $key:expr => $value:expr ,)* ) => {{
        let mut map = $crate::in_order_map::InOMap::new();
        $({
            map.insert($key, $value);
        })*;
        map
    }};
}

#[derive(Clone)]
pub struct InOMap<K, V>
where
    K: Eq + Clone,
    V: Clone,
{
    value: Vector<(K, V)>,
}

impl<K, V> From<Vector<(K, V)>> for InOMap<K, V>
where
    K: Eq + Clone,
    V: Clone,
{
    fn from(value: Vector<(K, V)>) -> Self {
        Self { value }
    }
}

impl<K, V> From<InOMap<K, V>> for Vector<(K, V)>
where
    K: Eq + Clone,
    V: Clone,
{
    fn from(value: InOMap<K, V>) -> Self {
        value.value
    }
}

impl<K, V> PartialEq for InOMap<K, V>
where
    K: Eq + Clone,
    V: Eq + Clone,
{
    fn eq(&self, other: &Self) -> bool {
        self.value.ptr_eq(&other.value) || self.value == other.value || {
            self.value.len() == other.value.len() && {
                self.value.iter().all(|(k, v)| other.get(k) == Some(v))
            }
        }
    }
}

impl<K, V> Eq for InOMap<K, V>
where
    K: Eq + Clone,
    V: Eq + Clone,
{
}

impl<K, V> PartialOrd for InOMap<K, V>
where
    K: Eq + Ord + Clone,
    V: Ord + Clone,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<K, V> Ord for InOMap<K, V>
where
    K: Eq + Ord + Clone,
    V: Ord + Clone,
{
    fn cmp(&self, other: &Self) -> Ordering {
        let mut self_pairs: Vec<_> = self.value.iter().collect();
        let mut other_pairs: Vec<_> = other.value.iter().collect();
        self_pairs.sort();
        other_pairs.sort();
        self_pairs.cmp(&other_pairs)
    }
}

impl<K, V> Hash for InOMap<K, V>
where
    K: Eq + Hash + Clone,
    V: Hash + Clone,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.len().hash(state);
        // Order-independent hash: wrapping_add individual pair hashes
        let mut hash = 0u64;
        for (k, v) in self.value.iter() {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            k.hash(&mut hasher);
            v.hash(&mut hasher);
            hash = hash.wrapping_add(hasher.finish());
        }
        hash.hash(state);
    }
}

impl<K, V> Serialize for InOMap<K, V>
where
    K: Serialize + Eq + Clone,
    V: Serialize + Clone,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (k, v) in self {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

// This is the trait that informs Serde how to deserialize MyMap.
impl<'de, K, V> Deserialize<'de> for InOMap<K, V>
where
    K: Deserialize<'de> + Clone + Eq + Deref,
    V: Deserialize<'de> + Clone,
    <K as Deref>::Target: Eq,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Instantiate our Visitor and ask the Deserializer to drive
        // it over the input data, resulting in an instance of MyMap.
        deserializer.deserialize_map(my_visitor::MyVisitor::new())
    }
}

impl<K, V> InOMap<K, V>
where
    K: Eq + Clone,
    V: Clone,
{
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            value: Default::default(),
        }
    }
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.value.len()
    }
}
impl<K, V> InOMap<K, V>
where
    K: Eq + Clone,
    V: Clone,
{
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.value.ptr_eq(&other.value)
    }
}

impl<K, V> InOMap<K, V>
where
    K: Eq + Clone,
    V: Clone,
{
    #[inline]
    #[must_use]
    pub fn iter(&self) -> imbl::vector::Iter<'_, (K, V), DefaultSharedPtr> {
        self.value.iter()
    }
    #[inline]
    #[must_use]
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(key, _value)| key)
    }

    #[inline]
    #[must_use]
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_key, value)| value)
    }

    pub fn clear(&mut self) {
        self.value.clear();
    }
}

impl<K, V> InOMap<K, V>
where
    V: Clone,
    K: Eq + Clone,
{
    #[must_use]
    pub fn get<BK>(&self, key: &BK) -> Option<&V>
    where
        BK: Eq + ?Sized,
        K: Borrow<BK> + PartialEq<BK>,
    {
        let key = key.borrow();
        self.iter().find(|(k, _)| k == key).map(|x| &x.1)
    }
    #[must_use]
    pub fn get_key_value<BK>(&self, key: &BK) -> Option<(&K, &V)>
    where
        BK: Eq + ?Sized,
        K: Borrow<BK> + PartialEq<BK>,
    {
        self.iter().find(|(k, _)| k == key).map(|(k, v)| (k, v))
    }
    #[inline]
    #[must_use]
    pub fn contains_key<BK>(&self, k: &BK) -> bool
    where
        BK: Eq + ?Sized,
        K: Borrow<BK> + PartialEq<BK>,
    {
        self.get(&k).is_some()
    }
    #[must_use]
    pub fn is_submap_by<B, RM, F>(&self, other: RM, mut cmp: F) -> bool
    where
        B: Clone,
        F: FnMut(&V, &B) -> bool,
        RM: Borrow<InOMap<K, B>>,
    {
        self.value
            .iter()
            .all(|(k, v)| other.borrow().get(k).map(|ov| cmp(v, ov)).unwrap_or(false))
    }

    #[must_use]
    pub fn is_proper_submap_by<B, RM, F>(&self, other: RM, cmp: F) -> bool
    where
        B: Clone,
        F: FnMut(&V, &B) -> bool,
        RM: Borrow<InOMap<K, B>>,
    {
        self.value.len() != other.borrow().value.len() && self.is_submap_by(other, cmp)
    }

    #[inline]
    #[must_use]
    pub fn is_submap<RM>(&self, other: RM) -> bool
    where
        V: PartialEq,
        RM: Borrow<Self>,
    {
        self.is_submap_by(other.borrow(), PartialEq::eq)
    }

    #[inline]
    #[must_use]
    pub fn is_proper_submap<RM>(&self, other: RM) -> bool
    where
        V: PartialEq,
        RM: Borrow<Self>,
    {
        self.is_proper_submap_by(other.borrow(), PartialEq::eq)
    }
}

impl<K, V> InOMap<K, V>
where
    V: Clone,
    K: Eq + Clone,
{
    /// Get a mutable iterator over the values of a hash map.
    ///
    /// Please note that the order is consistent between maps using
    /// the same hasher, but no other ordering guarantee is offered.
    /// Items will not come out in insertion order or sort order.
    /// They will, however, come out in the same order every time for
    /// the same map.
    #[inline]
    #[must_use]
    pub fn iter_mut(&mut self) -> imbl::vector::IterMut<'_, (K, V), DefaultSharedPtr> {
        self.value.iter_mut()
    }

    /// Get a mutable reference to the value for a key from a hash
    /// map.
    ///
    /// Time: O(n)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let mut map = inOMap!{123 => "lol"};
    /// if let Some(value) = map.get_mut(&123) {
    ///     *value = "omg";
    /// }
    /// assert_eq!(
    ///   map.get(&123),
    ///   Some(&"omg")
    /// );
    /// ```
    #[must_use]
    pub fn get_mut<BK>(&mut self, key: &BK) -> Option<&mut V>
    where
        BK: Eq + ?Sized,
        K: Borrow<BK> + PartialEq<BK>,
    {
        self.value
            .iter_mut()
            .find(|(k, _)| k == key.borrow())
            .map(|(_, v)| v)
    }

    /// Insert a key/value mapping into a map.
    ///
    /// If the map already has a mapping for the given key, the
    /// previous value is overwritten.
    ///
    /// Time: O(n)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let mut map = inOMap!{};
    /// map.insert(123, "123");
    /// map.insert(456, "456");
    /// assert_eq!(
    ///   map,
    ///   inOMap!{123 => "123", 456 => "456"}
    /// );
    /// ```
    #[inline]
    pub fn insert(&mut self, key: K, v: V) -> Option<V> {
        let previous = self
            .value
            .iter()
            .enumerate()
            .find(|(_, (k, _))| k == &key)
            .map(|(index, _)| index)
            .map(|x| self.value.remove(x));
        self.value.push_back((key, v));
        previous.map(|(_, v)| v)
    }

    /// Remove a key/value pair from a map, if it exists, and return
    /// the removed value.
    ///
    /// This is a copy-on-write operation, so that the parts of the
    /// set's structure which are shared with other sets will be
    /// safely copied before mutating.
    ///
    /// Time: O(n)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let mut map = inOMap!{123 => "123", 456 => "456"};
    /// assert_eq!(Some("123"), map.remove(&123));
    /// assert_eq!(Some("456"), map.remove(&456));
    /// assert_eq!(None, map.remove(&789));
    /// assert!(map.is_empty());
    /// ```
    pub fn remove<BK>(&mut self, k: &BK) -> Option<V>
    where
        BK: Eq + ?Sized,
        K: Borrow<BK> + PartialEq<BK>,
    {
        self.value
            .iter()
            .enumerate()
            .find(|x| &x.1 .0 == &*k)
            .map(|x| x.0)
            .map(|x| self.value.remove(x))
            .map(|x| x.1)
    }

    /// Remove a key/value pair from a map, if it exists, and return
    /// the removed key and value.
    ///
    /// Time: O(n)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let mut map = inOMap!{123 => "123", 456 => "456"};
    /// assert_eq!(Some((123, "123")), map.remove_with_key(&123));
    /// assert_eq!(Some((456, "456")), map.remove_with_key(&456));
    /// assert_eq!(None, map.remove_with_key(&789));
    /// assert!(map.is_empty());
    /// ```
    pub fn remove_with_key<BK>(&mut self, k: &BK) -> Option<(K, V)>
    where
        BK: Eq + ?Sized,
        K: Borrow<BK> + PartialEq<BK>,
    {
        self.value
            .iter()
            .enumerate()
            .find(|x| &x.1 .0 == &*k)
            .map(|x| x.0)
            .map(|x| self.value.remove(x))
    }

    /// Get the [`Entry`][Entry] for a key in the map for in-place manipulation.
    ///
    /// Time: O(n)
    ///
    /// [Entry]: enum.Entry.html
    #[must_use]
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        let found_index = self
            .value
            .iter()
            .enumerate()
            .find(|x| &x.1 .0 == &key)
            .map(|x| x.0);

        if let Some(index) = found_index {
            Entry::Occupied(OccupiedEntry {
                map: self,
                key,
                index,
            })
        } else {
            Entry::Vacant(VacantEntry { map: self, key })
        }
    }

    /// Construct a new hash map by inserting a key/value mapping into a map.
    ///
    /// If the map already has a mapping for the given key, the previous value
    /// is overwritten.
    ///
    /// Time: O(n)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let map = inOMap!{};
    /// assert_eq!(
    ///   map.update(123, "123"),
    ///   inOMap!{123 => "123"}
    /// );
    /// ```
    #[inline]
    #[must_use]
    pub fn update(&self, k: K, v: V) -> Self {
        let mut out = self.clone();
        out.insert(k, v);
        out
    }

    /// Construct a new hash map by inserting a key/value mapping into
    /// a map.
    ///
    /// If the map already has a mapping for the given key, we call
    /// the provided function with the old value and the new value,
    /// and insert the result as the new value.
    ///
    /// Time: O(n)
    #[must_use]
    pub fn update_with<F>(&self, k: K, v: V, f: F) -> Self
    where
        F: FnOnce(V, V) -> V,
    {
        match self.extract_with_key(&k) {
            None => self.update(k, v),
            Some((_, v2, m)) => m.update(k, f(v2, v)),
        }
    }

    /// Construct a new map by inserting a key/value mapping into a
    /// map.
    ///
    /// If the map already has a mapping for the given key, we call
    /// the provided function with the key, the old value and the new
    /// value, and insert the result as the new value.
    ///
    /// Time: O(n)
    #[must_use]
    pub fn update_with_key<F>(&self, k: K, v: V, f: F) -> Self
    where
        F: FnOnce(&K, V, V) -> V,
    {
        match self.extract_with_key(&k) {
            None => self.update(k, v),
            Some((_, v2, m)) => {
                let out_v = f(&k, v2, v);
                m.update(k, out_v)
            }
        }
    }

    /// Construct a new map by inserting a key/value mapping into a
    /// map, returning the old value for the key as well as the new
    /// map.
    ///
    /// If the map already has a mapping for the given key, we call
    /// the provided function with the key, the old value and the new
    /// value, and insert the result as the new value.
    ///
    /// Time: O(n)
    #[must_use]
    pub fn update_lookup_with_key<F>(&self, k: K, v: V, f: F) -> (Option<V>, Self)
    where
        F: FnOnce(&K, &V, V) -> V,
    {
        match self.extract_with_key(&k) {
            None => (None, self.update(k, v)),
            Some((_, v2, m)) => {
                let out_v = f(&k, &v2, v);
                (Some(v2), m.update(k, out_v))
            }
        }
    }

    /// Update the value for a given key by calling a function with
    /// the current value and overwriting it with the function's
    /// return value.
    ///
    /// The function gets an [`Option<V>`][std::option::Option] and
    /// returns the same, so that it can decide to delete a mapping
    /// instead of updating the value, and decide what to do if the
    /// key isn't in the map.
    ///
    /// Time: O(n)
    ///
    /// [std::option::Option]: https://doc.rust-lang.org/std/option/enum.Option.html
    #[must_use]
    pub fn alter<F>(&self, f: F, k: K) -> Self
    where
        F: FnOnce(Option<V>) -> Option<V>,
    {
        let pop = self.extract_with_key(&k);
        match (f(pop.as_ref().map(|&(_, ref v, _)| v.clone())), pop) {
            (None, None) => self.clone(),
            (Some(v), None) => self.update(k, v),
            (None, Some((_, _, m))) => m,
            (Some(v), Some((_, _, m))) => m.update(k, v),
        }
    }

    /// Construct a new map without the given key.
    ///
    /// Construct a map that's a copy of the current map, absent the
    /// mapping for `key` if it's present.
    ///
    /// Time: O(n)
    #[must_use]
    pub fn without<BK>(&self, k: &BK) -> Self
    where
        BK: Eq + ?Sized,
        K: Borrow<BK> + PartialEq<BK>,
    {
        match self.extract_with_key(k) {
            None => self.clone(),
            Some((_, _, map)) => map,
        }
    }

    /// Filter out values from a map which don't satisfy a predicate.
    ///
    /// This is slightly more efficient than filtering using an
    /// iterator, in that it doesn't need to rehash the retained
    /// values, but it still needs to reconstruct the entire tree
    /// structure of the map.
    ///
    /// Time: O(n ^ 2)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value;
    /// # use imbl_value::inOMap;
    /// let mut map = inOMap!{1 => 1, 2 => 2, 3 => 3};
    /// map.retain(|k, v| *k > 1);
    /// let expected = inOMap!{2 => 2, 3 => 3};
    /// assert_eq!(expected, map);
    /// ```
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&K, &V) -> bool,
    {
        self.value.retain(|(k, v)| f(k, v));
    }

    /// Remove a key/value pair from a map, if it exists, and return
    /// the removed value as well as the updated map.
    ///
    /// Time: O(n)
    #[must_use]
    pub fn extract<BK>(&self, k: &BK) -> Option<(V, Self)>
    where
        BK: Eq + ?Sized,
        K: Borrow<BK> + PartialEq<BK>,
    {
        self.extract_with_key(k).map(|(_, v, m)| (v, m))
    }

    /// Remove a key/value pair from a map, if it exists, and return
    /// the removed key and value as well as the updated list.
    ///
    /// Time: O(n)
    #[must_use]
    pub fn extract_with_key<BK>(&self, k: &BK) -> Option<(K, V, Self)>
    where
        BK: Eq + ?Sized,
        K: Borrow<BK> + PartialEq<BK>,
    {
        let mut out = self.clone();
        out.remove_with_key(k).map(|(k, v)| (k, v, out))
    }

    /// Construct the union of two maps, keeping the values in the
    /// current map when keys exist in both maps.
    ///
    /// Time: O(n ^ 2)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let map1 = inOMap!{1 => 1, 3 => 3};
    /// let map2 = inOMap!{2 => 2, 3 => 4};
    /// let expected = inOMap!{ 2 => 2, 3 => 3, 1 => 1,};
    /// assert_eq!(expected, map1.union(map2));
    /// ```
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let (mut to_mutate, to_consume, use_to_consume) = if self.len() >= other.len() {
            (self, other, false)
        } else {
            (other, self, true)
        };
        for (k, v) in to_consume.value.into_iter().rev() {
            match to_mutate.entry(k) {
                Entry::Occupied(mut e) if use_to_consume => {
                    e.insert(v);
                }
                Entry::Vacant(e) => {
                    e.insert(v);
                }
                _ => {}
            }
        }
        to_mutate.value = to_mutate.value.clone().into_iter().rev().collect();
        to_mutate
    }

    /// Construct the union of two maps, using a function to decide
    /// what to do with the value when a key is in both maps.
    ///
    /// The function is called when a value exists in both maps, and
    /// receives the value from the current map as its first argument,
    /// and the value from the other map as the second. It should
    /// return the value to be inserted in the resulting map.
    ///
    /// Time: O(n ^ 2)
    #[inline]
    #[must_use]
    pub fn union_with<F>(self, other: Self, mut f: F) -> Self
    where
        F: FnMut(V, V) -> V,
    {
        self.union_with_key(other, |_, v1, v2| f(v1, v2))
    }

    /// Construct the union of two maps, using a function to decide
    /// what to do with the value when a key is in both maps.
    ///
    /// The function is called when a value exists in both maps, and
    /// receives a reference to the key as its first argument, the
    /// value from the current map as the second argument, and the
    /// value from the other map as the third argument. It should
    /// return the value to be inserted in the resulting map.
    ///
    /// Time: O(n ^ 2)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let map1 = inOMap!{1 => 1, 3 => 4};
    /// let map2 = inOMap!{2 => 2, 3 => 5};
    /// let expected = inOMap!{1 => 1, 2 => 2, 3 => 9};
    /// assert_eq!(expected, map1.union_with_key(
    ///     map2,
    ///     |key, left, right| left + right
    /// ));
    /// ```
    #[must_use]
    pub fn union_with_key<F>(self, other: Self, mut f: F) -> Self
    where
        F: FnMut(&K, V, V) -> V,
    {
        if self.len() >= other.len() {
            self.union_with_key_inner(other, f)
        } else {
            other.union_with_key_inner(self, |key, other_value, self_value| {
                f(key, self_value, other_value)
            })
        }
    }

    fn union_with_key_inner<F>(mut self, other: Self, mut f: F) -> Self
    where
        F: FnMut(&K, V, V) -> V,
    {
        for (key, right_value) in other {
            match self.remove(&key) {
                None => {
                    self.insert(key, right_value);
                }
                Some(left_value) => {
                    let final_value = f(&key, left_value, right_value);
                    self.insert(key, final_value);
                }
            }
        }
        self
    }

    /// Construct the union of a sequence of maps, selecting the value
    /// of the leftmost when a key appears in more than one map.
    ///
    /// Time: O(n ^ 2)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let map1 = inOMap!{1 => 1, 3 => 3};
    /// let map2 = inOMap!{2 => 2};
    /// let expected = inOMap!{2 => 2, 1 => 1, 3 => 3};
    /// assert_eq!(expected, InOMap::unions(vec![map1, map2]));
    /// ```
    #[must_use]
    pub fn unions<I>(i: I) -> Self
    where
        I: IntoIterator<Item = Self>,
    {
        i.into_iter().fold(Self::default(), Self::union)
    }

    /// Construct the union of a sequence of maps, using a function to
    /// decide what to do with the value when a key is in more than
    /// one map.
    ///
    /// The function is called when a value exists in multiple maps,
    /// and receives the value from the current map as its first
    /// argument, and the value from the next map as the second. It
    /// should return the value to be inserted in the resulting map.
    ///
    /// Time: O(n ^ 2)
    #[must_use]
    pub fn unions_with<I, F>(i: I, f: F) -> Self
    where
        I: IntoIterator<Item = Self>,
        F: Fn(V, V) -> V,
    {
        i.into_iter()
            .fold(Self::default(), |a, b| a.union_with(b, &f))
    }

    /// Construct the union of a sequence of maps, using a function to
    /// decide what to do with the value when a key is in more than
    /// one map.
    ///
    /// The function is called when a value exists in multiple maps,
    /// and receives a reference to the key as its first argument, the
    /// value from the current map as the second argument, and the
    /// value from the next map as the third argument. It should
    /// return the value to be inserted in the resulting map.
    ///
    /// Time: O(n ^ 2)
    #[must_use]
    pub fn unions_with_key<I, F>(i: I, f: F) -> Self
    where
        I: IntoIterator<Item = Self>,
        F: Fn(&K, V, V) -> V,
    {
        i.into_iter()
            .fold(Self::default(), |a, b| a.union_with_key(b, &f))
    }

    /// Construct the symmetric difference between two maps by discarding keys
    /// which occur in both maps.
    ///
    /// This is an alias for the
    /// [`symmetric_difference`][symmetric_difference] method.
    ///
    /// Time: O(n ^ 2)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let map1 = inOMap!{1 => 1, 3 => 4};
    /// let map2 = inOMap!{2 => 2, 3 => 5};
    /// let expected = inOMap!{1 => 1, 2 => 2};
    /// assert_eq!(expected, map1.difference(map2));
    /// ```
    ///
    /// [symmetric_difference]: #method.symmetric_difference
    #[deprecated(
        since = "2.0.1",
        note = "to avoid conflicting behaviors between std and imbl, the `difference` alias for `symmetric_difference` will be removed."
    )]
    #[inline]
    #[must_use]
    pub fn difference(self, other: Self) -> Self {
        self.symmetric_difference(other)
    }

    /// Construct the symmetric difference between two maps by discarding keys
    /// which occur in both maps.
    ///
    /// Time: O(n ^ 2)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let map1 = inOMap!{1 => 1, 3 => 4};
    /// let map2 = inOMap!{2 => 2, 3 => 5};
    /// let expected = inOMap!{1 => 1, 2 => 2};
    /// assert_eq!(expected, map1.symmetric_difference(map2));
    /// ```
    #[inline]
    #[must_use]
    pub fn symmetric_difference(self, other: Self) -> Self {
        self.symmetric_difference_with_key(other, |_, _, _| None)
    }

    /// Construct the symmetric difference between two maps by using a function
    /// to decide what to do if a key occurs in both.
    ///
    /// This is an alias for the
    /// [`symmetric_difference_with`][symmetric_difference_with] method.
    ///
    /// Time: O(n ^ 2)
    ///
    /// [symmetric_difference_with]: #method.symmetric_difference_with
    #[deprecated(
        since = "2.0.1",
        note = "to avoid conflicting behaviors between std and imbl, the `difference_with` alias for `symmetric_difference_with` will be removed."
    )]
    #[inline]
    #[must_use]
    pub fn difference_with<F>(self, other: Self, f: F) -> Self
    where
        F: FnMut(V, V) -> Option<V>,
    {
        self.symmetric_difference_with(other, f)
    }

    /// Construct the symmetric difference between two maps by using a function
    /// to decide what to do if a key occurs in both.
    ///
    /// Time: O(n ^ 2)
    #[inline]
    #[must_use]
    pub fn symmetric_difference_with<F>(self, other: Self, mut f: F) -> Self
    where
        F: FnMut(V, V) -> Option<V>,
    {
        self.symmetric_difference_with_key(other, |_, a, b| f(a, b))
    }

    /// Construct the symmetric difference between two maps by using a function
    /// to decide what to do if a key occurs in both. The function
    /// receives the key as well as both values.
    ///
    /// This is an alias for the
    /// [`symmetric_difference_with`_key][symmetric_difference_with_key]
    /// method.
    ///
    /// Time: O(n ^ 2)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let map1 = inOMap!{1 => 1, 3 => 4};
    /// let map2 = inOMap!{2 => 2, 3 => 5};
    /// let expected = inOMap!{1 => 1, 3 => 9,  2 => 2,};
    /// assert_eq!(expected, map1.difference_with_key(
    ///     map2,
    ///     |key, left, right| Some(left + right)
    /// ));
    /// ```
    ///
    /// [symmetric_difference_with_key]: #method.symmetric_difference_with_key
    #[deprecated(
        since = "2.0.1",
        note = "to avoid conflicting behaviors between std and imbl, the `difference_with_key` alias for `symmetric_difference_with_key` will be removed."
    )]
    #[must_use]
    pub fn difference_with_key<F>(self, other: Self, f: F) -> Self
    where
        F: FnMut(&K, V, V) -> Option<V>,
    {
        self.symmetric_difference_with_key(other, f)
    }

    /// Construct the symmetric difference between two maps by using a function
    /// to decide what to do if a key occurs in both. The function
    /// receives the key as well as both values.
    ///
    /// Time: O(n ^ 2)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let map1 = inOMap!{1 => 1, 3 => 4};
    /// let map2 = inOMap!{2 => 2, 3 => 5};
    /// let expected = inOMap!{1 => 1, 3 => 9,  2 => 2,};
    /// assert_eq!(expected, map1.symmetric_difference_with_key(
    ///     map2,
    ///     |key, left, right| Some(left + right)
    /// ));
    /// ```
    #[must_use]
    pub fn symmetric_difference_with_key<F>(mut self, other: Self, mut f: F) -> Self
    where
        F: FnMut(&K, V, V) -> Option<V>,
    {
        let mut out = InOMap::default();
        for (key, right_value) in other {
            match self.remove(&key) {
                None => {
                    out.insert(key, right_value);
                }
                Some(left_value) => {
                    if let Some(final_value) = f(&key, left_value, right_value) {
                        out.insert(key, final_value);
                    }
                }
            }
        }
        out.union(self)
    }

    /// Construct the relative complement between two maps by discarding keys
    /// which occur in `other`.
    ///
    /// Time: O(m * n) where m is the size of the other map
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl::ordmap::OrdMap;
    /// let map1 = ordmap!{1 => 1, 3 => 4};
    /// let map2 = ordmap!{2 => 2, 3 => 5};
    /// let expected = ordmap!{1 => 1};
    /// assert_eq!(expected, map1.relative_complement(map2));
    /// ```
    #[inline]
    #[must_use]
    pub fn relative_complement(mut self, other: Self) -> Self {
        for (key, _) in other {
            let _ = self.remove(&key);
        }
        self
    }

    /// Construct the intersection of two maps, keeping the values
    /// from the current map.
    ///
    /// Time: O(n ^ 2)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let map1 = inOMap!{1 => 1, 2 => 2};
    /// let map2 = inOMap!{2 => 3, 3 => 4};
    /// let expected = inOMap!{2 => 2};
    /// assert_eq!(expected, map1.intersection(map2));
    /// ```
    #[inline]
    #[must_use]
    pub fn intersection(self, other: Self) -> Self {
        self.intersection_with_key(other, |_, v, _| v)
    }

    /// Construct the intersection of two maps, calling a function
    /// with both values for each key and using the result as the
    /// value for the key.
    ///
    /// Time: O(n ^ 2)
    #[inline]
    #[must_use]
    pub fn intersection_with<B, C, F>(self, other: InOMap<K, B>, mut f: F) -> InOMap<K, C>
    where
        B: Clone,
        C: Clone,
        F: FnMut(V, B) -> C,
    {
        self.intersection_with_key(other, |_, v1, v2| f(v1, v2))
    }

    /// Construct the intersection of two maps, calling a function
    /// with the key and both values for each key and using the result
    /// as the value for the key.
    ///
    /// Time: O(n ^ 2)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate imbl;
    /// # use imbl_value::InOMap;
    /// # use imbl_value::inOMap;
    /// let map1 = inOMap!{1 => 1, 2 => 2};
    /// let map2 = inOMap!{2 => 3, 3 => 4};
    /// let expected = inOMap!{2 => 5};
    /// assert_eq!(expected, map1.intersection_with_key(
    ///     map2,
    ///     |key, left, right| left + right
    /// ));
    /// ```
    #[must_use]
    pub fn intersection_with_key<B, C, F>(mut self, other: InOMap<K, B>, mut f: F) -> InOMap<K, C>
    where
        B: Clone,
        C: Clone,
        F: FnMut(&K, V, B) -> C,
    {
        let mut out = InOMap::default();
        for (key, right_value) in other {
            match self.remove(&key) {
                None => (),
                Some(left_value) => {
                    let result = f(&key, left_value, right_value);
                    out.insert(key, result);
                }
            }
        }
        out
    }
}

// Entries

/// A handle for a key and its associated value.
///
/// ## Performance Note
///
/// When using an `Entry`, the key is only ever hashed once, when you
/// create the `Entry`. Operations on an `Entry` will never trigger a
/// rehash, where eg. a `contains_key(key)` followed by an
/// `insert(key, default_value)` (the equivalent of
/// `Entry::or_insert()`) would need to hash the key once for the
/// `contains_key` and again for the `insert`. The operations
/// generally perform similarly otherwise.
pub enum Entry<'a, K, V>
where
    V: Clone,
    K: Eq + Clone,
{
    /// An entry which exists in the map.
    Occupied(OccupiedEntry<'a, K, V>),
    /// An entry which doesn't exist in the map.
    Vacant(VacantEntry<'a, K, V>),
}

impl<'a, K, V> Entry<'a, K, V>
where
    V: 'a + Clone,
    K: 'a + Eq + Clone,
{
    /// Insert the default value provided if there was no value
    /// already, and return a mutable reference to the value.
    pub fn or_insert(self, default: V) -> &'a mut V {
        self.or_insert_with(|| default)
    }

    /// Insert the default value from the provided function if there
    /// was no value already, and return a mutable reference to the
    /// value.
    pub fn or_insert_with<F>(self, default: F) -> &'a mut V
    where
        F: FnOnce() -> V,
    {
        match self {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(default()),
        }
    }

    /// Insert a default value if there was no value already, and
    /// return a mutable reference to the value.
    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        self.or_insert_with(Default::default)
    }

    /// Get the key for this entry.
    #[must_use]
    pub fn key(&self) -> &K {
        match self {
            Entry::Occupied(entry) => entry.key(),
            Entry::Vacant(entry) => entry.key(),
        }
    }

    /// Call the provided function to modify the value if the value
    /// exists.
    #[must_use]
    pub fn and_modify<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut V),
    {
        match &mut self {
            Entry::Occupied(ref mut entry) => f(entry.get_mut()),
            Entry::Vacant(_) => (),
        }
        self
    }
}

/// An entry for a mapping that already exists in the map.
pub struct OccupiedEntry<'a, K, V>
where
    V: Clone,
    K: Eq + Clone,
{
    map: &'a mut InOMap<K, V>,
    index: usize,
    key: K,
}

impl<'a, K, V> OccupiedEntry<'a, K, V>
where
    K: 'a + Eq + Clone,
    V: 'a + Clone,
{
    /// Get the key for this entry.
    #[must_use]
    pub fn key(&self) -> &K {
        &self.key
    }

    /// Remove this entry from the map and return the removed mapping.
    pub fn remove_entry(self) -> (K, V) {
        self.map.remove_with_key(&self.key).unwrap()
    }

    /// Get the current value.
    #[must_use]
    pub fn get(&self) -> &V {
        &self.map.value.get(self.index).unwrap().1
    }

    /// Get a mutable reference to the current value.
    #[must_use]
    pub fn get_mut(&mut self) -> &mut V {
        &mut self.map.value.get_mut(self.index).unwrap().1
    }

    /// Convert this entry into a mutable reference.
    #[must_use]
    pub fn into_mut(self) -> &'a mut V {
        &mut self.map.value.get_mut(self.index).unwrap().1
    }

    /// Overwrite the current value.
    pub fn insert(&mut self, mut value: V) -> V {
        ::std::mem::swap(
            &mut self.map.value.get_mut(self.index).unwrap().1,
            &mut value,
        );
        value
    }

    /// Remove this entry from the map and return the removed value.
    pub fn remove(self) -> V {
        self.remove_entry().1
    }
}

/// An entry for a mapping that does not already exist in the map.
pub struct VacantEntry<'a, K, V>
where
    V: Clone,
    K: Eq + Clone,
{
    map: &'a mut InOMap<K, V>,
    key: K,
}

impl<'a, K, V> VacantEntry<'a, K, V>
where
    K: 'a + Eq + Clone,
    V: 'a + Clone,
{
    /// Get the key for this entry.
    #[must_use]
    pub fn key(&self) -> &K {
        &self.key
    }

    /// Convert this entry into its key.
    #[must_use]
    pub fn into_key(self) -> K {
        self.key
    }

    /// Insert a value into this entry.
    pub fn insert(self, value: V) -> &'a mut V {
        self.map.insert(self.key.clone(), value);
        self.map.get_mut(&self.key).unwrap()
    }
}

impl<K, V> Add for InOMap<K, V>
where
    V: Clone,
    K: Eq + Clone,
{
    type Output = InOMap<K, V>;

    fn add(self, other: Self) -> Self::Output {
        self.union(other)
    }
}

impl<'a, K, V> Add for &'a InOMap<K, V>
where
    V: Clone,
    K: Eq + Clone,
{
    type Output = InOMap<K, V>;

    fn add(self, other: Self) -> Self::Output {
        self.clone().union(other.clone())
    }
}

impl<K, V> Sum for InOMap<K, V>
where
    V: Clone,
    K: Eq + Clone,
{
    fn sum<I>(it: I) -> Self
    where
        I: Iterator<Item = Self>,
    {
        it.fold(Self::default(), |a, b| a + b)
    }
}

impl<K, V, RK, RV> Extend<(RK, RV)> for InOMap<K, V>
where
    V: Clone + From<RV>,
    K: Eq + Clone + From<RK>,
{
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (RK, RV)>,
    {
        for (key, value) in iter {
            self.insert(From::from(key), From::from(value));
        }
    }
}

impl<'a, BK, K, V> Index<&'a BK> for InOMap<K, V>
where
    V: Clone,
    BK: Eq + ?Sized,
    K: Eq + Clone + Borrow<BK> + PartialEq<BK>,
{
    type Output = V;

    fn index(&self, key: &BK) -> &Self::Output {
        match self.get::<BK>(key) {
            None => panic!("InOMap::index: invalid key"),
            Some(v) => v,
        }
    }
}

impl<'a, BK, K, V> IndexMut<&'a BK> for InOMap<K, V>
where
    BK: Eq + ?Sized,
    K: Eq + Clone + Borrow<BK> + PartialEq<BK>,
    V: Clone,
{
    fn index_mut(&mut self, key: &BK) -> &mut Self::Output {
        match self.get_mut::<BK>(key) {
            None => panic!("InOMap::index_mut: invalid key"),
            Some(&mut ref mut value) => value,
        }
    }
}

impl<K, V> Debug for InOMap<K, V>
where
    V: Clone,
    K: Eq + Debug + Clone,
    V: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), ::std::fmt::Error> {
        let mut d = f.debug_map();
        for (k, v) in self {
            d.entry(k, v);
        }
        d.finish()
    }
}

/// An iterator over the elements of a map.
pub struct Iter<'a, K, V>
where
    K: Clone,
    V: Clone,
{
    it: imbl::vector::Iter<'a, (K, V), DefaultSharedPtr>,
}

// We impl Clone instead of deriving it, because we want Clone even if K and V aren't.
impl<'a, K, V> Clone for Iter<'a, K, V>
where
    K: Clone,
    V: Clone,
{
    fn clone(&self) -> Self {
        Iter {
            it: self.it.clone(),
        }
    }
}

impl<'a, K, V> Iterator for Iter<'a, K, V>
where
    K: Clone,
    V: Clone,
{
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.it.next().map(|(k, v)| (k, v))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.it.size_hint()
    }
}
impl<'a, K, V> ExactSizeIterator for Iter<'a, K, V>
where
    K: Clone,
    V: Clone,
{
}

impl<'a, K, V> IntoIterator for &'a InOMap<K, V>
where
    K: Eq + Clone,
    V: Clone,
{
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        Iter {
            it: self.value.iter(),
        }
    }
}

impl<K, V> IntoIterator for InOMap<K, V>
where
    K: Eq + Clone,
    V: Clone,
{
    type Item = (K, V);
    type IntoIter = imbl::vector::ConsumingIter<(K, V), DefaultSharedPtr>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.value.into_iter()
    }
}

// Conversions

impl<K, V> FromIterator<(K, V)> for InOMap<K, V>
where
    V: Clone,
    K: Eq + Clone,
{
    fn from_iter<T>(i: T) -> Self
    where
        T: IntoIterator<Item = (K, V)>,
    {
        let mut map = Self::default();
        for (k, v) in i {
            map.insert(k, v);
        }
        map
    }
}

impl<K, V> Default for InOMap<K, V>
where
    K: Clone + Eq,
    V: Clone,
{
    fn default() -> Self {
        Self {
            value: Default::default(),
        }
    }
}

impl<K, V> AsRef<InOMap<K, V>> for InOMap<K, V>
where
    K: Eq + Clone,
    V: Clone,
{
    #[inline]
    fn as_ref(&self) -> &Self {
        self
    }
}
