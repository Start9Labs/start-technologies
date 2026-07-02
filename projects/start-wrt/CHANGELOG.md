# Changelog

All notable changes to StartWRT are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

### Changed

- Relocated into the `start-technologies` monorepo as the `start-wrt` product. The
  three backend crates (`startwrt-core`/`ctrl`, `uciedit`, `uciedit_macros`) are now
  members of the root Cargo workspace and build against the **shared** `start-core`
  crate (`shared-libs/crates/start-core`, pulled in aliased as `startos`), the
  vendored `rpc-toolkit`, and the vendored `imbl-value` — replacing the previously
  embedded `start-os` submodule and the git/crates.io copies of those deps.
- `openwrt` is now the monorepo's only git submodule; the embedded `start-os`
  submodule was removed.
- Build orchestration moved from the standalone product `Makefile` to
  `projects/start-wrt/build.mk` (included by the root `Makefile`): `make startwrt`,
  `make startwrt-image`, `make startwrt-update`.
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
- Restored the release CI that the monorepo migration had dropped: `start-wrt.yaml` again
  has a `deploy` job that uploads the built images to S3 (`s3://startwrt-images`) and cuts a
  GitHub Release. To match `startos-iso.yaml`, it is `workflow_dispatch`-gated on a `deploy:
  release` input (rather than the old standalone workflow's `v*`-tag push) and reads the
  version from `backend/ctrl/Cargo.toml` (the standalone workflow read the now-removed
  `web/package.json`). Releases are cut on `Start9Labs/start-technologies` with
  product-namespaced tags (`startwrt/v<version>`), since the monorepo hosts every
  product's releases on independent cadences. Registry indexing/signing stays a local
  gate in `scripts/manage-release.sh`, which is re-pointed at the new repo, workflow,
  artifact name, and tag scheme; the user manual's download/source/issue links point
  at the monorepo too. Release assets follow the startos naming convention —
  `startwrt-<version>-<git hash>_spacemit-k1-{sdcard.img,sysupgrade.img.gz}` — instead
  of the raw OpenWrt output names, which carried no product, version, or hash.
- Restored the OpenWrt download-cache keying the migration had narrowed: the `image` job's
  cache key again includes `build/feeds.conf` (so changing the feed set busts the cache) and
  carries a `restore-keys` fallback (so a partial older cache can seed a fresh run).

## [0.1.0-beta.3]

StartWRT is Start's fork of OpenWrt — a router OS for home self-hosting built around
per-device Security Profiles, with profiles assigned by Ethernet port, WiFi password,
or inbound VPN. Features inbound/outbound WireGuard VPNs with VPN chaining, WiFi
schedules, dynamic DNS, and published-port forwarding. A Rust backend (`startwrt`
binary: RPC daemon + CLI over JSON-RPC 2.0, UCI as source of truth) serves an embedded
Angular + Taiga UI frontend, shipped as a flashable OpenWrt image for the SpaceMiT K1
(BananaPi-F3).
