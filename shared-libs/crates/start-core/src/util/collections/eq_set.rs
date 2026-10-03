use std::borrow::Borrow;
use std::fmt;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, visit_rs::TS)]
pub struct EqSet<T: Eq>(Vec<T>);
impl<T: Eq> Default for EqSet<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}
impl<T: Eq> EqSet<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }

    /// Returns a reference to the element in the set, if any, that is equal to
    /// the value.
    ///
    /// The value may be any borrowed form of the set's element type,
    /// but the ordering on the borrowed form *must* match the
    /// ordering on the element type.
    ///
    /// # Examples
    ///
    /// ```
    /// use start_core::util::collections::EqSet;
    ///
    /// let set = EqSet::from([1, 2, 3]);
    /// assert_eq!(set.get(&2), Some(&2));
    /// assert_eq!(set.get(&4), None);
    /// ```
    pub fn get<Q: ?Sized>(&self, value: &Q) -> Option<&T>
    where
        T: Borrow<Q>,
        Q: Eq,
    {
        self.0.iter().find(|k| (*k).borrow() == value)
    }

    /// Removes and returns an element in the set.
    /// There is no guarantee about which element this might be
    ///
    /// # Examples
    ///
    /// ```
    /// use start_core::util::collections::EqSet;
    ///
    /// let mut set = EqSet::new();
    /// set.insert("a");
    /// set.insert("b");
    /// while let Some(_val) = set.pop() { }
    /// assert!(set.is_empty());
    /// ```
    pub fn pop(&mut self) -> Option<T> {
        self.0.pop()
    }

    /// Returns `true` if the set contains a value for the specified value.
    ///
    /// The value may be any borrowed form of the set's value type, but the equality
    /// on the borrowed form *must* match the equality on the value type.
    ///
    /// # Examples
    ///
    /// ```
    /// use start_core::util::collections::EqSet;
    ///
    /// let mut set = EqSet::new();
    /// set.insert("a");
    /// assert_eq!(set.contains("a"), true);
    /// assert_eq!(set.contains("b"), false);
    /// ```
    pub fn contains<Q: ?Sized>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Eq,
    {
        self.get(value).is_some()
    }

    /// Inserts a value into the set.
    ///
    /// If the set did not have this value present, `None` is returned.
    ///
    /// If the set did have this value present, the value is updated, and the old
    /// value is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use start_core::util::collections::EqSet;
    ///
    /// let mut set = EqSet::new();
    /// assert_eq!(set.insert("a"), None);
    /// assert_eq!(set.is_empty(), false);
    ///
    /// set.insert("b");
    /// assert_eq!(set.insert("b"), Some("b"));
    /// assert!(set.contains("a"));
    /// ```
    pub fn insert(&mut self, value: T) -> Option<T> {
        if let Some(entry) = self.0.iter_mut().find(|a| *a == &value) {
            Some(std::mem::replace(entry, value))
        } else {
            self.0.push(value);
            None
        }
    }

    /// Tries to insert a value into the set.
    ///
    /// If the set already had this value present, nothing is updated.
    ///
    /// Returns whether the value was inserted.
    ///
    /// # Examples
    ///
    /// ```
    /// use start_core::util::collections::EqSet;
    ///
    /// let mut set = EqSet::new();
    /// assert!(set.try_insert("a"));
    /// assert!(!set.try_insert("a"));
    /// ```
    pub fn try_insert(&mut self, value: T) -> bool {
        if self.0.iter().find(|a| *a == &value).is_some() {
            false
        } else {
            self.0.push(value);
            true
        }
    }

    /// Removes a value from the set, returning the value if it
    /// was previously in the set.
    ///
    /// The value may be any borrowed form of the set's value type, but the equality
    /// on the borrowed form *must* match the equality on the value type.
    ///
    /// # Examples
    ///
    /// ```
    /// use start_core::util::collections::EqSet;
    ///
    /// let mut set = EqSet::new();
    /// set.insert("a");
    /// assert_eq!(set.remove("a"), Some("a"));
    /// assert_eq!(set.remove("a"), None);
    /// ```
    pub fn remove<Q: ?Sized>(&mut self, value: &Q) -> Option<T>
    where
        T: Borrow<Q>,
        Q: Eq,
    {
        if let Some((idx, _)) = self
            .0
            .iter()
            .enumerate()
            .find(|(_, v)| (*v).borrow() == value)
        {
            Some(self.0.swap_remove(idx))
        } else {
            None
        }
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// In other words, remove all elements `x` for which `f(&x)` returns `false`.
    /// The elements are visited in order.
    ///
    /// # Examples
    ///
    /// ```
    /// use start_core::util::collections::EqSet;
    ///
    /// let mut set: EqSet<i32> = (0..8).collect();
    /// // Keep only the elements with even-numbered values.
    /// set.retain(|x| *x % 2 == 0);
    /// assert!(set.into_iter().eq(vec![0, 2, 4, 6]));
    /// ```
    #[inline]
    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&T) -> bool,
    {
        self.0.retain(f)
    }

    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// If a value from `other` is already present in `self`, the respective
    /// value from `self` will be overwritten with the respective value from `other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use start_core::util::collections::EqSet;
    ///
    /// let mut a = EqSet::new();
    /// a.insert("a");
    /// a.insert("b");
    /// a.insert("c"); // Note: "c" also present in b.
    ///
    /// let mut b = EqSet::new();
    /// b.insert("c"); // Note: "c" also present in a.
    /// b.insert("d");
    /// b.insert("e");
    ///
    /// a.append(&mut b);
    ///
    /// assert_eq!(a.len(), 5);
    /// assert_eq!(b.len(), 0);
    /// ```
    pub fn append(&mut self, other: &mut Self) {
        other.retain(|v| !self.contains(v));
        self.0.append(&mut other.0)
    }

    // /// Creates an iterator that visits all elements (values) and
    // /// uses a closure to determine if an element should be removed. If the
    // /// closure returns `true`, the element is removed from the set and yielded.
    // /// If the closure returns `false`, or panics, the element remains in the set
    // /// and will not be yielded.
    // ///
    // /// The iterator also lets you mutate the value of each element in the
    // /// closure, regardless of whether you choose to keep or remove it.
    // ///
    // /// If the returned `ExtractIf` is not exhausted, e.g. because it is dropped without iterating
    // /// or the iteration short-circuits, then the remaining elements will be retained.
    // /// Use [`retain`] with a negated predicate if you do not need the returned iterator.
    // ///
    // /// [`retain`]: EqSet::retain
    // ///
    // /// # Examples
    // ///
    // /// Splitting a set into even and odd values, reusing the original set:
    // ///
    // /// ```
    // /// use start_core::util::collections::EqSet;
    // ///
    // /// let mut set: EqSet<(i32, i32)> = (0..8).map(|x| (x, x)).collect();
    // /// let evens: EqSet<_, _> = set.extract_if(|k, _v| k % 2 == 0).collect();
    // /// let odds = set;
    // /// assert_eq!(evens.values().copied().collect::<Vec<_>>(), [0, 2, 4, 6]);
    // /// assert_eq!(odds.values().copied().collect::<Vec<_>>(), [1, 3, 5, 7]);
    // /// ```
    // pub fn extract_if<F>(&mut self, pred: F) -> ExtractIf<'_, T, F>
    // where
    //     K: Eq,
    //     F: FnMut(&K, &mut V) -> bool,
    // {
    //     let (inner, alloc) = self.extract_if_inner();
    //     ExtractIf { pred, inner, alloc }
    // }

    /// Gets an iterator over the entries of the set, in no particular order.
    ///
    /// # Examples
    ///
    /// ```
    /// use start_core::util::collections::EqSet;
    ///
    /// let mut set = EqSet::new();
    /// set.insert("c");
    /// set.insert("b");
    /// set.insert("a");
    ///
    /// for value in set.iter() {
    ///     println!("{value}");
    /// }
    ///
    /// let first_value = set.iter().next().unwrap();
    /// assert_eq!(*first_value, "c");
    /// ```
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }

    /// Returns the number of elements in the set.
    ///
    /// # Examples
    ///
    /// ```
    /// use start_core::util::collections::EqSet;
    ///
    /// let mut a = EqSet::new();
    /// assert_eq!(a.len(), 0);
    /// a.insert("a");
    /// assert_eq!(a.len(), 1);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the set contains no elements.
    ///
    /// # Examples
    ///
    /// ```
    /// use start_core::util::collections::EqSet;
    ///
    /// let mut a = EqSet::new();
    /// assert!(a.is_empty());
    /// a.insert("a");
    /// assert!(!a.is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T: fmt::Debug + Eq> fmt::Debug for EqSet<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl<T: Eq> IntoIterator for EqSet<T> {
    type IntoIter = std::vec::IntoIter<T>;
    type Item = T;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<T: Eq> Extend<T> for EqSet<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        if let (_, Some(len)) = iter.size_hint() {
            self.0.reserve(len)
        }
        for v in iter {
            self.insert(v);
        }
    }
}

