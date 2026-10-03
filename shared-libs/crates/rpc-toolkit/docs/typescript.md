# TypeScript RPC bindings

Enable the opt-in `ts` Cargo feature. Derive `visit_rs::SerdeShape` for each RPC
parameter/result struct or enum, then implement `rpc_toolkit::ts::TS` with
`impl_ts_shape!`. Registered handlers need bindings or `.no_ts()`.

```rust
use rpc_toolkit::{impl_ts_shape, ts::handler_bindings};
use serde::{Deserialize, Serialize};
use visit_rs::SerdeShape;

#[derive(Deserialize, Serialize, SerdeShape)]
#[serde(rename_all = "camelCase")]
struct Request {
    user_name: String,
    #[serde(default)]
    enabled: bool,
}
impl_ts_shape!(Request { define: "Request" });
```

Given a composed handler tree, `handler_bindings(&root, "Api")?` returns
`Some(module)` or `None` when the root opts out. Write the module to a `.ts` or
`.d.ts` file. It exports the named definitions, `Api`, `RpcHandler`, `RpcParamType`
and `RpcReturnType`.

```typescript
import type { Api, RpcParamType, RpcReturnType } from './api'

type Params = RpcParamType<Api, 'users.create'>
type Result = RpcReturnType<Api, 'users.create'>
```

Parameters include every enclosing parent's parameters. A parent with a
`root_handler` is callable at its own method path; the empty method selects the
root itself. A namespace without a root handler and an invalid method path resolve
to `never`. A child that opts out has unknown parameters/results without weakening
its siblings' inference.

## JSON shapes

Parameter traversal uses serde deserialization rules; result traversal uses
serialization rules. Named input aliases receive an `Input` suffix (`RequestInput`);
output aliases retain the supplied name (`Request`). Definitions and the root name
must begin with an ASCII uppercase letter and contain only ASCII letters, digits
and underscores. The inference helpers and the generated utility names `Partial`
and `Exclude` are reserved. Distinct Rust types cannot share a definition name.
Recursive types need a named definition; anonymous recursion returns an error.

Supported shapes include:

- Named, tuple, newtype and unit structs; transparent wrappers and flattening.
- External, internal, adjacent and untagged enums; variant-level untagged forms.
- Serde renames, split serialize/deserialize renames, `rename_all_fields`, input
  aliases, directional skips, field/container defaults and conditional output keys.
- `from`, `try_from` and `into` conversions, using the conversion type's bindings.
- Primitives, nullable options, vectors/slices, fixed arrays, tuples up to 16
  elements, maps, `Box`/`Arc`/`Rc`, references and `Cow`.

`Option<T>` accepts null. An input object's option field can be omitted; an output
field remains required unless serde conditionally omits it. A default makes an
input field optional, not an output field. Tuple options remain positional.
Integers are JavaScript numbers; non-finite float outputs are null. Map keys are
strings, and map-key types do not need a `TS` implementation. `Flat<A,B>` combines
its two object shapes.

These are structural JSON types, not serde validators: numeric ranges/precision,
unknown-field rejection, duplicate aliases and application predicates are checked
by serde or the handler. Bindings describe JSON clients; CBOR-specific values need
an explicit override.

## Overrides and unsupported shapes

Custom serde hooks, identifier/remote derives, tagged structs, catch-all enum
variants and conditional tuple layouts return a generation error rather than
silently inventing a type. Required tuple fields following optional fields also
need an override. Fields and variants skipped in both directions impose no binding
requirements.

Use `.override_params_ts_as::<WireParams>()` or
`.override_return_ts_as::<WireResult>()` to substitute another `TS` type. To supply a
literal expression, pass `rpc_toolkit::ts::LiteralTS("expression".into())` to
`.override_params_ts(...)` or `.override_return_ts(...)`. The caller owns that
expression's validity and its agreement with custom serialization. These overrides
compose with CLI/display adapters. `.no_ts()` remains an opt-out through subsequent
decorations.

For individual field types with custom JSON representations, implement `TS`
directly; `TSVisitor::direction()` identifies input versus output traversal.

## Migration from the `ts-rs` feature

Use `ts` instead of rpc-toolkit's optional `ts-rs` feature. Replace
`HandlerTS::type_info` with `ts::handler_bindings`, and derive `visit_rs::TS`.
`SerdeShape` plus `impl_ts_shape!` remains available for manual schema ownership. Replace
`unknown_ts`/`custom_ts` with the parameter/return overrides above. `type_helpers`
lives in `rpc_toolkit::ts`.

Core and StartWRT generate their production server trees through this feature.
The same visitor renderer owns standalone `#[ts(export)]` namespaces; use
`make start-core-ts-bindings` or `make start-wrt-rpc-bindings` and rebuild consumers.
Input declarations follow deserialization; output declarations follow serialization.

## Verification

From the monorepo root, `make rpc-toolkit-test` runs reflection and RPC feature
matrices. Generated JSON fixtures are checked by the pinned TypeScript compiler
with strict and exact-optional-property checking; negative assertions must fail
compilation. The same suite exercises real inherited-parameter and callable-parent
RPC dispatch. `examples/generate_ts.rs` provides a complete module generator.
