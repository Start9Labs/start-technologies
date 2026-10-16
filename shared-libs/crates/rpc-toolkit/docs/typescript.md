# TypeScript RPC bindings

Enable the opt-in `ts` Cargo feature; without it the bridge macros expand to
nothing, so product builds carry no generator code. Derive `visit_rs::VisitFields`
for each RPC parameter/result struct or `visit_rs::VisitVariants` for each enum,
then implement `rpc_toolkit::ts::TS` with `reflect_ts!`. Registered handlers need
bindings or `.no_ts()`. RPC collects storage through static named field and variant
visitors; selected type-valued metadata uses the independent
`visit_rs::VisitTypeAttributes` capability at the runtime crate root. Declaration
facts retain the full raw inventory.

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

Every root generates a module pair, `Bindings { schemas, types }`:

- The **schema module** is runtime code: one [zod](https://zod.dev) schema per
  Rust type, `export const Request: z.ZodObject<{...}> = object({...}).meta({ id: 'Request' })`,
  and for a handler tree the root as a plain object of schemas
  (`export const Api = { _PARAMS, _RETURN, _CHILDREN }`). It imports `zod` and
  nothing else. Every declaration is annotated with its schema type, with member
  JSDoc inside the type literal: TypeScript's declaration emit drops comments
  from an inferred shape, so the annotation is what keeps field documentation in
  a consumer's `.d.ts` and editor hover.
- The **type module** is type-only: `export type Request = z.output<typeof
schemas.Request>` and `export type RequestInput = z.input<typeof
schemas.Request>`, the method-path helpers, and
  `export type Api = RpcSchemaTree<typeof schemas.Api>`. It imports the schema
  module with `import type`, so consumers that import only types ship no
  runtime code.

`handler_bindings(&root, "Api", "./api.schemas")?` returns `Some(bindings)` or
`None` when the root opts out; the third argument is the specifier the type
module uses to import the schema module. Write the pair side by side, for
example `api.ts` and `api.schemas.ts`. `export_namespace(crate, namespace,
specifier)` does the same for standalone `ts_export!` roots.

```typescript
import type { Api, RpcParamType, RpcReturnType } from './api'
import { Api as schemas } from './api.schemas'
import { z } from 'zod'

type Params = RpcParamType<Api, 'users.create'>
type Result = RpcReturnType<Api, 'users.create'>
const inputSchema = z.toJSONSchema(schemas._CHILDREN.users._CHILDREN.create._PARAMS, { io: 'input' })
```

A node's `_PARAMS` already includes every enclosing parent's parameters (`z.intersection(Parent, Own)`; a parent with no parameters adds nothing), so `RpcParamType` is a lookup and `z.toJSONSchema(node._PARAMS)` describes the whole request. A parent with a
`root_handler` is callable at its own method path; the empty method selects the
root itself. A namespace without a root handler and an invalid method path resolve
to `never`. A child that opts out retains its ancestors' parameters while its own
parameters/results remain unknown; its node carries `_OPAQUE: true`, which the type
projection drops. A parent's opted-out `root_handler` marks that callable method
opaque while retaining bindings for its children.

With the `mcp` feature, a handler tree's schema module also exports
`toolCatalog(tree)`, the MCP tool
descriptions `rpc_toolkit::mcp::McpServer` serves: one per callable method with
bindings, named by its dotted path, with `z.toJSONSchema` of `_PARAMS` (input
direction) as `inputSchema` and of `_RETURN` (output direction) as `outputSchema`
when the result is an object. Run it at build time and embed the JSON:

```typescript
import { Api, toolCatalog } from './api.schemas'
writeFileSync('mcp-tools.json', JSON.stringify(toolCatalog(Api)))
```

## One schema, two directions

A Rust type has one deserialization shape and one serialization shape. The
generator renders both and emits a single schema whose `z.input` is the
deserialization shape and whose `z.output` is the serialization shape;
`z.toJSONSchema(schema, { io: 'input' | 'output' })` renders either side. Where
the two shapes agree the schema is plain zod. Where they differ, the
narrowest differing part is wrapped in `wire(input, output)`, an identity
`z.codec` the schema module defines: a field with `#[serde(default)]` becomes
`wire(z.string().optional(), z.string())`; a split rename, an input alias or a
directional skip wraps the whole struct or enum.

`parse` runs the input side and then the output side, so it accepts values
valid in both directions. Input-only required fields, split renames, aliases,
omitted defaults and serialized non-finite floats can prevent parsing a value
that is valid in one direction. The schemas are structural JSON validators,
not serde: numeric precision, unknown-field rejection and application
predicates stay with serde or the handler.

## JSON shapes

Parameter traversal uses serde deserialization rules; result traversal uses
serialization rules. A type's output alias keeps the Rust declaration name
(`Request`); its input alias gains `Input` (`RequestInput`) unless
`#[visit(ts(input_rename = "..."))]` names it. The type module projects each
alias only for the directions a declaration is reached from — a type that only
ever appears in `_RETURN` positions has no `Input` alias — so a Rust type named
`FooInput` beside `Foo` collides only if both directions are in use; then one of
them needs a rename. Definitions and the root name must begin with an ASCII
uppercase letter and contain only ASCII letters, digits and underscores; `z`,
`wire`, `object`, `schemas`, `self`, hoisted `literal_N` constants and the helper
type names are reserved. Distinct Rust
types cannot share a definition name.

Supported shapes include:

- Named, tuple, newtype and unit structs; transparent wrappers and flattening.
- Internally tagged named structs. Input tags are optional: serde ignores them.
- External, internal, adjacent and untagged enums; variant-level untagged forms.
  An enum of unit variants becomes `z.enum`.
- Serde renames, split serialize/deserialize renames, `rename_all_fields`, input
  aliases, directional skips, field/container defaults and conditional output keys.
- `from`, `try_from` and `into` conversions, using the conversion type's bindings.
- Primitives, nullable options, vectors/slices, fixed arrays, tuples up to 16
  elements, maps, `Box`/`Arc`/`Rc`, references and `Cow`.

Integers are `z.int()`; floats are `z.number()` on input and nullable on output,
since non-finite values serialize as `null`. `Option<T>` is `T.nullable()`; typed
`TS::IS_OPTION` propagates through aliases, references and `Box`/`Arc`/`Rc`/`Cow`,
and RPC combines that fact with serde policy: input option fields normally allow
omission, but custom deserialization hooks can require them. Output fields remain
required unless serde conditionally omits them. Defaults make input fields
optional, not output fields. Tuple options remain positional. Map keys are
strings, and map-key types do not need a `TS` implementation. Objects are built
through the module's `object()` helper, `z.object(shape).meta({ additionalProperties: true })`:
zod still strips unknown keys when parsing and the inferred TypeScript type gains
no index signature, but the JSON Schema stays open, so the parts of an
intersection — whether zod renders them as `allOf` branches or merges them into one
object — accept each other's members. An empty struct is
the module's `empty` constant, `object({})` typed as `z.ZodType<{}, {}>`, so
intersecting it with inherited parameters stays transparent. `Flat<A,B>` combines its two shapes with `z.intersection`.

Rust type, field and variant docs become JSDoc on declarations and object members
— which TypeScript carries through `z.input`/`z.output` to editor hover — and
`.meta({ description })` on the schema, which `z.toJSONSchema` emits. Named
declarations also carry `.meta({ id })`, so JSON Schema output references them
through `$defs`.

## Recursion and declaration order

Schemas are runtime constants, so the schema module declares every schema after
the schemas it references eagerly. A reference back to a declaration still being
rendered — direct or through a chain of eager references — is emitted as an
annotated getter on the enclosing object member:

```typescript
export const Node = z.object({
  get next(): z.ZodNullable<typeof Node> {
    return Node.nullable()
  },
})
```

The getter is the only form TypeScript infers for a self-referential schema and
the only form that defers evaluation, so recursion must pass through an object
member: a struct field, an externally or adjacently tagged variant, or a tagged
struct variant. A cycle that does not — an internally tagged or untagged newtype
variant holding the enum, a newtype or tuple struct holding itself, a flattened
field closing a cycle without any member on the way — is a generation error
(`Recursive reference ... outside an object member`); break it with a wrapper
struct or an override. The schema type is derived alongside the expression for
every built-in bridge; a literal override is hoisted into a module constant
(`const literal_0 = ...`) and typed as `typeof` that constant, so it must be a
closed expression that uses only `z`.

## Overrides and unsupported shapes

Custom serde hooks, identifier/remote derives, catch-all enum
variants and conditional tuple layouts return a generation error rather than
silently inventing a shape. Required tuple fields following optional fields also
need an override. Raw reflection retains skipped fields and variants. Add explicit
`visit(opaque)` to avoid visiting bounds on hidden storage; serde skips alone do
not remove generic reflection bounds.

Use `.override_params_ts_as::<WireParams>()` or
`.override_return_ts_as::<WireResult>()` to substitute another `TS` type. To supply a
literal zod expression, pass `rpc_toolkit::ts::LiteralTS("z.string()".into())` to
`.override_params_ts(...)` or `.override_return_ts(...)`. The caller owns that
expression's validity and its agreement with custom serialization; it is hoisted
into a module constant like a `ts(type = "...")` hint. These overrides compose
with CLI/display adapters. `.no_ts()` remains an opt-out through subsequent
decorations.

For individual field types with custom JSON representations, implement `TS`
directly: `visitor.leaf(value, schema_type)` appends a primitive with its schema
type, `visitor.append_type::<T>()` references another type, `visitor.capture`
renders a nested expression, the `Expr` helpers compose it (`nullable`, `array`,
`union`, `intersection`, ...), `visitor.wire(input, output)` joins directional
forms and `TSVisitor::error` rejects a type with no JSON form, failing generation.
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
#[visit(opaque, ts(type = "z.string()"), wire = "String",
        type_attributes(visit::wire))]
