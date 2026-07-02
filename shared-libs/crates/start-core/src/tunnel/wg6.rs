//! IPv6 address delegation for the tunnel. The operator configures one routed
//! prefix (whatever their VPS delegates — commonly a single /64, sometimes a
//! /56 or /48); StartTunnel carves per-client global addresses out of it.
//!
//! The split adapts to how much room the prefix has:
//!
//! - **prefix shorter than /64** — there is room for many /64s, so each client
//!   is *delegated its own /64* (routed over WireGuard via `AllowedIPs`). A
//!   StartOS box can then hand real global addresses to its own containers. This
//!   is prefix delegation proper.
//! - **exactly a /64** — the common budget-VPS case (Hetzner, Vultr, …). All
//!   clients share the one /64; each gets a single /128 whose host bits are the
//!   client's tunnel IPv4 address, so the mapping is stable and allocation-free.
//! - **longer than a /64** (e.g. DigitalOcean's /124) — only a handful of
//!   addresses exist; each client gets one /128 indexed within that small space.
//!
//! Host value `1` in the base /64 is reserved for the tunnel itself (its address
//! on the WireGuard interface, and each client's IPv6 gateway).

use std::net::{Ipv4Addr, Ipv6Addr};

use ipnet::Ipv6Net;
use serde::{Deserialize, Serialize};

/// Host value reserved for the tunnel's own address on the WireGuard interface.
const SERVER_HOST: u128 = 1;

/// The tunnel's own IPv6 address, carved from the base of its configured prefix.
/// Doubles as the IPv6 gateway/DNS address advertised to clients.
pub fn server_addr(prefix: Ipv6Net) -> Ipv6Addr {
    Ipv6Addr::from(base(prefix) | SERVER_HOST)
}

/// The IPv6 delegated to one client: its primary global address and the network
/// routed to it over WireGuard — a `/64` under real prefix delegation, otherwise
/// the single `/128`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ClientV6 {
    pub addr: Ipv6Addr,
    pub delegated: Ipv6Net,
}

/// Allocate the IPv6 for the `index`-th client (enumeration order across the
/// whole server) whose tunnel IPv4 is `client_v4`. `None` when the prefix is too
/// small to fit another client. `index` is only consulted when the host space is
/// too small to embed the IPv4 (a per-client `/64`, or a longer-than-/64 prefix);
/// for a shared `/64` the address derives from `client_v4` alone, so it is stable
/// across additions and removals.
pub fn client_v6(prefix: Ipv6Net, index: u32, client_v4: Ipv4Addr) -> Option<ClientV6> {
    let plen = prefix.prefix_len();
    let base = base(prefix);
    let v4 = u32::from(client_v4) as u128;

    if plen < 64 {
        // Room for whole /64s: delegate one per client (index 0 reserved for the
        // tunnel's own /64). `sixty_fours` is 2^(64 - plen).
        let sixty_fours = 1u128 << (64 - plen);
        let slot = index as u128 + 1;
        if slot >= sixty_fours {
            return None;
        }
        let net_base = base | (slot << 64);
        let delegated = Ipv6Net::new(Ipv6Addr::from(net_base), 64).ok()?;
        Some(ClientV6 {
            addr: Ipv6Addr::from(net_base | 1),
            delegated,
        })
    } else if plen == 64 {
        // One shared /64: host bits are the client's IPv4, giving a stable,
        // collision-free /128 (tunnel v4s are 10.x, never the reserved host 1).
        let addr = Ipv6Addr::from(base | v4);
        Some(ClientV6 {
            addr,
            delegated: Ipv6Net::new(addr, 128).ok()?,
        })
    } else {
        // Longer than a /64: only `host_bits` of space. Index within it, skipping
        // the reserved host `1`.
        let host_bits = 128 - plen as u32;
        let capacity = 1u128 << host_bits;
        let host = index as u128 + 2;
        if host >= capacity {
            return None;
        }
        let addr = Ipv6Addr::from(base | host);
        Some(ClientV6 {
            addr,
            delegated: Ipv6Net::new(addr, 128).ok()?,
        })
    }
}

