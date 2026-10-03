#![cfg(feature = "ts")]

use serde::{Deserialize, Serialize};
use visit_rs::shape::SerdeShape;
use visit_rs::ts::{Direction, TSVisitor, export_namespace};

#[derive(Deserialize, Serialize, visit_rs::SerdeShape, visit_rs::TS)]
struct CustomField {
    #[serde(default, deserialize_with = "nullable_string")]
    #[visit(input_wire = "Option<String>")]
    value: String,
}

fn nullable_string<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Deserialize, visit_rs::TS)]
struct LegacyForm {
    value: Option<String>,
}

#[derive(Serialize, visit_rs::TS)]
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

#[derive(visit_rs::TS)]
#[ts(export, namespace = "directions", input_rename = "TaskParams")]
struct Task {
    input: Option<TaskInput>,
}

#[derive(visit_rs::TS)]
struct TaskInput(String);

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
            CustomField::visit_shape(visitor, direction)
        });
        let shape = visitor.into_module("Direct").unwrap();
        let expected = if direction == Direction::Input {
            "\"value\"?:((string|null))"
        } else {
            "\"value\":(string)"
        };
        assert!(ts.contains(expected), "{ts}");
        assert!(shape.contains(expected), "{shape}");
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
