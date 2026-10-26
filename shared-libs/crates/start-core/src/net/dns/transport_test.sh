set -euo pipefail
mount --make-rprivate /
mount -t tmpfs tmpfs /run
ip netns add peer
trap 'ip netns pids peer | xargs -r kill; wait || true' EXIT
ip link set lo up
ip link add eth0 type dummy
ip addr add 192.0.2.2/24 dev eth0
ip -6 addr add 2001:db8::2/64 dev eth0 nodad
ip link set eth0 up
ip route add default via 192.0.2.1
ip -6 route add default via 2001:db8::1
ip link add gw0 type veth peer name peer0
ip link set peer0 netns peer
ip addr add 198.51.100.2/24 dev gw0
ip -6 addr add fd00:100::2/64 dev gw0 nodad
ip -6 addr add fe80::2/64 dev gw0 nodad
ip link set gw0 up
ip netns exec peer ip link set lo up
ip netns exec peer ip addr add 198.51.100.1/24 dev peer0
ip netns exec peer ip addr add 192.0.2.53/32 dev peer0
ip netns exec peer ip -6 addr add fd00:100::1/64 dev peer0 nodad
ip netns exec peer ip -6 addr add fe80::53/64 dev peer0 nodad
ip netns exec peer ip -6 addr add 2001:db8::53/128 dev peer0 nodad
ip netns exec peer ip link set peer0 up
index=$(ip -o link show gw0 | cut -d: -f1)
table=$((1000 + index))
system_index=$(ip -o link show eth0 | cut -d: -f1)
system_table=$((1000 + system_index))
ip route add table "$system_table" default via 192.0.2.1 dev eth0
ip -6 route add table "$system_table" default via 2001:db8::1 dev eth0
for family in -4 -6; do
    ip "$family" rule add priority "$MAIN" lookup main suppress_prefixlength 0
    ip "$family" rule add priority "$REPLY" fwmark "$table" lookup "$table"
    ip "$family" rule add priority "$DEFAULT_OUTBOUND" lookup "$system_table"
    ip "$family" rule add priority "$DEFAULT_OUTBOUND_REJECT" unreachable
done
ip route add table "$table" default via 198.51.100.1 dev gw0
ip -6 route add table "$table" default via fd00:100::1 dev gw0
nft add table inet dns_test
nft 'add chain inet dns_test output { type route hook output priority mangle; }'
nft add rule inet dns_test output meta mark 0 ct state new meta mark set "$LOCAL_OUTBOUND_MARK"
ip netns exec peer socat UDP4-RECVFROM:5353,reuseaddr,fork EXEC:cat &
ip netns exec peer socat TCP4-LISTEN:5353,reuseaddr,fork EXEC:cat &
ip netns exec peer socat UDP6-RECVFROM:5353,ipv6only=1,reuseaddr,fork EXEC:cat &
ip netns exec peer socat TCP6-LISTEN:5353,ipv6only=1,reuseaddr,fork EXEC:cat &
for attempt in {1..100}; do
    if [ "$(ip netns exec peer ss -H -lnut 'sport = :5353' | wc -l)" -eq 4 ]; then
        break
    fi
    sleep 0.01
done
nft add rule inet dns_test output meta oifname eth0 udp dport 5353 counter
nft add rule inet dns_test output meta oifname eth0 tcp dport 5353 counter
export STARTOS_DNS_NETNS=1
export DNS_GATEWAY_INDEX="$index"
"$1" --exact net::dns::upstream::tests::gateway_dns_transport --nocapture
