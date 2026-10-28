# yasi: Yet Another String Interner

Uses Arc'd references, and performs Hash and Eq using Display, so that you can
intern anything implementing Display without allocating unless the value doesn't
already exist

## Place in the monorepo

- **Path:** `shared-libs/crates/yasi`
- **Package name:** `yasi` — build and test with `cargo … -p yasi` from the repo root.
- **Consumers (first-party, direct path deps):** `exver` and `imbl-value` (the latter enables the
  `serde` feature).

## Usage

```rust
use yasi::InternedString;

// Intern any Display + Into<String> value; equal strings share one allocation.
let a = InternedString::intern("hello, world, this is a long string");
let b = InternedString::from("hello, world, this is a long string");
assert_eq!(a, b);

// Intern a Display-only value without pre-allocating a String.
let c = InternedString::from_display(&format_args!("port-{}", 8080));

// Register a 'static constant.
let s = InternedString::from_static("static-label");

// InternedString derefs to str.
assert!(a.starts_with("hello"));
```

Strings of 20 bytes or fewer are inlined on the stack and skip the interning table entirely.

Optional features:

- `serde` — `Serialize` / `Deserialize` for `InternedString`.

RPC's `ts` consumer supplies the `InternedString` string bridge; enable bindings
on `rpc-toolkit`, not on this crate.

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — how it works (representations, Display-based hashing, the
  global table, and the lock-reentrancy discipline).
- [AGENTS.md](AGENTS.md) — building, testing, formatting, and PR conventions.

Licensed under MIT.
