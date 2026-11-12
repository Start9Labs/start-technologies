#!/bin/bash
set -euo pipefail

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
    printf 'Usage: %s <genuine-os-root> <genuine-hook-sources> [TPM-fixture-image]\n' "$0" >&2
    exit 1
fi
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source_root=$(realpath "$1")
hook_sources=$(realpath "$2")
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
node --input-type=module - "$here" "$source_root" "$hook_sources" "$temporary/hooks" <<'JS'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
const [here, source, hooks, output] = process.argv.slice(2)
const { requireNativeHookFunctions } = await import(pathToFileURL(join(here, '../lib/scripts/fde-initramfs-producer.mjs')))
const { prepareNativeHooks } = await import(pathToFileURL(join(here, '../lib/scripts/fde-initramfs-hooks.mjs')))
requireNativeHookFunctions(await readFile(join(source, 'usr/share/initramfs-tools/hook-functions')))
await prepareNativeHooks(hooks, output)
JS
image=$(docker image inspect --format '{{.Id}}' "${3:-startos-fde-tpm-test}")
docker run --rm --pull=never -i \
    --mount "type=bind,src=$source_root,dst=/source,readonly" \
    --mount "type=bind,src=$temporary/hooks,dst=/source/mnt,readonly" \
    --tmpfs /source/var/tmp:rw,exec,nosuid,nodev \
    --tmpfs /source/dev:rw,nosuid,nodev \
    --mount type=bind,src=/dev/null,dst=/source/dev/null,readonly \
    --mount type=bind,src=/dev/random,dst=/source/dev/random,readonly \
    --mount type=bind,src=/dev/urandom,dst=/source/dev/urandom,readonly \
    "$image" chroot /source /bin/bash -se <<'SH'
set -euo pipefail
export LC_ALL=C verbose=n BUSYBOXDIR=/bin
. /usr/share/initramfs-tools/hook-functions

prepare_root() {
    export DESTDIR="$1"
    mkdir -p "$DESTDIR"/{usr/{bin,sbin,lib,lib64},etc,var/tmp}
    for dir in bin sbin lib lib64; do
        ln -s "usr/$dir" "$DESTDIR/$dir"
    done
}

for BUSYBOX in y n; do
    export BUSYBOX
    prepare_root "/var/tmp/providers-$BUSYBOX"
    copy_exec /usr/bin/cat
    copy_file binary /usr/lib/klibc/bin/fstype
    sh /mnt/zz-busybox
    sh /mnt/klibc-utils
    count=0
    find "$DESTDIR/usr" -type f -print0 > /var/tmp/providers.list
    while IFS= read -r -d '' provider; do
        cmp "$provider" "/${provider#"$DESTDIR/"}"
        count=$((count + 1))
    done < /var/tmp/providers.list
    test "$count" -gt 1
    test ! -L "$DESTDIR/usr/bin/cat"
    cmp /usr/bin/cat "$DESTDIR/usr/bin/cat"
    test -L "$DESTDIR/usr/bin/fstype"
    test "$(realpath "$DESTDIR/usr/bin/fstype")" = "$DESTDIR/usr/lib/klibc/bin/fstype"
    case "$(readlink "$DESTDIR/usr/bin/fstype")" in /*) exit 1 ;; esac
    for excluded in gzip kinit zcat; do
        test ! -e "$DESTDIR/usr/lib/klibc/bin/$excluded"
    done
    : > /var/tmp/blank
    : > "$DESTDIR/blank"
    if /usr/lib/klibc/bin/fstype /var/tmp/blank > /var/tmp/source-type 2> /var/tmp/source-error; then
        source_status=0
    else
        source_status=$?
    fi
    if chroot "$DESTDIR" /usr/bin/fstype /blank > /var/tmp/copied-type 2> /var/tmp/copied-error; then
        copied_status=0
    else
        copied_status=$?
    fi
    test "$source_status" = 2
    test "$copied_status" = "$source_status"
    cmp /var/tmp/source-type /var/tmp/copied-type
    grep -Fq 'Not a typewriter' /var/tmp/copied-error
    mkdir "$DESTDIR/to-remove"
    chroot "$DESTDIR" /usr/bin/nuke /to-remove
    test ! -e "$DESTDIR/to-remove"
    if [ "$BUSYBOX" = y ]; then
        for alias in sh ls; do
            test -L "$DESTDIR/usr/bin/$alias"
            test "$(realpath "$DESTDIR/usr/bin/$alias")" = "$DESTDIR/usr/bin/busybox"
            case "$(readlink "$DESTDIR/usr/bin/$alias")" in /*) exit 1 ;; esac
        done
        test "$(chroot "$DESTDIR" /usr/bin/sh -c 'printf busybox-runtime-ok')" = busybox-runtime-ok
    fi
    printf 'Native BUSYBOX=%s layout retained %s regular source-path providers and executable command aliases.\n' "$BUSYBOX" "$count"
done

export BUSYBOX=n
prepare_root /var/tmp/providers-failure
printf occupied > "$DESTDIR/usr/lib/klibc"
if sh /mnt/klibc-utils; then
    printf 'Failed native provider copy was accepted\n' >&2
    exit 1
fi
printf 'Native klibc copy failure rejected.\n'
SH
