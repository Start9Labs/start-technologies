#!/bin/bash
set -euo pipefail

ARCH=$1
OUTPUT_ROOT=$(realpath -m "$2")
SOURCE_DIR=$(realpath "$(dirname "${BASH_SOURCE[0]}")")
source "$SOURCE_DIR/kernel-backports.conf"
OUTPUT="$OUTPUT_ROOT/$KERNEL_BACKPORT_VERSION"
case "$ARCH" in
    amd64|arm64) ;;
    *) echo "No Debian-signed kernel bridge for $ARCH" >&2; exit 1 ;;
esac
WORK=$(mktemp -d)
trap 'rm -rf -- "$WORK"' EXIT
mkdir -p "$WORK"/{etc/apt.conf.d,etc/preferences.d,state/lists/partial,cache/archives/partial,log,downloads} "$OUTPUT"
chmod 755 "$WORK"
if [ "$(id -u)" -eq 0 ]; then
    chown _apt "$WORK/downloads"
fi
touch "$WORK/state/status"
cat > "$WORK/apt.conf" <<EOF
Dir::Etc "$WORK/etc";
Dir::State "$WORK/state";
Dir::State::status "$WORK/state/status";
Dir::Cache "$WORK/cache";
Dir::Log "$WORK/log";
APT::Architecture "$ARCH";
APT::Architectures { "$ARCH"; };
EOF
cat > "$WORK/etc/sources.list" <<EOF
deb [arch=$ARCH check-valid-until=no signed-by=/usr/share/keyrings/debian-archive-keyring.gpg] https://snapshot.debian.org/archive/debian/$KERNEL_BACKPORT_SNAPSHOT/ forky main
EOF
export APT_CONFIG="$WORK/apt.conf"
apt-get update
apt-get --yes --print-uris --download-only --no-install-recommends install \
    "linux-image-$ARCH=$KERNEL_BACKPORT_UPSTREAM_VERSION" \
    "linux-headers-$ARCH=$KERNEL_BACKPORT_UPSTREAM_VERSION" > "$WORK/plan"

PACKAGES=()
while read -r filename; do
    package=${filename%%_*}
    case "$package" in
        linux-*"$KERNEL_BACKPORT_ABI"*|linux-*-"$ARCH")
            PACKAGES+=("$package=$KERNEL_BACKPORT_UPSTREAM_VERSION") ;;
    esac
done < <(awk '/^\047/ { print $2 }' "$WORK/plan")
if [ "${#PACKAGES[@]}" -eq 0 ]; then
    echo 'APT did not select any kernel packages' >&2
    exit 1
fi
(cd "$WORK/downloads" && apt-get download "${PACKAGES[@]}")
for deb in "$WORK/downloads"/*.deb; do
    "$SOURCE_DIR/repack-kernel-deb.sh" "$deb" "$OUTPUT"
done
(
    cd "$OUTPUT"
    dpkg-scanpackages . /dev/null > Packages
    gzip -n -c Packages > Packages.gz
)
printf 'Kernel backports archive: %s\n' "$OUTPUT"
