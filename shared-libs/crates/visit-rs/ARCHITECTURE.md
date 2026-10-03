# visit-rs architecture

The runtime defines generic value/type traversal traits. Its sibling
`visit-rs-derive` generates implementations; neither crate depends on rpc-toolkit.

`Static<T>`, `Covered<T>` and `Named<T>` select a type marker, covered value or named
field. `Variant` supplies enum variant metadata. Visitors determine the result type
and field handling. Returned iterators/streams preserve lazy traversal.

Raw attributes are structured as `AttributeMeta` under `meta`. Directional serde
shape traversal uses `SerdeShape` and `ShapeVisitor` instead: the derive lowers
serde's compile-time metadata to visitor callbacks, and the consumer owns rendering.
This keeps JSON rules out of TypeScript generation and separates input semantics
from serialization-oriented value reflection.

The Rust backend uses value visiting for shared networking/storage operations.
The optional `ts` module owns TypeScript rendering, declaration identity and
annotation-owned export collections. `TS` derives reuse `shape.rs` lowering;
input/output surrogate types also describe custom serde hooks. A single checked
registry owns aliases and pending/completed definitions across both directions.

rpc-toolkit reuses that renderer for handler parameters/results. Its own module
owns handler traversal, binding adapters and TypeScript method-inference helpers;
standalone types require no RPC dependency.
