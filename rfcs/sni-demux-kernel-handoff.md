# SNI Demux Kernel Handoff (tc-eBPF)

Status: draft spec for implementation. Owner: @dr-bonez.

## Background

The SNI demux (`shared-libs/crates/start-core/src/tunnel/forward/sni.rs`) lets
multiple targets share one external TCP port, routed by the TLS ClientHello's
SNI. Because SNI only exists after the TCP handshake, the daemon must accept
the connection, sniff the hello, open the internal leg from the client's own
source address (`crate::net::transparent::transparent_connect`, IP_TRANSPARENT

- the fwmark-`0x540001`/table-1344 reply divert), replay the buffered bytes,
  and then splice both sockets in userspace (`copy_bidirectional`) for the
  connection's entire lifetime.

The original design intent was to use userspace only to _identify_ the flow,
then hand the established connection to the kernel. That handoff cannot be
assembled from conntrack/nftables alone: NAT binds on a flow's first packet
(the SYN), an established local socket cannot be converted into a forwarded
flow, and nftables exposes no TCP sequence translation (synproxy does this
internally but is fixed-function, bound at SYN time). This spec describes the
design that _does_ achieve it: after the demux decision, record the constant
seq/ack offsets between the two legs in a BPF map, attach tc(clsact) programs
that rewrite and forward packets between the legs entirely in-kernel, and
silently retire both sockets with `TCP_REPAIR`. The daemon leaves the datapath;
the connection becomes a flow-map entry with an idle timeout, which also
structurally eliminates the fd-exhaustion failure class (see PR #3449 for the
interim userspace fixes this supersedes for handed-off flows).

## Goals

- Zero userspace bytes and zero held fds for a demuxed connection after
  handoff; CPU/latency comparable to plain kernel DNAT forwards.
- Source-address preservation semantics identical to today: the backend sees
  the real client `(ip, port)`; the client sees the tunnel's external
  `(ip, port)`.
- The existing userspace splice remains as the universal fallback — per flow
  (handoff failed/ineligible) and per host (kernel lacks support, feature
  disabled).
- Flows survive a `start-tunneld` restart once handed off (pinned maps/progs).

## Non-goals

- QUIC/UDP demux (out of scope, as in the current demux).
- IPv6 (the demux is IPv4-only today; keep the flow key layout extensible).
- Handoff of vhost passthrough connections on StartOS (`net/vhost.rs` uses the
  same transparent-connect mechanism and could adopt this later; target the
  tunnel first).
- MSS renegotiation: MSS was fixed per leg at SYN time; rely on PMTUD (see
  Datapath).

## Architecture overview

Per demuxed connection, three phases:

1. **Userspace (unchanged):** accept, read ClientHello, select target,
   `transparent_connect`, replay buffered bytes.
2. **Handoff (new):** quiesce both sockets, measure their exact stream
   positions and TCP parameters, install translation entries in a BPF flow
   map, retire both sockets via repair-mode close. Bounded attempts; on
   failure the flow simply stays in phase 1 forever (current behavior).
3. **Kernel (new):** tc ingress programs on the WAN interface(s) and the wg
   interface translate and forward packets between the two legs. Userspace
   only runs a GC sweep.

Naming for the rest of this doc:

- **Leg A** — client `C` ↔ tunnel external `(T, P_ext)`; the accepted socket.
- **Leg B** — spoofed `C` ↔ backend `(D, P_d)`; the transparent socket.
  Note both legs share the same client `(ip, port)` — the spoofing is what
  makes the translation a pure header/seq rewrite with no real NAT table.

## Handoff protocol (userspace control plane)

All steps operate on the two connected sockets. `CAP_NET_ADMIN` is required
(`start-tunneld` runs as root).

1. **Eligibility gate.** Attempt handoff right after the ClientHello replay
   (`upstream.write_all(&buf)` in `handle_conn`) — the natural quiescent gap
   while the backend computes its ServerHello. Skip if the feature is
   disabled or the host failed capability detection at startup.
2. **Stop application I/O** on both sockets (do not read or write further).
3. **Drain send queues:** poll `ioctl(SIOCOUTQ)` on both sockets until 0
   (all sent bytes ACKed — unACKed bytes would be lost at retirement because
   nothing will retransmit them). Peer ACKs must still flow, so ingress is
   NOT blocked yet. Deadline: ~500 ms.
4. **Block the flow at tc:** insert both direction entries into the flow map
   in state `BLOCK` (matching packets are dropped at tc ingress, which runs
   before socket delivery — including before the `socket transparent 1`
   divert for leg-B replies). TCP retransmission absorbs the drop window.
