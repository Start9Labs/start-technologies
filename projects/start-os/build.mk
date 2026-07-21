# When this product's build inputs change, mirror them into the `paths:` filter
# of .github/workflows/startos-iso.yaml (see root AGENTS.md "Coupled changes").

IMAGE_TYPE=$(shell if [ "$(PLATFORM)" = raspberrypi ]; then echo img; else echo iso; fi)
FIRMWARE_ROMS := projects/start-os/build/lib/firmware/$(PLATFORM) $(shell jq --raw-output '.[] | select(.platform[] | contains("$(PLATFORM)")) | "./projects/start-os/build/lib/firmware/$(PLATFORM)/" + .id + ".rom.gz"' projects/start-os/build/lib/firmware.json)
BUILD_SRC := $(call ls-files, projects/start-os/build/lib) build/lib/scripts/forward-port build/lib/scripts/forward-port6 projects/start-os/build/lib/depends projects/start-os/build/lib/conflicts $(FIRMWARE_ROMS) projects/start-os/build/lib/migration-images/.done
IMAGE_RECIPE_SRC := $(call ls-files, projects/start-os/build/image-recipe/)
STARTD_SRC := projects/start-os/startd.service projects/start-os/services.slice projects/start-os/startos-shutdown.service projects/start-os/startos-restart.service $(BUILD_SRC)
COMPILED_TARGETS := target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/startbox target/$(RUST_ARCH)-unknown-linux-musl/release/start-container projects/start-os/container-runtime/rootfs.$(ARCH).squashfs
STARTOS_TARGETS := $(STARTD_SRC) $(ENVIRONMENT_FILE) $(GIT_HASH_FILE) $(VERSION_FILE) $(COMPILED_TARGETS) target/$(RUST_ARCH)-unknown-linux-musl/release/startos-backup-fs $(PLATFORM_FILE) \
	$(shell if [ "$(PLATFORM)" = "raspberrypi" ]; then \
		echo target/aarch64-unknown-linux-musl/release/pi-beep; \
	fi) \
	$(shell /bin/bash -c 'if [[ "${ENVIRONMENT}" =~ (^|-)unstable($$|-) ]]; then \
		echo target/$(RUST_ARCH)-unknown-linux-musl/release/flamegraph; \
	fi') \
	$(shell /bin/bash -c 'if [[ "${ENVIRONMENT}" =~ (^|-)console($$|-) ]]; then \
		echo target/$(RUST_ARCH)-unknown-linux-musl/release/tokio-console; \
	fi')

.PHONY: start-os
# Build all StartOS OS-product artifacts (bins + web + container-runtime image).
start-os: $(STARTOS_TARGETS)

container-runtime-test: projects/start-os/container-runtime/node_modules/.package-lock.json $(call ls-files, projects/start-os/container-runtime/src) projects/start-os/container-runtime/package.json projects/start-os/container-runtime/tsconfig.json
	cd projects/start-os/container-runtime && npm test

projects/start-os/build/lib/migration-images/.done: projects/start-os/build/save-migration-images.sh
	ARCH=$(ARCH) ./projects/start-os/build/save-migration-images.sh projects/start-os/build/lib/migration-images
	touch $@

start-os-deb: results/$(BASENAME).deb

results/$(BASENAME).deb: debian/build.sh $(call ls-files,projects/start-os/debian) $(STARTOS_TARGETS)
	PLATFORM=$(PLATFORM) REQUIRES=debian ./build/os-compat/run-compat.sh ./debian/build.sh

start-os-$(IMAGE_TYPE): results/$(BASENAME).$(IMAGE_TYPE)

start-os-squashfs: results/$(BASENAME).squashfs

results/$(BASENAME).$(IMAGE_TYPE) results/$(BASENAME).squashfs: $(IMAGE_RECIPE_SRC) results/$(BASENAME).deb
	ARCH=$(ARCH) ./projects/start-os/build/image-recipe/run-local-build.sh "results/$(BASENAME).deb"

# The OTA payload legacy (0.3.5.1) boxes pull via their frozen rsync updater: the
# 0.4.0 base squashfs repackaged onto the published 0.3.5.1 rootfs. See
# projects/start-os/build/assemble-migration-payload.sh; deploy the result with
# scripts/deploy-migration-payload.sh.
MIGRATION_FROM_TAG := v0.3.5.1

