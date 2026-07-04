# SpacemiT K1 board-specific kernel module overrides
#
# The Armbian 6.18 vendor kernel uses "sha512.ko" instead of the mainline
# name "sha512_generic.ko". Override the FILES path so the kmod packaging
# system finds the correct module (or matches the correct built-in entry).

define KernelPackage/crypto-sha512/$(BOARD)
  FILES:=$(LINUX_DIR)/crypto/sha512.ko
  AUTOLOAD:=$(call AutoLoad,09,sha512)
endef
