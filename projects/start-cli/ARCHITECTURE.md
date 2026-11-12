# start-cli Architecture

`start-cli` is intentionally tiny. Almost all behavior lives in the shared
[`start-core`](../../shared-libs/crates/start-core) crate (cargo package `start-core`, lib name
`start_core`); this crate is just the `start-cli` entrypoint plus build/feature wiring.

## Place in the monorepo

This crate lives at `projects/start-cli/` and produces the standalone `start-cli` bin. Like the
other product bins (`startbox`, `start-container`, `registrybox`, `tunnelbox`) it depends on the
shared `start-core` crate, declared as
`start-core = { path = "../../shared-libs/crates/start-core" }`. See the root
[`ARCHITECTURE.md`](../../ARCHITECTURE.md) for the overall monorepo layout.

## What this crate contains

- `Cargo.toml` — declares the `start-cli` bin, the `start-core` dependency, and the
  feature flags (`beta`, `console`, `dev`, `test`, `unstable`) that forward to `start-core`.
- `src/main.rs` — the entrypoint. It:
  1. Reads `STARTOS_USE_PODMAN`; if unset/false it sets `PREFER_DOCKER` (so local
     `s9pk` packaging uses Docker by default).
  2. Builds a `start_core::bins::MultiExecutable`, enables the `start-cli` subcommand,
     sets it as the default, and `execute()`s.

`MultiExecutable` is the same multiplexer the OS uses for `startbox`: a single binary that
dispatches to a named sub-bin based on `argv[0]`/the first argument. `start-cli` enables only
the `start-cli` bin (and a deprecated `embassy-cli` alias) and defaults to it, so as a
standalone binary it behaves like a dedicated CLI.

## Request flow

```
argv ─▶ MultiExecutable ─▶ start_cli::main (in start-core)
                              │
                              ├─ build CliApp from start-core::main_api()
                              ├─ load ClientConfig (flags + -c + workspace + ~/.startos + /etc/startos)
                              └─ run:
                                   • remote command  ─▶ CliContext ─HTTP RPC / encrypted WebSocket─▶ server
                                   • local command   ─▶ executed in-process (s9pk, keys, util)
```

The actual CLI bin (`start_cli::main`) lives in
[`shared-libs/crates/start-core/src/bins/start_cli.rs`](../../shared-libs/crates/start-core/src/bins/start_cli.rs).
It constructs an `rpc_toolkit::CliApp` from `start-core::main_api()` and a `ClientConfig`,
renames the command to `start-cli`, stamps the start-cli product version, and runs it. Errors
are unwrapped from the RPC envelope and printed to stderr; the process exits with the RPC error code.

### Remote vs. local commands

`main_api()` (in [`shared-libs/crates/start-core/src/lib.rs`](../../shared-libs/crates/start-core/src/lib.rs))
registers every subcommand once with `with_call_remote::<CliContext>()` — those are dispatched
over the network through the target context's transport. Normal keyed StartOS, registry and Tunnel calls, including registry promotion, complete
a signed ephemeral-DH exchange before sending encrypted RPC.
The existing opaque signer supports both local keys and GPG-agent identities; fresh hello
signing precedes transport deadlines. Signed intent distinguishes RPC from each GUID. Failed encrypted admission never falls back to HTTP.
Ordinary local-authcookie calls and public registry calls without an initialized identity
retain HTTP. Typed progress can carry the same local credential over encrypted admission.
CLI `state`, setup, initialization and diagnostics use a client-lifetime proof identity
independently of stored keys; diagnostic local-cookie authorization retains its own namespace.
GUID clients select transport from the originating context; startup and normal keyed
continuations complete fresh same-peer admission. Their existing stream
consumers retain lifecycle ownership and require application completion rather than EOF.
Commands without a remote handler (`s9pk`, `init-key`, `pubkey`, `util`) execute locally. The `registry`
and `tunnel` groups can target a separate registry/tunnel host via `--registry`/`--tunnel`.

Registry CLI calls observe the verified peer immediately after admission and before RPC
execution. They atomically remember one public key per sanitized target under
`registry-identities/` beside the configured identity path; selected names survive mDNS
pinning. First contact is silent; key changes produce a best-effort stderr advisory with
old/new fingerprints and verification advice. Storage/output failures never change the
RPC result or command status. This registry-administration policy does not apply to
normal OS/Tunnel/WRT calls or to the OS's registry-cache connections.

Native terminal attach keeps its bidirectional GUID protocol. `service::attach_output` requires the explicit four-byte big-endian exit result; a Close or transport EOF cannot substitute for it. The existing blocking stdin worker polls its input and a cancellation socket; dropping the peer and bounded receiver releases idle reads and blocked sends before exit. Raw mode is restored before process exit. JSON attach arguments accept strings and legacy native encodings through shared Serde decoding; native CLI emission preserves non-UTF-8 bytes.

### Configuration

`ClientConfig` (in `start-core::context::config`) is the single config type — the workspace
`.startos/config.yaml`, `~/.startos/config.yaml`, `/etc/startos/config.yaml`, and `-c` files all
deserialize into it (a `schema` key marks a real workspace for the walk-up). `host`/`registry` are
each a `Profiles` namespace with one config key; `-H`/`-r` parse (via `Profiles`' `FromStr` /
`ValueParserFactory`) straight into `{ default: <value> }`, so the flag is just the top layer of
that namespace — no separate selector field. `ClientConfig::load` then layers the config files
under the flags: a `-c` chain, the discovered workspace (only its `host`/`registry`), `~/.startos`,
then `/etc/startos`. A bare URL is shorthand for `default`, and the layers combine into one
namespace — a union of aliases where a name defined at more than one level takes the higher tier's
value. `resolve_target` follows the `default` profile, chasing a value that names another profile
until it reaches a URL, which it parses only then. Its shared `resolve_profile` primitive also
resolves explicit `--from`/`--to` endpoints for registry package and OS promotion. `CliContext`
retains the merged `registry` profiles for these endpoint selections. `ClientConfig` also carries
tunnel, proxy, identity key path, root CAs, and the `--insecure` toggle. The loaded config becomes a
`CliContext` that the RPC handlers use. Proxy/TLS policy is parsed once for the ordinary HTTP client and
its HTTP/1-only, nonredirecting WebSocket-upgrade client; neither owns a separate reconnect
lifecycle.

## Man pages

`start_cli.rs` (in `start-core`) has a `#[test] export_manpage_start_cli` that runs `clap_mangen`
to write the committed man pages into this project's `man/` dir
(`projects/start-cli/man/`). Run it with `cargo test -p start-core export_manpage_start_cli`.

## Where to make changes

- New/changed CLI commands → `start-core` (`main_api()` and the relevant `src/<area>/` module),
  not here. This crate only changes when the entrypoint, features, or bin wiring change.
- Verifying a command end to end → build `start-cli` and run it against a test server/VM.

## Further reading

- [`README.md`](./README.md) — what `start-cli` is and how to use it.
- [`AGENTS.md`](./AGENTS.md) — how to contribute to this crate.
