#!/bin/bash
# Fetches the pinned rust-code-analysis binary. Upstream releases linux and windows only;
# every other platform builds it with `cargo install`.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
VERSION=v0.0.25
SHA256=9ec2a217b8ff191e02dab5d5f2eee6158b63fd975c532b2c5d67c2e6c7249894
mkdir -p "$HERE/bin"
if [ "$(uname -s)" = "Linux" ] && [ "$(uname -m)" = "x86_64" ]; then
  url="https://github.com/mozilla/rust-code-analysis/releases/download/$VERSION/rust-code-analysis-linux-cli-x86_64.tar.gz"
  tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
  curl -fsSL "$url" -o "$tmp/rca.tar.gz"
  echo "$SHA256  $tmp/rca.tar.gz" | sha256sum -c -
  tar -xzf "$tmp/rca.tar.gz" -C "$HERE/bin"
else
  cargo install rust-code-analysis-cli --version "${VERSION#v}" --root "$HERE"
fi
