use std::marker::PhantomData;
use std::ops::Bound;

use imbl::OrdMap;

/// Mutable iteration over an [`OrdMap`], in key order.
pub struct OrdMapIterMut<'a, K: 'a, V: 'a> {
    map: *mut OrdMap<K, V>,
    prev: Option<&'a K>,
    _marker: PhantomData<&'a mut (K, V)>,
}

// SAFETY: the iterator holds the exclusive borrow it was built from.
unsafe impl<'a, K, V> Send for OrdMapIterMut<'a, K, V> where &'a mut OrdMap<K, V>: Send {}

// SAFETY: the iterator holds the exclusive borrow it was built from.
unsafe impl<'a, K, V> Sync for OrdMapIterMut<'a, K, V> where &'a mut OrdMap<K, V>: Sync {}

impl<'a, K, V> From<&'a mut OrdMap<K, V>> for OrdMapIterMut<'a, K, V> {
    fn from(value: &'a mut OrdMap<K, V>) -> Self {
        Self {
            map: value,
            prev: None,
            _marker: PhantomData,
        }
    }
}

impl<'a, K: Ord + Clone, V: Clone> Iterator for OrdMapIterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);
    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: relies on `get_key_value_mut` leaving entries it already made unique in place.
        unsafe {
            let map: &'a mut OrdMap<K, V> = self.map.as_mut().unwrap();
            let Some((k, _)) = (if let Some(k) = self.prev.take() {
                map.range::<_, K>((Bound::Excluded(k), Bound::Unbounded))
                    .next()
            } else {
                map.get_min().map(|(k, v)| (k, v))
            }) else {
                return None;
            };
            let k = k.clone();
            let res = map.get_key_value_mut(&k);
            if let Some((k, _)) = &res {
                self.prev = Some(*k);
            }
            res
        }
    }
}
