#!/bin/bash
set -euo pipefail
ROOT=$(realpath "$(dirname "${BASH_SOURCE[0]}")/../../../..")
RECIPE="$ROOT/projects/start-os/build/image-recipe"
source "$RECIPE/kernel-backports.conf"
WORK=$(mktemp -d)
trap 'rm -rf -- "$WORK"' EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }
fixture() {
    local package=$1 architecture=$2 compression=$3
    rm -rf "$WORK/input"
    mkdir -p "$WORK/input/DEBIAN" "$WORK/input/boot"
    printf 'signed-kernel-payload\000keep-unchanged\377\n' > "$WORK/input/boot/vmlinuz-test"
    if [[ "$package" = linux-headers-* ]]; then
        mkdir -p "$WORK/input/usr/src/kernel-test"
        printf 'CC = $(CROSS_COMPILE)gcc-16\noverride KERNELRELEASE = test-abi\n' \
            > "$WORK/input/usr/src/kernel-test/.kernelvariables"
        mkdir -p "$WORK/input/usr/lib/modules/test-abi"
        ln -s ../../../src/kernel-test "$WORK/input/usr/lib/modules/test-abi/build"
        printf 'CONFIG_GCC_VERSION=160200\n' > "$WORK/input/usr/src/kernel-test/.config"
        (cd "$WORK/input" && md5sum usr/src/kernel-test/.kernelvariables usr/src/kernel-test/.config > DEBIAN/md5sums)
    fi
    cat > "$WORK/input/DEBIAN/control" <<EOF
Package: $package
Version: $KERNEL_BACKPORT_UPSTREAM_VERSION
Architecture: $architecture
Maintainer: Debian Kernel Team <debian-kernel@lists.debian.org>
Source: linux ($KERNEL_BACKPORT_UPSTREAM_VERSION)
Built-Using: linux (= $KERNEL_BACKPORT_UPSTREAM_VERSION)
Depends: linux-base-test (= $KERNEL_BACKPORT_UPSTREAM_VERSION),
 gcc-16-for-host, libc6 (>= 2.38)
Breaks: linux-base-old (<< $KERNEL_BACKPORT_UPSTREAM_VERSION~)
Description: test payload $KERNEL_BACKPORT_UPSTREAM_VERSION gcc-16-for-host
EOF
    dpkg-deb --root-owner-group -Z"$compression" --build "$WORK/input" "$WORK/source.deb" >/dev/null
}

for fixture_case in 'linux-image-test amd64 xz' 'linux-headers-test all gzip'; do
    read -r package architecture compression <<< "$fixture_case"
    fixture "$package" "$architecture" "$compression"
    "$RECIPE/repack-kernel-deb.sh" "$WORK/source.deb" "$WORK/output" >/dev/null
    deb="$WORK/output/${package}_${KERNEL_BACKPORT_VERSION}_${architecture}.deb"
    [ "$(dpkg-deb -f "$deb" Version)" = "$KERNEL_BACKPORT_VERSION" ] || fail version
    [ "$(dpkg-deb -f "$deb" Source)" = "linux ($KERNEL_BACKPORT_UPSTREAM_VERSION)" ] || fail source
    [ "$(dpkg-deb -f "$deb" Built-Using)" = "linux (= $KERNEL_BACKPORT_UPSTREAM_VERSION)" ] || fail provenance
    [ "$(dpkg-deb -f "$deb" Original-Maintainer)" = 'Debian Kernel Team <debian-kernel@lists.debian.org>' ] || fail attribution
    [ "$(dpkg-deb -f "$deb" X-Start9-Upstream-SHA256)" = "$(sha256sum "$WORK/source.deb" | cut -d' ' -f1)" ] || fail commitment
    dpkg-deb -f "$deb" Depends | grep -Fq "linux-base-test (= $KERNEL_BACKPORT_VERSION)" || fail dependency
    dpkg-deb -f "$deb" Depends | grep -Fq "$KERNEL_BACKPORT_COMPILER" || fail compiler
    dpkg-deb -f "$deb" Breaks | grep -Fq "<< $KERNEL_BACKPORT_VERSION~" || fail breaks
    dpkg-deb -f "$deb" Description | grep -Fq "$KERNEL_BACKPORT_UPSTREAM_VERSION gcc-16-for-host" || fail description
    rm -rf "$WORK/inspect" "$WORK/inspect-control"
    dpkg-deb --extract "$deb" "$WORK/inspect"
    cmp "$WORK/input/boot/vmlinuz-test" "$WORK/inspect/boot/vmlinuz-test" || fail kernel-bytes
    if [[ "$package" = linux-headers-* ]]; then
        grep -Fxq "CC = \$(CROSS_COMPILE)${KERNEL_BACKPORT_COMPILER%-for-host}" \
            "$WORK/inspect/usr/src/kernel-test/.kernelvariables" || fail compiler-selector
        grep -Fxq 'override KERNELRELEASE = test-abi' \
            "$WORK/inspect/usr/src/kernel-test/.kernelvariables" || fail abi
        cmp "$WORK/input/usr/src/kernel-test/.config" "$WORK/inspect/usr/src/kernel-test/.config" || fail kernel-config
        dpkg-deb --control "$deb" "$WORK/inspect-control"
        (cd "$WORK/inspect" && md5sum -c "$WORK/inspect-control/md5sums") || fail header-checksum
    else
        data=$(ar t "$WORK/source.deb" | grep '^data\.tar')
        cmp <(ar p "$WORK/source.deb" "$data") <(ar p "$deb" "$data") || fail payload
    fi
    cp "$deb" "$WORK/first.deb"
    sleep 1
    "$RECIPE/repack-kernel-deb.sh" "$WORK/source.deb" "$WORK/output" >/dev/null
    cmp "$WORK/first.deb" "$deb" || fail reproducibility
