use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

use imbl::shared_ptr::{DefaultSharedPtr, SharedPointerKind};
use imbl::{GenericOrdMap, GenericVector};
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

/// The shared string a [`GenericValue`] holds.
pub trait ValueStr: Clone + From<String> + Debug + Eq + Ord + Hash + Send + Sync + 'static {
    fn as_str(&self) -> &str;
    fn into_string(self) -> String;
    fn ptr_eq(a: &Self, b: &Self) -> bool;
    fn into_arc(self) -> Arc<String> {
        Arc::new(self.into_string())
    }
    fn from_str_ref(s: &str) -> Self {
        Self::from(s.to_owned())
    }
    fn from_arc(s: Arc<String>) -> Self {
        Self::from(Arc::unwrap_or_clone(s))
    }
}

impl ValueStr for Arc<String> {
    fn as_str(&self) -> &str {
        self
    }
    fn into_string(self) -> String {
        Arc::unwrap_or_clone(self)
    }
    fn ptr_eq(a: &Self, b: &Self) -> bool {
        Arc::ptr_eq(a, b)
    }
    fn from_arc(s: Arc<String>) -> Self {
        s
    }
    fn into_arc(self) -> Arc<String> {
        self
    }
}

#[cfg(feature = "arena")]
impl ValueStr for crate::arena::ArenaStr {
    fn as_str(&self) -> &str {
        crate::arena::ArenaStr::as_str(self)
    }
    fn into_string(self) -> String {
        self.into()
    }
    fn ptr_eq(a: &Self, b: &Self) -> bool {
        crate::arena::ArenaStr::ptr_eq(a, b)
    }
    fn from_str_ref(s: &str) -> Self {
        Self::new(s)
    }
}

/// Selects the map, shared pointer and string that back a [`GenericValue`].
pub trait ObjectKind: Sized + 'static {
    type Ptr: SharedPointerKind;
    type Str: ValueStr;
    type Map: ObjectMap<GenericValue<Self>> + Debug + Eq + Ord + Hash;
}

/// The array of a [`GenericValue`] of the kind.
pub type ValueVector<K> = GenericVector<GenericValue<K>, <K as ObjectKind>::Ptr>;

#[cfg(feature = "arena")]
type SortedPtr = crate::arena::ArenaK;
#[cfg(not(feature = "arena"))]
type SortedPtr = DefaultSharedPtr;
#[cfg(feature = "arena")]
type SortedStr = crate::arena::ArenaStr;
#[cfg(not(feature = "arena"))]
type SortedStr = Arc<String>;

/// Objects as [`InOMap`]s, which keep keys in insertion order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InOrder {}

impl ObjectKind for InOrder {
    type Ptr = DefaultSharedPtr;
    type Str = Arc<String>;
    type Map = InOMap<InternedString, GenericValue<InOrder>>;
}

/// Objects as [`imbl::OrdMap`]s, which keep keys sorted.
///
/// With the `arena` feature, its nodes and strings live in the [`crate::arena`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Sorted {}

/// The map behind an [`crate::OrdValue`] object.
pub type OrdObject = GenericOrdMap<InternedString, GenericValue<Sorted>, SortedPtr>;

impl ObjectKind for Sorted {
    type Ptr = SortedPtr;
    type Str = SortedStr;
    type Map = OrdObject;
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

impl<V: Clone, P: SharedPointerKind + 'static> ObjectMap<V>
    for GenericOrdMap<InternedString, V, P>
{
    type Entries<'a>
        = imbl::ordmap::Iter<'a, InternedString, V, P>
    where
        V: 'a;
    type EntriesMut<'a>
        = imbl::ordmap::IterMut<'a, InternedString, V, P>
    where
        V: 'a;
    type IntoEntries = <Self as IntoIterator>::IntoIter;

    fn len(&self) -> usize {
        GenericOrdMap::len(self)
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
        GenericOrdMap::get(self, key)
    }
    fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        GenericOrdMap::get_mut(self, key)
    }
    fn insert(&mut self, key: InternedString, value: V) -> Option<V> {
        GenericOrdMap::insert(self, key, value)
    }
    fn remove(&mut self, key: &str) -> Option<V> {
        GenericOrdMap::remove(self, key)
    }
    fn get_or_insert_with(&mut self, key: InternedString, default: impl FnOnce() -> V) -> &mut V {
        self.entry(key).or_insert_with(default)
    }
    fn ptr_eq(&self, other: &Self) -> bool {
        GenericOrdMap::ptr_eq(self, other)
    }
}