start-os-migration-squashfs: results/$(BASENAME).migration.squashfs

results/$(BASENAME).migration.squashfs: results/$(BASENAME).squashfs projects/start-os/build/assemble-migration-payload.sh projects/start-os/build/lib/scripts/migration-update-grub
	@if [ "$(PLATFORM)" = raspberrypi ]; then >&2 echo "migration payload: raspberrypi has no in-place migration — reflash required (#3443)"; exit 1; fi
	mkdir -p results/migration-base
	gh release download $(MIGRATION_FROM_TAG) --repo Start9Labs/start-technologies \
		--pattern 'startos-*_$(PLATFORM).iso' --dir results/migration-base --clobber
	docker run --rm -v "$(CURDIR)":/w -w /w -e OWNER_UID="$$(id -u)" -e OWNER_GID="$$(id -g)" start9/build-env bash -euc 'apt-get update -qq && apt-get install -yq --no-install-recommends xorriso && projects/start-os/build/assemble-migration-payload.sh --arch $(ARCH) --new-squashfs results/$(BASENAME).squashfs --old-image "$$(ls -1 results/migration-base/startos-*_$(PLATFORM).iso | head -n1)" --out $@'

# For creating os images. DO NOT USE
start-os-install: $(STARTOS_TARGETS)
	$(call mkdir,$(DESTDIR)/usr/bin)
	$(call mkdir,$(DESTDIR)/usr/sbin)
	$(call cp,target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/startbox,$(DESTDIR)/usr/bin/startbox)
	$(call ln,/usr/bin/startbox,$(DESTDIR)/usr/bin/startd)
	$(call ln,/usr/bin/startbox,$(DESTDIR)/usr/bin/start-cli)
	if [ "$(PLATFORM)" = "raspberrypi" ]; then $(call cp,target/aarch64-unknown-linux-musl/release/pi-beep,$(DESTDIR)/usr/bin/pi-beep); fi
	if /bin/bash -c '[[ "${ENVIRONMENT}" =~ (^|-)unstable($$|-) ]]'; then \
		$(call cp,target/$(RUST_ARCH)-unknown-linux-musl/release/flamegraph,$(DESTDIR)/usr/bin/flamegraph); \
	fi
	if /bin/bash -c '[[ "${ENVIRONMENT}" =~ (^|-)console($$|-) ]]'; then \
		$(call cp,target/$(RUST_ARCH)-unknown-linux-musl/release/tokio-console,$(DESTDIR)/usr/bin/tokio-console); \
	fi
	$(call cp,target/$(RUST_ARCH)-unknown-linux-musl/release/startos-backup-fs,$(DESTDIR)/usr/bin/startos-backup-fs)
	$(call ln,/usr/bin/startos-backup-fs,$(DESTDIR)/usr/sbin/mount.backup-fs)
	
	$(call mkdir,$(DESTDIR)/lib/systemd/system)
	$(call cp,projects/start-os/startd.service,$(DESTDIR)/lib/systemd/system/startd.service)
	$(call cp,projects/start-os/services.slice,$(DESTDIR)/lib/systemd/system/services.slice)
	$(call cp,projects/start-os/startos-shutdown.service,$(DESTDIR)/lib/systemd/system/startos-shutdown.service)
	$(call cp,projects/start-os/startos-restart.service,$(DESTDIR)/lib/systemd/system/startos-restart.service)
	if /bin/bash -c '[[ "${ENVIRONMENT}" =~ (^|-)unstable($$|-) ]]'; then \
		sed -i '/^Environment=/a Environment=RUST_BACKTRACE=full' $(DESTDIR)/lib/systemd/system/startd.service; \
	fi

	$(call mkdir,$(DESTDIR)/usr/lib)
	$(call rm,$(DESTDIR)/usr/lib/startos)
	$(call cp,projects/start-os/build/lib,$(DESTDIR)/usr/lib/startos)
	$(call cp,build/lib/scripts/forward-port,$(DESTDIR)/usr/lib/startos/scripts/forward-port)
	$(call cp,build/lib/scripts/forward-port6,$(DESTDIR)/usr/lib/startos/scripts/forward-port6)
	$(call mkdir,$(DESTDIR)/usr/lib/startos/container-runtime)
	$(call cp,projects/start-os/container-runtime/rootfs.$(ARCH).squashfs,$(DESTDIR)/usr/lib/startos/container-runtime/rootfs.squashfs)

	$(call cp,build/env/PLATFORM.txt,$(DESTDIR)/usr/lib/startos/PLATFORM.txt)
	$(call cp,build/env/ENVIRONMENT.txt,$(DESTDIR)/usr/lib/startos/ENVIRONMENT.txt)
	$(call cp,build/env/GIT_HASH.txt,$(DESTDIR)/usr/lib/startos/GIT_HASH.txt)
	$(call cp,build/env/VERSION.txt,$(DESTDIR)/usr/lib/startos/VERSION.txt)

