#!/usr/bin/env bash
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
shopt -s nullglob
if [ -n "${STARTWRT_CA_ROOTFS:-}" ]; then
    ROOTFS=("$STARTWRT_CA_ROOTFS")
else
    # A retained rootfs can belong to an earlier successful build.
    ROOTFS=("$ROOT"/projects/start-wrt/openwrt/build_dir/target-*/root-*)
fi
[ "${#ROOTFS[@]}" -eq 1 ] && [ -d "${ROOTFS[0]}" ] || {
    echo 'Build the StartWRT image or set STARTWRT_CA_ROOTFS to its unpacked root.' >&2
    exit 1
}
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

find "${ROOTFS[0]}/etc/ssl/certs" -type f -exec cat {} + > "$TMP/trust.pem"
openssl crl2pkcs7 -nocrl -certfile "$TMP/trust.pem" |
    openssl pkcs7 -print_certs -noout > "$TMP/subjects"

if grep -Eq '^subject=(.*, ?)?CN ?= ?emSign ECC Root CA - G3(,|$)' "$TMP/subjects"; then
    echo 'The image trust store still includes emSign ECC Root CA - G3.' >&2
    exit 1
fi
grep -Eq '^subject=(.*, ?)?CN ?= ?ISRG Root X1(,|$)' "$TMP/subjects" || {
    echo 'The image trust store is missing ISRG Root X1.' >&2
    exit 1
}
printf 'The image trust store excludes emSign ECC Root CA - G3 and keeps unrelated roots.\n'
