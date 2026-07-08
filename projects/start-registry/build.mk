# When this product's build inputs change, mirror them into the `paths:` filter
# of .github/workflows/start-registry.yaml (see root AGENTS.md "Coupled changes").

REGISTRY_TARGETS := target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/registrybox projects/start-registry/start-registryd.service

start-registry: target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/registrybox

start-registry-install: $(REGISTRY_TARGETS)
	$(call mkdir,$(DESTDIR)/usr/bin)
	$(call cp,target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/registrybox,$(DESTDIR)/usr/bin/start-registrybox)
	$(call ln,/usr/bin/start-registrybox,$(DESTDIR)/usr/bin/start-registryd)
	$(call ln,/usr/bin/start-registrybox,$(DESTDIR)/usr/bin/start-registry)

	$(call mkdir,$(DESTDIR)/lib/systemd/system)
	$(call cp,projects/start-registry/start-registryd.service,$(DESTDIR)/lib/systemd/system/start-registryd.service)

target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/registrybox: $(CORE_SRC) $(ENVIRONMENT_FILE)
	ARCH=$(ARCH) PROFILE=$(PROFILE) ./shared-libs/crates/start-core/build/build-registrybox.sh

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
