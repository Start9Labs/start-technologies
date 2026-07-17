# Changelog

All notable changes to StartWRT are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.2]

### Removed

- **The IPv6 "Reserve" option has been removed — it never worked and never
  could.** Devices choose their own IPv6 addresses via SLAAC, so a router
  cannot reserve one; the toggle wrote a DHCPv6 hint that no mainstream
  client ever requests. The device page now shows the IPv6 address read-only
  with an explanation, publishing a port no longer claims the IPv6 address
  "will be reserved" (the rule follows the device's current address
  automatically), and the LAN IPv6
  SLAAC toggle now locks while enabled published-port rules use IPv6 rather
  than when "reserved" devices exist. The `devices.update` RPC no longer
  accepts `ipv6_static`/`ipv6` fields. IPv4 reservations are unchanged.

- **The Start9 DDNS provider option has been removed.** The Start9 DDNS
  service has not launched yet, so selecting it saved a configuration that
  could never update a DNS record. Configurations previously saved with the
  Start9 provider now read back as Dynamic DNS disabled; pick one of the
  supported providers to re-enable. The option will return when the Start9
  service goes live.

### Fixed

- **Enabling LAN IPv6 no longer silently fails when the Admin profile routes
  through an IPv4-only VPN.** Saving the LAN IPv6 settings reported success
  but immediately reverted to disabled: the save re-derived the admin LAN's
  router advertisements from whether its outbound VPN carries IPv6, undoing
  the change in the same write. Editing the Admin profile onto an IPv4-only
  VPN likewise switched LAN IPv6 off behind your back — and kept it off even
  after switching the outbound back. The LAN IPv6 toggle is now the sole
  owner of that setting; with an IPv4-only VPN outbound, LAN devices still
  get local (ULA) IPv6 addresses while internet-bound IPv6 remains blocked
  by the VPN kill switch, so nothing leaks around the tunnel.

- **Published ports no longer reshuffle their order on every refresh.** The
  list is auto-refreshed every few seconds, and each refresh returned the
  rows in an arbitrary order, so the table visibly jumped around. Published
  ports now appear in a stable order, sorted by label.

- **IPv6 published-port rules now follow the target device when it changes
  its address.** Devices assign their own IPv6 addresses and change them
  routinely — privacy addresses rotate daily, and most operating systems
  derive new addresses whenever the ISP rotates the delegated prefix — but
  rules were only re-resolved on a prefix change, so a device-side change
  silently broke the forward. The router now watches the network for
  neighbor changes and retargets affected rules within seconds. Rules also
  now pin the device's long-lived (stable) address instead of whichever
  address happened to be observed first, which could be a short-lived
  privacy address that expired within days. The **Endpoints** column shows
  that same stable address, so the endpoint you copy always matches the rule.

- **Documentation corrected against the code in a full docs-vs-code audit.** The
  user guide no longer misstates product behavior: backups _do_ preserve assigned
  device names and data-usage history; a Fresh Start reflash sets a new admin
  password rather than clearing it; published-port statuses do not detect CGNAT;
  Ethernet profile changes apply immediately (there is no Save button); inbound
  VPN client configs must be captured when the client is created (the private
  key is never stored). Also refreshed drifted UI labels throughout the guide,
  corrected `API_CONTRACT.md` wire types and documented the previously missing
  endpoints, and fixed stale paths, commands, and structure descriptions across
  the developer docs.
- **Cloudflare Dynamic DNS now saves a working configuration.** The saved
  config was missing fields the update client requires (the Bearer-token
  marker and the zone), so Cloudflare updates could never succeed. The form
  now asks for the **Zone** — the domain registered with Cloudflare, e.g.
  `example.com` — instead of a Zone ID. Previously saved Cloudflare
  configurations must be re-saved with the zone filled in. Note that the
  DNS record must already exist in Cloudflare (the client updates records,
  it does not create them), and the API token needs Zone:Read and DNS:Edit
  permissions. Proxied (orange-cloud) records are supported: the client
  reads the registered IP through the Cloudflare API rather than DNS, which
  would only ever see the proxy's address.
- **Dynamic DNS now actually updates your provider.** The image was missing
  the `ddns-scripts` update client (and its Cloudflare and No-IP extensions),
  so DDNS settings were saved but no DNS record was ever updated. The FreeDNS
  provider also pointed at a service name (`freedns.afraid.org`) that modern
  `ddns-scripts` no longer recognizes; it now uses the afraid.org update-key
  service (`afraid.org-keyauth`), and configurations saved with the old name
  are still read back correctly. DDNS configurations are also bound to the
  WAN interface, so an update fires the moment the connection comes back up
  (e.g. after a modem reboot or PPPoE reconnect) instead of waiting for the
  next scheduled check.

