# pi-beep

A reimplementation of `beep` but using the PWM chip of the Raspberry Pi.

`pi-beep` generates tones on Raspberry Pi hardware by driving the PWM channel exposed through
`/sys/class/pwm/`, rather than relying on a PC-speaker driver. It is a tiny single-file binary whose
only dependency is `clap`.

## Place in the monorepo

- **Path:** `shared-libs/crates/pi-beep`
- **Cargo package:** `pi-beep` (binary `pi-beep`); package name matches the directory.
- **First-party:** built as part of the workspace as a direct member — no `[patch]`, no vendored upstream.
- **Consumers:**
  - `projects/start-os/build.mk` — OS image build; cross-compiles to
    `target/aarch64-unknown-linux-musl/release/pi-beep` and installs it to `/usr/bin/pi-beep` on
    `raspberrypi` builds.
  - `build-pi-beep.sh` (in this dir) — cross-compiles for aarch64-musl via the
    `rust-zig-builder` container.
  - The image recipe symlinks `/usr/local/bin/beep -> /usr/bin/pi-beep`, so callers expecting a
    plain `beep` resolve to this binary.

## Usage

```
Usage: pi-beep [OPTIONS]

Options:
  -f <FREQ_Hz>          frequency of the tone in Hertz (Hz) [default: 440]
  -l <LENGTH_ms>        length of the tone in milliseconds (ms) [default: 200]
  -d <DELAY_ms>         delay between repetitions of the tone
                        *without* delay after last repetition of the tone [default: 100]
  -D <DELAY_ms>         delay between repetitions of the tone
                        *with* delay after last repetition of the tone
  -r <REPS>             number of repetitions of the last tone [default: 1]
  -n, --new [<new>...]
  -h, --help            Print help information
```

`-n`/`--new` starts a fresh tone specification, so a single invocation can chain multiple tones.

## License

MIT — see [LICENSE](LICENSE).

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — how it's built and how the PWM driver works.
- [CONTRIBUTING.md](CONTRIBUTING.md) — how to build, test, and contribute.
- [AGENTS.md](AGENTS.md) — agent/dev operating rules (`CLAUDE.md` is a one-line `@AGENTS.md` import).
