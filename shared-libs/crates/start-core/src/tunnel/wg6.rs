//! IPv6 host addressing for the tunnel. A subnet may carry a routed IPv6 prefix;
//! every host on that subnet — the server and each client alike — gets exactly
//! one `/128` with its tunnel IPv4 embedded in the low 32 bits. The same rule
//! applies to the server (its `.1`) and every client, so addresses are stable,
//! collision-free, and computable with no allocation state (the UI can derive a
//! device's IPv6 without a backend round-trip).

use std::net::{Ipv4Addr, Ipv6Addr};

use ipnet::Ipv6Net;

/// The IPv6 address for a host whose tunnel IPv4 is `v4`, on a subnet whose
/// delegated prefix is `prefix`: the prefix's network bits OR'd with the 32-bit
/// IPv4. Uniform for the server and every client. A real per-subnet delegation
/// is a /64 (or shorter), leaving room for the whole IPv4 in the host bits.
pub fn host_v6(prefix: Ipv6Net, v4: Ipv4Addr) -> Ipv6Addr {
    Ipv6Addr::from(u128::from(prefix.network()) | u32::from(v4) as u128)
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
        let p = net("2604:a880:4:1d0::/64");
        let server = host_v6(p, "10.59.0.1".parse().unwrap()); // subnet .1
        let a = host_v6(p, "10.59.0.2".parse().unwrap());
        let b = host_v6(p, "10.59.1.2".parse().unwrap());
        assert_eq!(server, "2604:a880:4:1d0::a3b:1".parse::<Ipv6Addr>().unwrap());
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
    fn stays_in_prefix_up_to_the_96_boundary() {
        // /96 is the smallest prefix that still fits a full 32-bit IPv4 in the
        // host bits; `set_subnet_ipv6` rejects anything longer, so host_v6 is
        // only ever called with prefixes <= /96 and always stays in-prefix.
        for len in [48u8, 56, 64, 80, 96] {
            let p = Ipv6Net::new("2001:db8:abcd:ef00::".parse().unwrap(), len).unwrap();
            let h = host_v6(p, "10.59.3.7".parse().unwrap());
            assert!(p.contains(&h), "escaped prefix at /{len}");
        }
    }
}
