# SPDX-License-Identifier: GPL-2.0-only
#
# Copyright (C) 2024 Spacemit Ltd.
#
include $(TOPDIR)/rules.mk

ARCH:=riscv64
BOARD:=spacemit
BOARDNAME:=Spacemit RISC-V Chipsets
FEATURES:=ext4 squashfs
KERNELNAME:=Image dtbs
SUBTARGETS:=k1-sbc

KERNEL_PATCHVER:=6.18

include $(INCLUDE_DIR)/target.mk

DEFAULT_PACKAGES += mkf2fs e2fsprogs

define Target/Description
	Build firmware images for the SpaceMit K1 boards
endef

$(eval $(call BuildTarget))
