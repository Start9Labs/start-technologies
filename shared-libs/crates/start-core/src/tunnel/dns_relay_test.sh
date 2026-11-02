set -euo pipefail
mount --make-rprivate /
mount -t tmpfs tmpfs /run
exe=$1
test=$2
trap 'kill $(jobs -p) 2>/dev/null; wait 2>/dev/null || true' EXIT
ip link set lo up
for peer in client device; do
    ip netns add "$peer"
    ip link add "to-$peer" type veth peer name wg0 netns "$peer"
    ip addr add 10.59.0.1/32 dev "to-$peer"
    ip link set "to-$peer" up
    ip netns exec "$peer" ip link set lo up
    ip netns exec "$peer" ip link set wg0 up
done
ip netns exec client ip addr add 10.59.0.3/32 dev wg0
ip netns exec device ip addr add 10.59.0.13/32 dev wg0
for peer in client device; do
    ip netns exec "$peer" ip route add 10.59.0.1/32 dev wg0
    ip netns exec "$peer" ip route add 10.59.0.0/24 via 10.59.0.1 dev wg0
done
ip route add 10.59.0.3/32 dev to-client
ip route add 10.59.0.13/32 dev to-device
ip netns exec device env "$ROLE_VAR=device" "$exe" --exact "$test" --nocapture &
env "$ROLE_VAR=proxy" "$exe" --exact "$test" --nocapture &
ip netns exec client env "$ROLE_VAR=client" "$exe" --exact "$test" --nocapture
ip netns exec device env "$ROLE_VAR=device-self" "$exe" --exact "$test" --nocapture
