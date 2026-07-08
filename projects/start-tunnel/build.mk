# When this product's build inputs change, mirror them into the `paths:` filter
# of .github/workflows/start-tunnel.yaml (see root AGENTS.md "Coupled changes").

TUNNEL_TARGETS := target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/tunnelbox projects/start-tunnel/start-tunneld.service

start-tunnel: target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/tunnelbox

start-tunnel-install: target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/tunnelbox projects/start-tunnel/start-tunneld.service
	$(call mkdir,$(DESTDIR)/usr/bin)
	$(call cp,target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/tunnelbox,$(DESTDIR)/usr/bin/start-tunnelbox)
	$(call ln,/usr/bin/start-tunnelbox,$(DESTDIR)/usr/bin/start-tunneld)
	$(call ln,/usr/bin/start-tunnelbox,$(DESTDIR)/usr/bin/start-tunnel)

	$(call mkdir,$(DESTDIR)/lib/systemd/system)
	$(call cp,projects/start-tunnel/start-tunneld.service,$(DESTDIR)/lib/systemd/system/start-tunneld.service)

	$(call mkdir,$(DESTDIR)/usr/lib/startos/scripts)
	$(call cp,build/lib/scripts/forward-port,$(DESTDIR)/usr/lib/startos/scripts/forward-port)

	$(call mkdir,$(DESTDIR)/etc/apt/sources.list.d)
	$(call cp,apt/start9.list,$(DESTDIR)/etc/apt/sources.list.d/start9.list)
	$(call mkdir,$(DESTDIR)/usr/share/keyrings)
	$(call cp,apt/start9.gpg,$(DESTDIR)/usr/share/keyrings/start9.gpg)

target/$(RUST_ARCH)-unknown-linux-musl/$(PROFILE)/tunnelbox: $(CORE_SRC) $(ENVIRONMENT_FILE) $(GIT_HASH_FILE) projects/start-tunnel/web/dist/static/start-tunnel/index.html
	ARCH=$(ARCH) PROFILE=$(PROFILE) ./shared-libs/crates/start-core/build/build-tunnelbox.sh

start-tunnel-deb: results/$(TUNNEL_BASENAME).deb

results/$(TUNNEL_BASENAME).deb: debian/build.sh $(call ls-files,projects/start-tunnel/debian) $(TUNNEL_TARGETS) build/lib/scripts/forward-port
	PROJECT=start-tunnel PLATFORM=$(ARCH) REQUIRES=debian DEPENDS=wireguard-tools,iptables,nftables,conntrack ./build/os-compat/run-compat.sh ./debian/build.sh

.PHONY: start-tunnel-clean
start-tunnel-clean:
	rm -f results/start-tunnel-*.deb
	rm -rf dpkg-workdir/start-tunnel-* projects/start-tunnel/web/dist projects/start-tunnel/docs/book

# The tunnel web app is formatted by `web-format` (whole Angular workspace); this is the Rust crate.
.PHONY: start-tunnel-format start-tunnel-format-check
start-tunnel-format:
	$(FMT) cargo fmt -p start-tunnel

start-tunnel-format-check:
	$(FMT) cargo fmt --check -p start-tunnel