5. **Check receive queues:** if `ioctl(SIOCINQ)` != 0 on either socket, bytes
   arrived (and were ACKed by the kernel) between steps 3 and 4. They must be
   forwarded by userspace: delete the map entries (unblock), read + write
   those bytes through, and go to 3. Bounded retries (e.g. 3); on exhaustion,
   unblock and stay in userspace permanently for this flow.
6. **Measure (repair mode):** with both queues empty, set
   `TCP_REPAIR = 1` on both sockets, then read:
   - `TCP_QUEUE_SEQ` with `TCP_REPAIR_QUEUE = TCP_RECV_QUEUE` and
     `TCP_SEND_QUEUE` on each socket → `a_rcv_nxt`, `a_snd_nxt`,
     `b_rcv_nxt`, `b_snd_nxt` (exact next-byte sequence numbers; queues are
     empty so these align across legs).
   - `TCP_INFO` on each socket → negotiated window-scale shifts for both
     directions of both legs (see Window translation for orientation).
7. **Install translation:** update both map entries with the computed offsets
   (below), state `XLATE`.
8. **Retire the sockets:** `close()` both fds while still in repair mode —
   the kernel destroys the sockets silently (no FIN, no RST). The conntrack
   entries for both legs remain and are kept fresh by the forwarded traffic
   (see Datapath). Both fds are freed; the daemon is out of the datapath.

Any error at any step: remove map entries (if inserted), take the sockets out
of repair mode, and resume the userspace splice. Handoff must be strictly
best-effort.

## Translation math

All arithmetic is `u32` modulo 2³². At measurement time define:

```
OFF_CD = b_snd_nxt - a_rcv_nxt   // client→backend stream shift
OFF_DC = a_snd_nxt - b_rcv_nxt   // backend→client stream shift
```

Because the queues were drained, every byte ACKed on one leg was forwarded on
the other, so both legs carry the identical byte stream at a constant shift.
The offsets are therefore valid for _all_ sequence numbers, including
retransmissions of pre-handoff data in either direction (they arrive, get
translated, and are handled as duplicates by the peer stacks).

**Direction client→backend** (packet arrives on a WAN iface, key
`{C.ip, C.port, T.ip, P_ext}`):

```
ip.daddr := D.ip          tcp.dport := P_d
tcp.seq  := seq + OFF_CD
tcp.ack  := ack - OFF_DC
SACK block edges (option kind 5): edge - OFF_DC   // same space as ack
window   := rescale (see below)
```

then recompute checksums incrementally (`bpf_l3_csum_replace` /
`bpf_l4_csum_replace`, pseudo-header flag for the address) and return
`TC_ACT_OK` — the packet continues into the stack and is **forwarded
normally** (routing, fragmentation/PMTUD, neighbor resolution all handled by
the kernel; see Datapath for why this is safe and required).

**Direction backend→client** (packet arrives on the wg iface, key
`{D.ip, P_d, C.ip, C.port}`):

```
ip.saddr := T.ip          tcp.sport := P_ext
tcp.seq  := seq + OFF_DC
tcp.ack  := ack - OFF_CD
SACK block edges: edge - OFF_CD
window   := rescale
```

then `bpf_fib_lookup` toward `C.ip`, write the returned MACs, and
`bpf_redirect` to the WAN interface. This direction must NOT go through the
stack: after the source rewrite the packet carries the host's own address as
source, and `fib_validate_source` drops locally-sourced packets arriving on a
non-loopback interface. Redirect bypasses that. MTU is safe in this direction
(wg MTU < WAN MTU, so nothing arriving on wg can exceed the WAN egress MTU).

**Non-matching packets** (map miss): return `TC_ACT_UNSPEC` untouched. The
program must be a strict no-op for all other traffic.

### Window translation

