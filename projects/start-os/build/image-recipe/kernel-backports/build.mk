KERNEL_BACKPORTS_DIR := projects/start-os/build/image-recipe/kernel-backports
KERNEL_DEB_ARCH ?= $(shell dpkg --print-architecture)

.PHONY: start-os-kernel-backports start-os-kernel-backports-test
start-os-kernel-backports:
	FORCE_COMPAT=1 REQUIRES=debian ./build/os-compat/run-compat.sh ./$(KERNEL_BACKPORTS_DIR)/build-kernel-backports.sh $(KERNEL_DEB_ARCH) results/kernel-backports/$(KERNEL_DEB_ARCH)

start-os-kernel-backports-test: $(wildcard $(KERNEL_BACKPORTS_DIR)/*.sh) $(KERNEL_BACKPORTS_DIR)/kernel-backports.conf
	./$(KERNEL_BACKPORTS_DIR)/test.sh

start-os-scripts-test: start-os-kernel-backports-test
