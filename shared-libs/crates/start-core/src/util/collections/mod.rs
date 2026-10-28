pub mod eq_map;
pub mod eq_set;

use std::marker::PhantomData;
use std::ops::Bound;

pub use eq_map::EqMap;
pub use eq_set::EqSet;
use imbl::OrdMap;

pub fn ordmap_retain<K: Ord + Clone, V: Clone, F: FnMut(&K, &mut V) -> bool>(
    map: &mut OrdMap<K, V>,
    mut f: F,
) {
    let mut prev = None;
    loop {
        let next = if let Some(k) = prev.take() {
            map.range::<_, K>((Bound::Excluded(k), Bound::Unbounded))
                .next()
        } else {
            map.get_min().map(|(k, v)| (k, v))
        };
        let Some((k, _)) = next else {
            break;
        };
        let k = k.clone(); // hate that I have to do this but whatev
        let v = map.get_mut(&k).unwrap();

        if !f(&k, v) {
            map.remove(&k);
        }
        prev = Some(k);
    }
}

pub struct OrdMapIterMut<'a, K: 'a, V: 'a> {
    map: *mut OrdMap<K, V>,
    prev: Option<&'a K>,
    _marker: PhantomData<&'a mut (K, V)>,
}
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
            let k = k.clone(); // hate that I have to do this but whatev
            let res = map.get_key_value_mut(&k);
            if let Some((k, _)) = &res {
                self.prev = Some(*k);
            }
            res
        }
    }
}
