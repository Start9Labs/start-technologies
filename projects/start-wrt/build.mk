# When this product's build inputs change, mirror them into the `paths:` filter
# of .github/workflows/start-wrt.yaml (see root AGENTS.md "Coupled changes").
#
# UNVALIDATED: the riscv dockerized zigbuild and the OpenWrt image assembly
# (stage / image targets) have NOT been run since the monorepo migration. The
# binary+web build (the `startwrt` target) is the first thing to validate on a
# build host; the image targets follow. See projects/start-wrt/CONTRIBUTING.md.

STARTWRT_DIR := projects/start-wrt
STARTWRT_RUST_ARCH := riscv64gc
STARTWRT_ARCH := riscv64

STARTWRT_BIN := target/$(STARTWRT_RUST_ARCH)-unknown-linux-musl/$(PROFILE)/startwrt
STARTWRT_WEB_DIST := $(STARTWRT_DIR)/web/dist/startwrt/browser/index.html
STARTWRT_WEB_CONFIG := $(STARTWRT_DIR)/web/config.json
STARTWRT_GIT_HASH_FILE := $(STARTWRT_DIR)/build/env/GIT_HASH.txt

# Refresh GIT_HASH.txt on every make invocation (parse-time side-effect) so the
# stamp tracks HEAD; the file is then a prereq of config.json below, forcing a
# re-stamp whenever HEAD moves. (check-git-hash.sh rewrites the file only when
# the hash actually changes, so this is a no-op when HEAD is unchanged.)
_ := $(shell ./$(STARTWRT_DIR)/build/env/check-git-hash.sh)

STARTWRT_RUST_SRC := $(call ls-files, $(STARTWRT_DIR)/backend)
# Shared crates the backend path-depends on (see backend/ctrl/Cargo.toml):
# start-core (aliased `startos`, covered with patch-db by CORE_SRC from
# build/common.mk), plus rpc-toolkit and imbl-value. Without these prereqs a
# shared-crate edit leaves `make startwrt`/`startwrt-update` with a stale binary.
STARTWRT_SHARED_RUST_SRC := $(CORE_SRC) \
	$(call ls-files, shared-libs/crates/rpc-toolkit) \
	$(call ls-files, shared-libs/crates/imbl-value)
STARTWRT_WEB_SRC := $(call ls-files, $(STARTWRT_DIR)/web)

STARTWRT_OPENWRT := $(STARTWRT_DIR)/openwrt
STARTWRT_IMAGE_DIR := $(STARTWRT_OPENWRT)/bin/targets/spacemit

# Release-asset basename, following the startos convention from
# build/env/basename.sh (<project>-<version>-<githash7>_<platform>) — composed
# here from start-wrt's own stamps rather than by calling basename.sh, which
# reads StartOS's PLATFORM/ENVIRONMENT/GIT_HASH build state and a manifest path
# that doesn't exist for start-wrt. The version sed matches the "Determine
# version" step in .github/workflows/start-wrt.yaml (same manifest, same
# anchored pattern); the hash comes from the parse-time-refreshed stamp above
# (truncation to 7 chars drops any -modified marker, as basename.sh does).
STARTWRT_VERSION := $(shell sed -n 's/^version = "\([^"]*\)".*/\1/p' $(STARTWRT_DIR)/backend/ctrl/Cargo.toml | head -1)
STARTWRT_SHORT_HASH := $(shell head -c 7 $(STARTWRT_GIT_HASH_FILE))
STARTWRT_BASENAME := startwrt-$(STARTWRT_VERSION)-$(STARTWRT_SHORT_HASH)_spacemit-k1

# OpenWrt's own output names (what `make -C openwrt` produces)…
STARTWRT_SDCARD_SRC := openwrt-spacemit-k1-sbc-bananapi-f3-squashfs-sdcard.img
STARTWRT_SYSUPGRADE_SRC := openwrt-spacemit-k1-sbc-bananapi-f3-squashfs-sysupgrade.img.gz
# …renamed on copy into results/ to the release-asset names. Keep the
# -sdcard.img / -sysupgrade.img.gz endings: scripts/manage-release.sh and the
# registry-kind inference match on those suffixes/extensions.
STARTWRT_SDCARD_NAME := $(STARTWRT_BASENAME)-sdcard.img
STARTWRT_SYSUPGRADE_NAME := $(STARTWRT_BASENAME)-sysupgrade.img.gz
STARTWRT_IMAGES := results/$(STARTWRT_SDCARD_NAME) results/$(STARTWRT_SYSUPGRADE_NAME)

# Remote deploy target (binary only — web UI is embedded in the binary).
STARTWRT_REMOTE ?= root@192.168.0.1

.PHONY: startwrt
# Build the startwrt binary (embeds the web UI). Cross-compiled for the K1 via
# the dockerized cargo-zigbuild toolchain in build/build-rust.sh.
startwrt: $(STARTWRT_BIN)

$(STARTWRT_BIN): $(STARTWRT_RUST_SRC) $(STARTWRT_SHARED_RUST_SRC) Cargo.toml Cargo.lock $(STARTWRT_WEB_DIST) $(STARTWRT_DIR)/build/build-rust.sh
	ARCH=$(STARTWRT_ARCH) RUST_ARCH=$(STARTWRT_RUST_ARCH) PROFILE=$(PROFILE) ./$(STARTWRT_DIR)/build/build-rust.sh
	@touch $(STARTWRT_BIN)

