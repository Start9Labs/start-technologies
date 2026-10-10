# TypeScript RPC bindings

Enable the opt-in `ts` Cargo feature; without it the bridge macros expand to
nothing, so product builds carry no generator code. Derive `visit_rs::VisitFields` for each RPC
parameter/result struct or `visit_rs::VisitVariants` for each enum, then implement
`rpc_toolkit::ts::TS` with `reflect_ts!`. Registered handlers need bindings or
`.no_ts()`. RPC collects storage through static named field and variant visitors;
selected type-valued metadata uses the independent `visit_rs::VisitTypeAttributes`
capability at the runtime crate root. Declaration facts retain the full raw inventory.

```rust
use rpc_toolkit::{reflect_ts, ts::handler_bindings};
use serde::{Deserialize, Serialize};
use visit_rs::VisitFields;

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
struct Request {
    user_name: String,
    #[serde(default)]
    enabled: bool,
}
reflect_ts!(Request);
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
output aliases retain the Rust declaration name (`Request`). Definitions and the root name
must begin with an ASCII uppercase letter and contain only ASCII letters, digits
and underscores. The inference helpers and the generated utility names `Partial`
and `Exclude` are reserved. Distinct Rust types cannot share a definition name.
Recursive types need a named definition; anonymous recursion returns an error.

Supported shapes include:

- Named, tuple, newtype and unit structs; transparent wrappers and flattening.
- Internally tagged named structs. Input tags are optional: serde ignores them.
- External, internal, adjacent and untagged enums; variant-level untagged forms.
- Serde renames, split serialize/deserialize renames, `rename_all_fields`, input
  aliases, directional skips, field/container defaults and conditional output keys.
- `from`, `try_from` and `into` conversions, using the conversion type's bindings.
- Primitives, nullable options, vectors/slices, fixed arrays, tuples up to 16
  elements, maps, `Box`/`Arc`/`Rc`, references and `Cow`.

`Option<T>` accepts null. Typed `TS::IS_OPTION` propagates through aliases,
references and `Box`/`Arc`/`Rc`/`Cow`; optionality is not a syntactic `Option` test.
RPC combines that fact with serde policy: input option fields normally allow
omission, but custom deserialization hooks can require them. Output fields remain
required unless serde conditionally omits them. Defaults make input fields
optional, not output fields. Tuple options remain positional.
Integers are JavaScript numbers; non-finite float outputs are null. Map keys are
strings, and map-key types do not need a `TS` implementation. `Flat<A,B>` combines
its two object shapes.

Rust type, field and variant docs become escaped JSDoc on declarations and inline
shapes. Directional renames/skips select documented wire members. Flattened and
transparent fields retain payload docs; custom wire overrides retain owner docs
and use the replacement type's member docs. Tagged variants document their wire
property; literal and untagged variants retain union-branch comments.

These are structural JSON types, not serde validators: numeric ranges/precision,
unknown-field rejection, duplicate aliases and application predicates are checked
by serde or the handler. Bindings describe JSON clients; CBOR-specific values need
an explicit override.

## Overrides and unsupported shapes

Custom serde hooks, identifier/remote derives, catch-all enum
variants and conditional tuple layouts return a generation error rather than
silently inventing a type. Required tuple fields following optional fields also
need an override. Raw reflection retains skipped fields and variants. Add explicit
`visit(opaque)` to avoid visiting bounds on hidden storage; serde skips alone do
not remove generic reflection bounds.

Use `.override_params_ts_as::<WireParams>()` or
`.override_return_ts_as::<WireResult>()` to substitute another `TS` type. To supply a
literal expression, pass `rpc_toolkit::ts::LiteralTS("expression".into())` to
`.override_params_ts(...)` or `.override_return_ts(...)`. The caller owns that
expression's validity and its agreement with custom serialization. These overrides
compose with CLI/display adapters. `.no_ts()` remains an opt-out through subsequent
decorations.

For individual field types with custom JSON representations, implement `TS`
directly; `TSVisitor::direction()` identifies input versus output traversal, and
`TSVisitor::error` rejects a type with no JSON form, failing generation.
RPC interprets `visit(wire = "WireType")` for both directions, or `input_wire` /
`output_wire` for one direction. Select the existing literal locally with
`visit(type_attributes(visit::wire))` (or the corresponding directional path).
Reflection emits the compiler-resolved target from that same literal in the owner's
scope, not from a second bridge-site type table or runtime type-name lookup.

For serde conversions, opt into the relevant metadata path locally:

```rust
#[derive(Deserialize, VisitFields)]
#[serde(from = "DecoderDto")]
#[visit(opaque, type_attributes(serde::from))]
struct RequestState {
    count: u32,
}

#[derive(Deserialize, VisitFields)]
struct DecoderDto {
    count: u32,
}
reflect_ts!(DecoderDto);
reflect_ts!(RequestState);

impl From<DecoderDto> for RequestState {
    fn from(dto: DecoderDto) -> Self {
        Self { count: dto.count }
    }
}
```

Select `serde::try_from` and `serde::into` where those conversions are used.
Selectors apply only at the annotated container, variant or field; they are not
inherited or automatically inferred. All selected types require `TS` support,
including targets a particular direction does not use. Actual serde derives
validate their conversion implementations; the generic callback does not impose
one blanket `From` requirement on mixed metadata roles.

For custom hooks, reuse the actual decoder/serializer DTO in an authored wire hint:

```rust
#[visit(opaque, input_wire = "DecoderDto",
        type_attributes(visit::input_wire))]
