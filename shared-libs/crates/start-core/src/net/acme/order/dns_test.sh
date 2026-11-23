set -euo pipefail
exe=$1 test=$2
mount --make-rprivate /
mount -t tmpfs tmpfs /run
sysctl -qw net.ipv4.conf.all.rp_filter=0 net.ipv4.conf.default.rp_filter=0
ip netns add peer
trap 'ip netns pids peer | xargs -r kill; wait || true' EXIT
ip link set lo up
ip netns exec peer ip link set lo up
for i in 0 1; do
    ip link add gw$i type veth peer name peer$i
    ip link set peer$i netns peer
    ip addr add 10.$i.0.2/24 dev gw$i
    ip link set gw$i up
    ip netns exec peer ip addr add 10.$i.0.1/24 dev peer$i
    ip netns exec peer ip link set peer$i up
    table=$((1000 + $(ip -o link show gw$i | cut -d: -f1)))
    ip rule add priority 100 fwmark $table lookup $table
    ip route add table $table default via 10.$i.0.1 dev gw$i
done
ip netns exec peer ip addr add 10.64.0.1/32 dev lo
for host in 10 11; do
    ip netns exec peer ip addr add 192.0.2.$host/32 dev lo
done
STARTOS_ACME_DNS_RESPONDER=1 ip netns exec peer "$exe" --exact "$test" --nocapture &
sleep 0.5
STARTOS_ACME_DNS_NETNS=1 "$exe" --exact "$test" --nocapture
