#!/bin/bash
set -eo pipefail

cd "$(git rev-parse --show-toplevel)"
PROJECT_DIR=projects/start-wrt

ARCH=${ARCH:-riscv64}
RUST_ARCH=${RUST_ARCH:-riscv64gc}
PROFILE=${PROFILE:-release}
# The Rust crates are members of the monorepo workspace, so the binary lands in
# the workspace-root target/ (not the old backend/target/).
RUST_TARGET_DIR="target/${RUST_ARCH}-unknown-linux-musl/${PROFILE}"
FILES_DIR="$PROJECT_DIR/openwrt/files"

echo "==> Staging files into ${FILES_DIR}..."

# Clean previous staging
rm -rf "${FILES_DIR}"

# Single binary + symlinks for backward compatibility
mkdir -p "${FILES_DIR}/usr/bin"
cp "${RUST_TARGET_DIR}/startwrt" "${FILES_DIR}/usr/bin/startwrt"
ln -s startwrt "${FILES_DIR}/usr/bin/startwrt-ctrld"
ln -s startwrt "${FILES_DIR}/usr/bin/startwrt-cli"

# UCI config defaults from firstboot_config/
mkdir -p "${FILES_DIR}/etc/config"
for f in "$PROJECT_DIR"/backend/firstboot_config/*; do
    cp "$f" "${FILES_DIR}/etc/config/$(basename "$f")"
done

# Procd init script for startwrt-ctrld
mkdir -p "${FILES_DIR}/etc/init.d"
cat > "${FILES_DIR}/etc/init.d/startwrt" << 'INITEOF'
#!/bin/sh /etc/rc.common

START=99
STOP=10

USE_PROCD=1

start_service() {
    procd_open_instance
    procd_set_param command /usr/bin/startwrt-ctrld
    procd_set_param respawn
    procd_set_param stdout 1
    procd_set_param stderr 1
    procd_close_instance
}
INITEOF
chmod +x "${FILES_DIR}/etc/init.d/startwrt"

# Custom SmartDNS init script — uses our generated config instead of the
# stock UCI-generated one. The stock init script generates its own config
# from UCI at /var/etc/smartdns/smartdns.conf, ignoring ours.
cat > "${FILES_DIR}/etc/init.d/smartdns" << 'SMARTDNSEOF'
#!/bin/sh /etc/rc.common
# StartWRT SmartDNS service — uses our generated config instead of UCI.
START=19
STOP=82
USE_PROCD=1

CONF="/etc/smartdns/smartdns.conf"

start_service() {
    [ -f "$CONF" ] || return
    procd_open_instance
    procd_set_param command /usr/sbin/smartdns -f -c "$CONF"
    procd_set_param respawn
    procd_close_instance
}
SMARTDNSEOF
chmod +x "${FILES_DIR}/etc/init.d/smartdns"

# Serial console dispatcher — prints a hint when booted in setup mode,
# then drops to login. WiFi provisioning is handled by startwrt-ctrld
# reading EEPROM tag 0x2F at boot; nothing here calls into startwrt-cli.
mkdir -p "${FILES_DIR}/usr/sbin"
cat > "${FILES_DIR}/usr/sbin/startwrt-serial" << 'SERIALEOF'
#!/bin/sh
# Connect stdin/stdout/stderr to the console device.
# Required because procd's "respawn" action (unlike "askconsole") does not
# set up a controlling terminal for the process.
exec </dev/console >/dev/console 2>&1

# Suppress kernel messages on the serial console — printk writes directly
# to the UART and corrupts terminal I/O (echo, input) for interactive use.
dmesg -n 1

# Determine the boot block device from root= in /proc/cmdline
boot_part=$(cat /proc/cmdline | tr ' ' '\n' | sed -n 's|^root=/dev/||p')
case "$boot_part" in
    mmcblk*p[0-9]*) boot_dev="${boot_part%p[0-9]*}" ;;
    sd*[0-9]*)      boot_dev=$(echo "$boot_part" | sed 's/[0-9]*$//') ;;
    *)              boot_dev="" ;;
esac

# Check device type via sysfs: eMMC reports "MMC", SD cards report "SD"
boot_type=""
[ -n "$boot_dev" ] && [ -f "/sys/block/$boot_dev/device/type" ] && \
    boot_type=$(cat "/sys/block/$boot_dev/device/type")

if [ "$boot_type" != "MMC" ]; then
    # Booted from removable media — setup wizard mode.
    echo ""
    echo "========================================"
    echo "   StartWRT Setup Mode"
    echo "========================================"
    echo ""
    echo "Connect to WiFi 'StartWRT' to run the setup wizard."
    echo "If WiFi is not broadcasting, the board's EEPROM has no"
    echo "WiFi password — connect via ethernet to run the wizard."
    echo ""
    echo "Log in below for advanced options."
    echo ""
fi

exec /bin/login
SERIALEOF
chmod +x "${FILES_DIR}/usr/sbin/startwrt-serial"

# Override inittab to use respawnlate for the serial dispatcher.
# respawnlate runs in STATE_RUNNING (after sysinit completes), so hostname,
# ubus, and all init.d services are available before startwrt-serial starts.
# It also auto-starts without requiring Enter (unlike askconsole) and restarts
# the process on exit. askfirst on an explicit ttyS0 doesn't work on this
# hardware — Enter input is never received.
mkdir -p "${FILES_DIR}/etc"
cat > "${FILES_DIR}/etc/inittab" << 'INITTABEOF'
::sysinit:/etc/init.d/rcS S boot
::shutdown:/etc/init.d/rcS K shutdown
::respawnlate:/usr/sbin/startwrt-serial
INITTABEOF

# Hotplug script for remote access re-evaluation on WAN IP change
mkdir -p "${FILES_DIR}/etc/hotplug.d/iface"
cp "$PROJECT_DIR"/backend/hotplug/99-startwrt-remote-access "${FILES_DIR}/etc/hotplug.d/iface/99-startwrt-remote-access"
chmod +x "${FILES_DIR}/etc/hotplug.d/iface/99-startwrt-remote-access"
cp "$PROJECT_DIR"/backend/hotplug/99-startwrt-proxy-arp "${FILES_DIR}/etc/hotplug.d/iface/99-startwrt-proxy-arp"
chmod +x "${FILES_DIR}/etc/hotplug.d/iface/99-startwrt-proxy-arp"
cp "$PROJECT_DIR"/backend/hotplug/99-startwrt-published-ports "${FILES_DIR}/etc/hotplug.d/iface/99-startwrt-published-ports"
chmod +x "${FILES_DIR}/etc/hotplug.d/iface/99-startwrt-published-ports"

# Custom nftables rules auto-included by fw4 (/etc/nftables.d/*.nft).
# 10-startwrt-dnat-mark.nft marks DNAT-state reply traffic so port-forward
# replies route via the main table instead of a VPN tunnel.
mkdir -p "${FILES_DIR}/etc/nftables.d"
for f in "$PROJECT_DIR"/backend/nftables/*.nft; do
    cp "$f" "${FILES_DIR}/etc/nftables.d/$(basename "$f")"
done

# sysupgrade keep.d — additional files to include in config backups
mkdir -p "${FILES_DIR}/lib/upgrade/keep.d"
cat > "${FILES_DIR}/lib/upgrade/keep.d/startwrt" << 'KEEPEOF'
/etc/ssl/certs/startwrt-ca.pem
/etc/ssl/certs/startwrt-int.pem
/etc/ssl/certs/startwrt-server.pem
/etc/ssl/private/startwrt-ca.key
/etc/ssl/private/startwrt-int.key
/etc/ssl/private/startwrt-server.key
/etc/nlbwmon/data/
/etc/startwrt/pending-update
# Persistent device-name cache. Written atomically (temp + rename), so a live
# `sysupgrade --create-backup` always captures one complete JSON document.
/etc/startwrt/device_names.json
KEEPEOF

echo "==> Staging complete."
