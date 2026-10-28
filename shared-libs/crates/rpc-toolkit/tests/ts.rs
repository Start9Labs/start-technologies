#![cfg(feature = "ts")]
#![allow(dead_code)]

mod common;

use common::{declarations, SCHEMAS};
use rpc_toolkit::reflect_ts;
use rpc_toolkit::ts::{export_namespace, TSVisitor};

#[derive(visit_rs::VisitFields)]
struct Derived {
    #[serde(default)]
    count: u32,
    #[serde(serialize_with = "custom_hook", deserialize_with = "custom_hook")]
    #[visit(type_attributes(visit::wire))]
    #[visit(wire = "String")]
    #[visit(opaque)]
    custom: NotBound,
    #[serde(skip)]
    #[visit(opaque)]
    skipped: NotBound,
}
struct NotBound;
reflect_ts!(Derived);
rpc_toolkit::ts_export!(Derived, namespaces = ["derived"]);
reflect_ts!(RequiredOption);
reflect_ts!(impl [T] for Generic<T> where [T: rpc_toolkit::ts::TS]);
rpc_toolkit::ts_export!(Generic<String>, name = "Generic", namespaces = ["generic"]);
reflect_ts!(impl [T] for Opaque<T>);
reflect_ts!(SharedName);
rpc_toolkit::ts_export!(SharedName, namespaces = ["left", "right"]);
reflect_ts!(First);
reflect_ts!(Second);
rpc_toolkit::ts_export!(First, namespaces = ["collision"]);
rpc_toolkit::ts_export!(Second, namespaces = ["collision"]);

#[derive(serde::Serialize, serde::Deserialize, visit_rs::VisitFields)]
struct RequiredOption {
    #[serde(deserialize_with = "deserialize_option")]
    #[visit(type_attributes(visit::wire))]
    #[visit(wire = "Option<u32>")]
    amount: Option<u32>,
}
fn deserialize_option<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u32>, D::Error> {
    serde::Deserialize::deserialize(deserializer)
}

#[derive(visit_rs::VisitFields)]
struct Generic<T> {
    value: T,
    next: Option<Box<Generic<T>>>,
}

#[derive(visit_rs::VisitFields)]
#[visit(opaque, ts(type = "z.string()"))]
struct Opaque<T>(T);

#[derive(visit_rs::VisitFields)]
struct SharedName {
    value: String,
}

#[test]
fn one_owner_can_export_into_multiple_namespaces() {
    let left = export_namespace("ts", "left", SCHEMAS).unwrap();
    let right = export_namespace("ts", "right", SCHEMAS).unwrap();
    assert_eq!(left, right);
    assert!(left.schemas.contains("export const SharedName: z.ZodObject<{ \"value\": z.ZodString }> = /* @__PURE__ */ object({ \"value\": z.string() })"));
    assert!(left
        .types
        .contains("export type SharedName = z.output<typeof schemas.SharedName>;"));
    assert!(left
        .types
        .contains("export type SharedNameInput = z.input<typeof schemas.SharedName>;"));
}

#[derive(visit_rs::VisitFields)]
#[visit(ts(rename = "Same"))]
struct First {
    value: String,
}
#[derive(visit_rs::VisitFields)]
#[visit(ts(rename = "Same"))]
struct Second {
    value: u32,
}

#[derive(visit_rs::VisitFields)]
struct Node {
    value: String,
    next: Option<Box<Node>>,
}
reflect_ts!(Node);

#[test]
fn standalone_declarations_do_not_require_an_rpc_tree() {
    let bindings = declarations::<Node>();
    assert!(bindings.schemas.contains("export const Node: z.ZodObject<{ \"value\": z.ZodString; \"next\": z.ZodCodec<z.ZodOptional<z.ZodNullable<typeof Node>>, z.ZodNullable<typeof Node>> }> = /* @__PURE__ */ object({ \"value\": z.string(), get \"next\"()"));
    assert!(bindings.schemas.contains(
        "get \"next\"(): z.ZodCodec<z.ZodOptional<z.ZodNullable<typeof Node>>, z.ZodNullable<typeof Node>> { return wire(Node.nullable().optional(), Node.nullable()); }"
    ));
    assert!(bindings
        .schemas
        .starts_with("import { z } from 'zod'\nconst wire ="));
    assert!(bindings
        .types
        .contains("export type Node = z.output<typeof schemas.Node>;"));
    assert!(bindings
        .types
        .contains("export type NodeInput = z.input<typeof schemas.Node>;"));
    assert!(!bindings.types.contains("RpcHandler"));
    assert!(!declarations::<SharedName>().schemas.contains("const wire"));
}

