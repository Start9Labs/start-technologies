use imbl::{GenericOrdMap, GenericVector};

use super::{ArenaK, ArenaStr};

#[test]
fn strings_round_trip_and_share_one_allocation() {
    let a = ArenaStr::from("hello, arena");
    let b = a.clone();
    assert!(ArenaStr::ptr_eq(&a, &b));
    assert_eq!(&*b, "hello, arena");
    assert_eq!(ArenaStr::from(""), ArenaStr::from(String::new()));
    let long = "x".repeat(1 << 20);
    assert_eq!(ArenaStr::from(long.as_str()).len(), long.len());
}

#[test]
fn shared_nodes_copy_on_write() {
    let mut map: GenericOrdMap<u32, ArenaStr, ArenaK> = GenericOrdMap::new();
    for i in 0..10_000 {
        map.insert(i, ArenaStr::from(i.to_string()));
    }
    let snapshot = map.clone();
    for (_, v) in map.iter_mut() {
        *v = ArenaStr::from(format!("{v}!"));
    }
    assert_eq!(&*snapshot[&42], "42");
    assert_eq!(&*map[&42], "42!");
    drop(snapshot);
    let vector: GenericVector<u64, ArenaK> = (0..100_000).collect();
    let mut copy = vector.clone();
    copy.set(5, 7);
    assert_eq!((vector[5], copy[5]), (5, 7));
}

#[test]
fn concurrent_writers_agree_on_their_own_values() {
    let threads: Vec<_> = (0..8u32)
        .map(|t| {
            std::thread::spawn(move || {
                let mut map: GenericOrdMap<u32, ArenaStr, ArenaK> = GenericOrdMap::new();
                for round in 0..50 {
                    for i in 0..1_000 {
                        map.insert(i, ArenaStr::from(format!("{t}-{round}-{i}")));
                    }
                    let kept = map.clone();
                    for i in (0..1_000).step_by(3) {
                        map.remove(&i);
                    }
                    assert_eq!(&*kept[&999], format!("{t}-{round}-999"));
                }
                map.len()
            })
        })
        .collect();
    for t in threads {
        assert_eq!(t.join().unwrap(), 666);
    }
}
