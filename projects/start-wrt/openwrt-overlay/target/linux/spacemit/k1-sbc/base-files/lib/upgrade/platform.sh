# GPT partition map of the SD-card image (see
# image/partition_tables/partition_universal.json):
#
#   p1 fsbl   p2 env   p3 opensbi   p4 uboot   p5 bootfs (FAT)
#   p6 rootfs (squashfs)   p7 rootfs_data (ext4 overlay)
#
# sysupgrade writes every firmware partition (p1-p6) in place from the image
# and keeps the existing GPT, so all partition sizes are preserved. p7 is the
# overlay: it is grown to fill the eMMC on first boot, the image carries no
# data for it, and it is reformatted here. It is therefore excluded both from
# the partition-table comparison and from the per-partition copy below -- that
# is the one accommodation this target makes over a stock OpenWrt block
# upgrade.

platform_check_image() {
	local diskdev diff

	export_bootdevice && export_partdevice diskdev 0 || {
		echo "Unable to determine upgrade device"
		return 1
	}

	get_partitions "/dev/$diskdev" bootdisk

	#extract the boot sector from the image
	get_image "$@" | dd of=/tmp/image.bs count=1 bs=512b 2>/dev/null

	get_partitions /tmp/image.bs image

	#compare tables, ignoring the rootfs_data overlay (7) whose size
	#legitimately differs between the running system and the image
	grep -v '^ *7 ' /tmp/partmap.bootdisk > /tmp/partmap.bootdisk.cmp
	grep -v '^ *7 ' /tmp/partmap.image   > /tmp/partmap.image.cmp
	diff="$(grep -F -x -v -f /tmp/partmap.bootdisk.cmp /tmp/partmap.image.cmp)"

	rm -f /tmp/image.bs /tmp/partmap.*

	if [ -n "$diff" ]; then
		echo "Partition layout has changed. The full image will be written"
		echo "and the overlay reset to its default size."
		ask_bool 0 "Abort" && exit 1
		return 0
	fi
}

platform_copy_config() {
	local partdev

	# p5 is the FAT "bootfs" partition. Park the saved config tarball there
	# so 79_move_config can restore it into the fresh overlay on first boot.
	if export_partdevice partdev 5; then
		mount -t vfat -o rw,noatime "/dev/$partdev" /mnt
		cp -af "$UPGRADE_BACKUP" "/mnt/$BACKUP_FILE"
		umount /mnt
	fi
}

platform_do_upgrade() {
	local diskdev partdev part start size diff

	export_bootdevice && export_partdevice diskdev 0 || {
		echo "Unable to determine upgrade device"
		return 1
	}

	sync

	if [ "$UPGRADE_OPT_SAVE_PARTITIONS" = "1" ]; then
		get_partitions "/dev/$diskdev" bootdisk

		#extract the boot sector from the image
		get_image "$@" | dd of=/tmp/image.bs count=1 bs=512b 2>/dev/null

		get_partitions /tmp/image.bs image

		#compare tables, ignoring the rootfs_data overlay (7): it is grown
		#to fill the eMMC on first boot, so its size differs between the
		#running system and the freshly-built image
		grep -v '^ *7 ' /tmp/partmap.bootdisk > /tmp/partmap.bootdisk.cmp
		grep -v '^ *7 ' /tmp/partmap.image   > /tmp/partmap.image.cmp
		diff="$(grep -F -x -v -f /tmp/partmap.bootdisk.cmp /tmp/partmap.image.cmp)"
	else
		diff=1
	fi

	if [ -n "$diff" ]; then
		# Non-overlay layout changed (or partition preservation disabled):
		# write the whole image, as upstream does. This rewrites the GPT,
		# so the overlay is reset to the image's build-time size.
		get_image "$@" | dd of="/dev/$diskdev" bs=4096 conv=fsync

		# Separate removal and addition is necessary; otherwise partition 1
		# would be missing if it overlaps with the old partition 2.
		partx -d - "/dev/$diskdev"
		partx -a - "/dev/$diskdev"
	else
		# Layout matches: write each firmware partition from the image in
		# place -- bootloader included -- but skip the overlay (7). The
		# image carries no overlay data, and the GPT is left untouched so
		# every partition keeps its current size, including the grown
		# overlay.
		while read part start size; do
			[ "$part" = "7" ] && continue
			if export_partdevice partdev "$part"; then
				echo "Writing partition $part to /dev/$partdev..."
				get_image "$@" | dd of="/dev/$partdev" ibs=512 obs=1M skip="$start" count="$size" conv=fsync
			else
				echo "Unable to find partition $part device, skipped."
			fi
		done < /tmp/partmap.image
	fi

	rm -f /tmp/image.bs /tmp/partmap.*

	# Reset the overlay so the new rootfs starts clean. sysupgrade keeps
	# settings by restoring a saved config tarball into a *fresh* overlay
	# (see platform_copy_config / 79_move_config). Reformatting at the
	# partition's current size covers both paths above: a preserved (grown)
	# overlay, and one just shrunk by a full-image write.
	if export_partdevice partdev 7; then
		echo "Resetting overlay /dev/$partdev..."
		mkfs.ext4 -F -L rootfs_data "/dev/$partdev"
	fi
}