done

dpkg --compare-versions "$KERNEL_BACKPORT_VERSION" lt "$KERNEL_BACKPORT_UPSTREAM_VERSION~bpo13+1" || fail official-order
sed -i 's/^Version:.*/Version: 1.0/' "$WORK/input/DEBIAN/control"
dpkg-deb --root-owner-group --build "$WORK/input" "$WORK/wrong.deb" >/dev/null
if "$RECIPE/repack-kernel-deb.sh" "$WORK/wrong.deb" "$WORK/output" >/dev/null 2>&1; then
    fail 'unmatched source version accepted'
fi

awk '
/^case "\$IB_SUITE:\$\{IB_TARGET_PLATFORM%%-\*\}" in$/ { found=1 }
found { print }
found && /^esac$/ { exit }
' "$RECIPE/build.sh" > "$WORK/stage.sh"
[ -s "$WORK/stage.sh" ] || fail 'missing archive staging'
mkdir -p "$WORK/recipe" "$WORK/stage/config/archives"
cp "$RECIPE/kernel-backports.conf" "$WORK/recipe/"
cat > "$WORK/recipe/build-kernel-backports.sh" <<'EOF'
#!/bin/bash
printf '%s\n' "$@" > "$RESULTS_DIR/called"
EOF
chmod +x "$WORK/recipe/build-kernel-backports.sh"
export SOURCE_DIR="$WORK/recipe" RESULTS_DIR="$WORK"
cd "$WORK/stage"
for target in x86_64:amd64 x86_64-nonfree:amd64 x86_64-nvidia:amd64 aarch64:arm64 aarch64-nonfree:arm64 aarch64-nvidia:arm64; do
    IFS=: read -r platform arch <<< "$target"
    rm -f config/archives/kernel-backports.list.chroot "$WORK/called"
    IB_SUITE=trixie IB_TARGET_PLATFORM="$platform" IB_TARGET_ARCH="$arch" bash "$WORK/stage.sh"
    [ -f "$WORK/called" ] || fail "$platform archive build"
    grep -Fxq "deb [trusted=yes] file://$WORK/kernel-backports/$arch/$KERNEL_BACKPORT_VERSION ./" \
        config/archives/kernel-backports.list.chroot || fail "$platform archive URL"
