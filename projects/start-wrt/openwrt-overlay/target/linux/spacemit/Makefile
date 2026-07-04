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

# Armbian forward-ported vendor kernel (mainline 6.18.19 + SpacemiT patches)
# Must be set BEFORE include target.mk so kernel.mk evaluates them correctly.
CONFIG_KERNEL_GIT_CLONE_URI:=https://github.com/jmontleon/linux-bianbu.git
CONFIG_KERNEL_GIT_REF:=linux-6.18.y
CONFIG_KERNEL_GIT_LOCAL_REPOSITORY:=""

include $(INCLUDE_DIR)/target.mk

DEFAULT_PACKAGES += mkf2fs e2fsprogs

define Target/Description
	Build firmware images for the SpaceMit K1 boards
endef

$(eval $(call BuildTarget))