# --- web (Angular project in the root workspace; built via `npm run build:wrt`) ---
# Shares the workspace deps/build:deps machinery with the other apps: WEB_SHARED_SRC
# and .angular/.updated carry the shared libs + the @start9labs/start-core / patch-db
# client file: deps (defined in shared-libs/ts-modules/build.mk). $(STARTWRT_WEB_CONFIG)
# is start-wrt's own runtime config.json (separate from the root workspace config.json).
$(STARTWRT_WEB_DIST): $(STARTWRT_WEB_SRC) $(WEB_SHARED_SRC) .angular/.updated $(STARTWRT_WEB_CONFIG)
	npm --prefix . run build:wrt
	touch $(STARTWRT_WEB_DIST)

# Prod config: useMocks=false + gitHash stamped from the product's own
# build/env/GIT_HASH.txt (independent of the monorepo-root stamp). GIT_HASH.txt
# is a prereq so config.json is re-stamped whenever HEAD moves; it's refreshed
# at parse time via the $(shell ...) above.
$(STARTWRT_WEB_CONFIG): $(STARTWRT_GIT_HASH_FILE) $(STARTWRT_DIR)/web/config-sample.json $(STARTWRT_DIR)/web/update-config.sh
	./$(STARTWRT_DIR)/web/update-config.sh

# Run start-wrt's Rust unit tests (startwrt-core + uciedit), package-scoped and
# containerized — mirrors test-core. See build/run-tests.sh.
.PHONY: test-startwrt
test-startwrt: $(STARTWRT_RUST_SRC) $(ENVIRONMENT_FILE)
	./$(STARTWRT_DIR)/build/run-tests.sh

# --- OpenWrt image (HEAVY, UNVALIDATED) ---
# One-time feeds/config/download.
.PHONY: startwrt-openwrt-setup
startwrt-openwrt-setup: $(STARTWRT_OPENWRT)/.config
$(STARTWRT_OPENWRT)/.config: $(STARTWRT_DIR)/build/openwrt.diffconfig $(STARTWRT_DIR)/build/feeds.conf
	./$(STARTWRT_DIR)/build/openwrt-setup.sh

# Stage the binary + UCI configs + init scripts into openwrt/files/.
.PHONY: startwrt-stage
startwrt-stage: $(STARTWRT_OPENWRT)/files/.staged
$(STARTWRT_OPENWRT)/files/.staged: $(STARTWRT_BIN) $(call ls-files, $(STARTWRT_DIR)/backend/firstboot_config) $(STARTWRT_DIR)/build/stage-files.sh
	ARCH=$(STARTWRT_ARCH) RUST_ARCH=$(STARTWRT_RUST_ARCH) PROFILE=$(PROFILE) ./$(STARTWRT_DIR)/build/stage-files.sh
	touch $(STARTWRT_OPENWRT)/files/.staged

# Full flashable image. Delegates the OpenWrt build to make -C openwrt; this is
# the multi-hour step. Resource fencing (memory-aware -j, cgroup) from the old
# standalone Makefile can be ported here if host stability requires it.
.PHONY: startwrt-image
startwrt-image: $(STARTWRT_IMAGES)
$(STARTWRT_IMAGES) &: $(STARTWRT_OPENWRT)/.config $(STARTWRT_OPENWRT)/files/.staged
	$(MAKE) -C $(STARTWRT_OPENWRT) V=s -j$(shell nproc)
	mkdir -p results
	cp $(STARTWRT_IMAGE_DIR)/$(STARTWRT_SDCARD_SRC) results/$(STARTWRT_SDCARD_NAME)
	cp $(STARTWRT_IMAGE_DIR)/$(STARTWRT_SYSUPGRADE_SRC) results/$(STARTWRT_SYSUPGRADE_NAME)

# Deploy binary + restart daemon over SSH (atomic temp -> sync -> rename).
.PHONY: startwrt-update
startwrt-update: $(STARTWRT_BIN)
	cat $(STARTWRT_BIN) | \
	ssh $(STARTWRT_REMOTE) 'set -e; \
		trap "rm -f /usr/bin/.startwrt.new" EXIT; \
		cat > /usr/bin/.startwrt.new; \
		chmod +x /usr/bin/.startwrt.new; \
		sync; \
		mv /usr/bin/.startwrt.new /usr/bin/startwrt; \
		ln -sf startwrt /usr/bin/startwrt-ctrld; \
		ln -sf startwrt /usr/bin/startwrt-cli; \
		/etc/init.d/startwrt restart'

.PHONY: clean-startwrt
clean-startwrt:
	rm -f results/$(STARTWRT_SDCARD_NAME) results/$(STARTWRT_SYSUPGRADE_NAME)
	rm -rf $(STARTWRT_DIR)/web/dist
	rm -rf $(STARTWRT_OPENWRT)/files
	rm -rf $(STARTWRT_OPENWRT)/build_dir $(STARTWRT_OPENWRT)/staging_dir $(STARTWRT_OPENWRT)/tmp $(STARTWRT_OPENWRT)/bin
	rm -f $(STARTWRT_OPENWRT)/.config

# The web app is part of the root Angular workspace; its prettier formatting runs
# through `format-web`/`format-check-web`. This target is just the Rust crate.
.PHONY: format-startwrt format-check-startwrt
format-startwrt:
	cargo +nightly fmt -p startwrt-core -p uciedit -p uciedit_macros

format-check-startwrt:
	cargo +nightly fmt --check -p startwrt-core -p uciedit -p uciedit_macros
