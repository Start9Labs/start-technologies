#!/bin/bash
set -euo pipefail
cd "$(dirname -- "$0")/../../../.."
make --no-print-directory -f - check <<'MAKE'
include build/common.mk
required := build/rust-build-profile.sh \
 projects/start-os/build/lib/scripts/os-image-store \
 projects/start-os/build/lib/scripts/os-image-lock-name \
 projects/start-os/build/lib/scripts/os-image-lock-fd-name \
 projects/start-os/build/lib/scripts/os-public-source-cache-name \
 projects/start-os/build/lib/scripts/os-public-source-prune-name \
 projects/start-os/build/lib/scripts/fde-boot-mode \
 projects/start-os/build/lib/scripts/fde-boot-input-files \
 projects/start-os/build/lib/scripts/fde-descriptor-file \
 projects/start-os/build/dpkg-deps/uefi.platforms
missing := $(filter-out $(CORE_SRC),$(required))
ifneq ($(strip $(missing)),)
$(error Rust products do not track native build or embedded inputs: $(missing))
endif
.PHONY: check
check:
	@printf 'PASS: Rust product prerequisites retain native build and embedded OS inputs\n'
MAKE