done
for target in raspberrypi:arm64 rockchip64:arm64 riscv64:riscv64 riscv64-nonfree:riscv64; do
    IFS=: read -r platform arch <<< "$target"
    rm -f config/archives/kernel-backports.list.chroot "$WORK/called"
    IB_SUITE=trixie IB_TARGET_PLATFORM="$platform" IB_TARGET_ARCH="$arch" bash "$WORK/stage.sh"
    [ ! -f "$WORK/called" ] || fail "$platform kernel selection changed"
    [ ! -f config/archives/kernel-backports.list.chroot ] || fail "$platform retained bridge archive"
done
rm -f "$WORK/called"
IB_SUITE=bookworm IB_TARGET_PLATFORM=x86_64 IB_TARGET_ARCH=amd64 bash "$WORK/stage.sh"
[ ! -f "$WORK/called" ] || fail 'bridge used outside trixie'

awk '
/^KVER_ALL=/ { found=1 }
found && /^if \[ "\$\{IB_SUITE\}" = trixie \]/ { exit }
found { gsub(/\\\$/, "$"); print }
' "$RECIPE/build.sh" > "$WORK/kbuild.sh"
mkdir "$WORK/bin"
cat > "$WORK/bin/ls" <<'EOF'
#!/bin/bash
printf '/boot/vmlinuz-target-abi\n'
EOF
cat > "$WORK/bin/apt-cache" <<'EOF'
#!/bin/bash
[ "$*" = 'depends linux-headers-target-abi' ] || exit 1
printf '  Depends: linux-kbuild-from-header-metadata\n'
EOF
cat > "$WORK/bin/apt-get" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" > "$RESULTS_DIR/kbuild-installed"
EOF
chmod +x "$WORK/bin/"*
PATH="$WORK/bin:$PATH" bash "$WORK/kbuild.sh"
grep -Fxq 'install -y linux-kbuild-from-header-metadata' "$WORK/kbuild-installed" || fail 'target kbuild selection'
IMAGE="$WORK/image"
ABI=7.3.4+deb13-amd64
mkdir -p "$IMAGE"/{var/lib/dpkg,usr/lib/modules/$ABI,boot,etc/default}
cat > "$IMAGE/var/lib/dpkg/status" <<EOF
Package: linux-image-amd64
Status: install ok installed
Architecture: amd64
Version: $KERNEL_BACKPORT_UPSTREAM_VERSION~bpo13+1
Maintainer: Debian Kernel Team <debian-kernel@lists.debian.org>
Description: official backport fixture
EOF
printf 'selected-kernel' > "$IMAGE/usr/lib/modules/$ABI/vmlinuz"
cp "$IMAGE/usr/lib/modules/$ABI/vmlinuz" "$IMAGE/boot/vmlinuz-$ABI"
printf 'retained-kernel' > "$IMAGE/boot/vmlinuz-7.3.4+deb14-amd64"
printf 'GRUB_TOP_LEVEL="/boot/vmlinuz-%s"\n' "$ABI" > "$IMAGE/etc/default/grub"
"$RECIPE/verify-kernel-backports.sh" "$IMAGE" amd64
printf 'GRUB_TOP_LEVEL="/boot/vmlinuz-7.3.4+deb14-amd64"\n' > "$IMAGE/etc/default/grub"
if "$RECIPE/verify-kernel-backports.sh" "$IMAGE" amd64 >/dev/null 2>&1; then
    fail 'retained higher ABI selected'
fi
printf 'GRUB_TOP_LEVEL="/boot/vmlinuz-%s"\n' "$ABI" > "$IMAGE/etc/default/grub"
printf 'wrong-kernel' > "$IMAGE/boot/vmlinuz-$ABI"
if "$RECIPE/verify-kernel-backports.sh" "$IMAGE" amd64 >/dev/null 2>&1; then
    fail 'mismatched boot binary accepted'
fi
cp "$IMAGE/usr/lib/modules/$ABI/vmlinuz" "$IMAGE/boot/vmlinuz-$ABI"
sed -i 's/^Version:.*/Version: 7.2.12-1~bpo13+1/' "$IMAGE/var/lib/dpkg/status"
if "$RECIPE/verify-kernel-backports.sh" "$IMAGE" amd64 >/dev/null 2>&1; then
    fail 'old kernel package accepted'
fi
printf 'kernel backports tests passed\n'
