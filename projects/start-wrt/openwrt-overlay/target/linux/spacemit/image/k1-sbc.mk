# SPDX-License-Identifier: GPL-2.0-only
#
# Copyright (C) 2024 Spacemit Ltd.

define Device/bananapi-f3
  DEVICE_VENDOR := Banana Pi
  DEVICE_MODEL := BPI-F3
  DEVICE_DTS_DIR := $(DTS_DIR)/spacemit
  DEVICE_DTS := k1-x_deb1
  # The k1-x_deb1.dts root only exposes the SoC-level compatible
  # "spacemit,k1-x", so board_detect populates /tmp/sysinfo/board_name with
  # that string at boot. Without listing it here, sysupgrade's metadata_check
  # rejects every OTA on this hardware ("Device spacemit,k1-x not supported
  # by this image"). Include the SoC compat and the future board-level
  # compat alongside the profile name so the image is accepted regardless
  # of which identifier the DTS happens to expose.
  SUPPORTED_DEVICES := bananapi-f3 spacemit,k1-x bananapi,bpi-f3
  FILESYSTEMS := squashfs
  SOC := KeyStone
  KERNEL_NAME := Image
  KERNEL_IMG := Image.itb
  KERNEL := kernel-bin | fit none $$(KDIR)/image-$$(firstword $$(DEVICE_DTS)).dtb
  IMAGES := sdcard.img sysupgrade.img.gz
  IMAGE/sdcard.img := boot-common | sdcard-img
  IMAGE/sysupgrade.img.gz := boot-common | sdcard-img | gzip | append-metadata
endef
TARGET_DEVICES += bananapi-f3

