#!/bin/bash
set -eo pipefail

# Prepares the openwrt/ tree and readies it for an image build.
#
# openwrt/ is a DISPOSABLE checkout of pristine upstream OpenWrt (no git
# submodule, no Start9 fork): this script resets it to the commit pinned in
# build/openwrt-version and re-applies the Start9 delta on every run. Never
# commit work inside openwrt/ — the delta lives in openwrt-patches/ (modified
# upstream files) and openwrt-overlay/ (added files); see CONTRIBUTING.md
# "OpenWrt tree" for the workflow.
#
# Usage: openwrt-setup.sh [--tree-only]
#   --tree-only  stop after the tree is pristine+patched+overlaid (skip
#                feeds/config/download) — for testing or offline tree prep.

# Capture the monorepo root ONCE. We cd into the openwrt checkout below; since
# it's its own git repo, re-running `git rev-parse --show-toplevel` from inside
# it would return the checkout's root, not the monorepo root.
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
PROJECT_DIR=projects/start-wrt
OPENWRT_DIR="$PROJECT_DIR/openwrt"
PATCHES_DIR="$PROJECT_DIR/openwrt-patches"
OVERLAY_DIR="$PROJECT_DIR/openwrt-overlay"

# Pinned upstream release: OPENWRT_TAG + OPENWRT_COMMIT.
source "$PROJECT_DIR/build/openwrt-version"
OPENWRT_GIT_URL="${OPENWRT_GIT_URL:-https://github.com/openwrt/openwrt.git}"

# --- 1. Acquire pristine upstream at the pinned commit ---
# git init + fetch (rather than clone) so a pre-existing directory is fine —
# e.g. CI restores the dl/ cache into openwrt/dl before this script runs.
if [ ! -e "$OPENWRT_DIR/.git" ]; then
	echo "==> Initializing openwrt checkout..."
	mkdir -p "$OPENWRT_DIR"
	git -C "$OPENWRT_DIR" init -q
fi
if ! git -C "$OPENWRT_DIR" cat-file -e "$OPENWRT_COMMIT^{commit}" 2>/dev/null; then
	echo "==> Fetching upstream $OPENWRT_TAG from $OPENWRT_GIT_URL..."
	git -C "$OPENWRT_DIR" fetch --depth 1 "$OPENWRT_GIT_URL" \
		"refs/tags/$OPENWRT_TAG:refs/tags/$OPENWRT_TAG"
	ACTUAL="$(git -C "$OPENWRT_DIR" rev-parse "refs/tags/$OPENWRT_TAG^{commit}")"
	if [ "$ACTUAL" != "$OPENWRT_COMMIT" ]; then
		echo "ERROR: upstream tag $OPENWRT_TAG resolves to $ACTUAL" >&2
		echo "       but build/openwrt-version pins $OPENWRT_COMMIT." >&2
		echo "       Update the pin if this is intentional." >&2
		exit 1
	fi
fi

echo "==> Resetting openwrt/ to pristine $OPENWRT_TAG ($OPENWRT_COMMIT)..."
# -f discards local modifications to tracked files (a previous run's patches).
git -C "$OPENWRT_DIR" checkout -qf --detach "$OPENWRT_COMMIT"
# Drop untracked files (a previous run's overlay). Ignored paths — dl/, feeds/,
# build_dir/, staging_dir/, bin/, files/, .config — survive, so download/build
# caches and staged files are preserved.
git -C "$OPENWRT_DIR" clean -qfd

# --- 2. Apply the Start9 delta ---
echo "==> Applying Start9 patches..."
for p in "$PATCHES_DIR"/*.patch; do
	echo "      $(basename "$p")"
	git -C "$OPENWRT_DIR" apply "$ROOT/$p"
done

echo "==> Copying Start9 overlay..."
rsync -a "$OVERLAY_DIR"/ "$OPENWRT_DIR"/

if [ "$1" = "--tree-only" ]; then
	echo "==> OpenWrt tree ready (--tree-only: skipping feeds/config/download)."
	exit 0
fi

# --- 3. Feeds / config / download ---
echo "==> Copying feeds.conf to openwrt..."
cp "$PROJECT_DIR/build/feeds.conf" "$OPENWRT_DIR/feeds.conf"

echo "==> Updating feeds..."
cd "$OPENWRT_DIR"
./scripts/feeds update -a

echo "==> Installing feeds..."
./scripts/feeds install -a

cd "$ROOT"

echo "==> Copying diffconfig..."
cp "$PROJECT_DIR/build/openwrt.diffconfig" "$OPENWRT_DIR/.config"

echo "==> Expanding to full config..."
cd "$OPENWRT_DIR"
make defconfig

echo "==> Downloading sources..."
make download V=s

echo "==> OpenWrt setup complete."
