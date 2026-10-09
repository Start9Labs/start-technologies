#![cfg(feature = "ts")]
#![allow(dead_code)]

use rpc_toolkit::ts::{export_namespace, Direction, TSVisitor};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, visit_rs::VisitFields)]
struct CustomField {
    #[serde(default, deserialize_with = "nullable_string")]
    #[visit(type_attributes(visit::input_wire))]
    #[visit(input_wire = "Option<String>")]
    value: String,
}

fn nullable_string<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Deserialize, visit_rs::VisitFields)]
struct LegacyForm {
    value: Option<String>,
}

#[derive(Serialize, visit_rs::VisitFields)]
#[visit(type_attributes(visit::input_wire))]
#[visit(input_wire = "LegacyForm")]
struct CustomContainer {
    value: String,
}

impl<'de> Deserialize<'de> for CustomContainer {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let form = LegacyForm::deserialize(deserializer)?;
        Ok(Self {
            value: form.value.unwrap_or_default(),
        })
    }
}

#[derive(visit_rs::VisitFields)]
#[visit(ts(input_rename = "TaskParams"))]
struct Task {
    input: Option<TaskInput>,
}

#[derive(visit_rs::VisitFields)]
struct TaskInput(String);
rpc_toolkit::reflect_ts!(CustomField);
rpc_toolkit::reflect_ts!(LegacyForm);
rpc_toolkit::reflect_ts!(CustomContainer);
rpc_toolkit::reflect_ts!(Task);
rpc_toolkit::reflect_ts!(TaskInput);
rpc_toolkit::ts_export!(Task, namespaces = ["directions"]);

#[test]
fn directional_field_override_matches_custom_serde() {
    for json in ["{}", r#"{"value":null}"#, r#"{"value":"text"}"#] {
        let value: CustomField = serde_json::from_str(json).unwrap();
        assert!(serde_json::to_value(value).unwrap()["value"].is_string());
    }
    assert!(serde_json::from_str::<CustomField>(r#"{"value":2}"#).is_err());
    for direction in [Direction::Input, Direction::Output] {
        let mut visitor = TSVisitor::new();
        visitor.with_direction(direction, |visitor| visitor.append_type::<CustomField>());
        let ts = visitor.into_declarations().unwrap();
        let mut visitor = TSVisitor::new();
        visitor.with_direction(direction, |visitor| {
            rpc_toolkit::ts::visit_shape::<CustomField>(visitor)
        });
        let shape = visitor.into_module("Direct").unwrap();
        let expected = if direction == Direction::Input {
            "\"value\"?:((string|null))"
        } else {
            "\"value\":(string)"
        };
        assert!(ts.contains(expected), "{}", ts);
        assert!(shape.contains(expected), "{}", shape);
    }
}

#[test]
fn container_override_reuses_the_real_deserialization_representation() {
    let value: CustomContainer = serde_json::from_str(r#"{"value":null}"#).unwrap();
    assert_eq!(serde_json::to_string(&value).unwrap(), r#"{"value":""}"#);
    let mut visitor = TSVisitor::new();
    visitor.with_direction(Direction::Input, |visitor| {
        visitor.append_type::<CustomContainer>()
    });
    let input = visitor.into_declarations().unwrap();
    assert!(input.contains("export type LegacyFormInput ="));
    assert!(input.contains("\"value\"?:((string|null))"));
    let mut visitor = TSVisitor::new();
    visitor.append_type::<CustomContainer>();
    let output = visitor.into_declarations().unwrap();
    assert!(output.contains("\"value\":(string)"));
    assert!(!output.contains("LegacyForm"));
}

#[test]
fn explicit_input_alias_avoids_a_real_output_name_collision() {
    let module = export_namespace("directional_ts", "directions").unwrap();
    assert!(module.contains("export type Task ="));
    assert!(module.contains("export type TaskInput ="));
    assert!(module.contains("export type TaskParams ="));
    assert!(module.contains("\"input\"?:((TaskInputInput|null))"));
}
