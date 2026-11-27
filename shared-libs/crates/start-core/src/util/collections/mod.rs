pub mod eq_map;
pub mod eq_set;

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
