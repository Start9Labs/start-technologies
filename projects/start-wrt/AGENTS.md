# AGENTS.md

Operating rules for **StartWRT** — Start's OpenWrt-based router OS. This scope assumes
you've read the root [`AGENTS.md`](../../AGENTS.md); only start-wrt-specific rules live here.

StartWRT pairs a Rust backend (single `startwrt` binary: RPC daemon + CLI) with an Angular UI
(a project in the root Angular workspace) embedded into that binary, shipped as a flashable
OpenWrt image for the SpaceMiT K1 (BananaPi-F3). See [ARCHITECTURE.md](ARCHITECTURE.md), [CONTRIBUTING.md](CONTRIBUTING.md) (build
workflow), and [API_CONTRACT.md](API_CONTRACT.md) (the RPC contract).

## Monorepo integration (read this first)

start-wrt was migrated from its own repo into this monorepo. Key consequences:

- **Backend crates are members of the root Cargo workspace** (`projects/start-wrt/backend/{ctrl,uciedit,uciedit_macros}`), not a separate workspace. The binary therefore lands in the **workspace-root `target/`**, not `backend/target/`. Build with `cargo build -p startwrt-core --bin startwrt` from the repo root.
- **`startwrt-core` consumes shared code directly:** the old embedded `start-os` submodule is gone — its `core/` crate is now the shared `start-core`, pulled in **aliased** as `startos` (`startos = { package = "start-core", path = "../../../../shared-libs/crates/start-core" }`) so existing `use startos::…` imports resolve unchanged. `rpc-toolkit` and `imbl-value` likewise point at the vendored `shared-libs/crates/` copies, not git/crates.io.
- **The web is a project in the root Angular workspace** (`start-wrt` in the root `angular.json`), sharing the root `package.json`/`node_modules`/`tsconfig.json` like every other app. Build it with `npm run build:wrt`, serve with `npm run start:wrt`, type-check with `npm run check:wrt` (all run from the repo root). It adopts `@start9labs/shared` for its common utilities — see [`web/AGENTS.md`](web/AGENTS.md).
- **`openwrt/` is a disposable, gitignored build workspace** (no submodule, no fork, no git repo inside). `make start-wrt-openwrt-setup` rebuilds it from the sha256-pinned upstream release tarball ([`build/openwrt-version`](build/openwrt-version)) and applies the Start9 delta: [`openwrt-patches/`](openwrt-patches/) (modified upstream files, applied with `patch -p1`) + [`openwrt-overlay/`](openwrt-overlay/) (added files, rsynced over the tree). Every setup run **rebuilds the tree** (generated state — `dl/`, `build_dir/`, `feeds/`, `files/`, `.config`, keys — is preserved) — never keep work inside it; change the patch/overlay dirs instead (workflow in [CONTRIBUTING.md](CONTRIBUTING.md#openwrt-tree-pinned-upstream--patches--overlay)). The binary build does _not_ need the workspace, only the full image does.
- **Build targets live in [`build.mk`](build.mk)** (included by the root `Makefile`), not a standalone product Makefile. From the repo root: `make start-wrt` (binary+web), `make start-wrt-image` (full image), `make start-wrt-update STARTWRT_REMOTE=…` (deploy). When you change a build input in `build.mk`, mirror it into `.github/workflows/start-wrt.yaml` `paths:` (root AGENTS.md "Coupled changes").

## Operating rules

- Don't run `make start-wrt-image` (full OpenWrt build) unsolicited — it fetches the OpenWrt tree and takes hours. For backend work use `cargo build -p startwrt-core --bin startwrt`; for frontend work use `npm run start:wrt`. Use `make start-wrt-update STARTWRT_REMOTE=…` only when explicitly asked to deploy — a request to test on the [live bench](#live-bench) is that request, for the bench router alone.
- Read the component-level `AGENTS.md` before operating on that component — they document footguns specific to each tree.
- Cross-frontend/backend changes: update `API_CONTRACT.md`, the Rust handler, `web/src/app/services/api/api.service.ts`, and **both** `live-api.service.ts` and `mock-api.service.ts` together. Skipping any breaks the contract.

## Live bench

A developer may keep a physical test router that you can drive yourself. Its details are
**local and never committed** — an untracked `CLAUDE.local.md` at the repo root says whether
this machine has one and describes it. With no such file, there is no bench: say what you would
have tested and stop.

### Topology

- **`wrt-bench`** — an SSH alias for the router under test, reached as `root` at its WAN
  address on the bench's own management port (`bench.sh mgmt`, 2222 by default): a second
  dropbear plus a `bench_mgmt_ssh` firewall rule admitting this machine alone. Remote Access
  never governs that port, so ports 22, 80, and 443 behave exactly as the product sets them
  and every Remote Access mode is testable. The router sits behind an upstream NAT whose LAN
  this machine is on, and **this machine is the WAN-side client** for every inbound test.
- **`os-bench`** — an SSH alias for a StartOS server wired to a router LAN port, reached as
  `start9` through `ProxyJump wrt-bench`. It is the real LAN client for anything StartOS and
  StartWRT negotiate (PCP/UPnP, DNS injection, SNI, port-check probes, hairpin to published
  domains), and optional for anything else. Run `start-cli` on it as
  `ssh os-bench 'sudo start-cli …'`.
- **Synthetic LAN clients** — network namespaces _on the router_, each a veth port on `br-lan`
  with its own MAC and profile VLAN, leasing from the router's own dnsmasq. Use them for
  anything needing more than one client or a specific profile. They carry only the router's
  BusyBox tools, and a `network` restart detaches them — re-run `lan-client up` after one.
- **Serial console** — `/dev/wrt-console` on this machine: boot output, panics, and hangs, and
  a root shell once `bench.sh console login` answers its `login` prompt. It is the recovery
  path whenever SSH is gone.
- **Web UI and RPC** — `bench.sh ui rpc <method>` calls through the daemon's login and session
  layer exactly as the web UI does, against the router's LAN address (loopback bypasses
  auth). `bench.sh ui tunnel` puts the UI at `https://127.0.0.1:8443/` for a headless browser.
  Both work whatever Remote Access is set to.

[`bench/bench.sh`](bench/bench.sh) drives all of it; `bench.sh --help` lists the commands.
Its state — the console log and config snapshots — lives in
`~/.local/state/startwrt-bench/`, outside the repo. A snapshot is a `sysupgrade` backup:
it holds the router's password hash and private keys, so it never leaves that directory.
The router's root password — the UI and console login — lives in
`~/.config/startwrt-bench/root-password` (mode 600), which the script reads.

### Out of reach

The bench cannot exercise these. `CLAUDE.local.md` adds what a particular bench lacks, such
as what its upstream router offers.

- **Wi-Fi** — no Wi-Fi client: association, password-to-profile assignment, schedules, RF.
- **Sources on the public Internet** — the only WAN-side client sits on a private upstream
  network: global-source Remote Access, port forwards and hostname routes reached from the
  Internet, ACME and DDNS against a real zone.
- **A second WAN-side client or an inbound VPN peer** — either needs root on this machine.
- **Upstream conditions** — PPPoE, static WAN, CGNAT, a lost WAN link, prefix delegation;
  `ifdown wan` on the router stands in only partly.
- **Outbound VPN** — needs a server off the bench.
- **Flashing and release** — the setup wizard's flash, eMMC and boot-partition provisioning,
  OTA from a registry, buttons and LEDs.
- **Per-port profiles** — one LAN port; synthetic clients stand in for the others.
- **Real clients** — phones, browsers other than headless Chromium, real-OS DHCP
  fingerprints.

### Protocol

1. **Map the regression surface** from the diff before touching the bench. List everything
   the change can reach beyond its own feature: other callers of what it changed, each UCI
   config it writes and each service it reloads, the nft chains and include files it touches,
   shared `start-core` code (StartOS and StartTunnel run it too), boot and init paths (test
   across a reboot), and anything staged into the image — `build/stage-files.sh`,
   `firstboot_config/`, the diffconfig, the OpenWrt delta — which only a flashed image
   exercises, upgraded with settings kept: a config file such as `/etc/inittab` survives the
   upgrade and shadows the new one. Every item gets a live test in step 6 or a line in the
   report saying why it was only reasoned about. An item under [Out of reach](#out-of-reach)
   is named as untested, with the manual test or setup that would cover it.
2. **Preflight.** `bench.sh preflight`, then `bench.sh console start`. Pass `--with-os` when
   the change touches anything StartOS and StartWRT negotiate; without it an unreachable
   `os-bench` is only noted. A failed check you cannot fix is the first thing you report, not
   something to work around.
3. **Snapshot** before changing any router config: `bench.sh snapshot save <branch-topic>`.
4. **Deploy.** `make start-wrt-update STARTWRT_REMOTE=wrt-bench`, then `bench.sh deployed` —
   a test run against a binary you did not confirm is not evidence.
5. **Drive the scenario** a user would meet, from the side they would meet it: inbound from
   this machine, outbound and LAN-side from `os-bench` or a synthetic client, the UI from a
   headless browser through `bench.sh ui tunnel`. Drive an RPC handler through
   `bench.sh ui rpc` as well as `startwrt-cli`: the CLI runs the handler in its own
   short-lived process, the UI in the daemon. Reproduce a bug on the old binary first when
   you can; a fix that passes a test which never failed has proven nothing.
6. **Test for regressions.** Drive each item from step 1, then `bench.sh smoke` — the standing
   checks every deploy must pass whatever it changed. Run `smoke` again after a reboot when
   the change touches boot, init, or anything written at startup.
7. **Collect evidence** from the router, not only from the client: `nft list ruleset`,
   `uci show <config>`, `logread -e startwrt`, `ubus call …`, the console log across reboots.
8. **Clean up.** `bench.sh lan-client down --all`; `bench.sh snapshot restore <label>` if the
   test changed config the next session should not inherit; undo anything you registered on
   `os-bench` (a domain, a port forward, an installed package).
9. **Report** what you verified and how, the regression surface and which of it you drove
   live, what you did not verify and why, and the **physical actions left for the
   developer** — nothing else is handed back.

### Rules

- **Keep the console running** before any change that can cut SSH: WAN, firewall input,
  dropbear, a reboot. If SSH does not come back, recover over the console (`console login`,
  then `console run`).
- **The router password stays in its file.** Never print it, put it on a command line, or
  write it anywhere else — not in a log, a report, `CLAUDE.local.md`, or a commit.
- **The management port is not under test.** Never remove `dropbear.bench` or
  `firewall.bench_mgmt`, and never use its port in a test. A scan of the WAN shows it open to
  this machine; that is the bench, not the product.
- **A bench router's backups stay with it.** They carry the management port and the bench
  key; never restore one onto another router. `bench.sh mgmt remove` strips the port before
  the router leaves the bench.
- **Ask first** for anything that flashes firmware (`sysupgrade` of an image, eMMC or boot
  partitions), installs packages on the router (it alters the image under test), or touches a
  host other than `wrt-bench` and `os-bench`.
- **A changed host key is a stop**, except for the router right after a reflash the developer
  just did, which regenerates it: then clear the stale entry with `ssh-keygen -R`.
- **Nothing about the bench enters git** — no addresses, hostnames, MACs, keys, or captures in
  code, commits, PR bodies, or issues. Refer to the aliases.
- **Stays manual:** a router neither SSH nor the console can reach, reflashing and the
  setup wizard's flash, Wi-Fi association from real devices and RF behaviour, anything
  board-physical (buttons, LEDs, cabling).

### One-time setup (developer)

1. **Key.** `ssh-keygen -t ed25519 -N '' -C startwrt-bench -f ~/.ssh/id_ed25519_wrtbench`.
   Add the public half to the router in the StartWRT UI's SSH keys, and to StartOS under
   `System > SSH`. Give the router's WAN a DHCP reservation on the upstream router so the
   address holds.
2. **SSH aliases** in `~/.ssh/config`:

   ```
   Host wrt-bench
     HostName <router WAN address>
     Port 2222
     User root
     IdentityFile ~/.ssh/id_ed25519_wrtbench
     IdentitiesOnly yes
     ServerAliveInterval 5
     ServerAliveCountMax 3
   Host os-bench
     HostName <StartOS LAN address>
     User start9
     ProxyJump wrt-bench
     IdentityFile ~/.ssh/id_ed25519_wrtbench
     IdentitiesOnly yes
   ```

   Then run `bench.sh mgmt install`, which reaches the router on port 22 while Remote Access
   is on and adds the management port. Run it again after a fresh reflash, or when this
   machine's address changes.

3. **Console access**, scoped to one adapter rather than the `dialout` group. Read the
   adapter's IDs with `udevadm info -a -n /dev/ttyUSB0 | grep -m3 -E 'idVendor|idProduct|serial'`,
   then write `/etc/udev/rules.d/99-wrt-console.rules` and replug the adapter:

   ```
   SUBSYSTEM=="tty", ATTRS{idVendor}=="<vid>", ATTRS{idProduct}=="<pid>", OWNER="<you>", MODE="0600", SYMLINK+="wrt-console"
   ```

   Pin it to one adapter with `ATTRS{serial}=="<serial>"`, or, when the adapter reports no
   serial, with `ENV{ID_PATH}=="<path>"` from `udevadm info -q property -n /dev/ttyUSB0`,
   which ties it to that USB port. Every process running as you then reaches the router's
   `login` prompt and, during boot, the U-Boot prompt, which can write flash.

4. **Router password** for the UI and console login, typed where no transcript records it:

   ```sh
   install -d -m 700 ~/.config/startwrt-bench
   (umask 077; read -rsp 'Router password: ' p; echo; printf '%s\n' "$p" >~/.config/startwrt-bench/root-password)
   ```

5. **`CLAUDE.local.md`** at the repo root (gitignored by `*.local.md`), loaded by every Claude
   Code session in that checkout. A worktree needs its own copy.

   ```markdown
   # Local bench

   This machine has a StartWRT live bench — see projects/start-wrt/AGENTS.md "Live bench".

   - Router: `wrt-bench`, LAN <subnet>, profiles and VLANs: <…>
   - StartOS client: `os-bench` on router port <n>, profile <name>, domains: <…>
   - Upstream: <what the upstream router can and cannot do — IPv6, PCP/UPnP, hairpin>
   ```

6. **Permissions** in `.claude/settings.local.json` (gitignored), so a session is not stopped
   at every command. This grants unprompted root on the bench router and sudo on the bench
   server:

   ```json
   {
     "permissions": {
       "allow": ["Bash(projects/start-wrt/bench/bench.sh:*)", "Bash(ssh wrt-bench:*)", "Bash(ssh os-bench:*)", "Bash(make start-wrt-update STARTWRT_REMOTE=wrt-bench)"]
     }
   }
   ```

## Sub-scopes

- [`backend/AGENTS.md`](backend/AGENTS.md) — Rust workspace (ctrl, uciedit, uciedit_macros)
- [`web/AGENTS.md`](web/AGENTS.md) — Angular + Taiga UI frontend
