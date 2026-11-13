#!/bin/bash
set -euo pipefail

directory=$(cd "$(dirname "$0")/../dpkg-deps" && pwd)
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
mkdir "$work/dpkg-deps" "$work/lib"
cp "$directory/generate.sh" "$directory/depends" "$directory/conflicts" \
    "$directory/"*.depends "$work/dpkg-deps/"

for target in x86_64:x86_64:yes x86_64:x86_64-nonfree:yes \
    x86_64:x86_64-nvidia:yes aarch64:aarch64:yes \
    aarch64:aarch64-nonfree:yes aarch64:aarch64-nvidia:yes \
    aarch64:raspberrypi:no aarch64:rockchip64:no \
    riscv64:riscv64:no riscv64:riscv64-nonfree:no; do
    IFS=: read -r architecture platform uefi <<< "$target"
    ENVIRONMENT= ARCH="$architecture" PLATFORM="$platform" \
        bash "$work/dpkg-deps/generate.sh"
    for package in sbsigntool systemd-boot-efi systemd-ukify tpm2-tools; do
        if [[ $uefi == yes ]]; then
            grep -Fxq "$package" "$work/lib/depends"
        elif grep -Fxq "$package" "$work/lib/depends"; then
            echo "Unexpected UEFI dependency on $platform: $package" >&2
            exit 1
        fi
    done
    if [[ $platform == x86_64 ]]; then
        grep -Fxq grub-pc-bin "$work/lib/depends"
    fi
done
printf 'PASS: measured-boot dependencies follow the supported UEFI platforms\n'
