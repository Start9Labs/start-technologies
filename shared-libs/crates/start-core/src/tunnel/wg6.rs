//! IPv6 host addressing for the tunnel. A subnet may carry a routed IPv6 prefix;
//! every host on that subnet — the server and each client alike — gets exactly
//! one `/128` with its tunnel IPv4 embedded in the low 32 bits. The same rule
//! applies to the server (its `.1`) and every client, so addresses are stable
//! and computable with no allocation state (the UI can derive a device's IPv6
//! without a backend round-trip). On a /64 every host is distinct; on a smaller
//! block two hosts whose low IPv4 bits collide would share an address, so callers
//! validate uniqueness (see [`v6_conflict`] / [`first_v6_collision`]) and reject
//! a duplicate rather than hand it out.

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr};

use ipnet::Ipv6Net;

/// The IPv6 address for a host whose tunnel IPv4 is `v4`, on a subnet whose
/// delegated prefix is `prefix`: the prefix's network bits OR'd with the tunnel
/// IPv4 clamped to the prefix's host space. Uniform for the server and every
/// client. A /64 (the natural size) keeps the whole 32-bit IPv4; a smaller block
/// (e.g. a /124) keeps only its low host bits so the address stays in-prefix —
/// distinct there as long as the clients' low host bits differ.
pub fn host_v6(prefix: Ipv6Net, v4: Ipv4Addr) -> Ipv6Addr {
    let keep = (128 - prefix.prefix_len()).min(32);
    let mask = if keep >= 32 { u32::MAX } else { (1u32 << keep) - 1 };
    let host = (u32::from(v4) & mask) as u128;
    Ipv6Addr::from(u128::from(prefix.network()) | host)
}

/// An existing host (a tunnel IPv4 in `existing`) whose IPv6 under `prefix`
/// equals `candidate`'s — i.e. adding `candidate` would duplicate its address.
/// `None` when `candidate` is unique. Only ever non-`None` on a prefix too small
/// to hold every host's low IPv4 bits distinctly.
pub fn v6_conflict(
    prefix: Ipv6Net,
    candidate: Ipv4Addr,
    existing: impl IntoIterator<Item = Ipv4Addr>,
) -> Option<Ipv4Addr> {
    let addr = host_v6(prefix, candidate);
    existing
        .into_iter()
        .find(|&e| e != candidate && host_v6(prefix, e) == addr)
}