## [1.0.1]

### Added

- **`startwrt-cli verify` now checks the EEPROM WiFi password.** The factory
  QC utility reads EEPROM tag 0x2F, verifies it satisfies the password
  constraints (valid TLV, 12 characters from the non-ambiguous charset), and
  prints the password for visual comparison against the device sticker. On
  failure it reports the specific violation (invalid TLV, missing tag, wrong
  length and/or bytes outside the charset).

### Changed

- **The Inbound VPN dialog no longer steals focus when it opens**, which on
  mobile raised the keyboard the instant the dialog appeared.

## [1.0.0]

First stable release of StartWRT — Start9's OpenWrt-based router OS for home
self-hosting, built around per-device Security Profiles (assigned by Ethernet
port, WiFi password, or inbound VPN), with inbound/outbound WireGuard VPNs and
VPN chaining, WiFi schedules, dynamic DNS, and published-port forwarding.
Ships as a flashable image for the SpaceMiT K1 (BananaPi-F3), with OTA updates
delivered through the Start9 registry.

## [0.1.0-beta.4]

### Added

- **`--version` flag.** `startwrt --version` now reports the StartWRT version (e.g.
  `0.1.0-beta.4`); the CLI previously exposed no `--version`.

### Changed

- OpenWrt base upgraded **25.12.4 → 25.12.5** (`r33051-f5dae5ece4`), picking up
  the upstream stable-branch fixes. All three Start9 build-infra patches apply
  unchanged, and no upstream path collides with the Start9 overlay.

- The OpenWrt image now builds from **pristine upstream OpenWrt** (the release
  tarball pinned by sha256 in `build/openwrt-version`) with the Start9 delta
  applied at build time from in-repo `openwrt-patches/` (3 build-infra patches)
  and `openwrt-overlay/` (the SpacemiT K1 target + boot packages). The
  `Start9Labs/openwrt` fork and the monorepo's last git submodule are retired;
  cloning no longer needs `--recursive`, and the `openwrt/` build workspace
  contains no git repo at all. The prepared tree is byte-identical to the
  former fork (verified by git tree hash), so image contents are unchanged.

- Relocated into the `start-technologies` monorepo as the `start-wrt` product. The
  three backend crates (`startwrt-core`/`ctrl`, `uciedit`, `uciedit_macros`) are now
  members of the root Cargo workspace and build against the **shared** `start-core`
  crate (`shared-libs/crates/start-core`, pulled in aliased as `startos`), the
  vendored `rpc-toolkit`, and the vendored `imbl-value` — replacing the previously
  embedded `start-os` submodule and the git/crates.io copies of those deps.
- Build orchestration moved from the standalone product `Makefile` to
  `projects/start-wrt/build.mk` (included by the root `Makefile`): `make start-wrt`,
  `make start-wrt-image`, `make start-wrt-update`.
