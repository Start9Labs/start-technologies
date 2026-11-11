#!/bin/bash
set -euo pipefail

directory=$(cd "$(dirname "$0")/../dpkg-deps" && pwd)
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
mkdir "$work/dpkg-deps" "$work/lib"
cp "$directory/generate.sh" "$directory/depends" "$directory/conflicts" \
    "$directory/"*.depends "$work/dpkg-deps/"

for architecture in x86_64 aarch64 riscv64; do
    ENVIRONMENT= ARCH="$architecture" PLATFORM="$architecture" \
        bash "$work/dpkg-deps/generate.sh"
    for package in sbsigntool systemd-boot-efi systemd-ukify tpm2-tools; do
        if [[ $architecture == riscv64 ]]; then
            if grep -Fxq "$package" "$work/lib/depends"; then
                echo "Unexpected UEFI dependency on $architecture: $package" >&2
                exit 1
            fi
        else
            grep -Fxq "$package" "$work/lib/depends"
        fi
    done
    if [[ $architecture == x86_64 ]]; then
        grep -Fxq grub-pc-bin "$work/lib/depends"
    fi
done
printf 'PASS: measured-boot dependencies follow the supported UEFI architectures\n'