#[test]
fn derives_share_directional_shapes_and_explicit_overrides() {
    let bindings = export_namespace("ts", "derived", SCHEMAS).unwrap();
    assert!(bindings
        .schemas
        .contains("\"count\": wire(z.int().optional(), z.int())"));
    assert!(bindings.schemas.contains("\"custom\": z.string()"));
    assert!(!bindings.schemas.contains("skipped"));
    assert!(bindings.types.contains("export type Derived ="));

    let mut visitor = TSVisitor::new();
    visitor.declare::<Opaque<NotBound>>("OpaqueWire");
    assert!(visitor
        .into_bindings(None, SCHEMAS)
        .unwrap()
        .schemas
        .contains(
        "export const OpaqueWire: typeof literal_0 = /* @__PURE__ */ literal_0.meta({\nid: \"OpaqueWire\" });\n"
    ));
}

#[test]
fn custom_deserializers_do_not_inherit_options_missing_field_default() {
    assert!(serde_json::from_str::<RequiredOption>("{}").is_err());
    let value: RequiredOption = serde_json::from_str(r#"{"amount":null}"#).unwrap();
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::json!({"amount":null})
    );
    let bindings = declarations::<RequiredOption>();
    assert!(bindings
        .schemas
        .contains("\"amount\": z.int().nullable() }"));
    assert!(!bindings.schemas.contains("optional"));
}

#[test]
fn annotated_concrete_exports_support_generic_recursion() {
    let bindings = export_namespace("ts", "generic", SCHEMAS).unwrap();
    assert!(bindings
        .schemas
        .contains("export const Generic: z.ZodObject<{ \"value\": z.ZodString; \"next\": z.ZodCodec<z.ZodOptional<z.ZodNullable<typeof Generic>>, z.ZodNullable<typeof Generic>> }> = /* @__PURE__ */ object("));
    assert!(bindings.schemas.contains("get \"next\"(): z.ZodCodec<z.ZodOptional<z.ZodNullable<typeof Generic>>, z.ZodNullable<typeof Generic>>"));
    assert!(bindings
        .types
        .contains("export type Generic = z.output<typeof schemas.Generic>;"));
}

#[test]
fn collected_exports_reject_name_collisions() {
    assert!(export_namespace("ts", "collision", SCHEMAS)
        .unwrap_err()
        .to_string()
        .contains("Conflicting"));
}

#[test]
fn enclosing_modules_own_their_reserved_names() {
    let mut visitor = TSVisitor::new();
    visitor.reserve(["Node"]);
    visitor.append_type::<Node>();
    assert!(visitor
        .into_bindings(None, SCHEMAS)
        .unwrap_err()
        .to_string()
        .contains("Node"));

    let mut visitor = TSVisitor::new();
    visitor.append_type::<String>();
    assert!(visitor.into_bindings(Some("RpcHandler"), SCHEMAS).is_ok());
    for reserved in ["z", "wire", "object", "schemas"] {
        let mut visitor = TSVisitor::new();
        visitor.declare::<Node>(reserved);
        assert!(
            visitor.into_bindings(None, SCHEMAS).is_err(),
            "{}",
            reserved
        );
    }
}

struct Rejected;
impl rpc_toolkit::ts::TS for Rejected {
    fn visit_ts(visitor: &mut TSVisitor) {
        visitor.error("Rejected has no JSON form");
    }
}

#[test]
fn custom_implementations_report_errors() {
    let mut visitor = TSVisitor::new();
    visitor.append_type::<Vec<Rejected>>();
    assert!(visitor
        .into_bindings(None, SCHEMAS)
        .unwrap_err()
        .to_string()
        .contains("Rejected has no JSON form"));
}

#[derive(visit_rs::VisitFields)]
struct Later {
    first: Earlier,
}
reflect_ts!(Later);
#[derive(visit_rs::VisitFields)]
struct Earlier {
    value: String,
}
reflect_ts!(Earlier);

#[test]
fn declarations_precede_their_eager_references() {
    let bindings = declarations::<Later>();
    let earlier = bindings.schemas.find("export const Earlier").unwrap();
    let later = bindings.schemas.find("export const Later").unwrap();
    assert!(earlier < later, "{}", bindings.schemas);
}

#[derive(visit_rs::VisitVariants)]
#[serde(tag = "kind")]
enum Direct {
    Leaf,
    Branch(Box<Direct>),
}
reflect_ts!(Direct);
#[derive(visit_rs::VisitFields)]
struct Wrapped(Vec<Wrapped>);
reflect_ts!(Wrapped);

#[test]
fn recursion_outside_object_members_is_rejected() {
    for error in [
        common::rejection::<Direct>(),
        common::rejection::<Wrapped>(),
    ] {
        assert!(error.contains("outside an object member"), "{}", error);
    }
}
