#!/bin/bash

set -euo pipefail

START_CLI=$(realpath -s "${1:?path to start-cli}")
PACKAGE=${2:?installed package with a running subcontainer and sh}
NAME=${3:?subcontainer name}
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

attach() {
    "$START_CLI" package attach "$PACKAGE" --name "$NAME" "$@" </dev/null
}

for _ in {1..16}; do
    pids=()
    for i in {1..16}; do
        attach -- sh -c 'echo ATTACH_OK' >"$TMP/out.$i" 2>"$TMP/err.$i" &
        pids+=("$!")
    done
    failed=0
    for i in {1..16}; do
        if ! wait "${pids[$((i - 1))]}" || ! grep -q ATTACH_OK "$TMP/out.$i"; then
            cat "$TMP/err.$i" >&2
            failed=1
        fi
    done
    [ "$failed" -eq 0 ]
done

set +e
attach -- sh -c 'exit 7'
rc=$?
set -e
[ "$rc" -eq 7 ]

set +e
attach --user __attach_test_missing_user__ -- sh -c 'echo SHOULD_NOT_RUN' >"$TMP/failure.out" 2>"$TMP/failure.err"
rc=$?
set -e
[ "$rc" -ne 0 ]
if grep -q SHOULD_NOT_RUN "$TMP/failure.out"; then
    exit 1
fi
[ -s "$TMP/failure.err" ]

set +e
attach --force-tty -- sh -c 'echo TTY_OK; exit 7' >"$TMP/tty.out" 2>"$TMP/tty.err"
rc=$?
set -e
[ "$rc" -eq 7 ]
grep -q TTY_OK "$TMP/tty.out"
printf 'PASS: 256 concurrent attaches, exit 7, setup failure, forced TTY exit 7\n'