start-os-update-overlay: $(STARTOS_TARGETS)
	@echo "\033[33m!!! THIS WILL ONLY REFLASH YOUR DEVICE IN MEMORY !!!\033[0m"
	@echo "\033[33mALL CHANGES WILL BE REVERTED IF YOU RESTART THE DEVICE\033[0m"
	@if [ -z "$(REMOTE)" ]; then >&2 echo "Must specify REMOTE" && false; fi
	@if [ "`ssh $(REMOTE) 'cat /usr/lib/startos/VERSION.txt'`" != "`cat $(VERSION_FILE)`" ]; then >&2 echo "StartOS requires migrations: update-overlay is unavailable." && false; fi
	$(call ssh,"sudo systemctl stop startd")
	$(MAKE) start-os-install REMOTE=$(REMOTE) SSHPASS=$(SSHPASS) PLATFORM=$(PLATFORM)
	$(call ssh,"sudo systemctl start startd")

start-os-wormhole: target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/startbox
	@echo "Paste the following command into the shell of your StartOS server:"
	@echo
	@wormhole send target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/startbox 2>&1 | awk -Winteractive '/wormhole receive/ { printf "sudo /usr/lib/startos/scripts/chroot-and-upgrade \"cd /usr/bin && rm startbox && wormhole receive --accept-file %s && chmod +x startbox\"\n", $$3 }'

start-os-wormhole-deb: results/$(BASENAME).deb
	@echo "Paste the following command into the shell of your StartOS server:"
	@echo
	@wormhole send results/$(BASENAME).deb 2>&1 | awk -Winteractive '/wormhole receive/ { printf "sudo /usr/lib/startos/scripts/chroot-and-upgrade '"'"'cd $$(mktemp -d) && wormhole receive --accept-file %s && apt-get install -y --reinstall ./$(BASENAME).deb'"'"'\n", $$3 }'

start-os-wormhole-squashfs: results/$(BASENAME).squashfs
	$(eval SQFS_SUM := $(shell b3sum results/$(BASENAME).squashfs | head -c 32))
	$(eval SQFS_SIZE := $(shell du -s --bytes results/$(BASENAME).squashfs | awk '{print $$1}'))
	@echo "Paste the following command into the shell of your StartOS server:"
	@echo
	@wormhole send results/$(BASENAME).squashfs 2>&1 | awk -Winteractive '/wormhole receive/ { printf "sudo sh -c '"'"'/usr/lib/startos/scripts/prune-images $(SQFS_SIZE) && /usr/lib/startos/scripts/prune-boot && cd /media/startos/images && wormhole receive --accept-file %s && CHECKSUM=$(SQFS_SUM) /usr/lib/startos/scripts/upgrade ./$(BASENAME).squashfs'"'"'\n", $$3 }'

start-os-update: $(STARTOS_TARGETS)
	@if [ -z "$(REMOTE)" ]; then >&2 echo "Must specify REMOTE" && false; fi
	$(call ssh,'sudo /usr/lib/startos/scripts/chroot-and-upgrade --create')
	$(MAKE) start-os-install REMOTE=$(REMOTE) SSHPASS=$(SSHPASS) DESTDIR=/media/startos/next PLATFORM=$(PLATFORM)
	$(call ssh,'sudo /media/startos/next/usr/lib/startos/scripts/chroot-and-upgrade --no-sync "apt-get install -y $(shell cat ./projects/start-os/build/lib/depends)"')

start-os-update-startbox: target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/startbox # only update binary (faster than full update)
	@if [ -z "$(REMOTE)" ]; then >&2 echo "Must specify REMOTE" && false; fi
	$(call ssh,'sudo /usr/lib/startos/scripts/chroot-and-upgrade --create')
	$(call cp,target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/startbox,/media/startos/next/usr/bin/startbox)
	$(call ssh,'sudo /media/startos/next/usr/lib/startos/scripts/chroot-and-upgrade --no-sync true')

