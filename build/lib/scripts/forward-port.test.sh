#!/bin/bash

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

capture=$(mktemp)
trap 'rm -f "$capture"' EXIT

nft() {
    if [ "${1:-}" = "-f" ]; then
        cat >> "$capture"
        NFT_CALLS=$((NFT_CALLS + 1))
        if [ "$NFT_CALLS" -eq 2 ] && [ "${NFT_TRANSACTION_STATUS:-0}" -ne 0 ]; then
            return "$NFT_TRANSACTION_STATUS"
        fi
    elif [ "${1:-}" = "-a" ] && [ "${NFT_LIST_STATUS:-0}" -ne 0 ]; then
        return "$NFT_LIST_STATUS"
    fi
}
export capture
export -f nft

render_rules() {
    : > "$capture"
    local status=0
    env \
        -u UNDO \
        -u src_subnet \
        -u count \
        -u NFT_TRANSACTION_STATUS \
        -u NFT_LIST_STATUS \
        sip=192.0.2.10 \
        dip=10.0.3.2 \
        dprefix=24 \
        sport=4444 \
        dport=5555 \
        NFT_CALLS=0 \
        "$@" \
        ./build/lib/scripts/forward-port || status=$?
    cat "$capture"
    return "$status"
}

render_rules6() {
    : > "$capture"
    local status=0
    env \
        -u UNDO \
        -u src_subnet \
        -u NFT_TRANSACTION_STATUS \
        -u NFT_LIST_STATUS \
        sip=2001:db8::10 \
        dip=fd00:3::2 \
        dprefix=64 \
        sport=4444 \
        dport=5555 \
        NFT_CALLS=0 \
        "$@" \
        ./build/lib/scripts/forward-port6 || status=$?
    cat "$capture"
    return "$status"
}

assert_contains() {
    if ! grep -Fq "$2" <<< "$1"; then
        printf 'Missing expected rule:\n%s\n\nRendered rules:\n%s\n' "$2" "$1" >&2
        return 1
    fi
}

private=$(render_rules src_subnet=203.0.113.0/24)
assert_contains "$private" 'add rule ip startos prerouting ip saddr 203.0.113.0/24 ip daddr 192.0.2.10 meta l4proto { tcp, udp } th dport 4444 dnat to 10.0.3.2:5555'
assert_contains "$private" 'add rule ip startos prerouting ip saddr { 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 127.0.0.0/8, 169.254.0.0/16 } ip daddr 192.0.2.10 meta l4proto { tcp, udp } th dport 4444 dnat to 10.0.3.2:5555'
test "$(grep -c '^add rule ip startos prerouting ip ' <<< "$private")" -eq 2

public=$(render_rules)
assert_contains "$public" 'add rule ip startos prerouting ip daddr 192.0.2.10 meta l4proto { tcp, udp } th dport 4444 dnat to 10.0.3.2:5555'
if grep -Fq 'prerouting ip saddr' <<< "$public"; then
    printf 'Public forward unexpectedly filters source addresses:\n%s\n' "$public" >&2
    exit 1
fi
test "$(grep -c '^add rule ip startos prerouting ip ' <<< "$public")" -eq 1

status=0
render_rules NFT_TRANSACTION_STATUS=23 > /dev/null 2>&1 || status=$?
test "$status" -eq 23

status=0
render_rules NFT_LIST_STATUS=24 > /dev/null 2>&1 || status=$?
test "$status" -eq 24

status=0
render_rules6 NFT_TRANSACTION_STATUS=23 > /dev/null 2>&1 || status=$?
test "$status" -eq 23

status=0
render_rules6 NFT_LIST_STATUS=24 > /dev/null 2>&1 || status=$?
test "$status" -eq 24
