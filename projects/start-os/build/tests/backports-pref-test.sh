#!/bin/bash

set -euo pipefail

ROOT=$(realpath "$(dirname "${BASH_SOURCE[0]}")/../../../..")
RECIPE="$ROOT/projects/start-os/build/image-recipe/build.sh"
TMP=$(mktemp -d)
trap 'rm -rf -- "$TMP"' EXIT
mkdir -p "$TMP/config/archives"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

POLICY="$TMP/policy.sh"
if ! awk '
    /^if \[ "\$\{IB_TARGET_PLATFORM\}" = "raspberrypi" \]; then$/ { block=""; found=1 }
    /^# Hooks$/ { if (found) { printf "%s", block; done=1 }; exit }
    { if (found) block=block $0 ORS }
    END { if (!done) exit 1 }
' "$RECIPE" > "$POLICY"; then
    fail 'could not isolate backports preferences generation'
fi

cd "$TMP"
for IB_SUITE in trixie bookworm; do
    export IB_SUITE
    cat > "$TMP/expected" <<EOF
Package: linux-image-* linux-headers-* linux-base *nvidia*
Pin: release n=${IB_SUITE}-backports
Pin-Priority: 500
EOF
    for IB_TARGET_PLATFORM in x86_64 x86_64-nonfree x86_64-nvidia aarch64 aarch64-nonfree aarch64-nvidia riscv64 riscv64-nonfree rockchip64; do
        export IB_TARGET_PLATFORM
        bash "$POLICY"
        if ! diff -u "$TMP/expected" config/archives/backports.pref; then
            fail "$IB_TARGET_PLATFORM/$IB_SUITE backports policy mismatch"
        fi
    done

    cat > "$TMP/expected" <<EOF
Package: *nvidia*
Pin: release n=${IB_SUITE}-backports
Pin-Priority: 500

Package: linux-image-* linux-headers-*
Pin: origin "deb.debian.org"
Pin-Priority: -1

Package: linux-image-* linux-headers-*
Pin: origin "security.debian.org"
Pin-Priority: -1

Package: linux-image-rpi-* linux-headers-rpi-*
Pin: version *
Pin-Priority: -1
EOF
    IB_TARGET_PLATFORM=raspberrypi bash "$POLICY"
    if ! diff -u "$TMP/expected" config/archives/backports.pref; then
        fail "raspberrypi/$IB_SUITE backports policy mismatch"
    fi
done

printf 'backports preferences tests passed\n'
