#!/bin/bash
# Fetches the pinned big-code-analysis binary and verifies it against the release checksum.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
VERSION=2.1.0
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)   TRIPLE=x86_64-unknown-linux-gnu;  SHA256=6904518ff57968408dd3fa46a3fb533b8ac42cd035d5dd503090e24e19d5232a ;;
  Linux-aarch64)  TRIPLE=aarch64-unknown-linux-gnu; SHA256=6400d71fb8b436ee71a984a605172680eacf3ad9d4fb2046e24d2d1972f669d0 ;;
  Darwin-arm64)   TRIPLE=aarch64-apple-darwin;      SHA256=94faaa8f6f20952147e263222df4f65a11c8994af1da2e9d7882b3caae598212 ;;
  *) echo "complexity: no pinned bca build for $(uname -s)-$(uname -m); build it with 'cargo install big-code-analysis --version $VERSION --root $HERE'" >&2; exit 1 ;;
esac
url="https://github.com/dekobon/big-code-analysis/releases/download/v$VERSION/big-code-analysis-$VERSION-$TRIPLE.tar.gz"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
curl -fsSL "$url" -o "$tmp/bca.tar.gz"
echo "$SHA256  $tmp/bca.tar.gz" | sha256sum -c - >/dev/null
tar -xzf "$tmp/bca.tar.gz" -C "$tmp"
mkdir -p "$HERE/bin"
install -m 0755 "$tmp/big-code-analysis-$VERSION-$TRIPLE/bca" "$HERE/bin/bca"
