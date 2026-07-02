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

.PHONY: help startos metadata install-startos clean format format-check install-cli cli cli-deb startos-uis startos-ui startos-emulate-reflash startos-deb startos-$(IMAGE_TYPE) startos-squashfs startos-wormhole startos-wormhole-deb startos-update test test-core test-sdk test-container-runtime test-startwrt registry install-registry tunnel install-tunnel ts-bindings

help:
	@echo "No default target — specify one. Common targets:"
	@echo "  startos startos-deb startos-squashfs startos-ui startos-uis install-startos   (StartOS)"
	@echo "  cli cli-deb registry tunnel startwrt startwrt-image                           (other products)"
	@echo "  test test-core test-sdk test-container-runtime test-startwrt                  (tests)"
	@echo "  format format-check ts-bindings clean                                         (tooling)"
	@echo "See CONTRIBUTING.md for the full list."

touch:
	touch $(STARTOS_TARGETS)

metadata: $(VERSION_FILE) $(PLATFORM_FILE) $(ENVIRONMENT_FILE) $(GIT_HASH_FILE)

# Per-project cleans live in each build.mk; this only aggregates them.
clean: clean-core clean-web clean-sdk clean-cli clean-registry clean-tunnel clean-startos clean-startwrt clean-docs

# Per-project formats live in each build.mk; this only aggregates them. Run one
# project with e.g. `make format-cli`, or all of them with `make format`.
format: format-core format-web format-sdk format-cli format-registry format-tunnel format-startos format-startwrt

# Read-only formatting verification (used by CI); mirrors `format` per project.
format-check: format-check-core format-check-web format-check-sdk format-check-cli format-check-registry format-check-tunnel format-check-startos format-check-startwrt

test: | test-core test-sdk test-container-runtime test-startwrt
