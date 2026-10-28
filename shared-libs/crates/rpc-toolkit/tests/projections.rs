#![cfg(feature = "ts")]
#![allow(dead_code)]

mod common;

use common::{declarations, module, rejection, SCHEMAS};
use rpc_toolkit::ts::{TSVisitor, TS};

struct Hidden;

#[derive(visit_rs::VisitFields)]
struct Nested {
    #[visit(opaque, ts(skip))]
    context: Hidden,
    value: String,
}
rpc_toolkit::reflect_ts!(Nested);
#[derive(visit_rs::VisitFields)]
struct Projection {
    nested: Nested,
    required: u32,
    #[visit(ts(type = "z.number().nullable()"))]
    nullable: Option<f64>,
    #[visit(opaque, ts(skip))]
    omitted: Hidden,
    last: String,
}
rpc_toolkit::reflect_ts!(Projection);
#[derive(visit_rs::VisitFields)]
struct Tuple(#[visit(opaque, ts(skip))] Hidden, String);
rpc_toolkit::reflect_ts!(Tuple);
#[derive(visit_rs::VisitFields)]
#[visit(opaque)]
struct Newtype(#[visit(ts(skip))] Hidden);
rpc_toolkit::reflect_ts!(Newtype);

#[test]
fn omission_and_optional_state_are_scoped_to_each_field() {
    let schemas = declarations::<Projection>().schemas;
    assert!(!schemas.contains("context"));
    assert!(!schemas.contains("omitted"));
    assert!(schemas.contains("export const Nested: z.ZodObject<{ \"value\": z.ZodString }> = /* @__PURE__ */ object({ \"value\": z.string() })"));
    assert!(
        schemas.contains("const literal_0 = /* @__PURE__ */ z.number().nullable();\n"),
        "{}",
        schemas
    );
    assert!(schemas.contains("export const Projection: z.ZodObject<{ \"nested\": typeof Nested; \"required\": z.ZodInt; \"nullable\": z.ZodCodec<z.ZodOptional<typeof literal_0>, typeof literal_0>; \"last\": z.ZodString }> = /* @__PURE__ */ object({ \"nested\": Nested, \"required\": z.int(), \"nullable\": wire(literal_0.optional(), literal_0), \"last\": z.string() })"), "{}", schemas);
}
#[test]
fn omission_rejects_tuple_and_newtype_fields() {
    assert!(rejection::<Tuple>().contains("Omission requires a named field"));
    assert!(rejection::<Newtype>().contains("Omission requires a named field"));
}

/// Borrowed documentation.
#[derive(visit_rs::VisitFields)]
#[visit(ts(input_rename = "BorrowedParams"))]
struct Borrowed<'a>(&'a str);
rpc_toolkit::reflect_ts!(impl ['a] for Borrowed<'a> where []);
#[derive(visit_rs::VisitFields)]
struct Array<T, const N: usize>([T; N]);
rpc_toolkit::reflect_ts!(impl [T, const N: usize] for Array<T, N> where [T: TS]);
#[derive(visit_rs::VisitFields)]
struct LiteralFields {
    #[visit(ts(type = "z.boolean()"))]
    overridden: Nested,
    #[visit(ts(type = "z.number().nullable()"))]
    option: Option<Nested>,
    required: String,
}
rpc_toolkit::reflect_ts!(LiteralFields);

#[derive(visit_rs::VisitFields)]
#[visit(ts(type = "z.number()"))]
struct LiteralContainer(Nested);
rpc_toolkit::reflect_ts!(LiteralContainer);

#[test]
fn literals_do_not_register_replaced_definitions() {
    let schemas = declarations::<LiteralFields>().schemas;
    assert!(!schemas.contains("Nested"));
    assert!(
        schemas
            .contains("const literal_0 = /* @__PURE__ */ z.boolean();\nconst literal_1 = /* @__PURE__ */ z.number().nullable();\n"),
        "{}",
        schemas
    );
    assert!(schemas.contains("\"overridden\": literal_0"));
    assert!(schemas.contains("\"required\": z.string()"));
    assert!(
        schemas.contains("\"option\": wire(literal_1.optional(), literal_1)"),
        "{}",
        schemas
    );
    let schemas = declarations::<LiteralContainer>().schemas;
    assert!(schemas.contains(
        "const literal_0 = /* @__PURE__ */ z.number();\nexport const LiteralContainer: typeof literal_0 = /* @__PURE__ */ literal_0.meta({\nid: \"LiteralContainer\" });"
    ));
    assert!(!schemas.contains("Nested"));
}

macro_rules! bad_hint {
    ($ty:ident, $hint:meta) => {
        #[derive(visit_rs::VisitFields)]
        #[$hint]
        struct $ty(String);
        rpc_toolkit::reflect_ts!($ty);
    };
}
bad_hint!(BadValue, visit(ts(type = 42)));
bad_hint!(BadKey, visit(ts(unknown = "value")));
bad_hint!(BadList, visit(ts = "value"));
bad_hint!(Duplicate, visit(ts(type = "z.string()", type = "z.number()")));
bad_hint!(BadSkip, visit(ts(skip = true)));
bad_hint!(BadName, visit(ts(rename = "invalid-name")));
bad_hint!(BadNested, visit(ts(type("z.string()"))));
bad_hint!(BadOuter, visit(ts(type = "z.string()"), @ raw));
bad_hint!(UnknownOuter, visit(@ raw));

#[derive(visit_rs::VisitFields)]
struct BadOuterField {
    #[visit(ts(type = "z.string()"), @ raw)]
    value: u32,
}
rpc_toolkit::reflect_ts!(BadOuterField);

#[derive(visit_rs::VisitFields)]
struct BadOuterWire {
    #[visit(wire = "String", @ raw)]
    value: u32,
}
rpc_toolkit::reflect_ts!(BadOuterWire);

#[derive(visit_rs::VisitFields)]
struct ManualEmpty;
rpc_toolkit::impl_ts_shape!(ManualEmpty);
#[derive(visit_rs::VisitFields)]
struct ManualNamed(String);
rpc_toolkit::impl_ts_shape!(ManualNamed {
    define: "ManualAlias",
    input_define: "ManualParams"
});

#[test]
fn manual_bridges_keep_explicit_inline_and_alias_policy() {
    assert!(module::<ManualEmpty>()
        .schemas
        .contains("export const TestType: z.ZodNull = /* @__PURE__ */ z.null().meta({\nid: \"TestType\" });\n"));
    let bindings = declarations::<ManualNamed>();
    assert!(bindings.schemas.contains(
        "export const ManualAlias: z.ZodString = /* @__PURE__ */ z.string().meta({\nid: \"ManualAlias\" });"
    ));
    assert!(bindings
        .types
        .contains("export type ManualAlias = z.output<typeof schemas.ManualAlias>;\nexport type ManualParams = z.input<typeof schemas.ManualAlias>;"));
}

fn rejects_hint<T: TS>(message: &str) {
    assert!(rejection::<T>().contains(message));
}

#[test]
fn malformed_consumer_hints_are_runtime_errors() {
    rejects_hint::<BadValue>("Invalid TypeScript hint: type");
    rejects_hint::<BadKey>("Invalid TypeScript hint: unknown");
    rejects_hint::<BadList>("ts(...) list");
    rejects_hint::<Duplicate>("Duplicate TypeScript hint: type");
    rejects_hint::<BadSkip>("Invalid TypeScript hint: skip");
    rejects_hint::<BadName>("Invalid TypeScript definition name");
    rejects_hint::<BadNested>("Invalid TypeScript hint: type");
    rejects_hint::<BadOuter>("Malformed consumer metadata");
    rejects_hint::<BadOuterField>("Malformed consumer metadata");
    rejects_hint::<BadOuterWire>("Malformed consumer metadata");
}

#[test]
fn unknown_unparsed_metadata_remains_consumer_opaque() {
    assert!(declarations::<UnknownOuter>().schemas.contains(
        "export const UnknownOuter: z.ZodString = /* @__PURE__ */ z.string().meta({\nid: \"UnknownOuter\" });\n"
    ));
}

#[test]
fn precise_generic_bridges_preserve_docs_and_aliases() {
    let bindings = declarations::<Borrowed<'_>>();
    assert!(bindings.schemas.contains("Borrowed documentation.\n */\nexport const Borrowed: z.ZodString = /* @__PURE__ */ z.string().meta({\nid: \"Borrowed\", description: \"Borrowed documentation.\" });"), "{}", bindings.schemas);
    assert!(bindings.types.contains("Borrowed documentation.\n */\nexport type BorrowedParams = z.input<typeof schemas.Borrowed>;"), "{}", bindings.types);
    let mut v = TSVisitor::new();
    v.append_type::<Array<String, 2>>();
    assert!(v
        .into_bindings(Some("ArrayRoot"), SCHEMAS)
        .unwrap()
        .schemas
        .contains("export const ArrayRoot = z.tuple([z.string(), z.string()]);"));
}

#[derive(visit_rs::VisitFields)]
#[doc = "  preserved documentation  "]
#[serde(rename(serialize = "LegacyOutput", deserialize = "LegacyInput"))]
#[visit(ts(rename = "ExplicitAlias", input_rename = "ExplicitInput"))]
struct ConstantIdentity;
rpc_toolkit::reflect_ts!(ConstantIdentity);
#[derive(visit_rs::VisitFields)]
#[serde(rename(deserialize = "FallbackAlias"))]
struct LegacyFallback;
rpc_toolkit::reflect_ts!(LegacyFallback);
#[derive(visit_rs::VisitFields)]
#[visit(rename = "FirstAlias")]
#[serde(rename = "SecondAlias")]
struct LegacyFirst;
rpc_toolkit::reflect_ts!(LegacyFirst);
#[derive(visit_rs::VisitFields)]
#[serde(rename(serialize = "SerializedAlias", deserialize = "DeserializedAlias"))]
struct LegacyDirectional;
rpc_toolkit::reflect_ts!(LegacyDirectional);
#[derive(visit_rs::VisitFields)]
#[visit(ts(rename = "IgnoredGenericAlias", input_rename = "GenericInput"))]
struct GenericIdentity<T>(T);
rpc_toolkit::reflect_ts!(impl [T] for GenericIdentity<T> where [T: TS]);

#[test]
fn public_constants_match_reflected_declaration_identity_and_docs() {
    const ALIAS: Option<&str> = ConstantIdentity::DEFINE;
    const INPUT: Option<&str> = ConstantIdentity::INPUT_DEFINE;
    const DOCS: &[&str] = ConstantIdentity::DOCS;
    assert_eq!(ALIAS, Some("ExplicitAlias"));
    assert_eq!(INPUT, Some("ExplicitInput"));
    assert_eq!(DOCS, &["  preserved documentation  "]);
    assert_eq!(ConstantIdentity::define_name().as_deref(), ALIAS);
    assert_eq!(ConstantIdentity::input_define_name().as_deref(), INPUT);
    assert_eq!(ConstantIdentity::documentation(), DOCS);
    assert_eq!(LegacyFallback::DEFINE, Some("FallbackAlias"));
    assert_eq!(LegacyFirst::DEFINE, Some("FirstAlias"));
    assert_eq!(LegacyDirectional::DEFINE, Some("SerializedAlias"));
    assert_eq!(Borrowed::<'_>::DEFINE, Some("Borrowed"));
    assert_eq!(Borrowed::<'_>::INPUT_DEFINE, Some("BorrowedParams"));
    assert_eq!(Borrowed::<'_>::DOCS, &[" Borrowed documentation."]);
    assert_eq!(GenericIdentity::<String>::DEFINE, None);
    assert_eq!(
        GenericIdentity::<String>::INPUT_DEFINE,
        Some("GenericInput")
    );
    let bindings = declarations::<ConstantIdentity>();
    assert!(bindings
        .schemas
        .contains("export const ExplicitAlias: z.ZodNull = /* @__PURE__ */ z.null().meta({\nid: \"ExplicitAlias\", description: \" preserved documentation  \" });"), "{}", bindings.schemas);
    assert!(bindings
        .types
        .contains("export type ExplicitAlias = z.output<typeof schemas.ExplicitAlias>;"));
    assert!(bindings
        .types
        .contains("export type ExplicitInput = z.input<typeof schemas.ExplicitAlias>;"));
}
