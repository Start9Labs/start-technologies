# Changelog

## [0.4.0]

### Changed

- Replace the optional `ts-rs` feature with `ts`, generating complete typed RPC
  method-tree modules from directional serde JSON shapes. Use `SerdeShape` and
  `impl_ts_shape!`; migrate `HandlerTS::type_info` to `ts::handler_bindings` and
  `custom_ts`/`unknown_ts` to parameter/return overrides. See
  [the migration guide](docs/typescript.md#migration-from-the-ts-rs-feature).

### Added

- Recursive named definitions, alias-conflict errors, typed callable parents,
  inherited-parameter inference and binding overrides that compose with other
  handler adapters.
