# Provision the eMMC hardware boot partitions (boot0/boot1) with the image's
# own bootinfo + FSBL.
#
# The K1 BootROM boots eMMC from boot0 (an 80-byte bootinfo header at offset
# 0 pointing at the FSBL/u-boot-SPL, mirrored in boot1), which only factory
# provisioning ever wrote historically. sysupgrade rewrites OpenSBI/u-boot in
# the user area on every upgrade, so the FSBL must move in lockstep or an old
# factory FSBL is left chain-loading boot stages it was never tested with.
#
# Convergent and idempotent: after the image partitions are written, compare
# boot0/boot1 against the FSBL from the (just-written) `fsbl` partition and
# `bootinfo_emmc.bin` from the `bootfs` partition; write only on mismatch, in
# commit-safe order (boot1 mirror, boot0 FSBL, bootinfo sector last as the
# single-sector atomic commit), each write read back and compared. Failures
# are logged but never abort the upgrade — the user area is already written,
# and a board with a valid old boot0 still boots.
#
# Two callers: sysupgrade stage2 (both subtargets' platform_do_upgrade) and
# every boot (/lib/preinit/80_provision_boot0). Both impose the same contract:
# never fail hard, scratch space in /tmp only, busybox-only tools — and the
# stage2 ramfs binary list additionally lacks cmp, hence md5sum comparisons.
#
# Mirrors the Rust implementation in backend/ctrl/src/boot0.rs (wizard flash
# path); keep the two in sync.

_boot0_read_u32() { # <file> <byte offset>
	hexdump -s "$2" -n 4 -e '1/4 "%u"' "$1"
}

# Compare <sector count> sectors of <dev> at <sector offset> against
# <reference file>. Uses $_boot0_direct (set by the caller) to bypass the
# page cache where supported. Compared via md5sum, not cmp: the sysupgrade
# ramfs (stage2 switch_to_ramfs) copies a fixed binary list that includes
# md5sum but NOT cmp.
_boot0_region_matches() { # <dev> <sector offset> <sector count> <reference file>
	local want have
	want="$(md5sum < "$4")"
	have="$(dd if="$1" bs=512 skip="$2" count="$3" $_boot0_direct 2>/dev/null | md5sum)"
	[ "${want%% *}" = "${have%% *}" ]
}

_boot0_write_verified() { # <dev> <sector offset> <source file>
	local sectors=$(( ($(wc -c < "$3") + 511) / 512 ))
	dd if="$3" of="$1" bs=512 seek="$2" conv=fsync 2>/dev/null || return 1
	_boot0_region_matches "$1" "$2" "$sectors" "$3"
}

spacemit_provision_boot0() {
	local diskdev="$1"
	local boot0="/dev/${diskdev}boot0" boot1="/dev/${diskdev}boot1"
	local fsbl_dev bootfs_dev spl0 spl1 fsbl_sectors _boot0_direct=""
	local mnt="/tmp/spacemit_bootfs"
	local bootinfo="/tmp/bootinfo_emmc.bin" fsbl="/tmp/spacemit_fsbl.bin"
	local sector0="/tmp/spacemit_bootinfo_sector.bin"

	# Nothing to provision when the target has no eMMC boot partitions
	# (e.g. sysupgrade of an SD-booted system).
	[ -b "$boot0" ] && [ -b "$boot1" ] || return 0

	fsbl_dev="$(find_mmc_part fsbl "$diskdev")"
	bootfs_dev="$(find_mmc_part bootfs "$diskdev")"
	if [ -z "$fsbl_dev" ] || [ -z "$bootfs_dev" ]; then
		echo "boot0: fsbl/bootfs partition not found, skipping provisioning"
		return 0
	fi

	mkdir -p "$mnt"
	mount -t vfat -o ro "$bootfs_dev" "$mnt" || {
		echo "boot0: cannot mount $bootfs_dev, skipping provisioning"
		return 0
	}
	if [ ! -f "$mnt/bootinfo_emmc.bin" ]; then
		umount "$mnt"
		echo "boot0: image carries no bootinfo_emmc.bin, skipping provisioning"
		return 0
	fi
	cp -f "$mnt/bootinfo_emmc.bin" "$bootinfo"
	umount "$mnt"

	# Sanity-check the blob: magic 0xb00714f0 (= 2953254128), flash type
	# "eMMC", and sector-aligned SPL offsets (bytes 0x20/0x24).
	if [ "$(_boot0_read_u32 "$bootinfo" 0)" != "2953254128" ] ||
	   [ "$(dd if="$bootinfo" bs=1 skip=8 count=4 2>/dev/null)" != "eMMC" ]; then
		echo "boot0: invalid bootinfo_emmc.bin, skipping provisioning"
		return 0
	fi
	spl0=$(_boot0_read_u32 "$bootinfo" 32)
	spl1=$(_boot0_read_u32 "$bootinfo" 36)
	if [ $((spl0 % 512)) -ne 0 ] || [ $((spl1 % 512)) -ne 0 ]; then
		echo "boot0: unaligned SPL offsets in bootinfo, skipping provisioning"
		return 0
	fi

	dd if="$fsbl_dev" of="$fsbl" bs=64k 2>/dev/null
	fsbl_sectors=$(( $(wc -c < "$fsbl") / 512 ))
	if [ "$fsbl_sectors" -eq 0 ]; then
		echo "boot0: empty fsbl partition, skipping provisioning"
		return 0
	fi

	# The 80-byte bootinfo zero-padded to a full sector, as the image lays
	# out its own bootinfo at LBA 0.
	dd if=/dev/zero of="$sector0" bs=512 count=1 2>/dev/null
	dd if="$bootinfo" of="$sector0" conv=notrunc 2>/dev/null

	# Verification reads bypass the page cache where busybox dd supports it.
	dd if="$boot0" of=/dev/null bs=512 count=1 iflag=direct 2>/dev/null &&
		_boot0_direct="iflag=direct"

	# Gate: skip if boot0/boot1 already hold exactly the intended bytes.
	if _boot0_region_matches "$boot0" 0 1 "$sector0" &&
	   _boot0_region_matches "$boot0" $((spl0 / 512)) "$fsbl_sectors" "$fsbl" &&
	   _boot0_region_matches "$boot1" $((spl1 / 512)) "$fsbl_sectors" "$fsbl"; then
		echo "boot0: eMMC boot firmware already up to date"
		return 0
	fi

	echo "boot0: writing eMMC boot firmware to $boot0/$boot1..."
	echo 0 > "/sys/block/${diskdev}boot0/force_ro"
	echo 0 > "/sys/block/${diskdev}boot1/force_ro"

	# Commit-safe order: boot1 mirror, boot0 FSBL, bootinfo sector last.
	if _boot0_write_verified "$boot1" $((spl1 / 512)) "$fsbl" &&
	   _boot0_write_verified "$boot0" $((spl0 / 512)) "$fsbl" &&
	   _boot0_write_verified "$boot0" 0 "$sector0"; then
		echo "boot0: eMMC boot firmware written and verified"
	else
		echo "boot0: WARNING: eMMC boot firmware write failed" \
			"(user area is intact; re-run sysupgrade or reflash from microSD)"
	fi

	echo 1 > "/sys/block/${diskdev}boot0/force_ro"
	echo 1 > "/sys/block/${diskdev}boot1/force_ro"
	return 0
}
