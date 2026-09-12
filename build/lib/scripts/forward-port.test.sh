#!/bin/bash

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

capture=$(mktemp)
tagged_calls=$(mktemp)
list_calls=$(mktemp)
trap 'rm -f "$capture" "$tagged_calls" "$list_calls"' EXIT

nft() {
    local payload call
    if [ "${1:-}" = "-f" ]; then
        payload=$(cat)
        printf '%s\n' "$payload" >> "$capture"
        if grep -q '^add table ' <<< "$payload"; then
            return "${NFT_BASE_STATUS:-0}"
        fi

        call=$(($(cat "$tagged_calls") + 1))
        printf '%s\n' "$call" > "$tagged_calls"
        if [ "$call" -le "${NFT_STALE_FAILURES:-0}" ]; then
            printf '%s\n' "${NFT_STALE_STDERR:-Error: Could not process rule: No such file or directory}" >&2
            return "${NFT_STALE_STATUS:-2}"
        fi
        if [ "${NFT_TRANSACTION_STATUS:-0}" -ne 0 ]; then
            printf '%s\n' "${NFT_TRANSACTION_STDERR:-Error: synthetic transaction failure}" >&2
            return "$NFT_TRANSACTION_STATUS"
        fi
    elif [ "${1:-}" = "-a" ]; then
        call=$(($(cat "$list_calls") + 1))
        printf '%s\n' "$call" > "$list_calls"
        if [ "${NFT_LIST_STATUS:-0}" -ne 0 ]; then
            printf '%s\n' "${NFT_LIST_STDERR:-Error: synthetic list failure}" >&2
            return "$NFT_LIST_STATUS"
        fi
        if [ "${NFT_LISTING_HANDLES:-0}" = 1 ]; then
            printf 'meta l4proto tcp comment "%s" # handle %s\n' "$TAG" "$call"
        else
            printf '%s\n' "${NFT_LISTING:-}"
        fi
    fi
}
export capture tagged_calls list_calls
export -f nft

render() {
    local family="$1"
    shift
    local script sip dip dprefix
    case "$family" in
        ip)
            script=./build/lib/scripts/forward-port
            sip=192.0.2.10
            dip=10.0.3.2
            dprefix=24
            ;;
        ip6)
            script=./build/lib/scripts/forward-port6
            sip=2001:db8::10
            dip=fd00:3::2
            dprefix=64
            ;;
        *) return 64 ;;
    esac

    : > "$capture"
    printf '0\n' > "$tagged_calls"
    printf '0\n' > "$list_calls"
    local status=0
    env \
        -u UNDO \
        -u src_subnet \
        -u bridge_subnet \
        -u count \
        -u NFT_BASE_STATUS \
        -u NFT_STALE_FAILURES \
        -u NFT_STALE_STATUS \
        -u NFT_STALE_STDERR \
        -u NFT_TRANSACTION_STATUS \
        -u NFT_TRANSACTION_STDERR \
        -u NFT_LIST_STATUS \
        -u NFT_LIST_STDERR \
        -u NFT_LISTING \
        -u NFT_LISTING_HANDLES \
        sip="$sip" \
        dip="$dip" \
        dprefix="$dprefix" \
        sport=4444 \
        dport=5555 \
        "$@" \
        "$script" || status=$?
    cat "$capture"
    return "$status"
}

render_rules() {
    render ip "$@"
}

render_rules6() {
    render ip6 "$@"
}

assert_contains() {
    if ! grep -Fq "$2" <<< "$1"; then
        printf 'Missing expected rule:\n%s\n\nRendered rules:\n%s\n' "$2" "$1" >&2
        return 1
    fi
}

assert_forward_accepts_are_dnat_bound() {
    local family="$1" rendered="$2" line count=0
    while IFS= read -r line; do
        count=$((count + 1))
        grep -Fq 'ct status dnat' <<< "$line"
        grep -Fq 'ct original ' <<< "$line"
        grep -Fq ' proto-dst ' <<< "$line"
    done < <(grep "^add rule $family startos forward .* accept " <<< "$rendered")
    test "$count" -gt 0
}

private=$(render_rules src_subnet=203.0.113.0/24)
assert_contains "$private" 'add rule ip startos prerouting ip saddr 203.0.113.0/24 ip daddr 192.0.2.10 meta l4proto { tcp, udp } th dport 4444 dnat to 10.0.3.2:5555'
assert_contains "$private" 'add rule ip startos prerouting ip saddr { 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 127.0.0.0/8, 169.254.0.0/16 } ip daddr 192.0.2.10 meta l4proto { tcp, udp } th dport 4444 dnat to 10.0.3.2:5555'
assert_contains "$private" 'add rule ip startos forward ct status dnat ct original ip daddr 192.0.2.10 meta l4proto { tcp, udp } ct original proto-dst 4444 ip daddr 10.0.3.2 th dport 5555 ct state new accept'
test "$(grep -c '^add rule ip startos prerouting ip ' <<< "$private")" -eq 2
assert_forward_accepts_are_dnat_bound ip "$private"

