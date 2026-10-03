# Changelog

## [0.2.0]

### Added

- Shared TypeScript derive/rendering, checked annotation-owned export namespaces
  and directional custom serde representations.
- Explicit input declaration naming for input/output alias collisions.

- Workspace-owned proc macros and the directional `SerdeShape` derive.

### Fixed

- Multi-item metadata, serde rename-rule reuse, generic enum emission and skipped
  enum fields with correct tuple indices and visiting bounds.
- Feature-aligned metadata initializers and warning-free unit-enum iterators.
