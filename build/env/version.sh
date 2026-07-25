#!/bin/bash

# Prints a project's version. StartOS is the odd one out: its version carries a revision
# segment (0.4.0.1) that SemVer — and so Cargo — cannot express, so root package.json holds
# it and projects/start-os/Cargo.toml carries only a `0.4.0-rev.1` label. Every other project
# versions in its own Cargo.toml.
#
# Kept to grep/sed (no jq) because build/common.mk expands the callers with `:=` at Makefile
# parse time, on hosts that may have nothing else installed.

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

PROJECT=${1:-${PROJECT:-startos}}

case "$PROJECT" in
  startos | start-os)
    grep -m1 '"version"' package.json | sed -E 's/.*"version" *: *"([^"]*)".*/\1/'
    ;;
  *)
    grep -m1 '^version' "projects/${PROJECT}/Cargo.toml" | sed -E 's/^version *= *"([^"]*)".*/\1/'
    ;;
esac
