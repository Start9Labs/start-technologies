#!/bin/bash
set -euo pipefail

binary=$1
test_name=$2
scratch=$(mktemp -d)
pids=()
cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then grep -h . "$scratch/"*.log 2>/dev/null || true; fi
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  wait || true
  rm -rf "$scratch"
}
trap cleanup EXIT
trap 'exit 143' TERM
mount -t tmpfs tmpfs /run
mkdir -p /run/netns
ip link set lo up
sysctl -qw net.ipv4.ip_forward=1 net.ipv4.conf.all.rp_filter=0 net.ipv6.conf.lo.disable_ipv6=1
for role in client backend; do
  ip netns add "$role"
  ip link add "${role}0" type veth peer name "${role}1"
  ip link set "${role}1" netns "$role"
  ip link set "${role}0" up
  ip netns exec "$role" ip link set lo up
  ip netns exec "$role" ip link set "${role}1" up
  sysctl -qw "net.ipv4.conf.${role}0.rp_filter=2" "net.ipv4.conf.${role}0.src_valid_mark=0"
done
ip addr add 192.0.2.1/24 dev client0
ip addr add 10.0.0.1/24 dev backend0
ip netns exec client ip addr add 192.0.2.2/24 dev client1
ip netns exec backend ip addr add 10.0.0.2/24 dev backend1
ip netns exec client ip route add default via 192.0.2.1
ip netns exec backend ip route add default via 10.0.0.1
nft add table ip startos
nft 'add chain ip startos mangle_prerouting { type filter hook prerouting priority -150; }'

mkfifo "$scratch/gateway-done"
for role in backend gateway; do
  mkfifo "$scratch/$role-ready"
  if [ "$role" = backend ]; then
    STARTOS_UDP_ROLE=$role STARTOS_UDP_READY="$scratch/$role-ready" \
      ip netns exec backend "$binary" --exact "$test_name" --nocapture >"$scratch/$role.log" 2>&1 &
  else
    STARTOS_UDP_ROLE=$role STARTOS_UDP_READY="$scratch/$role-ready" STARTOS_UDP_DONE="$scratch/gateway-done" \
      "$binary" --exact "$test_name" --nocapture >"$scratch/$role.log" 2>&1 &
  fi
  pids+=("$!")
  IFS= read -r ready <"$scratch/$role-ready"
  test "$ready" = ready
done
STARTOS_UDP_ROLE=client ip netns exec client "$binary" --exact "$test_name" --nocapture
printf done >"$scratch/gateway-done"
for pid in "${pids[@]}"; do
  if ! wait "$pid"; then
    grep -h . "$scratch/"*.log
    exit 1
  fi
done
pids=()
