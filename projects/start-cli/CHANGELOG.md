# Changelog

All notable changes to `start-cli` are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). As of 1.0.0 the crate is
versioned **independently** of the StartOS release line (it was previously pinned to
the OS version).

Because `start-cli` is a thin client over `start-core`, most user-visible CLI changes originate
in `start-core`; record here anything that changes this crate's entrypoint, features, packaging,
or the CLI's externally observable behavior.

## [1.0.3]

### Added

- **`host`/`registry` profiles work in every config file, not just the workspace.** A config
  file's `host`/`registry` is a namespace of named profiles — `default`, `prod`, whatever you
  like — and a bare URL is shorthand for the `default` profile, so a legacy flat
  `host: https://box.local` still targets that URL. Profiles from a `-c` file, the workspace
  `.startos/config.yaml`, `~/.startos/config.yaml`, and `/etc/startos/config.yaml` all **combine**
  into one namespace, so you can keep reusable profiles in your home config and reach them from
  anywhere — inside a packaging workspace or not. A profile's value can be a URL or the name of
  another profile, and `-H`/`-r` just set the `default` profile for that invocation (`-H prod`
  points `default` at your `prod` profile); resolution follows the chain to a URL. A value that
  isn't a URL is reported only when it's the one resolved, so a stale ambient config never fails an
  unrelated command.

### Fixed

- **`.local` (mDNS) hostnames resolve again.** Every command against a `.local` address — the
  way most people reach their server — failed after ~5s with an opaque
  `error sending request for url (...)`, while `curl` against the same URL succeeded instantly.
  `start-cli` ships as a statically linked musl binary, and musl's `getaddrinfo` implements no
  NSS: it ignores `/etc/nsswitch.conf`, so `mdns4_minimal` is never consulted and the lookup
  falls through to unicast DNS, which dies on musl's hard-coded 5s timeout. A `.local` host is
  now resolved through the system resolver (`getent`, the same path `curl` takes) when the
  client context is built, so the HTTP client and the log/progress websockets alike are handed
  an address. Linux only — macOS resolves `.local` natively. (#3469)

- **An ambient config file no longer shadows the workspace `.startos/config.yaml`.** A `host`
  set in `~/.startos/config.yaml` or `/etc/startos/config.yaml` was merged into the same field as
  an explicit `-H`/`-r`, so it silently outranked the workspace's `default` profile — the opposite
  of the documented layering, and a trap for anyone with a leftover pre-1.0 flat config in their
  home directory. Profiles now layer in a strict order, highest precedence first: a command-line
  flag (`-H`/`-r`), a config file named with `-c`, the workspace config, `~/.startos/config.yaml`,
  then `/etc/startos/config.yaml`. Where the same profile name is defined at more than one level
  the higher tier wins; what you name on the command line overrides the workspace, what you don't
  is an ambient default beneath it. A `-H`/`-r` naming an unknown profile still errors rather than
  quietly falling through to a lower tier.

## [1.0.2]

### Changed

- **`s9pk init-workspace` clones the whole monorepo, not just the guide.** The checkout is no
  longer sparse or shallow, so a packager has the SDK and StartOS source on hand when the guide
  can't settle a question — and a repo they can open a fix PR from. `--filter=blob:none` keeps it
  cheap: a few seconds and roughly 75 MB, with `git log`, `blame`, and rebase all working normally.
  An existing workspace keeps its narrow checkout. Widen it in place by running
  `sparse-checkout disable` in `start-technologies/`, plus `fetch --unshallow` if you want history.
- **`init-workspace` uses an existing `start-technologies` if you point at one.** Symlink it into
  the workspace before running, and the clone is skipped.
- **`s9pk init-workspace` links `AGENTS.md` to the guide's Agent Context page.** The workspace
  context file moved to `projects/start-sdk/docs/src/agent-context.md`, so it publishes with the
  rest of the packaging guide instead of being reachable only by scaffolding a workspace or
  browsing the monorepo. A `start-technologies/` that is a symlink to a monorepo checkout you
  maintain yourself is now a supported layout, and the guide says so.
- **The scaffolded `AGENTS.local.md` explains what belongs in it.** The stub now names the split:
  your box, your registry, your packages, and any departure from the scaffolded layout go there,
  while anything that would help every packager belongs upstream in the guide.

### Fixed

- `init-workspace` repoints a workspace `AGENTS.md` left over from an earlier release. Those
  symlinks target a path that no longer exists; re-running `init-workspace` in such a workspace
  replaces the dangling link. Everything else it finds is still left untouched.

## [1.0.1]

- **`s9pk init-package` initializes a git repository in the new package.** Packages are
  their own git repos — the template ships a `.gitignore` and GitHub Actions workflows —
  so `init-package` now runs `git init` in the scaffold. No commit is made; your first
  commit is yours.
- **`s9pk pack` no longer requires a committed git repository.** It still stamps the
  manifest with the repo's commit hash when one exists (suffixed `-modified` if the tree
  is dirty), but a freshly scaffolded package — a `git init` with no commit yet, or a
  non-git directory — now builds with the hash simply omitted, instead of failing with
  `fatal: not a git repository`. The hash appears once you make your first commit. This
  is what lets a brand-new package build immediately after `init-package`.
