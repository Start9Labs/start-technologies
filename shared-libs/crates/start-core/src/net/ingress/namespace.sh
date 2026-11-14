set -euo pipefail
mount --make-rprivate /
mount -t tmpfs tmpfs /run
ip netns add ingress-peer
trap 'ip netns pids ingress-peer | xargs -r kill; wait || true' EXIT
ip link set lo up
ip link add ingress0 type veth peer name peer0
ip link set peer0 netns ingress-peer
ip link set ingress0 addrgenmode none
ip netns exec ingress-peer ip link set peer0 addrgenmode none
ip addr add 192.0.2.1/24 dev ingress0
ip -6 addr add 2001:db8:42::1/64 dev ingress0 nodad
ip -6 addr add fe80::1/64 dev ingress0 nodad
ip link set ingress0 up
ip link add backend0 type dummy
ip addr add "$OS_TARGET_V4/32" dev backend0
ip -6 addr add "$OS_TARGET_V6/128" dev backend0 nodad
ip link set backend0 up
ip netns exec ingress-peer ip link set lo up
ip netns exec ingress-peer ip addr add 192.0.2.2/24 dev peer0
ip netns exec ingress-peer ip -6 addr add 2001:db8:42::2/64 dev peer0 nodad
ip netns exec ingress-peer ip -6 addr add fe80::2/64 dev peer0 nodad
ip netns exec ingress-peer ip link set peer0 up
ip netns exec ingress-peer ip route add default via 192.0.2.1
ip netns exec ingress-peer ip -6 route add default via 2001:db8:42::1
nft add table inet ingress_test
nft 'add chain inet ingress_test prerouting { type nat hook prerouting priority dstnat; policy accept; }'
export STARTOS_INGRESS_NETNS=1
"$1" --exact net::ingress::tests::translated_ingress_keeps_original_destination_gateway_and_peer --nocapture
