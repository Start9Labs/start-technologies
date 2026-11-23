use std::iter::FusedIterator;
use std::marker::PhantomData;
use std::ops::Bound;

use imbl::OrdMap;

/// Mutable iteration over an [`OrdMap`], in key order.
pub struct OrdMapIterMut<'a, K: 'a, V: 'a> {
    map: *mut OrdMap<K, V>,
    prev: Option<&'a K>,
    done: bool,
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
            done: false,
            _marker: PhantomData,
        }
    }
}

impl<'a, K: Ord + Clone, V: Clone> Iterator for OrdMapIterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        // SAFETY: relies on `get_key_value_mut` leaving entries it already made unique in place.
        unsafe {
            let map: &'a mut OrdMap<K, V> = self.map.as_mut().unwrap();
            let Some((k, _)) = (if let Some(k) = self.prev.take() {
                map.range::<_, K>((Bound::Excluded(k), Bound::Unbounded))
                    .next()
            } else {
                map.get_min().map(|(k, v)| (k, v))
            }) else {
                self.done = true;
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

impl<'a, K: Ord + Clone, V: Clone> FusedIterator for OrdMapIterMut<'a, K, V> {}

#[cfg(test)]
mod tests {
    use imbl::OrdMap;

    use super::OrdMapIterMut;

    #[test]
    fn stays_exhausted() {
        let mut map = OrdMap::unit(1, 1);
        let mut iter = OrdMapIterMut::from(&mut map);
        assert_eq!(iter.next(), Some((&1, &mut 1)));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn visits_each_entry_once_in_key_order() {
        let mut map: OrdMap<u32, u32> = (0..200u32).map(|i| (i, i)).collect();
        let snapshot = map.clone();
        for (k, v) in OrdMapIterMut::from(&mut map) {
            *v += k + 1;
        }
        assert!((0..200u32).all(|i| map[&i] == 2 * i + 1));
        assert!((0..200u32).all(|i| snapshot[&i] == i));
    }
}
