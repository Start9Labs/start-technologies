#![cfg(feature = "ts")]
#![allow(dead_code)]

use rpc_toolkit::reflect_ts;
use rpc_toolkit::ts::{export_namespace, Direction, TSVisitor};

#[derive(visit_rs::VisitFields)]
struct Derived {
    #[serde(default)]
    count: u32,
    #[serde(serialize_with = "custom_hook", deserialize_with = "custom_hook")]
    #[visit(type_attributes(visit::wire))]
    #[visit(ts(type = "string"), wire = "rpc_toolkit::ts::Unknown")]
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
    #[visit(ts(type = "number|null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
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
#[visit(type_attributes(visit::wire))]
#[visit(ts(type = "string"), wire = "rpc_toolkit::ts::Unknown")]
#[visit(opaque)]
struct Opaque<T>(T);

#[derive(visit_rs::VisitFields)]
struct SharedName {
    value: String,
}

#[test]
fn one_owner_can_export_into_multiple_namespaces() {
    let left = export_namespace("ts", "left").unwrap();
    let right = export_namespace("ts", "right").unwrap();
    assert_eq!(left, right);
    assert!(left.contains("export type SharedName ="));
    assert!(left.contains("export type SharedNameInput ="));
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
    let mut visitor = TSVisitor::new();
    visitor.append_type::<Node>();
    let output = visitor.into_declarations().unwrap();
    assert!(output.contains("export type Node ="));
    assert!(output.contains("\"next\":((Node|null))"));
    assert!(!output.contains("RpcHandler"));

    let mut visitor = TSVisitor::new();
    visitor.with_direction(Direction::Input, |visitor| visitor.append_type::<Node>());
    let input = visitor.into_declarations().unwrap();
    assert!(input.contains("export type NodeInput ="));
    assert!(input.contains("\"next\"?:((NodeInput|null))"));
}

#[test]
fn derives_share_directional_shapes_and_explicit_overrides() {
    let mut visitor = TSVisitor::new();
    visitor.with_direction(Direction::Input, |visitor| visitor.append_type::<Derived>());
    let input = visitor.into_declarations().unwrap();
    assert!(input.contains("\"count\"?:"));
    assert!(input.contains("\"custom\":(string)"));
    assert!(!input.contains("skipped"));

    let output = export_namespace("ts", "derived").unwrap();
    assert!(output.contains("export type Derived ="));
    assert!(output.contains("\"count\":(number)"));
    assert!(!output.contains("skipped"));

    let mut visitor = TSVisitor::new();
    visitor.append_type::<Opaque<NotBound>>();
    assert_eq!(
        visitor.into_module("OpaqueWire").unwrap(),
        "export type OpaqueWire = string;\n"
    );
}

#[test]
fn custom_deserializers_do_not_inherit_options_missing_field_default() {
    assert!(serde_json::from_str::<RequiredOption>("{}").is_err());
    let value: RequiredOption = serde_json::from_str(r#"{"amount":null}"#).unwrap();
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::json!({"amount":null})
    );
    let mut visitor = TSVisitor::new();
    visitor.with_direction(Direction::Input, |visitor| {
        visitor.append_type::<RequiredOption>()
    });
    let input = visitor.into_declarations().unwrap();
    assert!(input.contains("\"amount\":(number|null)"));
    assert!(!input.contains("\"amount\"?:"));
}

#[test]
fn annotated_concrete_exports_support_generic_recursion() {
    let output = export_namespace("ts", "generic").unwrap();
    assert!(output.contains("export type Generic ="));
    assert!(output.contains("\"next\":((Generic|null))"));
}

#[test]
fn collected_exports_reject_name_collisions() {
    assert!(export_namespace("ts", "collision")
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
        .into_declarations()
        .unwrap_err()
        .to_string()
        .contains("Node"));

    let mut visitor = TSVisitor::new();
    visitor.append_type::<String>();
    assert!(visitor.into_module("RpcHandler").is_ok());
}
