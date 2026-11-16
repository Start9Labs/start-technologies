#!/bin/bash

set -euo pipefail

ROOT=$(realpath "$(dirname "${BASH_SOURCE[0]}")/../../../..")
POSTINST="$ROOT/projects/start-os/debian/postinst"
LOADER="$ROOT/projects/start-os/build/lib/grub-user.cfg"
ISOLATION="$ROOT/projects/start-os/build/lib/scripts/nvme-isolation-test"
MKCONFIG=$(command -v grub-mkconfig) || { echo 'Requires grub-common.' >&2; exit 1; }
TMP=$(mktemp -d)
trap 'rm -rf -- "$TMP"' EXIT
export FIXTURE="$TMP"
mkdir -p "$TMP/bin" "$TMP/os-root/config" "$TMP/boot/grub"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

cat > "$TMP/bin/id" <<'EOF'
#!/bin/sh
printf '0\n'
EOF
cat > "$TMP/bin/grub-probe" <<'EOF'
#!/bin/sh
case " $* " in
    *--target=fs_uuid*) echo fixture-uuid ;;
    *--target=partuuid*) echo fixture-partuuid ;;
    *--target=fs*) echo ext4 ;;
    *) echo /dev/fixture ;;
esac
EOF
sed -e "s|^sysconfdir=.*|sysconfdir=\"$TMP/image/etc\"|" \
    -e "s|^grub_probe=.*|grub_probe=\"$TMP/bin/grub-probe\"|" \
    "$MKCONFIG" > "$TMP/bin/grub-mkconfig"
chmod +x "$TMP/bin/"*
export PATH="$TMP/bin:$PATH"

for version in 1 2; do
    image="$TMP/image-$version"
    mkdir -p "$image/etc/default" "$image/etc/grub.d" "$image/usr/lib/startos"
    sed "s|/media/startos/root/config|$TMP/os-root/config|g" "$LOADER" > "$image/usr/lib/startos/grub-user.cfg"
    awk '/mkdir -p \/etc\/default\/grub.d$/ { copying=1 } copying { print } copying && /cp .*grub-user.cfg/ { found=1; exit } END { if (!found) exit 1 }' "$POSTINST" |
        sed "s|/etc/default/|$image/etc/default/|g; s|/usr/lib/startos/|$image/usr/lib/startos/|g" > "$TMP/install-loader.sh"
    sh "$TMP/install-loader.sh"
    cmp "$image/usr/lib/startos/grub-user.cfg" "$image/etc/default/grub.d/zz-startos-user.cfg"
    printf 'GRUB_CMDLINE_LINUX="boot=startos console=ttyS0,115200n8 console=tty0 image=%s"\nGRUB_CMDLINE_LINUX_DEFAULT="quiet"\n' "$version" > "$image/etc/default/grub"
    cat > "$image/etc/grub.d/10_linux" <<'EOF'
#!/bin/sh
[ "${FAIL_GENERATION:-0}" != 1 ] || exit 23
[ "${EMPTY_KERNELS:-0}" != 1 ] || exit 0
printf 'menuentry "StartOS" {\n linux /vmlinuz root=UUID=fixture %s %s\n}\n' "$GRUB_CMDLINE_LINUX" "$GRUB_CMDLINE_LINUX_DEFAULT"
printf 'menuentry "StartOS recovery" {\n linux /vmlinuz root=UUID=fixture %s recovery\n}\n' "$GRUB_CMDLINE_LINUX"
EOF
    chmod +x "$image/etc/grub.d/10_linux"
done
ln -s "$TMP/image-1" "$TMP/image"

generate() {
    grub-mkconfig -o "$TMP/boot/grub/grub.cfg" > "$TMP/generate.log" 2>&1 || { cat "$TMP/generate.log"; fail 'GRUB generation'; }
}
has() {
    grep -Fq -- "$1" "$TMP/boot/grub/grub.cfg" || fail "missing argument: $1"
}
lacks() {
    if grep -Fq -- "$1" "$TMP/boot/grub/grub.cfg"; then fail "unexpected argument: $1"; fi
}