```

The application owns that hint's agreement with the actual hook. Neither typed
metadata nor hook signatures infer arbitrary JSON shape or validation predicates.
A non-omitted opaque field needs a selected typed target, including literal replacements.

For a literal replacement of hidden storage, supply typed optionality separately:

```rust
#[visit(opaque, ts(type = "string"), wire = "String",
        type_attributes(visit::wire))]
```

`#[visit(opaque, ts(skip))]` omits a named field. Tuple and newtype omission is
rejected. Literal text owns nullability, not typed option status. Every replaced
field needs typed facts from ordinary storage or a selected wire target with
`TS::IS_OPTION`; RPC combines that fact with serde policy.
A literal `null` or source spelling alone cannot supply that fact. Reflection
retains all descriptors and attributes, including opaque storage and unselected
hints. Container and variant opacity applies to descendant storage fields, not to
selected metadata callbacks.

Nongeneric bridges use the Rust declaration name by default. Exceptional names
use `#[visit(ts(rename = "PublicName", input_rename = "PublicInput"))]`.
Lifetime-only types remain named; generic instances inline unless the type opts
into a generic family or is registered as a concrete root. `impl_ts_shape!` remains available for deliberate manual/inline
bridges and explicit `{ define, input_define }` policy. `TSVisitor::with_direction`
selects input or output. `TSWriter`, `type_writer` and `intersection_writer`
supply writers; `TSVisitor::reserve`, `declare`, `intersection` and `declarations`
manage composed declarations.

Generic bridges state their payload bounds explicitly and remain inline:

```rust
#[derive(Deserialize, Serialize, VisitFields)]
struct Payload<T> {
    value: T,
}
reflect_ts!(impl [T] for Payload<T> where [T: rpc_toolkit::ts::TS]);
```

`reflect_ts!(generic Payload<T>)` instead declares one TypeScript generic,
`Payload<T>` and `PayloadInput<T>`, and renders each instance as a reference such
as `Payload<string>`. Parameters are plain type identifiers; add other bounds with
`where [...]`. The family renders with `rpc_toolkit::ts::Param<N>` substituted for
each parameter, so the declaration's own bounds must hold for `Param`. Fields of a
parameter type stay required on input, and a parameter name may not shadow another
declaration. Register a family without an instance through
`rpc_toolkit::ts_export!(generic Payload<T>, namespaces = [""])`.

Standalone roots register beside their declarations with
`rpc_toolkit::ts_export!(Request, namespaces = ["events"])`. Concrete generic roots
supply their public name, for example
`rpc_toolkit::ts_export!(Payload<String>, name = "StringPayload", namespaces = [""])`.
Without `name`, a root uses `TS::DEFINE`. `export_namespace(crate_name, namespace)`
collects input/output declarations and transitive dependencies into a checked
module. Roots register through typed `ts_export!` calls, not export metadata.
The generator collects registrations by namespace. `reflect_ts!`, `ts_export!`
and `impl_ts_shape!` are RPC-owned `macro_rules!` helpers, not semantic proc derives.
Recursive generic bridges state payload bounds rather than whole-root traversal
bounds. The registry reserves pending identities before descent; borrowed targets
need no added `'static` bound.

RPC's `ts` consumer owns rendering, docs, declaration/export policy and scalar
bridges. Optional integrations use RPC features `chrono`, `ipnet`, `josekit`,
`url`, `yajrc`, `exver` and `patch-db`; CBOR bridges follow `cbor`. imbl-value and
yasi bridges use existing one-way dependencies. patch-db supplies generic
`reflect` support for `Dump`; leaf crates expose no TypeScript facilities.

## Migration from the `ts-rs` feature

Use `ts` instead of rpc-toolkit's optional `ts-rs` feature. Replace
`HandlerTS::type_info` with `ts::handler_bindings`, derive `visit_rs::VisitFields`
for structs or `visit_rs::VisitVariants` for enums, and add an adjacent
`rpc_toolkit::reflect_ts!` consumer bridge. Import `TS`,
`TSVisitor`, `Unknown` and rendering/export APIs from `rpc_toolkit::ts`; register
standalone roots with `rpc_toolkit::ts_export!`. Enable integrations on RPC rather
than requesting TypeScript features from reflection or leaf crates. Add local
`type_attributes` selectors for authoritative type-valued hints and conversions;
add explicit opacity for unvisitable storage. Replace
`unknown_ts`/`custom_ts` with the parameter/return overrides above. `type_helpers`
lives in `rpc_toolkit::ts`.

Core and StartWRT generate their production server trees through this feature.
The same visitor renderer consumes standalone `ts_export!` registrations; use
`make start-core-ts-bindings` or `make start-wrt-rpc-bindings` and rebuild consumers.
Input declarations follow deserialization; output declarations follow serialization.

## Verification

From the monorepo root, `make rpc-toolkit-test` runs reflection and RPC feature
matrices. Generated JSON fixtures are checked by the pinned TypeScript compiler
with strict and exact-optional-property checking; negative assertions must fail
compilation. The same suite exercises real inherited-parameter and callable-parent
RPC dispatch. `examples/generate_ts.rs` provides a complete module generator.
