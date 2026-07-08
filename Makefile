# Top-level orchestrator. Shared vars/macros live in build/common.mk; each
# product owns its targets in <project>/build.mk. One make process, one DAG —
# includes (not recursive make) so cross-project prerequisites resolve.
# No default build target — bare `make` prints `help`; specify a target explicitly.
.DEFAULT_GOAL := help

include build/common.mk
include shared-libs/crates/start-core/build.mk
include shared-libs/ts-modules/build.mk
include projects/start-sdk/build.mk
include projects/start-cli/build.mk
include projects/start-registry/build.mk
include projects/start-tunnel/build.mk
include projects/start-os/build.mk
include projects/start-wrt/build.mk
include projects/start-docs/build.mk

.PHONY: help start-os metadata start-os-install clean format format-check start-cli-install start-cli start-cli-deb start-os-uis start-os-ui start-os-emulate-reflash start-os-deb start-os-$(IMAGE_TYPE) start-os-squashfs start-os-wormhole start-os-wormhole-deb start-os-update test start-core-test start-sdk-test container-runtime-test start-wrt-test start-registry start-registry-install start-tunnel start-tunnel-install start-core-ts-bindings

help:
	@echo "No default target — specify one. Common targets:"
	@echo "  start-os start-os-deb start-os-squashfs start-os-ui start-os-uis start-os-install   (StartOS)"
	@echo "  start-cli start-cli-deb start-registry start-tunnel start-wrt start-wrt-image        (other products)"
	@echo "  test start-core-test start-sdk-test container-runtime-test start-wrt-test            (tests)"
	@echo "  format format-check start-core-ts-bindings clean                                     (tooling)"
	@echo "See CONTRIBUTING.md for the full list."

touch:
	touch $(STARTOS_TARGETS)

metadata: $(VERSION_FILE) $(PLATFORM_FILE) $(ENVIRONMENT_FILE) $(GIT_HASH_FILE)

# Per-project cleans live in each build.mk; this only aggregates them.
clean: start-core-clean web-clean start-sdk-clean start-cli-clean start-registry-clean start-tunnel-clean start-os-clean start-wrt-clean start-docs-clean

# Whole-repo formatting: rustfmt (pinned nightly, in a container), then prettier
# and taplo (pinned via npm, native). Per-project `<project>-format` targets format
# just one scope through the same tools/config.
format:
	$(FMT) cargo fmt --all
	npm --prefix . run format
	npm --prefix . run format:toml

# Read-only verification (what CI runs); mirrors `format`.
format-check:
	$(FMT) cargo fmt --all --check
	npm --prefix . run format:check
	npm --prefix . run format:toml:check

test: | start-core-test start-sdk-test container-runtime-test start-wrt-test