impl<T: Eq> FromIterator<T> for EqSet<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut res = Self(Vec::new());
        let iter = iter.into_iter();
        if let (_, Some(len)) = iter.size_hint() {
            res.0.reserve(len)
        }
        for v in iter {
            res.insert(v);
        }
        res
    }
}

impl<T: Eq, const N: usize> From<[T; N]> for EqSet<T> {
    /// Converts a `[T; N]` into a `EqSet<T>`.
    ///
    /// ```
    /// use start_core::util::collections::EqSet;
    ///
    /// let set1 = EqSet::from([(1, 2), (3, 4)]);
    /// let set2: EqSet<_> = [(1, 2), (3, 4)].into();
    /// assert_eq!(set1, set2);
    /// ```
    fn from(arr: [T; N]) -> Self {
        EqSet::from_iter(arr)
    }
}

impl<T: Eq> PartialEq for EqSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().all(|v| other.get(v) == Some(v))
    }
}
impl<T: Eq> Eq for EqSet<T> {}

impl<'de, T> Deserialize<'de> for EqSet<T>
where
    T: Deserialize<'de> + Eq,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor<T> {
            marker: PhantomData<T>,
        }

        impl<'de, T> serde::de::Visitor<'de> for Visitor<T>
        where
            T: Deserialize<'de> + Eq,
        {
            type Value = EqSet<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut values = EqSet(Vec::new());

                while let Some(value) = seq.next_element()? {
                    values.insert(value);
                }

                Ok(values)
            }
        }

        let visitor = Visitor {
            marker: PhantomData,
        };
        deserializer.deserialize_seq(visitor)
    }
}
