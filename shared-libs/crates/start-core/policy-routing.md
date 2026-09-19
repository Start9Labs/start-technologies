# Policy routing

StartOS decides where every packet leaves the server with a ladder of `ip rule` entries, a routing table per gateway, and a connection mark. This page states what that ladder must guarantee, derives its order from those guarantees, and records why each rule exists. The priorities are the constants in [`src/net/mod.rs`](src/net/mod.rs); [`src/net/policy_routing_model.sh`](src/net/policy_routing_model.sh) builds the ladder from them in a throwaway network namespace and checks every invariant below in every selection state.

**Change the ladder in this order:** state the invariant here, add an `expect` line for it to the model, then change the rule. A rule with no invariant behind it has no reason to keep its place, and the next fix will move it.

`cargo test -p start-core --features=test ladder_satisfies` runs the model. It skips where unprivileged user namespaces are unavailable, as in most containers, so run it natively.

## What the kernel knows when it routes

A rule can only match what the lookup carries: the input interface, the source address, the destination, and the packet mark. Three facts about _when_ those are known drive the whole design.

- **A forwarded packet is marked before it is routed.** `mangle_prerouting` runs first, so a container's reply already carries its connection's mark.
- **A locally generated reply is routed before it is marked.** The kernel routes a SYN-ACK, an ICMP reply, or a RST with mark 0 and only its source address known. `mangle_output` restores the mark afterwards and, being a `type route` chain, routes the packet a second time. If the first lookup fails, the packet is never built and there is nothing to route again.
- **A new local connection can acquire a source address during its first lookup.** That source can match the same rule an unmarked reply needs. `mangle_output` marks the still-unmarked packet for the selected gateway, or with the local rejection mark when that gateway cannot carry its family, and repeats the lookup.
- **The reverse-path filter routes too.** StartOS keeps Debian's default, `rp_filter=2` on every interface. For every inbound IPv4 packet the kernel looks up the way back, as a local packet from the packet's destination (after DNAT) to its source, and drops the packet if that lookup fails. It uses the packet's mark only where `src_valid_mark=1`. IPv6 has no such filter.

## Marks and tables

| Name              | Value                                   | Meaning                                                                                              |
| ----------------- | --------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| gateway table     | `1000 + ifindex`                        | One `default` route out that gateway. In IPv6, `blackhole default` when the gateway cannot carry v6. |
| connection mark   | the arrival gateway's table id          | Set on a connection's first packet by `mark-<iface>`, copied onto later packets by `restore-mark`.   |
| divert mark       | `0x00540001`, table `5344`              | A backend's reply to a transparent socket. The table delivers locally.                               |
| local reject mark | `0x00540002`                            | A new host connection whose selected gateway cannot carry its address family.                        |
| WireGuard fwmark  | whatever `wg show <iface> fwmark` reads | The encrypted transport packets of a tunnel.                                                         |

Bridges and loopback get no table and no mark; a gateway is any other managed interface. The divert table is shared with StartTunnel and StartWRT, and StartWRT numbers its own tables by VLAN tag, so it sits above 4094. `main` holds every specific route; a gateway table holds only a default.

## Invariants

Each name is the label the model prints when that invariant breaks.

### Delivery

