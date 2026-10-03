#!/bin/bash

set -euo pipefail

START_CONTAINER=$(realpath -s "${1:?path to start-container}")
BUSYBOX=$(realpath "${2:?path to static busybox}")
[ "$EUID" -eq 0 ] || { echo 'Run inside a disposable Linux VM as root.' >&2; exit 1; }
TMP=$(mktemp -d)
ROOT="$TMP/root"
mkdir -p "$ROOT/bin" "$ROOT/etc" "$ROOT/proc" "$ROOT/work"
mount --bind "$ROOT" "$ROOT"
trap 'umount -R "$ROOT" && rm -rf "$TMP"' EXIT
cp "$BUSYBOX" "$ROOT/bin/busybox"
ln -s busybox "$ROOT/bin/sh"
touch "$ROOT/fixture"
printf 'root:x:0:0:root:/:/bin/sh\n' >"$ROOT/etc/passwd"
printf 'root:x:0:\n' >"$ROOT/etc/group"
mount -t proc proc "$ROOT/proc"

run_exec() {
    "$START_CONTAINER" subcontainer exec-command "$ROOT" -- sh -c \
        'test "$PWD" = / && test -f /fixture && test ! -e /bin/start-container && echo EXEC_OK'
}

for _ in {1..16}; do
    pids=()
    for i in {1..16}; do
        run_exec >"$TMP/out.$i" 2>"$TMP/err.$i" &
        pids+=("$!")
    done
    failed=0
    for i in {1..16}; do
        if ! wait "${pids[$((i - 1))]}" || ! grep -qx EXEC_OK "$TMP/out.$i"; then
            cat "$TMP/err.$i" >&2
            failed=1
        fi
    done
    [ "$failed" -eq 0 ]
done
[ ! -e "$ROOT/.put_old" ]

mkdir "$ROOT/.put_old"
run_exec | grep -qx EXEC_OK
[ -d "$ROOT/.put_old" ]
rmdir "$ROOT/.put_old"
echo PRESERVE >"$ROOT/.put_old"
run_exec | grep -qx EXEC_OK
grep -qx PRESERVE "$ROOT/.put_old"

mount -o remount,bind,ro "$ROOT"
run_exec | grep -qx EXEC_OK
"$START_CONTAINER" subcontainer exec-command --workdir /work --env TEST_VALUE=ok --user 1234:2345 "$ROOT" -- sh -c \
    'test "$PWD" = /work && test "$TEST_VALUE" = ok && test "$(/bin/busybox id -u)" = 1234 && test "$(/bin/busybox id -g)" = 2345'
"$START_CONTAINER" subcontainer exec-command "$ROOT" -- /bin/busybox unshare -Ur sh -c \
    'test "$(/bin/busybox id -u)" = 0'
set +e
"$START_CONTAINER" subcontainer exec-command "$ROOT" -- sh -c 'exit 7'
rc=$?
set -e
[ "$rc" -eq 7 ]
printf 'PASS: 256 concurrent execs, untouched .put_old, read-only root, workdir/env/user, user namespace, exit 7\n'