start-os-update-deb: results/$(BASENAME).deb # better than update, but only available from debian
	@if [ -z "$(REMOTE)" ]; then >&2 echo "Must specify REMOTE" && false; fi
	$(call ssh,'sudo /usr/lib/startos/scripts/chroot-and-upgrade --create')
	$(call mkdir,/media/startos/next/var/tmp/startos-deb)
	$(call cp,results/$(BASENAME).deb,/media/startos/next/var/tmp/startos-deb/$(BASENAME).deb)
	$(call ssh,'sudo /media/startos/next/usr/lib/startos/scripts/chroot-and-upgrade --no-sync "apt-get install -y --reinstall /var/tmp/startos-deb/$(BASENAME).deb"')

start-os-update-squashfs: results/$(BASENAME).squashfs
	@if [ -z "$(REMOTE)" ]; then >&2 echo "Must specify REMOTE" && false; fi
	$(eval SQFS_SUM := $(shell b3sum results/$(BASENAME).squashfs | head -c 32))
	$(eval SQFS_SIZE := $(shell du -s --bytes results/$(BASENAME).squashfs | awk '{print $$1}'))
	$(call ssh,'sudo /usr/lib/startos/scripts/prune-images $(SQFS_SIZE)')
	$(call ssh,'sudo /usr/lib/startos/scripts/prune-boot')
	$(call cp,results/$(BASENAME).squashfs,/media/startos/images/next.rootfs)
	$(call ssh,'sudo CHECKSUM=$(SQFS_SUM) /usr/lib/startos/scripts/upgrade /media/startos/images/next.rootfs')

start-os-emulate-reflash: $(STARTOS_TARGETS)
	@if [ -z "$(REMOTE)" ]; then >&2 echo "Must specify REMOTE" && false; fi
	$(call ssh,'sudo /usr/lib/startos/scripts/chroot-and-upgrade --create')
	$(MAKE) start-os-install REMOTE=$(REMOTE) SSHPASS=$(SSHPASS) DESTDIR=/media/startos/next PLATFORM=$(PLATFORM)
	$(call ssh,'sudo rm -f /media/startos/config/disk.guid /media/startos/config/overlay/etc/hostname')
	$(call ssh,'sudo /media/startos/next/usr/lib/startos/scripts/chroot-and-upgrade --no-sync "apt-get install -y $(shell cat ./projects/start-os/build/lib/depends)"')

projects/start-os/container-runtime/debian.$(ARCH).squashfs: ./projects/start-os/container-runtime/download-base-image.sh
	ARCH=$(ARCH) ./projects/start-os/container-runtime/download-base-image.sh

projects/start-os/container-runtime/package-lock.json: projects/start-sdk/dist/package.json shared-libs/ts-modules/start-core/dist/package.json
	npm --prefix projects/start-os/container-runtime i
	touch projects/start-os/container-runtime/package-lock.json

projects/start-os/container-runtime/node_modules/.package-lock.json: projects/start-os/container-runtime/package-lock.json
	npm --prefix projects/start-os/container-runtime ci
	touch projects/start-os/container-runtime/node_modules/.package-lock.json

# TODO: make container-runtime its own makefile?
projects/start-os/container-runtime/dist/index.js: projects/start-os/container-runtime/node_modules/.package-lock.json $(call ls-files, projects/start-os/container-runtime/src) projects/start-os/container-runtime/package.json projects/start-os/container-runtime/tsconfig.json 
	npm --prefix projects/start-os/container-runtime run build

# Depends on dist/index.js: `run build` above does `rm -rf dist`, which wipes the
# vendored dist/node_modules, so the vendoring must (re-)run after every JS build —
# otherwise a src-only incremental rebuild (or a -j race) ships a dist with no deps.
projects/start-os/container-runtime/dist/node_modules/.package-lock.json projects/start-os/container-runtime/dist/package.json projects/start-os/container-runtime/dist/package-lock.json: projects/start-os/container-runtime/dist/index.js projects/start-os/container-runtime/package.json projects/start-os/container-runtime/package-lock.json projects/start-sdk/dist/package.json shared-libs/ts-modules/start-core/dist/package.json projects/start-os/container-runtime/install-dist-deps.sh
	./projects/start-os/container-runtime/install-dist-deps.sh
	touch projects/start-os/container-runtime/dist/node_modules/.package-lock.json

