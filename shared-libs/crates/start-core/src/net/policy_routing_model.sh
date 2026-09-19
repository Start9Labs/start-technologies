#!/bin/bash
# Model of the StartOS policy-routing ladder, checked against the invariants in
# policy-routing.md. Runs inside `unshare -rn`; the priorities arrive in the
# environment from the constants in net/mod.rs.
#
#   eth0    LAN gateway, v4 + GUA + ULA        wg0  tunnel gateway, v4 + GUA
#   lxcbr0  container bridge, 10.0.3.5 pinned   wg1  tunnel gateway, v4 only

fail=0
ip link set lo up
sysctl -qw net.ipv4.ip_forward=1 net.ipv6.conf.all.forwarding=1
sysctl -qw net.ipv4.conf.all.rp_filter=0
for dev in eth0 wg0 wg1 lxcbr0; do
  ip link add $dev type dummy
  ip link set $dev up
  sysctl -qw net/ipv4/conf/$dev/rp_filter=2
done
for gateway in eth0 wg0 wg1; do
  sysctl -qw net/ipv4/conf/$gateway/src_valid_mark=1
done

table() { echo $((1000 + $(ip -o link show $1 | cut -d: -f1))); }
T_ETH=$(table eth0) T_WG0=$(table wg0) T_WG1=$(table wg1)
WG_FWMARK=51820 DIVERT_MARK=0x540001 DIVERT_TABLE=5344

ip addr add 192.168.1.5/24 dev eth0
ip -6 addr add 2001:db8:1::5/64 dev eth0 nodad
ip -6 addr add fd12:3456::5/64 dev eth0 nodad
ip addr add 10.59.0.2/24 dev wg0
ip -6 addr add 2001:db8:59::2/124 dev wg0 nodad
ip addr add 10.60.0.2/24 dev wg1
ip addr add 10.0.3.1/24 dev lxcbr0
ip -6 addr add fd00:3::1/64 dev lxcbr0 nodad

ip route add default via 192.168.1.1 dev eth0
ip -6 route add default via 2001:db8:1::1 dev eth0
ip route add default via 192.168.1.1 dev eth0 table $T_ETH
ip -6 route add default via 2001:db8:1::1 dev eth0 table $T_ETH
ip route add default dev wg0 scope link table $T_WG0
ip -6 route add default dev wg0 table $T_WG0
ip route add default dev wg1 scope link table $T_WG1
ip -6 route add blackhole default table $T_WG1
ip route add local default dev lo table $DIVERT_TABLE
ip -6 route add local default dev lo table $DIVERT_TABLE

for family in -4 -6; do
  ip $family rule add fwmark $DIVERT_MARK lookup $DIVERT_TABLE priority $DIVERT
  ip $family rule add lookup main suppress_prefixlength 0 priority $MAIN
  for t in $T_ETH $T_WG0 $T_WG1; do
    ip $family rule add fwmark $t lookup $t priority $REPLY
  done
  ip $family rule add fwmark $LOCAL_OUTBOUND_REJECT_MARK unreachable priority $LOCAL_OUTBOUND_REJECT
  ip $family rule add lookup main priority $AUTO_MAIN
  ip $family rule add lookup default priority $AUTO_DEFAULT
done
ip -6 rule add fwmark $T_WG0 to 2000::/3 lookup $T_WG0 priority $TUNNEL_REPLY
ip rule add from 192.168.1.5 fwmark 0/0xffffffff lookup $T_ETH priority $SOURCE
ip rule add from 10.59.0.2 fwmark 0/0xffffffff lookup $T_WG0 priority $SOURCE
ip rule add from 10.60.0.2 fwmark 0/0xffffffff lookup $T_WG1 priority $SOURCE
ip -6 rule add from 2001:db8:1::5 fwmark 0/0xffffffff lookup $T_ETH priority $SOURCE
ip -6 rule add from fd12:3456::5 fwmark 0/0xffffffff lookup $T_ETH priority $SOURCE
ip -6 rule add from 2001:db8:59::2 fwmark 0/0xffffffff lookup $T_WG0 priority $SOURCE

# pin_system <table>: an empty table id models a gateway whose interface is gone.
pin_system() {
  for family in -4 -6; do
    ip $family rule add fwmark $WG_FWMARK lookup main priority $WG_ENCAP
    [ -n "$1" ] && ip $family rule add lookup $1 priority $DEFAULT_OUTBOUND
    ip $family rule add unreachable priority $DEFAULT_OUTBOUND_REJECT
  done
}
unpin_system() {
  for family in -4 -6; do
    ip $family rule del priority $WG_ENCAP
    ip $family rule del priority $DEFAULT_OUTBOUND 2>/dev/null
    ip $family rule del priority $DEFAULT_OUTBOUND_REJECT
  done
}
pin_service() {
  ip rule add from 10.0.3.5 lookup $1 priority $SERVICE_OUTBOUND
  ip rule add from 10.0.3.5 unreachable priority $SERVICE_OUTBOUND_REJECT
  ip -6 rule add from fd00:3::5 lookup $1 priority $SERVICE_OUTBOUND
  ip -6 rule add from fd00:3::5 unreachable priority $SERVICE_OUTBOUND_REJECT
}
unpin_service() {
  for family in -4 -6; do
    ip $family rule del priority $SERVICE_OUTBOUND
    ip $family rule del priority $SERVICE_OUTBOUND_REJECT
  done
}

