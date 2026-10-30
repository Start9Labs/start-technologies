#!/bin/sh

# packet-steering.uc pins only NAPI threads, and the K1's Ethernet drivers start without them.
threaded=1
[ "$1" = 0 ] && threaded=0
for dev in /sys/class/net/*; do
	[ -e "$dev/device" ] && [ ! -e "$dev/phy80211" ] || continue
	echo "$threaded" > "$dev/threaded" 2>/dev/null
done

steering_flows="$(uci -q get network.@globals[0].steering_flows)"
[ "${steering_flows:-0}" -gt 0 ] && set -- -l "$steering_flows" "$@"
exec /usr/libexec/network/packet-steering.uc "$@"
