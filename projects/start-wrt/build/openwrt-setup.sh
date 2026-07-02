#!/bin/bash
set -eo pipefail

# Capture the monorepo root ONCE. We cd into the openwrt submodule below; since
# it's its own git repo, re-running `git rev-parse --show-toplevel` from inside it
# would return the submodule root, not the monorepo root.
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
PROJECT_DIR=projects/start-wrt

echo "==> Copying feeds.conf to openwrt..."
cp "$PROJECT_DIR/build/feeds.conf" "$PROJECT_DIR/openwrt/feeds.conf"

echo "==> Updating feeds..."
cd "$PROJECT_DIR/openwrt"
./scripts/feeds update -a

echo "==> Installing feeds..."
./scripts/feeds install -a

cd "$ROOT"

echo "==> Copying diffconfig..."
cp "$PROJECT_DIR/build/openwrt.diffconfig" "$PROJECT_DIR/openwrt/.config"

echo "==> Expanding to full config..."
cd "$PROJECT_DIR/openwrt"
make defconfig

echo "==> Downloading sources..."
make download V=s

echo "==> OpenWrt setup complete."
