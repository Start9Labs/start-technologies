# visit-rs-derive architecture

Reflection macros generate the runtime's visiting traits without constructing
values for static traversal. Field-selection and naming helpers are shared across
struct and enum emission. Raw metadata generation is enabled with `meta`.

`SerdeShape` parses the same AST separately for serde input and output rules through
`serde_derive_internals`. It emits callbacks for struct/variant styles, enum tagging
and selected fields. Callback emission and visiting bounds come from the same
selected fields. Custom serde hooks report unsupported shapes instead of claiming
that the Rust field type describes its wire representation.

`serde_derive_internals` is pinned to one release; syn uses its matching major.
The downstream RPC tests check generated expressions against real serde fixtures
and the monorepo's pinned TypeScript compiler.