```

`ts(type = "...")` holds a zod expression. `#[visit(opaque, ts(skip))]` omits a
named field. Tuple and newtype omission is rejected. Literal text owns
nullability, not typed option status. Every replaced field needs typed facts from
ordinary storage or a selected wire target with `TS::IS_OPTION`; RPC combines that
fact with serde policy. A literal `null` or source spelling alone cannot supply
that fact. Reflection retains all descriptors and attributes, including opaque
storage and unselected hints. Container and variant opacity applies to descendant
storage fields, not to selected metadata callbacks.

Nongeneric bridges use the Rust declaration name by default. Exceptional names
use `#[visit(ts(rename = "PublicName", input_rename = "PublicInput"))]`.
Lifetime-only types remain named; generic instances inline unless the type opts
into a generic family or is registered as a concrete root. `impl_ts_shape!` remains
available for deliberate manual/inline bridges and explicit `{ define,
input_define }` policy. `TSWriter`, `type_writer` and `intersection_writer` supply
handler writers; `TSVisitor::reserve`, `declare`, `literal`, `in_direction` and
`into_bindings` manage composed declarations.

Generic bridges state their payload bounds explicitly and remain inline:

```rust
#[derive(Deserialize, Serialize, VisitFields)]
struct Payload<T> {
    value: T,
}
reflect_ts!(impl [T] for Payload<T> where [T: rpc_toolkit::ts::TS]);
```

