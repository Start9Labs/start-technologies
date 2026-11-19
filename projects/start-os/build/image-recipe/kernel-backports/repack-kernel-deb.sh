#!/bin/bash
set -euo pipefail

SOURCE=$(realpath "$1")
OUTPUT=$(realpath -m "$2")
source "$(dirname "${BASH_SOURCE[0]}")/kernel-backports.conf"
WORK=$(mktemp -d)
trap 'rm -rf -- "$WORK"' EXIT

PACKAGE=$(dpkg-deb -f "$SOURCE" Package)
ARCH=$(dpkg-deb -f "$SOURCE" Architecture)
UPSTREAM_VERSION=$(dpkg-deb -f "$SOURCE" Version)
if [ "$UPSTREAM_VERSION" != "$KERNEL_BACKPORT_UPSTREAM_VERSION" ]; then
    echo "Unexpected kernel package version: $UPSTREAM_VERSION" >&2
    exit 1
fi

dpkg-deb --control "$SOURCE" "$WORK/control"
awk -v old="$UPSTREAM_VERSION" -v version="$KERNEL_BACKPORT_VERSION" \
    -v compiler="$KERNEL_BACKPORT_COMPILER" -v maintainer="$KERNEL_BACKPORT_MAINTAINER" \
    -v hash="$(sha256sum "$SOURCE" | cut -d' ' -f1)" '
function replace_version(value, result, position) {
    while ((position = index(value, old)) != 0) {
        result = result substr(value, 1, position - 1) version
        value = substr(value, position + length(old))
    }
    return result value
}
/^[^ \t]/ { field = $1 }
/^Version:/ { print "Version: " version; next }
/^Maintainer:/ {
    sub(/^Maintainer:/, "Original-Maintainer:")
    print
    print "Maintainer: " maintainer
    next
}
field ~ /^(Depends|Pre-Depends|Recommends|Suggests|Breaks|Conflicts|Replaces|Provides):$/ {
    $0 = replace_version($0)
    gsub(/gcc-[0-9]+-for-host/, compiler)
}
{ print }
END {
    print "Origin: Start9"
    print "X-Start9-Upstream-SHA256: " hash
}
' "$WORK/control/control" > "$WORK/control.new"
mv "$WORK/control.new" "$WORK/control/control"

# Preserve Debian's signed kernel, modules and ABI byte-for-byte.
DATA=$(ar t "$SOURCE" | grep '^data\.tar')
(cd "$WORK" && ar x "$SOURCE" debian-binary "$DATA")
if [[ "$PACKAGE" = linux-headers-* ]]; then
    dpkg-deb --extract "$SOURCE" "$WORK/data"
    mapfile -t selectors < <(find "$WORK/data" -name .kernelvariables -type f)
    for selector in "${selectors[@]}"; do
        cp -p "$selector" "$WORK/compiler-reference"
        sed -i -E "/^CC =/s/gcc-[0-9]+$/${KERNEL_BACKPORT_COMPILER%-for-host}/" "$selector"
        touch -r "$WORK/compiler-reference" "$selector"
        relative=${selector#"$WORK/data/"}
        hash=$(md5sum "$selector" | cut -d' ' -f1)
        awk -v path="$relative" -v hash="$hash" '
            $2 == path { $0 = hash "  " path }
            { print }
        ' "$WORK/control/md5sums" > "$WORK/md5sums.new"
        mv "$WORK/md5sums.new" "$WORK/control/md5sums"
    done
    if [ "${#selectors[@]}" -gt 0 ]; then
        DATA=data.tar.gz
        # Extraction defers symlinks and leaves their parent timestamps nondeterministic.
        find "$WORK/data" -type d -exec touch -d @0 {} +
        tar --format=gnu --sort=name --owner=0 --group=0 --numeric-owner \
            -C "$WORK/data" -cf - . | gzip -n > "$WORK/$DATA"
    fi
fi
tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner \
    -C "$WORK/control" -cf - . | gzip -n > "$WORK/control.tar.gz"
(cd "$WORK" && ar rcD package.deb debian-binary control.tar.gz "$DATA")
if [[ "$PACKAGE" != linux-headers-* ]]; then
    cmp <(ar p "$SOURCE" "$DATA") <(ar p "$WORK/package.deb" "$DATA")
fi
mkdir -p "$OUTPUT"
DEB="$OUTPUT/${PACKAGE}_${KERNEL_BACKPORT_VERSION}_${ARCH}.deb"
mv "$WORK/package.deb" "$DEB"
printf '%s\n' "$DEB"
