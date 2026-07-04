# SPDX-License-Identifier: GPL-2.0-only
#
# Copyright (C) 2024 Spacemit LTD.
#

include $(TOPDIR)/rules.mk
include $(INCLUDE_DIR)/kernel.mk
include $(INCLUDE_DIR)/u-boot.mk

INTERNAL_ENV:=$(shell echo $${OPENWRT_INTERNAL_ENV})

ifeq ($(INTERNAL_ENV), yes)
PKG_SOURCE_PROTO:=local
PKG_RELEASE:=1
PKG_VERSION:=2022.10
LOCAL_SOURCE_DIR=$(TOPDIR)/../bsp-src/uboot-2022.10
else
PKG_NAME:=uboot
PKG_RELEASE:=1
PKG_VERSION:=2022.10

PKG_SOURCE_PROTO:=git
PKG_SOURCE_URL:=https://github.com/spacemit-com/uboot-2022.10.git
PKG_SOURCE_VERSION:=ad097c0e1d191a1d3820b2babe6ddc975c68577c
PKG_MIRROR_HASH:=skip
endif

include $(INCLUDE_DIR)/package.mk

define U-Boot/Default
  BUILD_TARGET:=spacemit
  BUILD_DEVICES=$(1)
  UBOOT_IMAGE:=u-boot.itb
  DTS_DIR:=arch/riscv/dts
  UENV:=default
  DEFAULT:=y
endef

define U-Boot/k1
  NAME:=K1 deb1
  OPENSBI:=generic
  DEPENDS:=+opensbi_spacemit
  BUILD_DEVICES:=k1_deb1
endef

UBOOT_TARGETS := \
	k1

ifeq ($(INTERNAL_ENV), yes)

define Build/Configure
	$(MAKE) -C $(LOCAL_SOURCE_DIR) k1_defconfig
endef

define Build/Compile
	+$(UBOOT_MAKE_FLAGS) $(MAKE) -j4 CROSS_COMPILE=$(TARGET_CROSS) -C $(LOCAL_SOURCE_DIR)
endef

define Build/Clean
	$(MAKE) -C $(LOCAL_SOURCE_DIR) clean
endef

define Build/InstallDev
	$(INSTALL_DIR) $(STAGING_DIR_IMAGE)
	$(INSTALL_DIR) $(STAGING_DIR_IMAGE)/factory

	$(INSTALL_BIN) $(LOCAL_SOURCE_DIR)/$(UBOOT_IMAGE) $(STAGING_DIR_IMAGE)/
	$(INSTALL_BIN) $(LOCAL_SOURCE_DIR)/$(UBOOT_IMAGE) $(PKG_BUILD_DIR)/
	$(INSTALL_BIN) $(LOCAL_SOURCE_DIR)/FSBL.bin $(STAGING_DIR_IMAGE)/factory/
	$(INSTALL_BIN) $(LOCAL_SOURCE_DIR)/bootinfo*.bin $(STAGING_DIR_IMAGE)/factory/
	$(INSTALL_BIN) $(LOCAL_SOURCE_DIR)/u-boot-env-default.bin $(STAGING_DIR_IMAGE)/env.bin

endef

else

define Build/Configure
	$(call Build/Configure/U-Boot)
	$(MAKE) -C $(PKG_BUILD_DIR) k1_defconfig
endef

define Build/InstallDev
	$(INSTALL_DIR) $(STAGING_DIR_IMAGE)
	$(INSTALL_DIR) $(STAGING_DIR_IMAGE)/factory

	$(INSTALL_BIN) $(PKG_BUILD_DIR)/$(UBOOT_IMAGE) $(STAGING_DIR_IMAGE)/
	$(INSTALL_BIN) $(PKG_BUILD_DIR)/FSBL.bin $(STAGING_DIR_IMAGE)/factory/
	$(INSTALL_BIN) $(PKG_BUILD_DIR)/bootinfo*.bin $(STAGING_DIR_IMAGE)/factory/
	$(INSTALL_BIN) $(PKG_BUILD_DIR)/u-boot-env-default.bin $(STAGING_DIR_IMAGE)/env.bin

endef

endif

$(eval $(call BuildPackage/U-Boot))