`reflect_ts!(generic Payload<T>)` instead declares one schema factory,
`export const Payload = <T extends z.ZodType>(T: T) => { const self = ...; return self }`,
with `Payload<T>` and `PayloadInput<T>` type aliases, and renders each instance
as a call such as `Payload(z.string())`. A factory's return type is inferred, so
its members' JSDoc does not survive declaration emit. Parameters are plain type identifiers;
add other bounds with `where [...]`. The family renders with
`rpc_toolkit::ts::Param<N>` substituted for each parameter, so the declaration's
own bounds must hold for `Param`. Fields of a parameter type stay required on
input, a parameter name may not shadow another declaration, and a family refers
to itself as `self`. Register a family without an instance through
`rpc_toolkit::ts_export!(generic Payload<T>, namespaces = [""])`.

Standalone roots register beside their declarations with
`rpc_toolkit::ts_export!(Request, namespaces = ["events"])`. Concrete generic roots
supply their public name, for example
`rpc_toolkit::ts_export!(Payload<String>, name = "StringPayload", namespaces = [""])`.
Without `name`, a root uses `TS::DEFINE`. `export_namespace(crate_name, namespace,
specifier)` collects the roots and their transitive dependencies into a checked
module pair. Roots register through typed `ts_export!` calls, not export metadata.
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

## Consuming the schemas

The generated modules depend on `zod` 4 and resolve it from the consuming
package. A consumer that wants runtime validation imports the schema module;
`z.toJSONSchema` renders JSON Schema for either direction. The type module keeps
the previous type-only surface, so existing consumers need no change.

Every top-level schema initializer carries a `/* @__PURE__ */` annotation, which
bundlers honour across the whole constructor chain: a schema module can sit
behind a package's index and esbuild, webpack or Rollup still drop every
declaration, hoisted literal and API tree the consumer never imports. The
annotation only holds because schema construction has no side effects beyond
`z.globalRegistry` metadata; keep it that way in custom `TS` implementations.

Core and StartWRT generate their production server trees through this feature:
`make start-core-ts-bindings` and `make start-wrt-rpc-bindings` write the pairs
and rebuild consumers from them.

## Verification

From the monorepo root, `make rpc-toolkit-test` runs reflection and RPC feature
matrices. Generated module pairs are checked by the pinned TypeScript compiler
with strict and exact-optional-property checking against `zod`; negative
assertions must fail compilation, formatted output must be stable under prettier,
and schema modules are evaluated under node to check JSON Schema output. The same
suite exercises real inherited-parameter and callable-parent RPC dispatch.
`examples/generate_ts.rs` writes a complete module pair.