- **`divert`** — A backend's reply to a source-preserving proxy socket is delivered to that socket, whatever its destination address. The destination is the client's address, which may be on-link, so this outranks every rule that routes by destination. (#3306, #3558)
- **`specific`** — A destination with a specific route in `main` uses it, whatever the packet's mark, source, or outbound selection. LAN hosts, containers, and VPN peers stay reachable under any selection, including an engaged kill switch, and inbound packets forwarded to a container reach `lxcbr0` even though they carry their gateway's mark. `suppress_prefixlength 0` is what limits this to specific routes. `main` must therefore hold no `throw` route and no reject-type default. (#3911, replacing the route mirror of #3117)

### Connection affinity

A connection that arrived through a gateway is answered through that gateway, and no outbound selection or rejection may interfere. Nothing on an inbound path is NATed, so the client's address survives and the reply has to find its own way back. (#3117, #3234)

- **`reply-forwarded`** — A container's reply, marked in prerouting, takes its gateway's table.
- **`reply-marked`** — A local reply, once `mangle_output` has restored its mark, takes its gateway's table. (#3307)
- **`reply-unmarked`** — A local reply's first, unmarked lookup succeeds through the gateway that owns its source address. Without this the lookup falls into whatever comes later in the ladder: into a `blackhole default` (#3632) or into a kill-switch rejection (#4006), and the SYN-ACK is never sent. Every address of a gateway but a link-local one has a source rule, in both families. The rule matches mark zero so the repeated lookup of a newly marked outbound connection cannot use it.
- **`reverse-path`** — The reverse-path lookup for an inbound packet succeeds through the gateway it arrived on. A packet forwarded to a container has the container's address as the source of that lookup, which an engaged kill switch rejects, so the lookup must carry the connection mark and use the reply rule. That takes two things: the first packet of a connection carries the mark, not only the conntrack entry, and each gateway interface sets `src_valid_mark=1`. It is set per interface because on `all` it would validate a diverted reply against its local-delivery route and drop it as a martian. (#4006)
- **`tunnel-reply`** — A reply to a global IPv6 destination that arrived through a WireGuard gateway returns through the tunnel even when `main` has an on-link route to the client. IPv6 has no NAT, so a client on the server's own LAN arrives with an on-link address; answered directly, the tunnel server's conntrack never sees the handshake complete and drops the rest. This is the one exception to `specific`, and it is confined three ways: mark-keyed, because a forwarded reply's source is the container's; `to 2000::/3`, because inbound packets carry the mark too and those bound for a container's ULA must still reach `main`; and WireGuard-only, because a LAN table's default would send an on-link reply through the router. (#3996)

### Outbound selection

- **`wg-transport`** — A tunnel's encrypted transport packets route by `main`, never by a selection and never into a rejection. Otherwise a selected tunnel carries its own transport, and a disconnected one can never reconnect. `restore-mark` leaves a packet that already has a mark alone, which keeps this fwmark intact. (#3117, #4006)
- **`service-selection`** — A service pinned to a gateway sends every destination `main` has no specific route for through that gateway, in both families. (#3101, #3936)
- **`service-kill-switch`** — When that gateway cannot carry the traffic — its table is empty, or its interface is gone — the traffic is rejected. It never falls through to the system-wide selection or to `main`. (#4003)
- **`service-over-system`** — A service's own selection wins over the system-wide one. (#3946)
- **`system-selection`**, **`system-kill-switch`** — The same two guarantees for the server itself and every service without a pin. A new host connection is marked in `mangle_output`, then its repeated lookup uses the selected table or the local rejection rule instead of a source-address reply rule. (#3101, #4003)
- **`v6-leak-guard`** — A selected gateway that is up but cannot carry IPv6 drops IPv6 rather than leaking it. Containers hit the `blackhole default` in its table; new host connections receive the local rejection mark. (#3388)
- **`auto`** — With nothing selected, `main` picks each family's default by metric. These rules sit above NetworkManager's own, because StartOS imports every WireGuard peer as `0.0.0.0/0, ::/0` and NetworkManager would otherwise route the whole server into the tunnel. (#3037, #3388)

## The ladder

Both families carry every rule unless noted.

| Priority | Rule                                          | Installed                                  | Serves                                     |
| -------- | --------------------------------------------- | ------------------------------------------ | ------------------------------------------ |
| 48       | `fwmark T to 2000::/3 lookup T` (IPv6)        | per WireGuard gateway that carries IPv6    | `tunnel-reply`                             |
| 49       | `fwmark 0x540001 lookup 5344`                 | always                                     | `divert`                                   |
| 50       | `lookup main suppress_prefixlength 0`         | always                                     | `specific`                                 |
| 51       | `fwmark T lookup T`                           | per gateway                                | replies, `reverse-path`                    |
| 52       | `fwmark 0x00540002 unreachable`               | always                                     | host `system-kill-switch`, `v6-leak-guard` |
| 60       | `from <address> fwmark 0 lookup T`            | per gateway address                        | `reply-unmarked`                           |
| 70       | `from <container> lookup T`                   | per address of a pinned service            | `service-selection`, `service-over-system` |
| 71       | `from <container> unreachable`                | with 70                                    | `service-kill-switch`                      |
| 74       | `fwmark <wg fwmark> lookup main`              | per tunnel, while a gateway is selected    | `wg-transport`                             |
| 75       | `lookup T`                                    | while the selected gateway's device exists | `system-selection`                         |
| 76       | `unreachable`                                 | while a gateway is selected                | `system-kill-switch`                       |
| 1000     | `lookup main`                                 | always                                     | `auto`                                     |
| 1100     | `lookup default`                              | always                                     | `auto`                                     |
| ~30000   | NetworkManager's per-tunnel full-tunnel rules | by NetworkManager                          | never reached                              |

### Why this order

Two rules need an order only when some packet can match both and they would send it different ways. A packet carries one mark — a gateway's, the divert mark, the local rejection mark, a tunnel's fwmark, or none — and one kind of source — a gateway's address, a container's, or neither — so most pairs never meet. The numbers above are one layout that satisfies the table below; `src/net/mod.rs` asserts exactly these relations at compile time and nothing else, and the model breaks when any one of them is reversed.

| Rule                 | Must precede           | Because                                                                                                                              |
| -------------------- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| 48 tunnel reply      | 50, 70–71, 75–76, 1000 | `tunnel-reply`: `main`'s on-link route, a selection, or Auto would each take the reply off the tunnel.                               |
| 49 divert            | 50, 70–71, 75–76, 1000 | `divert`: a diverted reply has a container's source and any destination, so every one of these matches it.                           |
| 50 main              | 51, 60, 70–71, 75–76   | `specific`: each of these looks up a table that holds only a default, or rejects.                                                    |
| 51 reply mark        | 70–71, 75–76, 1000     | Connection affinity: a container's marked reply also matches its service rules, and every marked reply matches the system-wide ones. |
| 52 local rejection   | 60, 75–76, 1000        | A host connection marked for rejection must not use its gateway source address or any later default.                                 |
| 60 source address    | 75–76, 1000            | `reply-unmarked`. It never meets 70–71: their sources are containers'.                                                               |
| 70 service lookup    | 71, 75–76, 1000        | `service-selection`, `service-over-system`.                                                                                          |
| 71 service rejection | 75, 1000               | `service-kill-switch`: a service whose gateway is gone must not fall to the system-wide selection or to `main`.                      |
| 74 WireGuard fwmark  | 75–76                  | `wg-transport`.                                                                                                                      |
| 75 system lookup     | 76, 1000               | `system-selection`.                                                                                                                  |
| 76 system rejection  | 1000                   | `system-kill-switch`.                                                                                                                |

Every other pair is free, and the model holds with each of them swapped:

- 48 and 49 carry different marks.
- 51, 52 and 60 require different marks.
- 60 and 70–71 match different sources.
- 74 matches the host's own packets carrying a tunnel's fwmark, so it never meets 48, 49, 51 or 70–71, and 50, 74 and 1000 all read `main`. Against 60, either answer keeps a tunnel's transport on the underlay.
- 1100 reads a table StartOS leaves empty, so it bears on nothing.

Nothing unqualified may share 49, since equal priorities evaluate in insertion order. Reconciliation also identifies some rules by priority alone, and would misread or delete a neighbour that shared one: 60 must differ from 48, 49, 51 and 70; 52 from 48, 49, 51 and 60; 51 from 49; and 74 from 48, 49 and 51. Those are asserted as inequalities, not as orders.

An unqualified rejection must never precede 51 or 60. The priority-52 rejection is safe there because its mark cannot occur on a reply. A terminal blackhole at priority 1200 was tried and removed in #3388 for killing replies to inbound tunnel connections; the kill switch in #4006 met the same two failures, `reply-unmarked` and `reverse-path`, before those two rules were completed.

### Installing and removing

A lookup is installed before the rejection that backs it and removed after it, so switching a selection on or off never leaves the rejection as the only match. A lookup that fails to install still gets its rejection. A selection that moves to another gateway adds the new lookup before deleting the old one: equal priorities evaluate in insertion order, so the old rule keeps routing until it is gone (#4005).

Tables are written with `ip route replace`, never flushed and refilled (#3133). Nothing is cleaned up when an interface's watcher stops; `gc_policy_routing` removes the rules and tables of interfaces that no longer exist, finding them through priorities 51 and 60 (#2867). A reply rule found at the wrong priority is deleted. Kernel state does not survive a reboot and every OS update reboots, so no migration of old rules is carried.

Each mangle chain is rebuilt in one transaction (#3298). `restore-mark` acts only on an unmarked packet. In `mangle_output`, a remaining unmarked packet receives the selected gateway's table id, or the local rejection mark when that gateway is disconnected or cannot carry the packet's family. The divert mark is set only in prerouting and only on a packet that matches a transparent socket; it never touches the connection mark or the output chain (#3306).

## Limits

- A reply sent from a UDP socket bound to the wildcard address routes like an unbound connection: it follows the selection, and an engaged kill switch rejects it. A server that must answer off-link clients over UDP sets its source with `IP_PKTINFO`.
- A selection change does not move or end established flows.
- An engaged kill switch rejects the server's own DNS queries unless the resolver has a specific route, as the LAN router does. Until the gateway returns, NetworkManager cannot resolve a tunnel endpoint given as a hostname through any other resolver.
