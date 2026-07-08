# i18n Patterns in `core/`

## Library & Setup

**Crate:** [`rust-i18n`](https://crates.io/crates/rust-i18n) v3.1.5 (`core/Cargo.toml`)

**Initialization** (`core/src/lib.rs:3`):

```rust
rust_i18n::i18n!("locales", fallback = ["en_US"]);
```

This macro scans `core/locales/` at compile time and embeds all translations as constants.

**Prelude re-export** (`core/src/prelude.rs:4`):

```rust
pub use rust_i18n::t;
```

Most modules import `t!` via the prelude.

## Translation File

**Location:** `core/locales/i18n.yaml`
**Format:** YAML v2 (~755 keys)

**Supported languages:** `en_US`, `de_DE`, `es_ES`, `fr_FR`, `pl_PL`

**Entry structure:**

```yaml
namespace.sub.key-name:
  en_US: 'English text with %{param}'
  de_DE: 'German text with %{param}'
  # ...
```

## Using `t!()`

```rust
// Simple key
t!("error.unknown")

// With parameter interpolation (%{name} in YAML)
t!("bins.deprecated.renamed", old = old_name, new = new_name)
```

## Key Naming Conventions

Keys use **dot-separated hierarchical namespaces** with **kebab-case** for multi-word segments:

```
<module>.<submodule>.<descriptive-name>
```

Examples:

- `error.incorrect-password` — error kind label
- `bins.start-init.updating-firmware` — startup phase message
- `backup.bulk.complete-title` — backup notification title
- `help.arg.acme-contact` — CLI help text for an argument
- `context.diagnostic.starting-diagnostic-ui` — diagnostic context status

### Top-Level Namespaces

| Namespace         | Purpose                                                    |
| ----------------- | ---------------------------------------------------------- |
| `error.*`         | `ErrorKind` display strings (see `src/error.rs`)           |
| `bins.*`          | CLI binary messages (deprecated, start-init, startd, etc.) |
| `init.*`          | Initialization phase labels                                |
| `setup.*`         | First-run setup messages                                   |
| `context.*`       | Context startup messages (diagnostic, setup, CLI)          |
| `service.*`       | Service lifecycle messages                                 |
| `backup.*`        | Backup/restore operation messages                          |
| `registry.*`      | Package registry messages                                  |
| `net.*`           | Network-related messages                                   |
| `middleware.*`    | Request middleware messages (auth, etc.)                   |
| `disk.*`          | Disk operation messages                                    |
| `lxc.*`           | Container management messages                              |
| `system.*`        | System monitoring/metrics messages                         |
| `notifications.*` | User-facing notification messages                          |
| `update.*`        | OS update messages                                         |
| `util.*`          | Utility messages (TUI, RPC)                                |
| `ssh.*`           | SSH operation messages                                     |
| `shutdown.*`      | Shutdown-related messages                                  |
| `logs.*`          | Log-related messages                                       |
| `auth.*`          | Authentication messages                                    |
| `help.*`          | CLI help text (`help.arg.<arg-name>`)                      |
| `about.*`         | CLI command descriptions                                   |

## Locale Selection

`core/src/bins/mod.rs:15-36` — `set_locale_from_env()`:

1. Reads `LANG` environment variable
2. Strips `.UTF-8` suffix
3. Exact-matches against available locales, falls back to language-prefix match (e.g. `en_GB` matches `en_US`)

## Adding New Keys

1. Add the key to `core/locales/i18n.yaml` with all 5 language translations
2. Use the `t!("your.key.name")` macro in Rust code
3. Follow existing namespace conventions — match the module path where the key is used
4. Use kebab-case for multi-word segments
5. Translations are validated at compile time
