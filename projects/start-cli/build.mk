# When this product's build inputs change, mirror them into its entry in
# .github/scripts/changed-products.mjs (see root AGENTS.md "Coupled changes").

CLI_TARGETS := target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/start-cli

start-cli: $(GIT_HASH_FILE)
	./projects/start-cli/build/build-cli.sh

RUST_BINS_start-cli := start-cli:start-cli
RUST_OUTPUTS_start-cli := $(CLI_TARGETS)
RUST_INPUTS_start-cli := $(CORE_SRC) $(ENVIRONMENT_FILE) build/build-rust-bins.sh

# Build into a workspace-relative path for packaging (musl release).
$(CLI_TARGETS): $(RUST_INPUTS_start-cli)
	ARCH=$(ARCH) PROFILE=$(PROFILE) ./build/build-rust-bins.sh $(RUST_BINS_start-cli)
	touch $@

start-cli-install: $(CLI_TARGETS) build/bash-completion
	$(call mkdir,$(DESTDIR)/usr/bin)
	$(call cp,$(CLI_TARGETS),$(DESTDIR)/usr/bin/start-cli)
	$(call install-bash-completion,start-cli)

start-cli-deb: results/$(CLI_BASENAME).deb

results/$(CLI_BASENAME).deb: debian/build.sh build/common.mk build/bash-completion projects/start-cli/build.mk $(CLI_TARGETS)
	PROJECT=start-cli PLATFORM=$(ARCH) REQUIRES=debian DEPENDS=bash-completion ./build/os-compat/run-compat.sh ./debian/build.sh

.PHONY: start-cli-clean
start-cli-clean:
	rm -f results/start-cli-*.deb
	rm -rf dpkg-workdir/start-cli-*

.PHONY: start-cli-format start-cli-format-check
start-cli-format:
	$(FMT) cargo fmt -p start-cli

start-cli-format-check:
	$(FMT) cargo fmt --check -p start-cli
