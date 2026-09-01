# Changelog

All notable changes to StartWRT are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.0]

### Added

- Automatic port forwarding (PCP + UPnP IGD). A LAN device can now open and
  renew its own port forwards using the standard PCP and UPnP protocols —
  StartOS servers use this to configure themselves automatically behind a
  StartWRT router, and game consoles/torrent clients are covered too.
  Authorization is per-device and **off by default**: enable it with the new
  "Allow automatic port forwarding" toggle on the device's detail page. A
  device can only ever forward ports to itself — and because PCP runs over UDP,
  a request is honored only when it actually arrives from the network the
  device lives on, so a device on one network can't open forwards on behalf of
  a device on another. Requests that would take
  over a manually published port are refused (conversely, publishing a port an
  automatic forward holds removes the automatic forward — manual rules win).
  Ports the router itself answers on from the internet are refused too, so an
  automatic forward can never take over your remote access to the router, its
  SSH, or a VPN server you've exposed.
  Forwards are stored as tagged UCI firewall redirects (so they survive
  reboots and never collide with manual published-port rules), renew on an
  in-memory lease (no flash writes on renewal), and expire on the lifetime the
  device requested when it stops renewing them — at most a week, even for a
  device that asks to hold the port indefinitely. A forward is also removed
  once the device no longer holds the address it points at — its DHCP lease
  lapsed, or it returned on a different address — so a forward can never
  quietly deliver Internet traffic to whichever device is given that address
  next (devices with a reserved address are unaffected). Turning the toggle
  back off — or forgetting the device — closes that device's forwards and
  hostname routes immediately.
  The Published Ports page gains a read-only "Automatic" section showing each
  port use's device, kind (PCP, UPnP, or SNI), and expiry. UPnP clients see a complete gateway:
  the router advertises the `WANCommonInterfaceConfig` service clients use to
  recognize an Internet Gateway Device, answers the status actions they check
  before mapping anything, and supports reading mappings back
  (`GetSpecificPortMappingEntry`/`GetGenericPortMappingEntry`) — a device sees
  only its own. The router identifies itself as "StartWRT" to those clients. The
  UPnP endpoints refuse browser-shaped requests — DNS-rebinding requests and
  blind cross-origin writes alike — so a malicious web page cannot use a LAN
  device's browser to read the network's public IP, fingerprint the router, or
  open that device's ports. Uses the shared `start-core` PCP/IGD server cores.
  Devices can also register **SNI hostname routes** on a shared external port
  (over PCP's HOSTNAME extension or the `X_START9_AddHostnameMapping` UPnP
  vendor action): the router reads each TLS connection's requested hostname
  and delivers it to whichever device owns it, so several devices — or several
  services on one StartOS server with their own domains — share one port such
  as 443. Hostname routes appear in the Automatic section with their hostname,
  follow the same per-device permission and lease expiry as plain forwards,
  claim their shared port whole (plain forwards on it are refused; ports the
  router itself answers on — SSH, an inbound VPN — are refused to hostname
  routes for the same reason), and are re-registered by the device within
  minutes after a router restart rather than persisted. Remote access to the
  router's own web interface is the exception, not a casualty: hostname
  routes and remote access share port 443 — connections naming a routed
  hostname reach its device, and everything else (such as browsing the
  router by IP address) still reaches the router interface, accepted from
  exactly the sources your Remote Access setting allows, so enabling one
  feature never silently disables the other.
  A routed hostname works from inside your own network too — a laptop on your
  LAN can open the same public address and reach the device — with one
  consequence worth knowing: because the device answers a local client
  directly rather than through the router, the router puts its own address on
  those connections, so the device cannot tell one local client from another
  in its logs. Connections from another Security Profile, and from the
  Internet, still carry the original address.
