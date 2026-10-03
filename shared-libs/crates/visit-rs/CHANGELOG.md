# Changelog

## [0.2.0]

### Added

- Shared TypeScript derive/rendering, checked annotation-owned export namespaces
  and directional custom serde representations.
- Explicit input declaration naming for input/output alias collisions.

- Monorepo-owned runtime and derive crates, consumed through workspace paths.
- `SerdeShape` traversal for separate serde input/output JSON representations.

### Fixed

- Structured multi-item attribute metadata, serde rename-rule parity, import-free
  generic enum derives and skipped enum fields across synchronous/asynchronous
  traversal.
- Metadata-free derives when `meta` is disabled.
