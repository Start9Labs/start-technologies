#!/bin/bash

set -euo pipefail

START_CONTAINER=$(realpath -s "${1:?path to start-container}")
BUSYBOX=$(realpath "${2:?path to static busybox}")
[ "$EUID" -eq 0 ] || { echo 'Run in a private mount namespace as root.' >&2; exit 1; }
cmp -s "$START_CONTAINER" /usr/bin/start-container || {
    echo 'Install the tested binary at /usr/bin/start-container for launcher tests.' >&2
    exit 1
}
TMP=$(mktemp -d)
ROOT="$TMP/root"
wrapper_pid=
init_pid=

cleanup() {
    if [ -n "$wrapper_pid" ]; then
        if [ -z "$init_pid" ] && [ -r "/proc/$wrapper_pid/task/$wrapper_pid/children" ]; then
            read -r init_pid _ <"/proc/$wrapper_pid/task/$wrapper_pid/children" || true
        fi
        kill -KILL "$wrapper_pid" 2>/dev/null || true
        wait "$wrapper_pid" 2>/dev/null || true
    fi
    if [ -n "$init_pid" ]; then
        kill -KILL "$init_pid" 2>/dev/null || true
    fi
    if mountpoint -q "$ROOT"; then
        umount -R "$ROOT"
    fi
    rm -rf "$TMP"
}
trap cleanup EXIT
mkdir -p "$ROOT/bin" "$ROOT/dev" "$ROOT/etc" "$ROOT/proc"
mount --bind "$ROOT" "$ROOT"
touch "$ROOT/dev/null"
mount --bind /dev/null "$ROOT/dev/null"
cp "$BUSYBOX" "$ROOT/bin/busybox"
ln -s busybox "$ROOT/bin/sh"
mkfifo "$TMP/ready"
exec {ready_fd}<>"$TMP/ready"

is_running() {
    grep -q '^State:[[:space:]]*[^Z[:space:]]' "/proc/$1/status" 2>/dev/null
}

wait_dead() {
    for _ in {1..100}; do
        if ! is_running "$1"; then
            return
        fi
        sleep 0.05
    done
    echo "Process $1 survived its launch wrapper" >&2
    return 1
}

for user in root 1234:2345; do
    init_pid=
    unshare --pid --fork "$START_CONTAINER" subcontainer launch-init --user "$user" "$ROOT" -- /bin/sh -c '
        trap "" TERM
        /bin/busybox sleep 300 &
        echo READY
        wait
    ' >"$TMP/ready" 2>"$TMP/stderr" &
    wrapper_pid=$!
    if ! read -r -t 10 ready <&"$ready_fd" || [ "$ready" != READY ]; then
        cat "$TMP/stderr" >&2
        echo "Init did not become ready for $user" >&2
        exit 1
    fi
    read -r init_pid _ <<<"$(<"/proc/$wrapper_pid/task/$wrapper_pid/children")"
    read -r descendant_pid _ <<<"$(<"/proc/$init_pid/task/$init_pid/children")"
    [ -n "$init_pid" ] && [ -n "$descendant_pid" ]
    grep -q '^NSpid:.*[[:space:]]1$' "/proc/$init_pid/status"
    kill -TERM "$init_pid"
    is_running "$init_pid"
    is_running "$descendant_pid"
    kill -KILL "$wrapper_pid"
    wait "$wrapper_pid" 2>/dev/null || [ "$?" -eq 137 ]
    wrapper_pid=
    wait_dead "$init_pid"
    wait_dead "$descendant_pid"
    init_pid=
    umount "$ROOT/proc"
done
printf 'PASS: SIGKILL of the launch wrapper kills init and descendants, as root and numeric user\n'

for tty in pipe pty; do
    tty_args=()
    if [ "$tty" = pty ]; then
        tty_args=(--force-tty --force-stderr-tty)
    fi
    for phase in early ready graceful; do
        init_pid=
        "$START_CONTAINER" subcontainer launch "${tty_args[@]}" "$ROOT" >"$TMP/stdout" 2>"$TMP/stderr" &
        wrapper_pid=$!
        for _ in {1..200}; do
            read -r init_pid _ <"/proc/$wrapper_pid/task/$wrapper_pid/children" || true
            if [ -n "$init_pid" ]; then break; fi
            sleep 0.01
        done
        [ -n "$init_pid" ] || { cat "$TMP/stderr" >&2; exit 1; }
        if [ "$phase" != early ]; then
            ready=false
            for _ in {1..200}; do
                caught=$(awk '/^SigCgt:/ {print $2}' "/proc/$init_pid/status")
                if (( (16#$caught & (1 << 14)) != 0 )); then
                    ready=true
                    break
                fi
                sleep 0.01
            done
            "$ready" || { cat "$TMP/stderr" >&2; exit 1; }
        fi
        if [ "$phase" = graceful ]; then
            kill -TERM "$wrapper_pid"
            wait_dead "$wrapper_pid"
            wait "$wrapper_pid"
        else
            kill -KILL "$wrapper_pid"
            wait "$wrapper_pid" 2>/dev/null || [ "$?" -eq 137 ]
        fi
        wrapper_pid=
        wait_dead "$init_pid"
        init_pid=
        if mountpoint -q "$ROOT/proc"; then
            umount "$ROOT/proc"
        fi
        printf 'PASS: commandless init teardown (%s, %s)\n' "$tty" "$phase"
    done
done
