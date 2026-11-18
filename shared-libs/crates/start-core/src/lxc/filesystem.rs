use crate::disk::mount::filesystem::FileSystem;
use crate::disk::mount::filesystem::idmapped::{IdMap, IdMapped};

pub(crate) const BASE_IDMAP: IdMap = IdMap {
    from_id: 0,
    to_id: 100000,
    range: 65536,
};

/// Service views apply the LXC base map, followed by package-supplied remaps.
/// Raw filesystem owners remain unshifted on disk.
pub(crate) fn service_filesystem<F: FileSystem>(filesystem: F, custom: &[IdMap]) -> IdMapped<F> {
    IdMapped::new(filesystem, service_idmap(custom))
}

// Re-idmapping replaces the source map; layered remaps require one composed map.
// Explicit remaps win collisions; ties preserve the lowest on-disk ID.
fn service_idmap(custom: &[IdMap]) -> Vec<IdMap> {
    let base = BASE_IDMAP;
    if custom.is_empty() {
        return vec![base];
    }
    let len = base.range as usize;

    let mut host = vec![0u32; len];
    let mut explicit = vec![false; len];
    for d in 0..base.range {
        let mut c = d;
        let mut touched = false;
        for m in custom {
            if c >= m.from_id && c - m.from_id < m.range {
                c = m.to_id.saturating_add(c - m.from_id);
                touched = true;
            }
        }
        host[d as usize] = base.to_id.saturating_add(c);
        explicit[d as usize] = touched;
    }

    let mut owner: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for d in 0..base.range {
        match owner.get(&host[d as usize]) {
            None => {
                owner.insert(host[d as usize], d);
            }
            Some(&prev) if explicit[d as usize] && !explicit[prev as usize] => {
                owner.insert(host[d as usize], d);
            }
            Some(_) => {}
        }
    }
    let mut keep = vec![None; len];
    for (h, d) in owner {
        keep[d as usize] = Some(h);
    }

    let mut out = Vec::new();
    let mut d = 0u32;
    while (d as usize) < len {
        if let Some(h0) = keep[d as usize] {
            let mut n = 1u32;
            while ((d + n) as usize) < len && keep[(d + n) as usize] == Some(h0.saturating_add(n)) {
                n += 1;
            }
            out.push(IdMap {
                from_id: d,
                to_id: h0,
                range: n,
            });
            d += n;
        } else {
            d += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idmap(from_id: u32, to_id: u32, range: u32) -> IdMap {
        IdMap {
            from_id,
            to_id,
            range,
        }
    }

    #[test]
    fn empty_custom_is_just_the_base_map() {
        assert_eq!(service_idmap(&[]), vec![BASE_IDMAP]);
    }

    #[test]
    fn override_is_layered_on_top_of_the_base_map() {
        assert_eq!(
            service_idmap(&[idmap(0, 1000, 1)]),
            vec![
                idmap(0, 101000, 1),
                idmap(1, 100001, 999),
                idmap(1001, 101001, 64535),
            ],
        );
    }

    #[test]
    fn chaining_idmaps_compose_sequentially() {
        let custom = [idmap(0, 1000, 1), idmap(1000, 2000, 1)];
        let flat = service_idmap(&custom);
        assert_eq!(apply(&flat, 0), Some(102000));
        assert_eq!(apply(&flat, 5), Some(100005));
        assert_eq!(compose_ref(&custom, 1000), Some(102000));
        assert_eq!(apply(&flat, 1000), None);
    }

    fn compose_ref(custom: &[IdMap], on_disk: u32) -> Option<u32> {
        let base = BASE_IDMAP;
        if on_disk >= base.range {
            return None;
        }
        let mut c = on_disk;
        for m in custom {
            if c >= m.from_id && c - m.from_id < m.range {
                c = m.to_id.saturating_add(c - m.from_id);
            }
        }
        Some(base.to_id.saturating_add(c))
    }

    fn apply(map: &[IdMap], on_disk: u32) -> Option<u32> {
        map.iter().find_map(|m| {
            (on_disk >= m.from_id && on_disk - m.from_id < m.range)
                .then(|| m.to_id.saturating_add(on_disk - m.from_id))
        })
    }

    fn is_valid_uid_map(map: &[IdMap]) -> bool {
        let overlaps = |mut v: Vec<(u32, u32)>| {
            v.sort();
            v.windows(2).any(|w| w[0].1 > w[1].0)
        };
        let froms: Vec<(u32, u32)> = map
            .iter()
            .map(|m| (m.from_id, m.from_id + m.range))
            .collect();
        let tos: Vec<(u32, u32)> = map.iter().map(|m| (m.to_id, m.to_id + m.range)).collect();
        !overlaps(froms) && !overlaps(tos)
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config { cases: 48, ..proptest::test_runner::Config::default() })]

        #[test]
        fn flatten_is_the_injective_restriction_of_composition(
            custom in proptest::collection::vec((0u32..60_000, 0u32..60_000, 1u32..2_000), 0..4usize),
        ) {
            let custom: Vec<IdMap> = custom.into_iter().map(|(f, t, r)| idmap(f, t, r)).collect();
            let flat = service_idmap(&custom);

            proptest::prop_assert!(is_valid_uid_map(&flat), "invalid uid_map: {flat:?}");

            let comp: Vec<Option<u32>> =
                (0..BASE_IDMAP.range).map(|d| compose_ref(&custom, d)).collect();
            let mut hits: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
            for c in comp.iter().flatten() {
                *hits.entry(*c).or_default() += 1;
            }

            for d in 0..BASE_IDMAP.range {
                let f = apply(&flat, d);
                let c = comp[d as usize];
                proptest::prop_assert!(f.is_none() || f == c, "d={d}: flat={f:?} compose={c:?}");
                if let Some(h) = c {
                    if hits[&h] == 1 {
                        proptest::prop_assert_eq!(f, Some(h), "d={} dropped a unique target", d);
                    }
                }
            }
        }
    }
}
