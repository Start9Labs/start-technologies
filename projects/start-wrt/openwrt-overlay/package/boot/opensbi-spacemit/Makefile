# SPDX-License-Identifier: GPL-2.0-only
#
# Copyright (C) 2024 Spacemit Ltd.
#

include $(TOPDIR)/rules.mk

INTERNAL_ENV:=$(shell echo $${OPENWRT_INTERNAL_ENV})

ifeq ($(INTERNAL_ENV), yes)
PKG_NAME:=local_opensbi
PKG_RELEASE:=1
PKG_VERSION:=1.3
LOCAL_SOURCE_DIR=$(TOPDIR)/../bsp-src/opensbi
else
PKG_NAME:=opensbi
PKG_RELEASE:=1
PKG_VERSION:=1.3

PKG_SOURCE_PROTO:=git
PKG_SOURCE_URL:=https://github.com/spacemit-com/opensbi.git
PKG_SOURCE_VERSION:=0596140e025b02417786806f707df29d71307bda
PKG_MIRROR_HASH:=skip
endif

PKG_TARGETS:=bin
PKG_FLAGS:=nonshared
PKG_LICENSE:=BSD-2-Clause
PKG_LICENSE_FILES:=COPYING.BSD
PKG_BUILD_PARALLEL:=1


include $(INCLUDE_DIR)/package.mk

define Package/opensbi
    SECTION:=boot
    CATEGORY:=Boot Loaders
    DEPENDS:=@TARGET_spacemit
    VARIANT:=$(subst _,/,$(subst opensbi_,,$(1)))
    TITLE:=Spacemit customized version of OpenSBI
    OPENSBI_IMAGE:=
    PLAT:=
endef

define Package/opensbi_spacemit
  $(Package/opensbi)
  TITLE:=Spacemit customized version of OpenSBI
  OPENSBI_IMAGE:=fw_dynamic.itb
  PLAT:=generic
endef

export GCC_HONOUR_COPTS=s

MAKE_VARS = \
	CROSS_COMPILE="$(TARGET_CROSS)"

ifeq ($(INTERNAL_ENV), yes)

define Build/Compile
	$(eval $(Package/opensbi_$(BUILD_VARIANT))) \
		+$(MAKE_VARS) $(MAKE) -C $(LOCAL_SOURCE_DIR) \
		PLATFORM=$(PLAT) PLATFORM_DEFCONFIG=k1_defconfig
endef

define Build/Clean
	$(MAKE) -C $(LOCAL_SOURCE_DIR) clean
endef

define Build/InstallDev
	$(INSTALL_DIR) $(STAGING_DIR_IMAGE)
	$(CP) $(LOCAL_SOURCE_DIR)/build/platform/$(PLAT)/firmware/fw_dynamic.itb $(STAGING_DIR_IMAGE)/
endef

$(eval $(call BuildPackage,opensbi_spacemit))

else

define Build/Compile
	$(eval $(Package/opensbi_$(BUILD_VARIANT))) \
		+$(MAKE_VARS) $(MAKE) -C $(PKG_BUILD_DIR) \
		PLATFORM=$(PLAT) PLATFORM_DEFCONFIG=k1_defconfig
endef

define Build/Clean
	$(MAKE) -C $(PKG_BUILD_DIR) clean
endef

define Build/InstallDev
	$(INSTALL_DIR) $(STAGING_DIR_IMAGE)
	$(CP) $(PKG_BUILD_DIR)/build/platform/$(PLAT)/firmware/fw_dynamic.itb $(STAGING_DIR_IMAGE)/
endef

$(eval $(call BuildPackage,opensbi_spacemit))

endif

