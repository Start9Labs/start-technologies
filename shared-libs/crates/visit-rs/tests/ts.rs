#![cfg(feature = "ts")]

use visit_rs::ts::{Direction, TSVisitor, export_namespace};
use visit_rs::{SerdeShape, impl_ts_shape};

#[derive(visit_rs::TS)]
#[ts(export, namespace = "derived")]
struct Derived {
    #[serde(default)]
    count: u32,
    #[serde(serialize_with = "custom_hook", deserialize_with = "custom_hook")]
    #[ts(type = "string")]
    custom: NotBound,
    #[serde(skip)]
    skipped: NotBound,
}
struct NotBound;

#[derive(serde::Serialize, serde::Deserialize, visit_rs::TS)]
struct RequiredOption {
    #[serde(deserialize_with = "deserialize_option")]
    #[ts(type = "number|null")]
    amount: Option<u32>,
}
fn deserialize_option<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u32>, D::Error> {
    serde::Deserialize::deserialize(deserializer)
}

#[derive(visit_rs::TS)]
#[ts(export, namespace = "generic", concrete(T = String))]
struct Generic<T> {
    value: T,
    next: Option<Box<Generic<T>>>,
}

#[derive(visit_rs::TS)]
#[ts(type = "string")]
struct Opaque<T>(T);

#[derive(visit_rs::TS)]
#[ts(export, namespace = ["left", "right"])]
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

#[derive(visit_rs::TS)]
#[ts(export, namespace = "collision", rename = "Same")]
struct First {
    value: String,
}
#[derive(visit_rs::TS)]
#[ts(export, namespace = "collision", rename = "Same")]
struct Second {
    value: u32,
}

#[derive(SerdeShape)]
struct Node {
    value: String,
    next: Option<Box<Node>>,
}
impl_ts_shape!(Node { define: "Node" });

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
    assert!(
        export_namespace("ts", "collision")
            .unwrap_err()
            .to_string()
            .contains("Conflicting")
    );
}

#[test]
fn enclosing_modules_own_their_reserved_names() {
    let mut visitor = TSVisitor::new();
    visitor.reserve(["Node"]);
    visitor.append_type::<Node>();
    assert!(
        visitor
            .into_declarations()
            .unwrap_err()
            .to_string()
            .contains("Node")
    );

    let mut visitor = TSVisitor::new();
    visitor.append_type::<String>();
    assert!(visitor.into_module("RpcHandler").is_ok());
}
