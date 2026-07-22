# start-cli

`start-cli` is the command-line client for StartOS. It is a thin `bin` crate over
[`start-core`](../../shared-libs/crates/start-core) (the shared Rust backend, crate `start-core`,
lib name `start_core`). The CLI surfaces the same RPC API the StartOS server exposes, plus
local developer tooling for building and signing `.s9pk` packages.

Most subcommands are _remote_ calls: you point `start-cli` at a running StartOS server
(`--host`/`-H`) and it invokes the server's RPC API over HTTPS. A handful of commands
(`s9pk`, `init-key`, `pubkey`, `util`) run locally and are what package authors use day to day.

## Quickstart

Build the binary from the monorepo root:

```sh
make start-cli                                            # build the start-cli bin
cargo build -p start-cli --bin start-cli            # dev shortcut (debug)
cargo build -p start-cli --bin start-cli --release  # dev shortcut (release)
```

Run it:

```sh
target/debug/start-cli --help

# talk to a server
target/debug/start-cli -H https://server.local auth login
target/debug/start-cli -H https://server.local package list

# local developer tooling (no server needed)
target/debug/start-cli init-key                    # create a developer key
target/debug/start-cli pubkey                       # print its public key
target/debug/start-cli s9pk pack ...                # build a package
```

> In a StartOS image, `start-cli` is provided as a symlink to the multiplexed
> `startbox` binary (see the OS `Makefile`), so it is always on the server's `PATH`.

## Connecting to a server

`-H/--host` accepts either a URL (`https://server.local`) or a `host` profile name
defined in a config file (see [Profiles](#profiles) below). Common flags:

| Flag                        | Purpose                                 |
| --------------------------- | --------------------------------------- |
| `-H, --host <url\|profile>` | Target server URL or config profile     |
| `--registry <url\|profile>` | Target registry for `registry` commands |
| `--proxy <url>`             | HTTP/SOCKS proxy for outbound requests  |
| `--id-key-path <path>`      | Identity signing key location           |
| `--insecure`                | Skip TLS verification (testing only)    |

### Profiles

Every config file carries named `host` and `registry` **profiles**:

```yaml
host:
  default: https://dev-vm.local
  prod: https://prodbox.local
registry:
  default: https://registry.start9.com
```

`-H prod` targets the `prod` profile; a command with no `-H` uses `default`. A bare URL is
shorthand for the `default` profile, so `host: https://dev-vm.local` and
`host: { default: https://dev-vm.local }` mean the same thing. `-H`/`-r` simply set the `default`
profile for that one invocation, on top of whatever the config files provide.

A profile's value is either a URL or the name of another profile, so `default: prod` (or `-H prod`)
points `default` at `prod`, and resolution follows the chain until it reaches a URL.

Profiles are read from these sources, highest precedence first:

1. `-H`/`-r` on the command line (set the `default` profile)
2. a config file named on the command line (`-c`)
3. the nearest workspace `.startos/config.yaml` (found by walking up from the current directory)
4. `~/.startos/config.yaml`
5. `/etc/startos/config.yaml`

The profiles from every source **combine** into one namespace, and a name defined at more than one
level takes its value from the higher tier — so `-H`'s `default` outranks the workspace's, which
outranks `~/.startos`'s. You can keep reusable profiles in `~/.startos/config.yaml` and reach them
with `-H` from anywhere, while a workspace still overrides `default` for the box you're standing in,
and a `host` left in `~/.startos/config.yaml` can't hijack the workspace's `default`. See
[`shared-libs/crates/start-core/src/context/config.rs`](../../shared-libs/crates/start-core/src/context/config.rs).

## Command surface

The full command tree comes from `start-core::main_api()`. Top-level groups include:

- `server`, `package`, `net`, `auth`, `db`, `ssh`, `wifi`, `disk`,
  `notification`, `backup`, `diagnostic`, `init`, `setup`, `kiosk` — server management (remote).
- `registry`, `tunnel` — operate against a registry / StartTunnel server.
- `s9pk`, `init-key`, `pubkey`, `util` — local packaging/dev tooling.
- `echo`, `state`, `git-info` — diagnostics.

Run `start-cli <group> --help` for any group.

## Features

Cargo features forward to `start-core`: `beta`, `console`, `dev`, `test`, `unstable`.
None are enabled by default.

## Documentation

- [`ARCHITECTURE.md`](./ARCHITECTURE.md) — how the crate is built (entrypoint, request flow, config).
- [`CONTRIBUTING.md`](./CONTRIBUTING.md) — how to contribute.
- [`AGENTS.md`](./AGENTS.md) — agent/dev rules; `CLAUDE.md` is a one-line `@AGENTS.md` import.

## License

MIT. See [LICENSE](../../LICENSE).
