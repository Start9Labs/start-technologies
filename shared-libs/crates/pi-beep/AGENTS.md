# AGENTS.md — pi-beep

A tiny first-party bin in the start-technologies monorepo at `shared-libs/crates/pi-beep` (Cargo package
`pi-beep`, binary `pi-beep`). It reimplements the Unix `beep` command using the Raspberry Pi's PWM
hardware exposed through `/sys/class/pwm/`. The whole crate is a single ~117-line `src/main.rs` whose
only dependency is `clap`. `CLAUDE.md` is a one-line `@AGENTS.md` import; edit this file instead.
See [ARCHITECTURE.md](ARCHITECTURE.md) and [CONTRIBUTING.md](CONTRIBUTING.md).

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## Layout

- `src/main.rs` — the whole crate.
  - `main` — builds the `clap` `Command` (flags `-f` freq, `-l` length, `-d`/`-D` delay, `-r` reps,
    `-n`/`--new` to start a fresh tone spec), exports `pwm0` if `/sys/class/pwm/pwmchip0/pwm0` is
    missing, then drives one or more tone specs via the inner `rec` recursion over `--new` groups.
  - `run_beep` — plays a tone `reps` times, sleeping `delay` between reps (and after the last rep
    only when invoked via `-D`).
  - `beep` — the PWM driver. Computes the period from the frequency, writes `duty_cycle`, `period`,
    and `enable` under `/sys/class/pwm/pwmchip0/pwm0/`, sleeps for the tone length, then disables.
- `README.txt` — plaintext usage/help text (not Markdown; preserved as-is).
- `Cargo.toml` — package metadata; sole dependency `clap`.
- `build-pi-beep.sh` — cross-compiles for aarch64-musl via the `rust-zig-builder` container (what
  the OS image build runs).
- `LICENSE` — MIT.

## Build & test (run from the repo root)

```bash
cargo build -p pi-beep                                          # host build
cargo build -p pi-beep --target=aarch64-unknown-linux-musl      # cross-compile for RPi (needs rust-zig-builder)
ARCH=aarch64 PROFILE=release ./shared-libs/crates/pi-beep/build-pi-beep.sh      # what the OS image build runs
cargo test -p pi-beep                                           # no tests defined; passes trivially
```

## Gotchas

- **RPi-only at runtime.** The binary writes directly to `/sys/class/pwm/`, so it does nothing
  useful off a Raspberry Pi with the PWM kernel driver loaded and the right device permissions. It
  only ships in `raspberrypi` OS image builds.
- **Cross-compiled for aarch64-musl** via the `rust-zig-builder` container (see the build script).
  Don't expect it to be exercised by a plain host `cargo build` in CI.
- **PWM export ordering.** `period`/`duty_cycle`/`enable` only exist once `pwm0` is exported.
  `main` writes `0` to `/sys/class/pwm/pwmchip0/export` when the `pwm0` dir is absent, before any
  `beep` call touches those files.
- **It panics on I/O errors** (every sysfs write/read is `.unwrap()`). That's intentional for a
  fire-and-forget tool — keep failures loud rather than swallowing them.
