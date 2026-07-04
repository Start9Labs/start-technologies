# SPDX-License-Identifier: GPL-2.0-only
#
# Copyright (C) 2024 Spacemit Ltd.

define Device/MUSE-N1
  DEVICE_VENDOR := Spacemit
  DEVICE_MODEL := N1 nas
  DEVICE_DTS_DIR:= ../dts
  DEVICE_DTS := k1-x_MUSE-N1
  SOC := KeyStone
  KERNEL_NAME := Image
  KERNEL_IMG := Image.itb
  KERNEL := kernel-bin | fit none
  IMAGES := pack.zip
  IMAGE/pack.zip := $(KERNEL_IMG) | boot-common | archive-zip
endef
TARGET_DEVICES += MUSE-N1