generate
has 'boot=startos console=ttyS0,115200n8 console=tty0 image=1'
has quiet
mkdir "$TMP/os-root/config/grub.d"
generate
has image=1
cat > "$TMP/os-root/config/grub.d/50-user.cfg" <<'EOF'
GRUB_CMDLINE_LINUX="$GRUB_CMDLINE_LINUX administrator=kept"
EOF
cat > "$TMP/os-root/config/grub.d/60-remove.cfg" <<'EOF'
GRUB_CMDLINE_LINUX="$(printf '%s\n' "$GRUB_CMDLINE_LINUX" | sed 's/ console=ttyS0,115200n8//')"
GRUB_CMDLINE_LINUX_DEFAULT=""
EOF
cat > "$TMP/os-root/config/grub.d/70-order.cfg" <<'EOF'
GRUB_CMDLINE_LINUX="$GRUB_CMDLINE_LINUX ordered=last"
EOF
printf 'this is not shell syntax {{{\n' > "$TMP/os-root/config/grub.d/ignored.txt"
generate
has 'image=1 administrator=kept ordered=last'
lacks console=ttyS0
lacks quiet
cp "$TMP/boot/grub/grub.cfg" "$TMP/first-boot.cfg"
generate
cmp "$TMP/first-boot.cfg" "$TMP/boot/grub/grub.cfg" || fail 'repeated generation changed overrides'
ln -sfn "$TMP/image-2" "$TMP/image"
generate
has 'image=2 administrator=kept ordered=last'
lacks image=1

cat > "$TMP/os-root/config/grub.d/80-replace.cfg" <<'EOF'
GRUB_CMDLINE_LINUX="boot=startos replacement=complete"
GRUB_CMDLINE_LINUX_DEFAULT="replacement_default=complete"
EOF
generate
has 'boot=startos replacement=complete replacement_default=complete'
lacks administrator=kept
rm "$TMP/os-root/config/grub.d/80-replace.cfg"
cp "$ROOT/projects/start-os/build/image-recipe/raspberrypi/squashfs/etc/default/grub.d/raspberrypi.cfg" "$TMP/image/etc/default/grub.d/raspberrypi.cfg"
generate
has 'console=serial0,115200 console=tty1'
has usb-storage.quirks=
has 'administrator=kept ordered=last'
lacks console=ttyS0
rm "$TMP/image/etc/default/grub.d/raspberrypi.cfg"

sed "s|/media/startos/config|$TMP/os-root/config|g; s|/boot/grub|$TMP/boot/grub|g; s|/etc/default/grub.d|$TMP/image/etc/default/grub.d|g; s|/proc/cmdline|$TMP/proc/cmdline|g; s|/sys/|$TMP/sys/|g" "$ISOLATION" > "$TMP/isolation"
chmod +x "$TMP/isolation"
config="$TMP/os-root/config/grub.d/90-nvme-isolation.cfg"
isolate() {
    "$TMP/isolation" "$@" > "$TMP/isolation.log" 2>&1 || { cat "$TMP/isolation.log"; fail "isolation $*"; }
    has 'administrator=kept ordered=last'
    has boot=startos
}
reject() {
    local status=0
    cp "$TMP/boot/grub/grub.cfg" "$TMP/before-grub.cfg"
    if [ -f "$config" ]; then cp "$config" "$TMP/before-override.cfg"; else rm -f "$TMP/before-override.cfg"; fi
    "$TMP/isolation" "$@" > "$TMP/isolation.log" 2>&1 || status=$?
    [ "$status" != 0 ] || fail "accepted invalid isolation $*"
    cmp "$TMP/before-grub.cfg" "$TMP/boot/grub/grub.cfg" || fail 'rejection changed boot config'
    if [ -f "$TMP/before-override.cfg" ]; then
        cmp "$TMP/before-override.cfg" "$config" || fail 'rejection changed diagnostic overrides'
    else
        [ ! -e "$config" ] || fail 'rejection left diagnostic overrides'
    fi
    [ -z "$(find "$TMP/boot/grub" -name '.nvme-isolation.*' -print)" ] || fail 'temporary work left behind'
}

