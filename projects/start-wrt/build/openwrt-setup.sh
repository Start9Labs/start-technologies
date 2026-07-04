#!/bin/bash
set -eo pipefail

# Prepares the openwrt/ tree and readies it for an image build.
#
# openwrt/ is a DISPOSABLE build workspace (think node_modules/): a plain
# directory — no git repo, no submodule, no fork — rebuilt by this script from
# the sha256-pinned upstream release tarball (build/openwrt-version) plus the
# Start9 delta: openwrt-patches/ (modified upstream files) and openwrt-overlay/
# (added files). Never keep work inside openwrt/ — every run rebuilds it; see
# CONTRIBUTING.md "OpenWrt tree" for the workflow. Generated content the
# tarball doesn't provide (dl/, build_dir/, staging_dir/, bin/, feeds/, files/,
# .config, signing keys, …) is preserved across rebuilds.
#
# Usage: openwrt-setup.sh [--tree-only]
#   --tree-only  stop after the tree is rebuilt (pristine+patched+overlaid),
#                skipping feeds/config/download — for testing or offline prep.
#                Note: feeds symlink farms (package/feeds, target/linux/feeds)
#                are only recreated by the full run's `feeds install`.

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
PROJECT_DIR=projects/start-wrt
OPENWRT_DIR="$PROJECT_DIR/openwrt"
PATCHES_DIR="$PROJECT_DIR/openwrt-patches"
OVERLAY_DIR="$PROJECT_DIR/openwrt-overlay"
NEW_DIR="$OPENWRT_DIR.new"
OLD_DIR="$OPENWRT_DIR.old"

# Pinned upstream release: OPENWRT_VERSION + OPENWRT_TARBALL_SHA256.
source "$PROJECT_DIR/build/openwrt-version"
OPENWRT_TARBALL_URL="${OPENWRT_TARBALL_URL:-https://github.com/openwrt/openwrt/archive/refs/tags/v$OPENWRT_VERSION.tar.gz}"
TARBALL="$OPENWRT_DIR/dl/openwrt-v$OPENWRT_VERSION.tar.gz"

tarball_ok() {
	[ -f "$TARBALL" ] && echo "$OPENWRT_TARBALL_SHA256  $TARBALL" | sha256sum -c --quiet - 2>/dev/null
}

# --- 0. Crash recovery from a previous interrupted run ---
# Interrupted at the final swap: the prepared tree moved away but the new one
# never landed — put it back and rebuild from there.
if [ ! -d "$OPENWRT_DIR" ] && [ -d "$OLD_DIR" ]; then
	mv "$OLD_DIR" "$OPENWRT_DIR"
fi
# Interrupted mid-preserve: rescue any preserved entries stranded in .new
# before discarding it.
if [ -d "$NEW_DIR" ] && [ -d "$OPENWRT_DIR" ]; then
	find "$NEW_DIR" -mindepth 1 -maxdepth 1 | while read -r entry; do
		name="$(basename "$entry")"
		[ -e "$OPENWRT_DIR/$name" ] || mv "$entry" "$OPENWRT_DIR/$name"
	done
fi
rm -rf "$NEW_DIR" "$OLD_DIR"

# --- 1. Acquire + verify the upstream source tarball ---
if ! tarball_ok; then
	echo "==> Downloading upstream v$OPENWRT_VERSION..."
	mkdir -p "$OPENWRT_DIR/dl"
	curl -fL --retry 3 -o "$TARBALL" "$OPENWRT_TARBALL_URL"
	if ! tarball_ok; then
		echo "ERROR: $TARBALL does not match OPENWRT_TARBALL_SHA256" >&2
		echo "       ($OPENWRT_TARBALL_SHA256 in build/openwrt-version)." >&2
		echo "       Update the pin if a new release is intentional." >&2
		rm -f "$TARBALL"
		exit 1
	fi
fi

# --- 2. Rebuild the tree: pristine extract + Start9 delta ---
# Build the fresh tree in .new, then swap it in. tar restores the archive's
# stable mtimes, so re-preps don't invalidate build_dir wholesale.
echo "==> Extracting pristine v$OPENWRT_VERSION..."
mkdir -p "$NEW_DIR"
tar -xzf "$TARBALL" --strip-components=1 -C "$NEW_DIR"

echo "==> Applying Start9 patches..."
for p in "$PATCHES_DIR"/*.patch; do
	echo "      $(basename "$p")"
	patch -d "$NEW_DIR" -p1 -s --no-backup-if-mismatch < "$p"
	# Re-stamp patched files with a stable mtime (patch(1) sets 'now', which
	# would churn the build): reuse the tarball's top-level Makefile mtime.
	sed -n 's|^+++ b/||p' "$p" | while read -r f; do
		touch -r "$NEW_DIR/Makefile" "$NEW_DIR/$f"
	done
done

echo "==> Copying Start9 overlay..."
rsync -a "$OVERLAY_DIR"/ "$NEW_DIR"/

# Preserve top-level entries the tarball doesn't provide — generated state
# (dl/ incl. this tarball, build_dir/, staging_dir/, bin/, feeds/, files/,
# .config, key-build*, …). Tarball-provided paths always come fresh. A stray
# .git (from the pre-tarball era) is deliberately left behind to die.
find "$OPENWRT_DIR" -mindepth 1 -maxdepth 1 | while read -r entry; do
	name="$(basename "$entry")"
	case "$name" in .git) continue ;; esac
	[ -e "$NEW_DIR/$name" ] || mv "$entry" "$NEW_DIR/$name"
done

mv "$OPENWRT_DIR" "$OLD_DIR"
mv "$NEW_DIR" "$OPENWRT_DIR"
rm -rf "$OLD_DIR"

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
