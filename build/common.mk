ls-files = $(shell git ls-files --cached --others --exclude-standard $1)
PROFILE = release

# rustfmt runs in a container pinned to a specific nightly (our rustfmt.toml uses
# nightly-only options); prettier and taplo are pinned via npm and run natively.
# Set FMT_NATIVE=1 to run rustfmt on the host instead.
FMT := ./build/fmt/run-fmt.sh

PLATFORM_FILE := $(shell ./build/env/check-platform.sh)
ENVIRONMENT_FILE := $(shell ./build/env/check-environment.sh)
GIT_HASH_FILE := $(shell ./build/env/check-git-hash.sh)
VERSION_FILE := $(shell ./build/env/check-version.sh)
BASENAME := $(shell PROJECT=startos ./build/env/basename.sh)
PLATFORM := $(shell if [ -f $(PLATFORM_FILE) ]; then cat $(PLATFORM_FILE); else echo unknown; fi)
ARCH := $(shell if [ "$(PLATFORM)" = "raspberrypi" ]; then echo aarch64; elif [ "$(PLATFORM)" = "rockchip64" ]; then echo aarch64; else echo $(PLATFORM) | sed 's/-nonfree$$//g; s/-nvidia$$//g'; fi)
RUST_ARCH := $(shell if [ "$(ARCH)" = "riscv64" ]; then echo riscv64gc; else echo $(ARCH); fi)
REGISTRY_BASENAME := $(shell PROJECT=start-registry PLATFORM=$(ARCH) ./build/env/basename.sh)
TUNNEL_BASENAME := $(shell PROJECT=start-tunnel PLATFORM=$(ARCH) ./build/env/basename.sh)
CLI_BASENAME := $(shell PROJECT=start-cli PLATFORM=$(ARCH) ./build/env/basename.sh)
# start-core path-depends on these first-party sibling crates (start-core/Cargo.toml
# + their transitive path deps); they must be prereqs of every Rust bin or a
# shared-crate edit leaves `make start-os`/`start-cli`/`start-registry`/`start-tunnel` shipping a
# stale binary. patch-db is globbed separately below.
CORE_SRC := $(call ls-files, shared-libs/crates/start-core) \
	$(call ls-files, shared-libs/crates/exver) \
	$(call ls-files, shared-libs/crates/imbl-value) \
	$(call ls-files, shared-libs/crates/jsonpath) \
	$(call ls-files, shared-libs/crates/rpc-toolkit) \
	$(call ls-files, shared-libs/crates/yasi) \
	$(shell git ls-files shared-libs/crates/patch-db) build/builder-alias.sh $(GIT_HASH_FILE)
PATCH_DB_CLIENT_SRC := $(shell git ls-files shared-libs/crates/patch-db/client)
GZIP_BIN := $(shell which pigz || which gzip)
TAR_BIN := $(shell which gtar || which tar)

ifeq ($(REMOTE),)
	mkdir = mkdir -p $1
	rm = rm -rf $1
	cp = cp -r $1 $2
	ln = ln -sf $1 $2
else
	ifeq ($(SSHPASS),)
		ssh = ssh $(REMOTE) $1
	else 
		ssh = sshpass -p $(SSHPASS) ssh $(REMOTE) $1
	endif
	mkdir = $(call ssh,'sudo mkdir -p $1')
	rm  = $(call ssh,'sudo rm -rf $1')
	ln = $(call ssh,'sudo ln -sf $1 $2')
define cp
	$(TAR_BIN) --transform "s|^$1|x|" -czv -f- $1 | $(call ssh,"sudo tar --transform 's|^x|$2|' -xzv -f- -C /")
endef
endif

.DELETE_ON_ERROR:

# --- vendored patch-db TS client (consumed by web) ---
shared-libs/crates/patch-db/client/node_modules/.package-lock.json: shared-libs/crates/patch-db/client/package.json
	npm --prefix shared-libs/crates/patch-db/client ci
	touch shared-libs/crates/patch-db/client/node_modules/.package-lock.json

shared-libs/crates/patch-db/client/dist/index.js: $(PATCH_DB_CLIENT_SRC) shared-libs/crates/patch-db/client/node_modules/.package-lock.json
	rm -rf shared-libs/crates/patch-db/client/dist
	npm --prefix shared-libs/crates/patch-db/client run build
	touch shared-libs/crates/patch-db/client/dist/index.js