# expect <invariant> <device | LOCAL | REJECT> <ip route get arguments>
expect() {
  local invariant=$1 want=$2 out got
  shift 2
  out=$(ip route get "$@" 2>&1)
  if grep -q '^local ' <<<"$out"; then
    got=LOCAL
  elif grep -q ' dev ' <<<"$out"; then
    got=$(grep -o 'dev [a-z0-9]*' <<<"$out" | head -1 | cut -d' ' -f2)
  else
    got=REJECT
  fi
  if [ "$got" != "$want" ]; then
    echo "$state: $invariant: ip route get $*: want $want, got $got ($out)"
    fail=1
  fi
}

from_container="from 10.0.3.5 iif lxcbr0"
from_container_v6="from fd00:3::5 iif lxcbr0"

every_state() {
  expect divert LOCAL 203.0.113.9 mark $DIVERT_MARK $from_container
  expect divert LOCAL 192.168.1.77 mark $DIVERT_MARK $from_container
  expect specific eth0 192.168.1.77
  expect specific eth0 192.168.1.77 $from_container
  expect specific lxcbr0 10.0.3.5 from 10.59.0.2
  expect specific lxcbr0 fd00:3::5 from 2001:db8:59::2
  expect specific lxcbr0 10.0.3.5 mark $T_WG0 from 203.0.113.9 iif wg0
  expect specific lxcbr0 fd00:3::5 mark $T_WG0 from 2001:db8:99::9 iif wg0
  expect reverse-path lxcbr0 10.0.3.5 mark $T_ETH from 203.0.113.9 iif eth0
  expect tunnel-reply wg0 2001:db8:1::77 mark $T_WG0 $from_container_v6
  expect reply-forwarded eth0 203.0.113.9 mark $T_ETH $from_container
  expect reply-forwarded wg0 203.0.113.9 mark $T_WG0 $from_container
  expect reply-marked eth0 203.0.113.9 mark $T_ETH
  expect reply-unmarked eth0 203.0.113.9 from 192.168.1.5
  expect reply-unmarked wg0 203.0.113.9 from 10.59.0.2
  expect reply-unmarked eth0 2001:db8:99::9 from 2001:db8:1::5
  expect reply-unmarked eth0 fd99::9 from fd12:3456::5
  expect reply-unmarked wg0 2001:db8:99::9 from 2001:db8:59::2
}

state="auto"
every_state
expect auto eth0 203.0.113.9
expect auto eth0 2001:db8:99::9
expect auto eth0 203.0.113.9 $from_container

state="system pinned to wg0"
pin_system $T_WG0
every_state
expect system-selection wg0 203.0.113.9
expect system-selection wg0 2001:db8:99::9
expect system-selection wg0 203.0.113.9 $from_container
expect system-selection wg0 203.0.113.9 from 192.168.1.5 mark $T_WG0
expect system-selection wg0 2001:db8:99::9 from 2001:db8:1::5 mark $T_WG0
expect wg-transport eth0 198.51.100.1 mark $WG_FWMARK
unpin_system

state="system pinned, gateway gone"
pin_system ""
every_state
expect system-kill-switch REJECT 203.0.113.9
expect system-kill-switch REJECT 2001:db8:99::9
expect system-kill-switch REJECT 203.0.113.9 $from_container
expect system-kill-switch REJECT 203.0.113.9 from 192.168.1.5 mark $LOCAL_OUTBOUND_REJECT_MARK
expect system-kill-switch REJECT 2001:db8:99::9 from 2001:db8:1::5 mark $LOCAL_OUTBOUND_REJECT_MARK
expect system-kill-switch REJECT 2001:db8:99::9 from 2001:db8:1::5 mark 0x540003
expect wg-transport eth0 198.51.100.1 mark $WG_FWMARK
unpin_system

state="system pinned to wg1, which carries no v6"
pin_system $T_WG1
every_state
expect system-selection wg1 203.0.113.9
expect v6-leak-guard REJECT 2001:db8:99::9
expect v6-leak-guard REJECT 2001:db8:99::9 $from_container_v6
expect v6-leak-guard REJECT 2001:db8:99::9 from 2001:db8:1::5 mark $LOCAL_OUTBOUND_REJECT_MARK
unpin_system

state="service pinned to wg0"
pin_service $T_WG0
every_state
expect service-selection wg0 203.0.113.9 $from_container
expect service-selection wg0 2001:db8:99::9 $from_container_v6
expect auto eth0 203.0.113.9
unpin_service

state="service pinned, gateway gone"
pin_service 1999
every_state
expect service-kill-switch REJECT 203.0.113.9 $from_container
expect service-kill-switch REJECT 2001:db8:99::9 $from_container_v6
expect auto eth0 203.0.113.9
unpin_service

state="service pinned, gateway gone, system pinned to wg0"
pin_service 1999
pin_system $T_WG0
every_state
expect service-kill-switch REJECT 203.0.113.9 $from_container
expect service-kill-switch REJECT 2001:db8:99::9 $from_container_v6
expect system-selection wg0 203.0.113.9
unpin_service
unpin_system

state="service pinned to wg1, system pinned to wg0"
pin_service $T_WG1
pin_system $T_WG0
every_state
expect service-over-system wg1 203.0.113.9 $from_container
expect system-selection wg0 203.0.113.9
unpin_service
unpin_system

exit $fail