- **`s9pk init-workspace` no longer fails when a `.startos` exists above the target.**
  A leftover global `~/.startos` (or any enclosing workspace) used to trip a
  "Cannot create a workspace inside an existing one" guard and block workspace creation
  anywhere under it. Nesting is now allowed: `init-workspace` just creates the workspace,
  paying no attention to outer ones. When building, signing, or reading config, start-cli
  walks up from the current directory and uses the nearest `.startos/`, so a nested
  workspace transparently overrides an outer one (conceptually a deep merge of every
  `.startos/` on the path). The one refusal that remains is running **inside a package
  repo** — a workspace holds package repos; it isn't one. (There is no automatic
  migration from an older global `~/.startos`; copy `developer.key.pem` to a workspace's
  `.startos/build-key` yourself to reuse a signing key.)
- **Better "no workspace" errors when building/signing.** `s9pk pack` (and therefore
  `make` / `make publish`) needs a workspace signing key; when none is found above the
  cwd it now explains that packaging happens inside a workspace (which also brings the
  AI guide) and points to `init-workspace`. If you're **inside a package repo**, the
  error names the parent directory to run it in (`cd <parent> && start-cli s9pk
init-workspace`), so an existing package repo is one command away from building. The
  `init-workspace`-inside-a-package-repo refusal points at the same parent.
- **`--version` now reports `start-cli`'s own version** (`1.0.1`) rather than the StartOS
  platform version it was previously wired to.

## [1.0.0]

- **`package start --force`.** `start-cli package start <id> --force` starts a service
  even if it has an unresolved critical task (the backend gate lives in `start-core`).
- **Independent versioning.** `start-cli` now carries its own version (starting at
  `1.0.0`) in its `Cargo.toml`, decoupled from the StartOS release line.
- **Debian package.** `start-cli` is now packaged as a `.deb` (`make start-cli-deb`), so it
  can be installed and updated via apt. The build version is read from the crate
  manifest. `make start-cli-install` now stages the binary into `DESTDIR` for packaging; for
  a local PATH install run `build-cli.sh --install`.

## [0.4.0-beta.10]

- Client for the StartOS RPC API, built as a thin bin over the shared `start-core` crate
  (package `start-core`, lib `start_core`).
- Remote command surface dispatched over HTTPS: `server`, `package`, `net`, `auth`, `db`,
  `ssh`, `wifi`, `disk`, `notification`, `backup`, `diagnostic`, `init`, `setup`, `kiosk`,
  `registry`, `tunnel`.
- Local developer tooling: `s9pk` packaging, `init-key`/`pubkey` developer keys, `util` helpers.
- `STARTOS_USE_PODMAN` toggles the local container backend for `s9pk` packaging (defaults to Docker).
- Deprecated `embassy-cli` alias retained for backward compatibility.

### Added

- **Packaging workspace commands (#3251).** `s9pk init-workspace [PATH]` provisions an AI-ready packaging workspace (shallow-clones start-docs, links `AGENTS.md`/`CLAUDE.md`, generates an ed25519 build key and a multi-profile `.startos/config.yaml`), and `init-package "<Name>"` scaffolds a package from the template. s9pk signing now uses the workspace build key, and `-H`/`-r` accept a profile name from the workspace config or a literal URL.

### Fixed

- The publish cookie store locks on a stable file to stop a publish race (#3291).
- `ws_continuation` honors `--root-ca` / `--insecure` (#3274).
- `choose` falls back to a generic non-tty prompt instead of failing when stdin isn't a terminal (#3265).

[1.0.3]: https://github.com/Start9Labs/start-technologies/releases/tag/start-cli/v1.0.3
[1.0.2]: https://github.com/Start9Labs/start-technologies/releases/tag/start-cli/v1.0.2
[1.0.1]: https://github.com/Start9Labs/start-technologies/releases/tag/start-cli/v1.0.1
[1.0.0]: https://github.com/Start9Labs/start-technologies/releases/tag/start-cli/v1.0.0