- The UI now detects when the running firmware ships a newer interface than
  the page is displaying (every RPC response and `system.info` report the
  firmware's build stamp and the UI compares it to its own). An update
  installed from the current tab reloads the page automatically once the
  router is back (the "Updated to vX" confirmation follows after login); an
  update applied any other way — CLI deploy, another device — shows a
  "Refresh Needed" dialog with a Reload button rather than reloading out from
  under unsaved work. Detection rides an `x-startwrt-git-hash` header on every
  RPC response, so an open tab notices within seconds of its next request even
  when the update restarted the daemon too quickly to drop a connection; pages
  that make no requests while idle re-check every 30 seconds.
- **6in4 tunnels can now be configured on the router.** The `6in4` protocol
  and the SIT kernel module it needs now ship in the image, so an IPv6 tunnel
  from a broker such as Hurricane Electric can be set up over SSH — useful for
  reaching IPv6 on an ISP that provides none. There is no UI for this yet.
  Previously these packages had to be built and sideloaded by hand after every
  update, since a sysupgrade does not preserve separately installed packages.
- **Hardware documentation.** A new Hardware page in the user guide lists the
  router's specifications and publishes the SpacemiT K1 reference schematic the
  board descends from, noting where the shipped router differs from that
  reference design and why.

### Changed

- **Publishing a port the router itself answers on now asks for confirmation.**
  Ports the router serves from the internet — remote access to its web
  interface and SSH (80/443/22, including "When behind NAT" mode while the
  router sits behind another router) and an inbound VPN's listen port —
  previously could be published to a device without warning, silently cutting
  that router service off from outside your network. Saving such a rule now
  surfaces the conflict in a confirmation dialog; you can still publish the
  port deliberately, and you're asked once per rule. Detection follows the
  live configuration (nothing is asked for ports no router service uses) and
  matches transports, so e.g. a UDP-only forward on 443 doesn't warn.
- The firmware build stamp is now identical everywhere it appears: the
  `startwrt` binary (UI `ETag`, `system.info`, `startwrt verify`) now carries
  the same full-hash `-modified`-suffixed stamp the Settings → General
  **Build** field bakes in via `config.json`, instead of a separate
  short-hash `-dirty` stamp.

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

- **The WAN IPv6 "6RD" mode now works.** Selecting it wrote a valid
  configuration, but the image shipped without the `6rd` protocol handler or
  the SIT kernel module it depends on, so the WAN interface simply came up
  without IPv6 — with nothing in the UI to indicate why. Both now ship.

- **Disabled outbound VPNs are no longer offered as a Security Profile's
  outbound route.** The VPN picker on the profile's WAN / Internet tab listed
  every VPN, including disabled ones — and picking one silently cut that
  profile off the Internet, since a disabled VPN's tunnel never comes up. The
  picker now lists only enabled VPNs, and the router refuses a profile pointed
  at a disabled — or nonexistent — VPN, so the command line can't create the
  same dead end.

  The same applied to VPN chaining, where it was quieter and worse: a disabled
  VPN could be picked as another VPN's **Connects to** target, and the chain
  then silently collapsed to a single hop. The downstream VPN connected
  normally — straight out your Internet connection instead of through the VPN
  you chained it to — and every screen reported success. Disabled VPNs are no
  longer offered as chain targets, and the router rejects one outright.

- **The bottom of the screen is no longer cut off on shorter displays.** On
  screens shorter than a page's contextual help content, the sidebar's
  Collapse button and the bottom of the page — including a form's Cancel/Save
  buttons, even when scrolled all the way down — were clipped, whether or not
  the help panel was open. The layout now always fits the screen exactly, and
  long help content scrolls within its panel.

- **A narrow window no longer cuts off the right side of the page.** Below
  the mobile-layout width, page content is laid out at the width the collapsed
  sidebar leaves free; with the sidebar still expanded, that layout ran under
  the window's right edge — hiding a form's Cancel/Save buttons and other
  right-edge content — with no way to scroll to it. The sidebar now collapses
  automatically when the window becomes that narrow (and re-collapses after
  navigating on a phone), and expanding it by hand at that width now shows a
  horizontal scrollbar, so everything stays reachable either way.

- **Changing a published port no longer briefly drops the firewall.** Applying
  a port-forward change restarted the firewall, which tears the whole ruleset
  down and rebuilds it as two separate steps — for a moment in between, the
  router had no firewall and no NAT, and connections started in that window
  were unfiltered. It now reloads instead, applying the new ruleset in a single
  atomic step with no gap, and a ruleset that failed to build leaves the
  running one untouched rather than leaving nothing in place.

- **Enabling LAN IPv6 no longer silently fails when the Admin profile routes
  through an IPv4-only VPN.** Saving the LAN IPv6 settings reported success
  but immediately reverted to disabled: the save re-derived the admin LAN's
  router advertisements from whether its outbound VPN carries IPv6, undoing
  the change in the same write. Editing the Admin profile onto an IPv4-only
  VPN likewise switched LAN IPv6 off behind your back — and kept it off even
  after switching the outbound back. The LAN IPv6 toggle is now the sole
  owner of that setting; with an IPv4-only VPN outbound, LAN devices still
  get local (ULA) IPv6 addresses while internet-bound IPv6 remains blocked
  by the VPN kill switch, so nothing leaks around the tunnel. That same
  fault could also leave a router where the LAN IPv6 page read "Disabled"
  while individual Security Profiles carried on handing out IPv6 addresses —
  the page and the network disagreeing, with no way to bring them back into
  line. Routers left in that state are now repaired automatically on the
  first start after updating, which turns IPv6 off for those profiles too;
  turn it back on from the LAN IPv6 page if you want it, and this time it
  applies everywhere at once.

- **Turning IPv6 off now tells your devices to drop their IPv6 addresses.**
  Devices choose their own IPv6 addresses from a prefix the router advertises,
  and the only way to take one back is to advertise it one last time as
  expired. The router was restarting its advertisement service instead of
  reloading it, which skips that goodbye entirely — so after disabling IPv6
  (on the LAN, on a Security Profile, or on the WAN) devices carried on using
  addresses that no longer worked, for up to 90 minutes, until the addresses
  timed out on their own. The notice is now sent while the prefix is still
  live. A device that is asleep or misses the notice still falls back to the
  timeout.

- **The Devices list no longer shows IPv6 addresses a device has given up.**
  The router remembers a neighbouring address long after the device stops
  using it, so a device could keep displaying an IPv6 address for hours after
  it dropped it — most visibly after turning IPv6 off, where the address on
  screen suggested nothing had changed. The router now confirms the device
  still answers on an address before showing it, and leaves the field empty
  when it does not.

- **Published ports no longer reshuffle their order on every refresh.** The
  list is auto-refreshed every few seconds, and each refresh returned the
  rows in an arbitrary order, so the table visibly jumped around. Published
  ports now appear in a stable order, sorted by label.

- **A device that missed its one chance to share its name over mDNS/Bonjour is
  no longer stuck with a generic label until the router reboots.** The name
  lookup was attempted exactly once per device, and it usually fired at the
  worst moment — the instant the device first appeared (before its Bonjour
  service finished starting), or during the reconnection rush right after a
  router reboot — and sleeping phones and laptops don't answer at all. The
  router now retries silent devices on a backoff schedule (about a minute
  after the first miss, stretching to a day) before concluding the device has
  no name to share; a device that answers is remembered permanently.

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

- Browsers could keep serving a stale, cached copy of the web UI after a
  firmware update — the router previously sent no cache headers, leaving cache
  behavior to per-browser heuristics (Firefox/Safari could silently run an old
  UI against the new backend). The embedded UI is now served with explicit
  headers: stable-named files (`index.html`, `assets/`) revalidate on every
  load against a per-build `ETag` (a cheap `304 Not Modified` when unchanged),
  while Angular's content-hashed bundles are cached as `immutable`. Requests
  for assets from an older build now get a `404` instead of a mis-typed
  `index.html` fallback.
- The "Updated to vX" confirmation shown after a firmware update is now
  translated instead of always appearing in English.

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

- **Published Ports endpoints no longer disappear when a WAN setting can't be
  read.** A network interface hand-configured with a protocol StartWRT doesn't
  manage (e.g. a `6in4` tunnel on `wan6`) made the WAN/LAN IPv6 settings
  endpoints error, and the Published Ports page treated that one failure as
  fatal — every port's IPv4 endpoint showed `—` even though the forwards were
  active. Unmanaged protocols now read back gracefully (reported as IPv6
  disabled) and are preserved untouched on disk, and the page now loads each
  WAN setting independently, so one failure can no longer blank out the
  endpoint list.

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
