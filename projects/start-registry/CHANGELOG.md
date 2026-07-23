# Changelog

All notable changes to `start-registry` (the Start Registry server) are documented here. This project is versioned **independently** (starting at `1.0.0`); its version lives in `Cargo.toml`. Format follows [Keep a Changelog](https://keepachangelog.com/).

## [1.0.1]

- **Registry responses stay compatible with pre-beta.10 clients.** `package.get` re-adds a
  stubbed (empty-object) manifest `alerts` field for clients older than StartOS `0.4.0-beta.10`,
  which require it present (`alerts` was removed from the manifest in beta.10). Removed the
  long-dead `0.4.0-alpha.17`/`alpha.18` client-compatibility shims.

## [1.0.0]

- **Signer/admin authentication moved to the new request-signature scheme.** Every authenticated
  request is signed with the client's identity key and carried in the `X-Start-Auth-Sig` header,
  bound to the registry hostname; on the registry host itself, the local authcookie is presented
  as an `Authorization: Bearer` token instead of a `Cookie`. Requires `start-cli` ≥ 1.1.0.

- **`--version` reports `start-registry`'s own version** (`1.0.0`), not the StartOS platform
  version.

- **Independent versioning.** `start-registry` now carries its own version (starting at `1.0.0`) in its `Cargo.toml`, decoupled from the StartOS release line; its `.deb` is versioned from the manifest.

## [0.4.0-beta.10] — StartOS 0.4.0-beta.10

The registry server as it ships in this monorepo. Built as the multi-call binary `registrybox`, dispatching to `start-registryd` (server) and `start-registry` (CLI).

### Added

- Combined `index` endpoint exposing registry name, icon, package index, OS index, and signers.
- `info`, `os`, `package`, `admin`, `db`, and `metrics` RPC subcommands, all reachable from the `start-registry` CLI via `call-remote`.
- Package and OS asset hosting with Blake3 content commitments and signer verification.
- HTTP JSON-RPC (`/rpc`), WebSocket RPC continuations (`/ws/rpc`), and a REST continuation channel (`/rest/rpc`).
- Local + signature auth on the RPC route; admin-only commands gated by an `admin` metadata flag.
- Anonymous download/user metrics persisted in a SQLite database, surfaced via `metrics summary` / `metrics users` (admin-only).
- PatchDB-backed registry database (`<datadir>/registry.db`) with a migration runner.
- Configurable listen address (default `127.0.0.1:5959`), public hostname(s), Tor SOCKS proxy, data directory, and config-file path.
- `start-registryd.service` systemd unit and Debian packaging that installs `registrybox` as `/usr/bin/start-registrybox` with `start-registryd` / `start-registry` symlinks.
- **Mirror fallback for package downloads (#3217).** When fetching a package from upstream fails, the registry retries the buffered download and then falls back to alternate mirrors. The buffered HTTP source is unified around a single `UploadingFile` reader type with a new `DownloadHandle::download_from` that owns the per-chunk write loop, `Content-Range` parsing, and truncate/seek-between-attempts, while the retry policy (`MirrorRetry`) lives outside as a factory closure yielding the next response. Uploads keep their `O_DIRECT` path untouched.

### Changed

- **Package manifest schema served by the index updated.** The registry's package index and `get` paths track the StartOS manifest changes shipped this release:
  - `nestedRuntime` is split into `userspaceFilesystems` and `virtualNetworking` (#3271).
  - the package `alerts` field is removed (#3333); the registry no longer indexes or serves alert messages.
- **Legacy OS-version URL mapping removed (#3327).** The registry drops the URL mapping it carried for old StartOS versions.

### Security

- Inherits the workspace-wide dependency and advisory cleanup shipped this release — resolved Dependabot alerts across the Rust workspace (#3301) and forked-dep RUSTSEC advisories in tokio-tar and async-acme (#3303).
- Outbound fetches that route through the host SOCKS proxy now correctly handle `.onion` (Tor) and `.local` (mDNS) targets (b5dec33cf).

[0.4.0-beta.10]: https://github.com/Start9Labs/start-technologies