- The Angular web UI is now a project (`start-wrt`) in the **root Angular workspace**
  instead of a standalone app. It shares the root `package.json`/`node_modules`/
  `tsconfig.json` and builds via `npm run build:wrt` (serve `npm run start:wrt`,
  type-check `npm run check:wrt`) — and so upgrades in lockstep with the other Start9
  Angular apps. `RELATIVE_URL`, `pauseFor`, and the markdown pipe now come from
  `@start9labs/shared`. The HTTP/RPC/connection stack (its aborting-timeout, code-0
  reconnect flow is deliberately different from shared's), the bespoke error surfacing,
  `WorkspaceConfig`, the WebSocket progress types, and the i18n-routed `validation-errors`
  provider stay local where the shared code would regress behavior or the shapes don't fit.
- `@start9labs/shared` is now marked `sideEffects: false` so importing a few symbols from
  its barrel tree-shakes cleanly (start-wrt's embedded UI bundle would otherwise pull in
  ~875 kB of unused shared code). This also shrinks the other apps' bundles.
- Restored the release CI that the monorepo migration had dropped, then folded StartWRT
  into the monorepo-wide release tool: `start-wrt.yaml` again has a `deploy` job, but it
  now _only_ uploads the built images to S3 (`s3://startwrt-images`) — the CDN the registry
  serves from. To match `startos-iso.yaml`, it is `workflow_dispatch`-gated on a `deploy:
release` input (rather than the old standalone workflow's `v*`-tag push) and reads the
  version from `backend/ctrl/Cargo.toml` (the standalone workflow read the now-removed
  `web/package.json`). Tagging, cutting the GitHub release, and the registry publishing are
  now driven by the top-level `scripts/manage-release.sh` (a new `wrt` project kind alongside
  os/cli/deb/npm), which replaces the standalone
  `projects/start-wrt/scripts/manage-release.sh`. Registry publishing mirrors the OS's staged
  flow: `register start-wrt` indexes a CI build into a beta registry, where beta routers
  (UCI `startwrt.system.registry` pointed at it) soak the version as a normal OTA update, and
  `release start-wrt` then promotes it into the production registry — both deliberate local,
  developer-key-gated steps. Releases are cut on `Start9Labs/start-technologies` with the
  monorepo's `<project>/v<version>` tag convention (`start-wrt/v<version>`), since the
  monorepo hosts every product's releases on independent cadences. Release assets follow the
  startos naming convention —
  `startwrt-<version>-<git hash>_spacemit-k1-{sdcard.img.gz,sysupgrade.img.gz}` — instead
  of the raw OpenWrt output names, which carried no product, version, or hash; the sdcard
  image is now gzipped (it was previously published raw), and balenaEtcher flashes the
  `.img.gz` directly.
- Restored the OpenWrt download-cache keying the migration had narrowed: the `image` job's
  cache key again includes `build/feeds.conf` (so changing the feed set busts the cache) and
  carries a `restore-keys` fallback (so a partial older cache can seed a fresh run).

### Fixed

- Changing the admin password now enforces the 12-character minimum. The
  Settings → Password form and its `auth.set-password` backend endpoint (also
  reached via the `startwrt auth set-password` CLI) accepted passwords shorter
  than 12 characters, even though first-time setup required it and the docs
  documented the minimum. Both the frontend form and the backend now reject
  passwords under 12 characters on change, matching initial setup.

- Changing the Router IP can no longer strand the network on a colliding subnet.
  The LAN IPv4 page exposed a "Router IP" (3rd-octet) field that duplicated the
  Admin Security Profile's subnet field but, unlike it, had no collision guard —
  so setting the router onto a subnet already used by another profile put two
  interfaces on the same /24, producing overlapping routes that silently broke
  all access to the router (unrecoverable even by a keep-settings reflash). The
  duplicate field is removed (the LAN page now only selects the /16 network
  block; the Admin profile is the single source of truth for the 3rd octet), and
  the backend now rejects `profiles.create`/`profiles.edit` requests whose /24
  collides with an existing profile (including the admin LAN). The `lan.ipv4-set`
  endpoint (reached via the CLI) is covered by the same guard, so a direct
  RPC/CLI call can't strand the router either.

- The Settings → General **Build** field no longer goes stale after new commits.
  `build.mk` had lost the wiring that refreshes `build/env/GIT_HASH.txt` on every
  build and treats it as a prerequisite of `web/config.json`, so the UI's `gitHash`
  froze at whatever it was when `config.json` was first generated. `build.mk` now
  refreshes `GIT_HASH.txt` at parse time and re-stamps `config.json` whenever `HEAD`
  moves.
- The **Build** field now shows the `-modified` marker on a dirty build. It shortened
  the 40-char hash with `slice(0, 12)`, which dropped the trailing `-modified` suffix;
  it now preserves any trailing marker (matching the `-dirty` indicator the
  `startwrt verify` CLI already shows).
- Adding an Outbound VPN no longer silently does nothing. On submit the dialog
  called `tuiMarkControlAsTouchedAndValidate`, which re-ran the WireGuard `.conf`
  async validator and left the form stuck `PENDING` (the in-flight validation is
  cancelled when the file input remounts during the pending phase), so the create
  request was never sent. Submit now completes directly when the form is already
  valid, and only marks fields touched — without re-validating — when it isn't.
- Web-only changes are now re-embedded into the `startwrt` binary on rebuild. The
  UI is baked in at compile time via `include_dir!`, which does not register the
  embedded files as cargo dependencies, so `ctrl`'s `build.rs` now emits a
  `cargo:rerun-if-changed` for the web `dist` directory. Previously a changed web
  bundle was silently ignored unless a `.rs` file also changed, shipping a stale
  UI.

## [0.1.0-beta.3]

StartWRT is Start's fork of OpenWrt — a router OS for home self-hosting built around
per-device Security Profiles, with profiles assigned by Ethernet port, WiFi password,
or inbound VPN. Features inbound/outbound WireGuard VPNs with VPN chaining, WiFi
schedules, dynamic DNS, and published-port forwarding. A Rust backend (`startwrt`
binary: RPC daemon + CLI over JSON-RPC 2.0, UCI as source of truth) serves an embedded
Angular + Taiga UI frontend, shipped as a flashable OpenWrt image for the SpaceMiT K1
(BananaPi-F3).
