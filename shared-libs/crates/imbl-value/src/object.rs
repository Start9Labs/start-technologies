use std::fmt::Debug;
use std::hash::Hash;

use imbl::shared_ptr::DefaultSharedPtr;
use imbl::OrdMap;
use yasi::InternedString;

use crate::{GenericValue, InOMap};

/// The map backing a JSON object whose values are `V`.
pub trait ObjectMap<V>: Clone + Default + FromIterator<(InternedString, V)> {
    type Entries<'a>: ExactSizeIterator<Item = (&'a InternedString, &'a V)>
    where
        Self: 'a,
        V: 'a;
    type EntriesMut<'a>: Iterator<Item = (&'a InternedString, &'a mut V)>
    where
        Self: 'a,
        V: 'a;
    type IntoEntries: ExactSizeIterator<Item = (InternedString, V)>;

    fn len(&self) -> usize;
    fn entries(&self) -> Self::Entries<'_>;
    fn entries_mut(&mut self) -> Self::EntriesMut<'_>;
    fn into_entries(self) -> Self::IntoEntries;
    fn get(&self, key: &str) -> Option<&V>;
    fn get_mut(&mut self, key: &str) -> Option<&mut V>;
    fn insert(&mut self, key: InternedString, value: V) -> Option<V>;
    fn remove(&mut self, key: &str) -> Option<V>;
    fn get_or_insert_with(&mut self, key: InternedString, default: impl FnOnce() -> V) -> &mut V;
    fn ptr_eq(&self, other: &Self) -> bool;
}

/// Selects the map that backs a [`GenericValue`]'s objects.
pub trait ObjectKind: Sized + 'static {
    type Map: ObjectMap<GenericValue<Self>> + Debug + Eq + Ord + Hash;
}

/// Objects as [`InOMap`]s, which keep keys in insertion order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InOrder {}

impl ObjectKind for InOrder {
    type Map = InOMap<InternedString, GenericValue<InOrder>>;
}

/// Objects as [`OrdMap`]s, which keep keys sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Sorted {}

impl ObjectKind for Sorted {
    type Map = OrdMap<InternedString, GenericValue<Sorted>>;
}

impl<V: Clone> ObjectMap<V> for InOMap<InternedString, V> {
    type Entries<'a>
        = crate::in_order_map::Iter<'a, InternedString, V>
    where
        V: 'a;
    type EntriesMut<'a>
        = std::iter::Map<
        imbl::vector::IterMut<'a, (InternedString, V), DefaultSharedPtr>,
        fn(&'a mut (InternedString, V)) -> (&'a InternedString, &'a mut V),
    >
    where
        V: 'a;
    type IntoEntries = <Self as IntoIterator>::IntoIter;

    fn len(&self) -> usize {
        InOMap::len(self)
    }
    fn entries(&self) -> Self::Entries<'_> {
        self.into_iter()
    }
    fn entries_mut(&mut self) -> Self::EntriesMut<'_> {
        self.iter_mut().map(|(k, v)| (&*k, v))
    }
    fn into_entries(self) -> Self::IntoEntries {
        self.into_iter()
    }
    fn get(&self, key: &str) -> Option<&V> {
        InOMap::get(self, key)
    }
    fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        InOMap::get_mut(self, key)
    }
    fn insert(&mut self, key: InternedString, value: V) -> Option<V> {
        InOMap::insert(self, key, value)
    }
    fn remove(&mut self, key: &str) -> Option<V> {
        InOMap::remove(self, key)
    }
    fn get_or_insert_with(&mut self, key: InternedString, default: impl FnOnce() -> V) -> &mut V {
        self.entry(key).or_insert_with(default)
    }
    fn ptr_eq(&self, other: &Self) -> bool {
        InOMap::ptr_eq(self, other)
    }
}

impl<V: Clone> ObjectMap<V> for OrdMap<InternedString, V> {
    type Entries<'a>
        = imbl::ordmap::Iter<'a, InternedString, V, DefaultSharedPtr>
    where
        V: 'a;
    type EntriesMut<'a>
        = imbl::ordmap::IterMut<'a, InternedString, V, DefaultSharedPtr>
    where
        V: 'a;
    type IntoEntries = <Self as IntoIterator>::IntoIter;

    fn len(&self) -> usize {
        OrdMap::len(self)
    }
    fn entries(&self) -> Self::Entries<'_> {
        self.iter()
    }
    fn entries_mut(&mut self) -> Self::EntriesMut<'_> {
        self.iter_mut()
    }
    fn into_entries(self) -> Self::IntoEntries {
        self.into_iter()
    }
    fn get(&self, key: &str) -> Option<&V> {
        OrdMap::get(self, key)
    }
    fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        OrdMap::get_mut(self, key)
    }
    fn insert(&mut self, key: InternedString, value: V) -> Option<V> {
        OrdMap::insert(self, key, value)
    }
    fn remove(&mut self, key: &str) -> Option<V> {
        OrdMap::remove(self, key)
    }
    fn get_or_insert_with(&mut self, key: InternedString, default: impl FnOnce() -> V) -> &mut V {
        self.entry(key).or_insert_with(default)
    }
    fn ptr_eq(&self, other: &Self) -> bool {
        OrdMap::ptr_eq(self, other)
    }
}
