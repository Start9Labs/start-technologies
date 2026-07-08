# pi-beep Architecture

A single-file Rust binary that reimplements the Unix `beep` command on the Raspberry Pi's PWM
hardware. It generates tones by writing to the kernel's `/sys/class/pwm/` sysfs interface rather
than relying on a PC-speaker driver.

## Place in the monorepo

- **Path:** `shared-libs/crates/pi-beep`
- **Cargo package:** `pi-beep` (binary `pi-beep`); the package name matches the directory name.
- **Crate type:** binary (`src/main.rs`).
- **First-party:** built as part of the workspace; no `[patch]`, no vendored upstream.
- **Consumers:**
  - `projects/start-os/build.mk` — the OS image build. The `aarch64-unknown-linux-musl/release/pi-beep`
    target invokes the build script, and on `raspberrypi` platform builds the binary is copied to
    `/usr/bin/pi-beep`.
  - `shared-libs/crates/start-core/build/build-pi-beep.sh` — cross-compiles via the `rust-zig-builder`
    container (`cargo zigbuild ... -p pi-beep --bin pi-beep --target=$ARCH-unknown-linux-musl`).
  - `projects/start-os/build/image-recipe/build.sh` symlinks `/usr/local/bin/beep ->
/usr/bin/pi-beep`, so callers expecting plain `beep` (e.g. `init_resize.sh`) hit this binary.

## How it works

The only dependency is `clap`. `main`:

1. Builds a `clap` `Command` with `args_override_self(true)` and these flags:
   `-f <FREQ_Hz>` (f64, default 440), `-l <LENGTH_ms>` (u64, default 200),
   `-d <DELAY_ms>` (u64, default 100, delay _between_ reps), `-D <DELAY_ms>` (u64, delay _with_ a
   trailing delay after the last rep), `-r <REPS>` (usize, default 1), and `-n`/`--new` which begins
   a fresh tone specification (raw, variadic args).
2. If `/sys/class/pwm/pwmchip0/pwm0` does not exist, writes `0` to
   `/sys/class/pwm/pwmchip0/export` to export the channel.
3. Calls the inner `rec` helper, which plays the current tone spec and then re-parses any trailing
   `--new` argument group as another spec (recursing), so one invocation can chain several tones.

`run_beep(freq, len, delay, reps, end_delay)` plays `beep` `reps` times, sleeping `delay` between
reps; the sleep after the final rep happens only when `end_delay` is set (i.e. when `-D` was used).

`beep(freq, len)` is the PWM driver:

- Reads `period`; if it is `"0\n"`, seeds it with `1000` so the channel is in a writable state.
- Computes `new_period = round((1/freq) * 1_000_000_000)` nanoseconds.
- Writes `duty_cycle=0`, then `period=new_period`, then `duty_cycle=new_period/2` (a 50% duty cycle
  square wave), then `enable=1`.
- Sleeps for `len`, then writes `enable=0` to silence the tone.

All sysfs paths are module constants (`PWM_DIR`, `EXPORT_FILE`, `PERIOD_FILE`, `DUTY_FILE`,
`SWITCH_FILE`) pointing under `/sys/class/pwm/pwmchip0/`. Every filesystem operation `.unwrap()`s,
so any sysfs failure aborts the process.

## Further reading

- [README.txt](README.txt) — CLI usage and option defaults.
- [CONTRIBUTING.md](CONTRIBUTING.md) — how to build, test, and contribute.
- [AGENTS.md](AGENTS.md) — agent/dev operating rules (`CLAUDE.md` imports it).