isolate baseline
[ ! -e "$config" ] || fail 'baseline created override'
isolate apst
has nvme_core.default_ps_max_latency_us=0
isolate hmb
has nvme.max_host_mem_size_mb=0
lacks nvme_core.default_ps_max_latency_us=
isolate aspm
has pcie_aspm=off
lacks nvme.max_host_mem_size_mb=
isolate portpm
has pcie_port_pm=off
lacks pcie_aspm=
isolate apst aspm portpm
has 'nvme_core.default_ps_max_latency_us=0 pcie_aspm=off pcie_port_pm=off'
cp "$config" "$TMP/combination.cfg"
ln -sfn "$TMP/image-1" "$TMP/image"
generate
has image=1
has nvme_core.default_ps_max_latency_us=0
cmp "$TMP/combination.cfg" "$config" || fail 'image change modified saved test'
isolate baseline
[ ! -e "$config" ] || fail 'baseline did not delete diagnostic override'
lacks nvme_core.default_ps_max_latency_us=
lacks pcie_aspm=
lacks pcie_port_pm=

reject
reject unknown
reject ''
reject apst apst
reject baseline apst
reject collect /dev/nvme0 extra
cat > "$TMP/os-root/config/grub.d/80-conflict.cfg" <<'EOF'
GRUB_CMDLINE_LINUX_DEFAULT="$GRUB_CMDLINE_LINUX_DEFAULT nvme_core.default_ps_max_latency_us=5500"
EOF
reject baseline
reject hmb
rm "$TMP/os-root/config/grub.d/80-conflict.cfg"
isolate apst
printf 'GRUB_CMDLINE_LINUX="unterminated\n' > "$TMP/os-root/config/grub.d/80-broken.cfg"
reject hmb
rm "$TMP/os-root/config/grub.d/80-broken.cfg"
cat > "$TMP/os-root/config/grub.d/99-conflict.cfg" <<'EOF'
GRUB_CMDLINE_LINUX="$GRUB_CMDLINE_LINUX pcie_port_pm=off"
EOF
reject hmb
reject baseline
cat > "$TMP/os-root/config/grub.d/99-conflict.cfg" <<'EOF'
GRUB_CMDLINE_LINUX="boot=startos removed_diagnostic=1"
EOF
reject hmb
cat > "$TMP/os-root/config/grub.d/99-conflict.cfg" <<'EOF'
GRUB_CMDLINE_LINUX="$GRUB_CMDLINE_LINUX nvme_core.default_ps_max_latency_us=0"
EOF
reject apst
rm "$TMP/os-root/config/grub.d/99-conflict.cfg"
export FAIL_GENERATION=1
reject hmb
unset FAIL_GENERATION
export EMPTY_KERNELS=1
reject baseline
unset EMPTY_KERNELS
isolate baseline

mkdir -p "$TMP/proc" "$TMP/sys/module/nvme_core/parameters" "$TMP/sys/module/nvme/parameters" \
    "$TMP/sys/class/nvme/nvme2" "$TMP/sys/devices/0000:01:00.0"
printf 'boot=startos actual_boot=reported\n' > "$TMP/proc/cmdline"
printf '0\n' > "$TMP/sys/module/nvme_core/parameters/default_ps_max_latency_us"
printf '128\n' > "$TMP/sys/module/nvme/parameters/max_host_mem_size_mb"
ln -s "$TMP/sys/devices/0000:01:00.0" "$TMP/sys/class/nvme/nvme2/device"
cat > "$TMP/bin/nvme" <<'EOF'
#!/bin/sh
printf 'nvme %s\n' "$*" >> "$FIXTURE/collect-commands.log"
printf 'hmpre: 128\n'
EOF
cat > "$TMP/bin/lspci" <<'EOF'
#!/bin/sh
printf 'lspci %s\n' "$*" >> "$FIXTURE/collect-commands.log"
EOF
chmod +x "$TMP/bin/nvme" "$TMP/bin/lspci"
"$TMP/isolation" collect /dev/nvme2 > "$TMP/collect.log"
grep -q actual_boot=reported "$TMP/collect.log" || fail 'booted cmdline not collected'
grep -q 'max_host_mem_size_mb=128' "$TMP/collect.log" || fail 'HMB parameter not collected'
grep -q 'lspci -vvv -s 0000:01:00.0' "$TMP/collect-commands.log" || fail 'wrong PCIe device collected'
grep -q 'nvme get-feature /dev/nvme2 -f 0x0c -H' "$TMP/collect-commands.log" || fail 'wrong controller collected'
"$TMP/isolation" collect > "$TMP/collect.log"
grep -q 'Could not identify' "$TMP/collect.log" || fail 'missing PCIe device stopped collection'

printf 'GRUB override and NVMe isolation tests passed\n'