projects/start-os/container-runtime/rootfs.$(ARCH).squashfs: projects/start-os/container-runtime/debian.$(ARCH).squashfs projects/start-os/container-runtime/container-runtime.service projects/start-os/container-runtime/update-image.sh projects/start-os/container-runtime/update-image-local.sh projects/start-os/container-runtime/deb-install.sh projects/start-os/container-runtime/dist/index.js projects/start-os/container-runtime/dist/node_modules/.package-lock.json target/$(RUST_ARCH)-unknown-linux-musl/release/start-container
	ARCH=$(ARCH) ./projects/start-os/container-runtime/update-image-local.sh

projects/start-os/build/lib/depends projects/start-os/build/lib/conflicts: $(ENVIRONMENT_FILE) $(PLATFORM_FILE) $(shell ls projects/start-os/build/dpkg-deps/*)
	PLATFORM=$(PLATFORM) ARCH=$(ARCH) projects/start-os/build/dpkg-deps/generate.sh

$(FIRMWARE_ROMS): projects/start-os/build/lib/firmware.json ./projects/start-os/build/download-firmware.sh $(PLATFORM_FILE)
	./projects/start-os/build/download-firmware.sh $(PLATFORM)

target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/startbox: $(CORE_SRC) $(COMPRESSED_WEB_UIS) projects/start-os/web/patchdb-ui-seed.json $(ENVIRONMENT_FILE) projects/start-os/build/build-startbox.sh
	ARCH=$(ARCH) PROFILE=$(PROFILE) ./projects/start-os/build/build-startbox.sh
	touch target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/startbox

target/$(RUST_ARCH)-unknown-linux-musl/release/start-container: $(CORE_SRC) $(ENVIRONMENT_FILE) projects/start-os/build/build-start-container.sh
	ARCH=$(ARCH) ./projects/start-os/build/build-start-container.sh
	touch target/$(RUST_ARCH)-unknown-linux-musl/release/start-container

# used by github actions
compiled-$(ARCH).tar: $(COMPILED_TARGETS) $(ENVIRONMENT_FILE) $(GIT_HASH_FILE) $(VERSION_FILE)
	tar -cvf $@ $^

target/$(RUST_ARCH)-unknown-linux-musl/release/startos-backup-fs: $(call ls-files, projects/start-os/backup-fs) $(ENVIRONMENT_FILE)
	ARCH=$(ARCH) PROFILE=release ./projects/start-os/backup-fs/build-backup-fs.sh
	touch $@

# --- pi-beep: first-party crate, built like the other in-repo bins ---
target/aarch64-unknown-linux-musl/release/pi-beep: $(call ls-files, shared-libs/crates/pi-beep) $(ENVIRONMENT_FILE)
	ARCH=aarch64 PROFILE=release ./shared-libs/crates/pi-beep/build-pi-beep.sh
	touch $@

# --- external cargo dev tools (crates.io) bundled into unstable/console images ---
target/$(RUST_ARCH)-unknown-linux-musl/release/tokio-console: ./build/build-cargo-dep.sh
	ARCH=$(ARCH) ./build/build-cargo-dep.sh tokio-console
	touch $@

target/$(RUST_ARCH)-unknown-linux-musl/release/flamegraph: ./build/build-cargo-dep.sh
	ARCH=$(ARCH) ./build/build-cargo-dep.sh flamegraph
	touch $@

.PHONY: start-os-clean
start-os-clean:
	rm -f results/startos-*
	rm -rf dpkg-workdir/startos-*
	rm -rf projects/start-os/web/dist projects/start-os/docs/book
	rm -rf projects/start-os/container-runtime/dist projects/start-os/container-runtime/node_modules
	rm -f projects/start-os/container-runtime/*.squashfs
	rm -rf projects/start-os/build/lib/firmware projects/start-os/build/lib/migration-images
	rm -rf projects/start-os/build/image-recipe/deb

# The ui/setup-wizard web apps and the container-runtime are formatted by
# `web-format`/`web-format-check` (root prettier config, run from the repo root so
# the root .prettierignore applies). This target is just the Rust crates.
.PHONY: start-os-format start-os-format-check
start-os-format:
	$(FMT) cargo fmt -p start-os -p startos-backup-fs

start-os-format-check:
	$(FMT) cargo fmt --check -p start-os -p startos-backup-fs
