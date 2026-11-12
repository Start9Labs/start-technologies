#!/bin/bash
set -euo pipefail

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
    printf 'Usage: %s <genuine-startbox> <installed-hook-functions> [TPM-fixture-image]\n' "$0" >&2
    exit 1
fi

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
startbox=$(realpath "$1")
hook_functions=$(realpath "$2")
image=$(docker image inspect --format '{{.Id}}' "${3:-startos-fde-tpm-test}")
fixture_root=/fde-tools-root
docker run --rm --pull=never -i --env "DESTDIR=$fixture_root" \
    --mount "type=bind,src=/dev/random,dst=$fixture_root/dev/random,readonly" \
    --mount "type=bind,src=/dev/urandom,dst=$fixture_root/dev/urandom,readonly" \
    --mount "type=bind,src=$startbox,dst=/usr/bin/startbox,readonly" \
    --mount "type=bind,src=$hook_functions,dst=/usr/share/initramfs-tools/hook-functions,readonly" \
    --mount "type=bind,src=$here/../lib/scripts/fde-initramfs-tools-hook,dst=/fde-tools-hook,readonly" \
    "$image" bash -se <<'SH'
set -euo pipefail
export LC_ALL=C
root="$DESTDIR"
mkdir -p "$root"/{usr/{bin,sbin,lib,lib64},etc}
for dir in bin sbin lib lib64; do
    ln -s "usr/$dir" "$root/$dir"
done
export verbose=n

before=$(sha256sum /usr/bin/startbox /usr/share/initramfs-tools/hook-functions)
test -z "$(sh /fde-tools-hook prereqs)"
sh /fde-tools-hook
ldconfig -r "$root"
test "$(sha256sum /usr/bin/startbox /usr/share/initramfs-tools/hook-functions)" = "$before"
cmp /usr/bin/startbox "$root/usr/bin/startbox"
cmp /usr/bin/tpm2 "$root/usr/bin/tpm2"
cmp /usr/sbin/veritysetup "$root/usr/sbin/veritysetup"
diff -u \
    <(dpkg-query -L "tpm2-tools:$(dpkg --print-architecture)" | grep '^/usr/bin/tpm2_' | sort) \
    <(find "$root/usr/bin" -name 'tpm2_*' -type l -printf '/usr/bin/%f\n' | sort)

status=0
chroot "$root" /usr/bin/startbox start-fde-prepare > /tmp/applet.stdout 2> /tmp/applet.stderr || status=$?
test "$status" = 1
grep -Fq 'Invalid Request' /tmp/applet.stderr
chroot "$root" /usr/sbin/veritysetup --version
dd if=/dev/zero of="$root/image" bs=4096 count=3 status=none
hash=$(veritysetup format "$root/image" "$root/hash" | awk '/^Root hash:/ { print $NF }')
test -n "$hash"
chroot "$root" /usr/sbin/veritysetup verify /image /hash "$hash"
printf x | dd of="$root/image" bs=1 conv=notrunc status=none
status=0
chroot "$root" /usr/sbin/veritysetup verify /image /hash "$hash" \
    > /tmp/tamper.stdout 2> /tmp/tamper.stderr || status=$?
test "$status" != 0
grep -Fq 'Verification of data area failed' /tmp/tamper.stderr
chroot "$root" /usr/bin/tpm2_unseal --version

status=0
chroot "$root" /usr/bin/tpm2_getcap -T device:/dev/fde-no-such-tpm properties-fixed \
    > /tmp/device.stdout 2> /tmp/device.stderr || status=$?
test "$status" != 0
grep -Fq 'Failed to open specified TCTI device file /dev/fde-no-such-tpm' /tmp/device.stderr

alias_target=$(readlink /usr/bin/tpm2_pcrread)
ln -sfn /bin/sh /usr/bin/tpm2_pcrread
partial=$(mktemp -d)
status=0
DESTDIR="$partial" sh /fde-tools-hook > /tmp/alias.stdout 2> /tmp/alias.stderr || status=$?
rm -rf "$partial"
test "$status" != 0
grep -Fxq 'Unsupported TPM2 alias: /usr/bin/tpm2_pcrread' /tmp/alias.stderr
ln -sfn "$alias_target" /usr/bin/tpm2_pcrread

provider=$(find "$root/usr/lib" -type f -name 'libtss2-tcti-device.so.*' -print)
test -n "$provider"
mv "$provider" "$provider.absent"
status=0
chroot "$root" /usr/bin/tpm2_getcap -T device:/dev/fde-no-such-tpm properties-fixed \
    > /tmp/missing.stdout 2> /tmp/missing.stderr || status=$?
test "$status" != 0
if grep -Fq 'Failed to open specified TCTI device file' /tmp/missing.stderr; then
    printf 'The device provider remained available after removing its implementation\n' >&2
    exit 1
fi
dpkg --remove --force-depends "libtss2-tcti-device0t64:$(dpkg --print-architecture)"
partial=$(mktemp -d)
status=0
DESTDIR="$partial" sh /fde-tools-hook > /tmp/package.stdout 2> /tmp/package.stderr || status=$?
rm -rf "$partial"
test "$status" != 0
grep -Fq 'is not installed' /tmp/package.stderr
printf 'Native copy_exec preserves product/tools/aliases, supplies the device TCTI provider, and rejects alias/package drift.\n'
SH