/// The first pair of `hosts` (tunnel IPv4s) that map to the same IPv6 under
/// `prefix`, with that shared address — for rejecting a prefix that can't give
/// every host a distinct address.
pub fn first_v6_collision(
    prefix: Ipv6Net,
    hosts: impl IntoIterator<Item = Ipv4Addr>,
) -> Option<(Ipv4Addr, Ipv4Addr, Ipv6Addr)> {
    let mut seen: BTreeMap<Ipv6Addr, Ipv4Addr> = BTreeMap::new();
    for v4 in hosts {
        let addr = host_v6(prefix, v4);
        if let Some(&prev) = seen.get(&addr) {
            return Some((prev, v4, addr));
        }
        seen.insert(addr, v4);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(s: &str) -> Ipv6Net {
        s.parse().unwrap()
    }

    #[test]
    fn embeds_the_ipv4_in_the_low_bits() {
        // 10.59.7.2 == 0x0a3b0702
        assert_eq!(
            host_v6(net("2001:db8:abcd::/64"), "10.59.7.2".parse().unwrap()),
            "2001:db8:abcd::a3b:702".parse::<Ipv6Addr>().unwrap()
        );
    }

    #[test]
    fn server_and_clients_use_the_same_rule() {
        let p = net("2001:db8:abcd::/64");
        let server = host_v6(p, "10.59.0.1".parse().unwrap()); // subnet .1
        let a = host_v6(p, "10.59.0.2".parse().unwrap());
        let b = host_v6(p, "10.59.1.2".parse().unwrap());
        assert_eq!(server, "2001:db8:abcd::a3b:1".parse::<Ipv6Addr>().unwrap());
        assert_ne!(server, a);
        assert_ne!(a, b);
        // Every host stays inside the subnet's prefix.
        for h in [server, a, b] {
            assert!(p.contains(&h));
        }
    }

    #[test]
    fn works_for_prefixes_shorter_than_64() {
        // A /56 still lands the whole IPv4 in the host bits, inside the prefix.
        let p = net("2001:db8:1200::/56");
        let h = host_v6(p, "10.59.0.5".parse().unwrap());
        assert_eq!(h, "2001:db8:1200::a3b:5".parse::<Ipv6Addr>().unwrap());
        assert!(p.contains(&h));
    }

    #[test]
    fn stays_in_prefix_at_every_length() {
        // The IPv4 is clamped to the host space, so the address is always inside
        // the prefix — even for prefixes longer than /96, where only the low
        // host bits of the IPv4 survive.
        for len in [0u8, 48, 56, 64, 80, 96, 112, 124, 127, 128] {
            let p = Ipv6Net::new("2001:db8:abcd:ef00::".parse().unwrap(), len).unwrap();
            let h = host_v6(p, "10.59.3.7".parse().unwrap());
            assert!(p.contains(&h), "escaped prefix at /{len}");
        }
    }

    #[test]
    fn collision_detection() {
        let v4 = |s: &str| s.parse::<Ipv4Addr>().unwrap();
        // A /64 fits the whole IPv4, so distinct v4s never collide.
        let big = net("2001:db8:abcd::/64");
        assert_eq!(
            wg6_conflict(big, "10.59.0.2", &["10.59.0.1", "10.59.16.2"]),
            None,
        );
        assert!(first_v6_collision(big, [v4("10.59.0.1"), v4("10.59.0.2"), v4("10.59.16.2")]).is_none());

        // On a /124 only the low nibble survives: .2 and .18 (0x02 vs 0x12) both
        // map to ::f2, and .17 (0x11) collides with the server .1.
        let small = net("2001:db8:abcd:1::f0/124");
        assert_eq!(wg6_conflict(small, "10.59.0.18", &["10.59.0.2"]), Some(v4("10.59.0.2")));
        assert_eq!(wg6_conflict(small, "10.59.0.17", &["10.59.0.1"]), Some(v4("10.59.0.1")));
        // A fresh low nibble is fine.
        assert_eq!(wg6_conflict(small, "10.59.0.3", &["10.59.0.1", "10.59.0.2"]), None);
        // first_v6_collision finds the clashing pair.
        let (a, b, addr) = first_v6_collision(small, [v4("10.59.0.1"), v4("10.59.0.2"), v4("10.59.0.18")]).unwrap();
        assert_eq!((a, b), (v4("10.59.0.2"), v4("10.59.0.18")));
        assert_eq!(addr, "2001:db8:abcd:1::f2".parse::<Ipv6Addr>().unwrap());
    }

    fn wg6_conflict(prefix: Ipv6Net, candidate: &str, existing: &[&str]) -> Option<Ipv4Addr> {
        v6_conflict(
            prefix,
            candidate.parse().unwrap(),
            existing.iter().map(|s| s.parse().unwrap()),
        )
    }

    #[test]
    fn narrow_prefix_keeps_low_host_bits() {
        // A small (/124) block: only the low nibble of the IPv4 survives, so
        // subnet .1 -> ::f1, .2 -> ::f2. Distinct as long as the clients' low
        // nibbles differ.
        let p = net("2001:db8:abcd:1::f0/124");
        assert_eq!(
            host_v6(p, "10.59.0.1".parse().unwrap()),
            "2001:db8:abcd:1::f1".parse::<Ipv6Addr>().unwrap()
        );
        assert_eq!(
            host_v6(p, "10.59.0.2".parse().unwrap()),
            "2001:db8:abcd:1::f2".parse::<Ipv6Addr>().unwrap()
        );
    }
}