The raw window field is interpreted by the receiver using the scale shift
negotiated on _its_ leg. The shifts can differ between legs (the client and
the demux's sockets negotiate independently), so translate:

```
win' = clamp( (win << ws_sender_leg) >> ws_receiver_leg, 0, 0xFFFF )
```

where `ws_sender_leg` is the shift the packet's window is currently encoded
with and `ws_receiver_leg` is the shift the new recipient will apply. Obtain
all four shifts from `TCP_INFO` (`tcpi_snd_wscale` / `tcpi_rcv_wscale`) on
both sockets during step 6. **Implementation note:** verify the orientation
of `snd_wscale`/`rcv_wscale` (which side advertised which) with a targeted
unit probe before trusting it; the requirement is: the effective (unscaled)
window a peer computes after handoff must equal the effective window the
sender intended. Store the two per-direction net shifts (signed) in the map.
Right-shifting when the net shift is negative loses precision toward a
smaller window, which is the safe direction.

### Timestamps (PAWS) — phased

Each leg negotiated TCP timestamps independently, with different TSval clocks.
Passing a client TSval through to the backend can violate PAWS monotonicity
(the backend already saw the demux socket's TSvals on leg B).

- **Phase 1 (required for v1 correctness): disable timestamp negotiation on
  the demux's sockets** so handed-off flows carry no TS option at all. There
  is no per-socket knob for the option in the SYN, so set
  `net.ipv4.tcp_timestamps = 0` host-wide on the tunnel appliance (shipped as
  sysctl config with the `.deb`, documented as a tradeoff — the tunnel is a
  dedicated appliance; its own SSH/API connections lose PAWS/RTT precision,
  which is acceptable at its rates). SACK must still be translated (do not
  disable SACK; it matters for loss recovery on lossy client paths).
- **Phase 2 (restores timestamps): translate TSval/TSecr in the BPF program**
  with per-direction constant offsets chosen at handoff so that the first
  post-handoff TSval in each direction continues monotonically from the last
  pre-handoff TSval sent on that leg (record last-sent TSvals during the
  freeze window; `TCP_TIMESTAMP` getsockopt in repair mode yields each
  socket's offset from the host clock). TSecr is translated with the inverse
  offset of the opposite direction. Both fields live in the same bounded
  option-parsing loop as SACK.

## eBPF program & maps

- Hook: `clsact` ingress on every WAN interface that carries demux listeners
  (from the gateway/ip-info the daemon already tracks) and on
  `wg-start-tunnel`. One program, direction determined by map lookup.
- Option parsing: single bounded loop over the TCP options (≤ 40 bytes),
  handling kind 5 (SACK) and kind 8 (timestamps, phase 2). Everything else is
  passed through untouched.
- Map `sni_flows`: `BPF_MAP_TYPE_LRU_HASH`, key
  `{ saddr: u32, daddr: u32, sport: u16, dport: u16 }` (program checks
  `ip->protocol == TCP` before lookup), value:

  ```
  state        u8   // BLOCK | XLATE
  win_shift    i8   // net window shift for this direction
  nat_addr     u32  // replacement address (dst for c→d, src for d→c)
  nat_port     u16  // replacement port
  off_seq      u32  // added to seq
  off_ack      u32  // subtracted from ack / SACK edges
  redirect_if  u32  // 0 = TC_ACT_OK into stack; else bpf_redirect target
  last_seen    u64  // bpf_ktime_get_ns, updated per packet
  fin_rst      u8   // bit 0: FIN seen this dir; bit 1: RST seen
  ts_off_*          // phase 2
  ```

  Two entries per flow (one per direction). Size the map for ~64k entries
  (32k flows); LRU eviction is the backstop against GC bugs — an evicted
  live flow degrades to both peers timing out, never to misrouting.

- Pinning: pin the maps and links under `/sys/fs/bpf/start-tunnel/` so
  handed-off flows survive daemon restarts. On startup the daemon adopts
  existing pins (verifying program version/struct layout via a versioned pin
  name; on mismatch, tear down and re-create — in-flight handed-off flows die
  with a one-time warning).

## Datapath integration (verified against the current prod ruleset)

- **tc ingress runs before netfilter prerouting**, so conntrack only ever
  sees the rewritten packets:
  - client→backend rewritten packets carry leg B's exact origin tuple
    (`C → D:P_d`, source-spoofed leg) and are associated with leg B's
    existing conntrack entry as ESTABLISHED — the `forward` chain's
    `ct state established,related accept` admits them through the
    default-drop policy. No new nft rules are needed.
  - backend→client packets bypass the stack entirely (`bpf_redirect`), which
    leaves leg A's conntrack entry seeing only the origin direction; the
    entry is already assured/established, so this is harmless. Both entries
    expire naturally after the flow ends.
- **Reply divert:** while the sockets are alive, leg-B replies are diverted
  into the transparent socket by the `socket transparent 1` mangle rule. The
  `BLOCK` state must therefore be installed at tc (earlier in the path)
  before queue-drain checks, and after retirement the socket-match simply no
  longer fires. No changes to `ensure_divert_infra`.
- **Masquerade/postrouting:** the existing rules
  (`ip saddr 10.59.88.0/24 oifname eth* masquerade`, per-forward hairpin
  rules) do not match either rewritten tuple. Confirm in the netns test.
- **PMTUD:** client→backend segments can exceed the wg MTU (leg A negotiated
  its MSS against the WAN MTU). Because this direction traverses the normal
  forwarding path, the kernel emits ICMP fragmentation-needed for DF packets
  and the client's PMTUD converges, same as for plain DNAT forwards (the
  existing `wg-mss-clamp` rule only touches SYNs and does not help
  post-handshake). Verify ICMP frag-needed egress is not blocked by nft.
- **rp_filter:** client→backend rewritten packets (saddr = client, arriving
  WAN, routed to wg) pass strict RPF. The reply direction never hits
  `fib_validate_source` (redirect). No sysctl loosening required.

## Flow lifecycle & GC

- The BPF program records FIN (per direction) and RST (either direction) in
  `fin_rst` and keeps stamping `last_seen`.
- A daemon GC task sweeps the map every ~30 s and deletes entry pairs where:
  RST seen, or FIN seen in both directions, with `last_seen` older than 60 s
  (2×MSL-ish linger for retransmitted FINs/ACKs); or `last_seen` older than a
  hard idle timeout (default 4 h — post-handoff there are no keepalives from
  the host, so pick generously; make it configurable).
- On daemon startup, adopt pinned maps and run an immediate sweep.

## Failure modes

| Failure                                                   | Behavior                                                                                        |
| --------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Kernel lacks clsact/BPF features, BPF load fails          | detected once at startup; feature off, log once; all flows userspace                            |
| Quiesce deadline exceeded / retries exhausted             | flow stays on the userspace splice (with PR #3449's keepalive protections)                      |
| Map full (LRU)                                            | eviction kills oldest flow (both peers RTO out); GC sizing makes this rare; count evictions     |
| Daemon crash mid-handoff (BLOCK installed, sockets alive) | sockets die with the process (kernel RSTs on close); stale BLOCK entries swept by GC on restart |
| Program/map version mismatch on restart                   | recreate; in-flight handed-off flows are lost (logged)                                          |

## Code layout & build

- New module `shared-libs/crates/start-core/src/tunnel/forward/handoff/`:
  `mod.rs` (control plane: quiesce/measure/install/retire + GC task),
  `bpf.rs` (load/attach/pin management), called from `handle_conn` in
  `sni.rs` behind the feature gate.
- **Everything `#[cfg(target_os = "linux")]`** with no-op stubs otherwise —
  CI builds `*-apple-darwin` (see `projects/start-tunnel/AGENTS.md`); follow
  the `transparent.rs` pattern.
- BPF toolchain: recommend **aya** (pure-Rust userspace loader; no libc/libbpf
  C dependency, which matters for the musl riscv64/aarch64 cross-builds) with
  the BPF program in C compiled by clang in `build.rs`, or aya-ebpf if the
  team accepts the bpf-linker/nightly build dependency. BPF bytecode is
  target-independent, so one embedded object serves all host architectures.
  Decision point for the implementer + maintainer; whichever is chosen must
  be added to the CI build image and `projects/start-tunnel/CONTRIBUTING.md`.
- Config: a tunnel db setting (`/settings/kernel-handoff`, default **off**
  for the first release; flip after soak on tunnel-chad), surfaced in CLI
  only (`start-tunnel settings ...`) — i18n for any user-facing strings, all
  5 locales.
- Runtime deps: none new at runtime (no iproute2/tc shellouts — attach via
  netlink from the loader library).
- `LimitNOFILE`/keepalive fixes from PR #3449 remain: they protect phase-1
  (pre-handoff) connections and permanent-userspace flows.

## Testing

Netns integration harness (root; follow the precedent noted in
`transparent.rs` — an `#[ignore]`d root-gated test or a script under
`build/`), with a client ns ↔ tunnel ns ↔ backend ns over veth + a real
wireguard link:

1. Bulk transfer integrity both directions across a handoff (checksummed
   payloads, ≥ 100 MB).
2. Handoff under packet loss (netem 2 % both legs): verifies SACK
   translation and pre/post-handoff retransmit correctness.
3. Differing window scales between legs (force via `TCP_WINDOW_CLAMP` /
   small `rmem` on one side): throughput sanity, no stalls.
4. PMTUD: WAN MTU 1500, wg MTU 1380; client sends full-size segments
   post-handoff; assert ICMP frag-needed and convergence.
5. Teardown: FIN-initiated by each side, RST, and idle-GC; assert map
   entries removed and conntrack entries expire.
6. Fd accounting: N handed-off flows → daemon fd count flat; kill -9 the
   daemon → flows keep passing traffic (pinned maps); restart adopts and GC
   works.
7. Quiesce-failure fallback: continuous-stream flow (never drains) stays
   userspace and still works.
8. Phase 2 only: timestamps on both legs, PAWS not violated under
   clock-offset skew between client and host.

## Success criteria

- Steady-state on tunnel-chad: 0 daemon fds held for handed-off flows;
  demux CPU ≈ 0 under load; no regression in SNI routing behavior;
  fallback rate (< a few % of flows, logged) and LRU evictions ≈ 0.

## Open questions

1. Adopt for `net/vhost.rs` passthrough on StartOS after tunnel soak?
2. Phase 2 timestamp translation vs. leaving `tcp_timestamps=0` permanently
   on the appliance — decide after measuring real-world impact.
3. Handoff retry for long-lived flows that failed initial quiesce
   (opportunistic re-attempt on idle detection) — v2 candidate.
