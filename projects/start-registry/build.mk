# When this product's build inputs change, mirror them into its entry in
# .github/scripts/changed-products.mjs (see root AGENTS.md "Coupled changes").

REGISTRY_TARGETS := target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/registrybox projects/start-registry/start-registryd.service

start-registry: target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/registrybox

start-registry-install: $(REGISTRY_TARGETS)
	$(call mkdir,$(DESTDIR)/usr/bin)
	$(call cp,target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/registrybox,$(DESTDIR)/usr/bin/start-registrybox)
	$(call ln,/usr/bin/start-registrybox,$(DESTDIR)/usr/bin/start-registryd)
	$(call ln,/usr/bin/start-registrybox,$(DESTDIR)/usr/bin/start-registry)

	$(call mkdir,$(DESTDIR)/lib/systemd/system)
	$(call cp,projects/start-registry/start-registryd.service,$(DESTDIR)/lib/systemd/system/start-registryd.service)

RUST_BINS_start-registry := start-registry:registrybox
RUST_OUTPUTS_start-registry := target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/registrybox
RUST_INPUTS_start-registry := $(CORE_SRC) $(ENVIRONMENT_FILE) $(RUST_BIN_BUILDER_INPUTS)

$(RUST_OUTPUTS_start-registry): $(RUST_INPUTS_start-registry)
	ARCH=$(ARCH) PROFILE=$(PROFILE) ./build/build-rust-bins.sh $(RUST_BINS_start-registry)

start-registry-deb: results/$(REGISTRY_BASENAME).deb

results/$(REGISTRY_BASENAME).deb: debian/build.sh $(call ls-files,projects/start-registry/debian) $(REGISTRY_TARGETS)
	PROJECT=start-registry PLATFORM=$(ARCH) REQUIRES=debian DEPENDS=ca-certificates ./build/os-compat/run-compat.sh ./debian/build.sh

.PHONY: start-registry-clean
start-registry-clean:
	rm -f results/start-registry-*.deb
	rm -rf dpkg-workdir/start-registry-*

.PHONY: start-registry-format start-registry-format-check
start-registry-format:
	$(FMT) cargo fmt -p start-registry

start-registry-format-check:
	$(FMT) cargo fmt --check -p start-registry