fn base(prefix: Ipv6Net) -> u128 {
    u128::from(prefix.network())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(s: &str) -> Ipv6Net {
        s.parse().unwrap()
    }

    #[test]
    fn server_addr_is_prefix_base_plus_one() {
        assert_eq!(
            server_addr(net("2001:db8:abcd::/64")),
            "2001:db8:abcd::1".parse::<Ipv6Addr>().unwrap()
        );
        assert_eq!(
            server_addr(net("2001:db8:1200::/56")),
            "2001:db8:1200::1".parse::<Ipv6Addr>().unwrap()
        );
    }

    #[test]
    fn shared_64_embeds_the_client_ipv4() {
        let p = net("2001:db8:abcd::/64");
        let v4: Ipv4Addr = "10.59.7.2".parse().unwrap();
        let c = client_v6(p, 0, v4).unwrap();
        // low 32 bits == the IPv4; delegated is the /128 itself.
        let expected = "2001:db8:abcd::a3b:702".parse::<Ipv6Addr>().unwrap();
        assert_eq!(c.addr, expected);
        assert_eq!(c.delegated, Ipv6Net::new(expected, 128).unwrap());
        // Stable regardless of index (index ignored for a shared /64).
        assert_eq!(client_v6(p, 99, v4).unwrap().addr, expected);
    }

    #[test]
    fn shared_64_distinct_clients_do_not_collide() {
        let p = net("2001:db8:abcd::/64");
        let a = client_v6(p, 0, "10.59.0.2".parse().unwrap()).unwrap().addr;
        let b = client_v6(p, 1, "10.59.0.3".parse().unwrap()).unwrap().addr;
        let c = client_v6(p, 2, "10.59.1.2".parse().unwrap()).unwrap().addr;
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
        // None land on the reserved server host.
        for addr in [a, b, c] {
            assert_ne!(addr, server_addr(p));
        }
    }

    #[test]
    fn short_prefix_delegates_a_64_per_client() {
        let p = net("2001:db8:1200::/56");
        let v4: Ipv4Addr = "10.59.0.2".parse().unwrap();
        let c0 = client_v6(p, 0, v4).unwrap();
        let c1 = client_v6(p, 1, v4).unwrap();
        // Client 0 gets /64 index 1 (index 0 reserved for the tunnel).
        assert_eq!(c0.delegated, net("2001:db8:1200:1::/64"));
        assert_eq!(c0.addr, "2001:db8:1200:1::1".parse::<Ipv6Addr>().unwrap());
        assert_eq!(c1.delegated, net("2001:db8:1200:2::/64"));
        // The delegated /64 never overlaps the tunnel's own /64 (index 0).
        assert_ne!(c0.delegated.network(), p.network());
    }

    #[test]
    fn short_prefix_exhausts_gracefully() {
        // A /63 has exactly two /64s: index 0 reserved, so only one client fits.
        let p = net("2001:db8:1200::/63");
        assert!(client_v6(p, 0, "10.59.0.2".parse().unwrap()).is_some());
        assert!(client_v6(p, 1, "10.59.0.3".parse().unwrap()).is_none());
    }

    #[test]
    fn narrow_prefix_indexes_within_host_space() {
        // A /124 (DigitalOcean) has 16 addresses: host 0 network, host 1 tunnel,
        // clients start at host 2.
        let p = net("2001:db8:abcd:1::/124");
        let c0 = client_v6(p, 0, "10.59.0.2".parse().unwrap()).unwrap();
        assert_eq!(c0.addr, "2001:db8:abcd:1::2".parse::<Ipv6Addr>().unwrap());
        assert_eq!(c0.delegated.prefix_len(), 128);
        // Hosts 2..=15 -> 14 clients fit; the 15th does not.
        assert!(client_v6(p, 13, "10.59.0.3".parse().unwrap()).is_some());
        assert!(client_v6(p, 14, "10.59.0.4".parse().unwrap()).is_none());
    }
}
