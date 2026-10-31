#!/bin/bash
set -euo pipefail

ROOT=$(realpath "$(dirname "${BASH_SOURCE[0]}")/../../../..")
RECIPE="$ROOT/projects/start-os/build/image-recipe/build.sh"
TMP=$(mktemp -d)
trap 'rm -rf -- "$TMP"' EXIT

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

awk '
    /^PLATFORM_CONFIG_EXTRAS=\(\)$/ { found=1 }
    /^BOOTAPPEND_LIVE=/ { exit }
    found { print }
    END { if (!found) exit 1 }
' "$RECIPE" > "$TMP/platform.sh"

check() {
    local version=$1 expected=$2 status=0
    sed "s/RPI_KERNEL_VERSION=.*/RPI_KERNEL_VERSION='$version'/" "$TMP/platform.sh" > "$TMP/case.sh"
    IB_TARGET_PLATFORM=raspberrypi IB_TARGET_ARCH=arm64 bash -e "$TMP/case.sh" > "$TMP/output" 2>&1 || status=$?
    if [ "$expected" = reject ]; then
        [ "$status" -ne 0 ] || fail "$version reached image configuration"
        grep -q 'CVE-2026-98432' "$TMP/output" || fail "$version lacked a diagnostic"
    else
        [ "$status" -eq 0 ] || fail "$version rejected"
        IB_TARGET_PLATFORM=raspberrypi IB_TARGET_ARCH=arm64 bash -ec '
            source "$1"
            [ "${PLATFORM_CONFIG_EXTRAS[*]}" = "--firmware-binary false --firmware-chroot false --linux-packages linux-image-$2 --linux-flavours rpi-v8 rpi-2712" ]
        ' bash "$TMP/case.sh" "$version" || fail "$version changed Pi flavours or package selection"
    fi
}

for version in 6.18.33+rpt 6.18.50+rpt 6.18.55+rpt 6.18.5+rpt 6.18.56 6.18.59+rpt-rt 6.18.59+rpt1 6.12.113+rpt 7.2.11+rpt ''; do
    check "$version" reject
done
for version in 6.18.56+rpt 6.18.59+rpt 6.18.100+rpt; do
    check "$version" accept
done

for platform in x86_64 x86_64-nonfree x86_64-nvidia aarch64 aarch64-nonfree aarch64-nvidia riscv64 riscv64-nonfree rockchip64; do
    IB_TARGET_PLATFORM="$platform" IB_TARGET_ARCH="${platform%%-*}" bash -e "$TMP/platform.sh" || fail "$platform affected by Pi guard"
done

printf 'rpi kernel guard tests passed (13 version cases, 9 other platforms)\n'
