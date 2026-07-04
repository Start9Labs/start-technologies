# SPDX-License-Identifier: GPL-2.0-only
#
# Copyright (C) 2024 Spacemit Ltd.
#
include $(TOPDIR)/rules.mk
include $(INCLUDE_DIR)/image.mk

#note Config.in together
#FAT32_BLOCK_SIZE=1024
#FAT32_BLOCKS=$(shell echo $$(($(CONFIG_SPACEMIT_SD_BOOT_PARTSIZE)*1024*1024/$(FAT32_BLOCK_SIZE))))

#get bootfs part size from json file, use partition_universal.json as sdcard partition table default
PARTITION_FILE=$(PWD)/image/partition_tables/partition_universal.json
BOOTFS_SIZE := $(shell jq -r '.partitions[] | select(.name=="bootfs") | .size' $(PARTITION_FILE))
BOOTFS_SIZE_KB := $(shell echo "$(BOOTFS_SIZE)" | sed 's/M/*1024/' | bc)

KERNEL_LOADADDR:=0x1400000
define Build/boot-common
	rm -f $@.boot
	mkfs.fat -n boot -C $@.boot $(BOOTFS_SIZE_KB)
	
	#mcopy -i $@.boot $(KDIR_TMP)/$(KERNEL_INITRAMFS_IMAGE) ::$(KERNEL_IMG)
	mcopy -i $@.boot $(IMAGE_KERNEL) ::$(KERNEL_IMG)
	mcopy -i $@.boot env_k1-x.txt ::
	$(foreach dts,$(shell echo $(DEVICE_DTS)),mcopy -i $@.boot $(KDIR)/image-$(dts).dtb ::$(dts).dtb;)

	# boot-common runs once per IMAGE/ variant (sdcard.img and
	# sysupgrade.img.gz), and KDIR_TMP is shared between them. Under a
	# parallel build the two recipes race here, so use rsync rather than
	# cp -rf: rsync stages each file under a temp name and renames it
	# atomically, and merges into pre-existing directories without error.
	rsync -a $(STAGING_DIR_IMAGE)/ $(KDIR_TMP)/
endef

define Build/sdcard-img
	./gen_spacemit_sdcard_img.sh $@ $@.boot $(IMAGE_ROOTFS) \
		$(PARTITION_FILE) $(CONFIG_TARGET_ROOTFS_PARTSIZE)
	mv $@-sdcard.img $@
endef

define Build/archive-zip
	rm -f $@

	cp -f $(IMAGE_ROOTFS) $(KDIR_TMP)/rootfs.squashfs
	cp -f $@.boot $(KDIR_TMP)/bootfs.img
	cp -f partition_tables/* $(KDIR_TMP)/

	./gen_spacemit_archive_zip.sh \
		$@
endef

define Device/Default
  PROFILES := Default
  KERNEL_NAME := Image
  KERNEL_IMG := Image.itb
  KERNEL := kernel-bin | fit none
  IMAGES := sdcard.img
  IMAGE/sdcard.img := boot-common | sdcard-img
endef

include $(SUBTARGET).mk

$(eval $(call BuildImage))
