# imbl-value

A JSON `Value` type built on `imbl`.

`imbl-value` mirrors `serde_json::Value` but backs its collections with the persistent immutable data
structures from the [`imbl`](https://crates.io/crates/imbl) crate: arrays are `imbl::Vector`, objects are an
insertion-order-preserving `InOMap`, and strings are `Arc<String>`. Cloning a value is therefore cheap, and
snapshots structurally share memory. Numbers reuse `serde_json::Number`, so numeric behavior matches
`serde_json` exactly. The crate ships its own serde (de)serialization, a `json!` macro, and optional proptest
and ts-rs integrations.

## Place in the monorepo

- **Path:** `shared-libs/crates/imbl-value`
- **Package name:** `imbl-value` — build and test with `cargo … -p imbl-value`.
- **Crate type:** library.
- **Consumers (first-party):** `patch-db`, `json-patch`, `json-ptr`, `jsonpath_lib`, `rpc-toolkit`, and
  `start_core` — each depends on it as a direct path dependency (no `[patch]` redirection).

## Usage

Build a value with the `json!` macro and read it back with the typed accessors:

```rust
use imbl_value::{json, Value};

let v: Value = json!({
    "name": "start9",
    "ports": [80, 443],
    "tls": true,
});

assert_eq!(v["name"].as_str(), Some("start9"));
assert_eq!(v["ports"][1].as_u64(), Some(443));
```

Convert to and from arbitrary `serde` types:

```rust
use imbl_value::{to_value, from_value, Value};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct Config { name: String, retries: u8 }

let cfg = Config { name: "start9".into(), retries: 3 };
let v: Value = to_value(&cfg).unwrap();
let back: Config = from_value(v).unwrap();
```

## Features

- `arbitrary` — proptest `Arbitrary` impl and value/number/array/object strategies (off by default).
- `ts-rs` — `TS` impls for `Value`/`InOMap` for TypeScript type generation (off by default; required by
  `start_core`).

## Provenance

The `Value` (de)serialization impls (`src/de.rs`, `src/ser.rs`), the `Index` impls (`src/index.rs`), and the
`json!` macro (`src/macros.rs`) are derived from [serde_json](https://github.com/serde-rs/json), adapted to
the `imbl`-backed `Value`. serde_json is Copyright (c) Erick Tryzelaar and David Tolnay, dual-licensed
MIT OR Apache-2.0; those portions are used here under MIT. See [LICENSE](LICENSE).

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — how it works: the `Value` enum, module map, `InOMap`, and features.
- [CONTRIBUTING.md](CONTRIBUTING.md) — toolchain, build/test, and PR conventions.
- [AGENTS.md](AGENTS.md) — layout, commands, and gotchas for agents. `CLAUDE.md` is a one-line `@AGENTS.md`
  import.

## License

MIT — see [LICENSE](LICENSE). Portions derived from serde_json (see [Provenance](#provenance)) are used
under serde_json's MIT license; the notice is reproduced in [LICENSE](LICENSE).