public=$(render_rules)
assert_contains "$public" 'add rule ip startos prerouting ip daddr 192.0.2.10 meta l4proto { tcp, udp } th dport 4444 dnat to 10.0.3.2:5555'
if grep -Fq 'prerouting ip saddr' <<< "$public"; then
    printf 'Public forward unexpectedly filters source addresses:\n%s\n' "$public" >&2
    exit 1
fi
test "$(grep -c '^add rule ip startos prerouting ip ' <<< "$public")" -eq 1
assert_forward_accepts_are_dnat_bound ip "$public"

range=$(render_rules sport=4400 dport=4400 count=3)
assert_contains "$range" 'th dport 4400-4402 dnat to 10.0.3.2'
assert_contains "$range" 'ct original proto-dst . th dport { 4400 . 4400, 4401 . 4401, 4402 . 4402 } ip daddr 10.0.3.2 ct state new accept'
assert_forward_accepts_are_dnat_bound ip "$range"

offset=$(render_rules sport=4400 dport=5500 count=3)
assert_contains "$offset" 'dnat to th dport map { 4400 : 10.0.3.2 . 5500, 4401 : 10.0.3.2 . 5501, 4402 : 10.0.3.2 . 5502 }'
assert_contains "$offset" 'ct original proto-dst . th dport { 4400 . 5500, 4401 . 5501, 4402 . 5502 } ip daddr 10.0.3.2 ct state new accept'
assert_forward_accepts_are_dnat_bound ip "$offset"

private6=$(render_rules6 src_subnet=2001:db8:1::/64 bridge_subnet=fd00:3::/64)
assert_contains "$private6" 'add rule ip6 startos prerouting ip6 saddr 2001:db8:1::/64 ip6 daddr 2001:db8::10 meta l4proto { tcp, udp } th dport 4444 dnat to [fd00:3::2]:5555'
assert_contains "$private6" 'add rule ip6 startos prerouting ip6 saddr fd00:3::/64 ip6 daddr 2001:db8::10 meta l4proto { tcp, udp } th dport 4444 dnat to [fd00:3::2]:5555'
assert_contains "$private6" 'add rule ip6 startos forward ct status dnat ct original ip6 daddr 2001:db8::10 meta l4proto { tcp, udp } ct original proto-dst 4444 ip6 daddr fd00:3::2 th dport 5555 ct state new accept'
assert_forward_accepts_are_dnat_bound ip6 "$private6"

public6=$(render_rules6)
assert_contains "$public6" 'add rule ip6 startos prerouting ip6 daddr 2001:db8::10 meta l4proto { tcp, udp } th dport 4444 dnat to [fd00:3::2]:5555'
if grep -Fq 'prerouting ip6 saddr' <<< "$public6"; then
    printf 'Public IPv6 forward unexpectedly filters source addresses:\n%s\n' "$public6" >&2
    exit 1
fi
assert_forward_accepts_are_dnat_bound ip6 "$public6"

for family in ip ip6; do
    retried=$(render "$family" NFT_STALE_FAILURES=1 NFT_LISTING_HANDLES=1)
    test "$(cat "$tagged_calls")" -eq 2
    test "$(cat "$list_calls")" -eq 8
    assert_contains "$retried" "delete rule $family startos prerouting handle 1"
    assert_contains "$retried" "delete rule $family startos prerouting handle 5"

    status=0
    render "$family" NFT_STALE_FAILURES=3 NFT_STALE_STATUS=25 > /dev/null 2>&1 || status=$?
    test "$status" -eq 25
    test "$(cat "$tagged_calls")" -eq 3
    test "$(cat "$list_calls")" -eq 12

    status=0
    render "$family" NFT_TRANSACTION_STATUS=23 > /dev/null 2>&1 || status=$?
    test "$status" -eq 23
    test "$(cat "$tagged_calls")" -eq 1

    status=0
    render "$family" NFT_LIST_STATUS=24 > /dev/null 2>&1 || status=$?
    test "$status" -eq 24
    test "$(cat "$tagged_calls")" -eq 0
    test "$(cat "$list_calls")" -eq 1

    status=0
    render "$family" NFT_BASE_STATUS=26 > /dev/null 2>&1 || status=$?
    test "$status" -eq 26
    test "$(cat "$tagged_calls")" -eq 0
    test "$(cat "$list_calls")" -eq 0
done
