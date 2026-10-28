#![cfg(feature = "ts")]
#![allow(dead_code)]

mod common;

use common::{declarations, SCHEMAS};
use rpc_toolkit::ts::{export_namespace, TSVisitor};
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
    let expected = "wire(object({ \"value\": z.string().nullable().optional() }), object({ \"value\": z.string() }))";
    let bindings = declarations::<CustomField>();
    assert!(bindings.schemas.contains(expected), "{}", bindings.schemas);
    let mut visitor = TSVisitor::new();
    rpc_toolkit::ts::visit_shape::<CustomField>(&mut visitor);
    assert_eq!(visitor.ts, expected);
}

#[test]
fn container_override_reuses_the_real_deserialization_representation() {
    let value: CustomContainer = serde_json::from_str(r#"{"value":null}"#).unwrap();
    assert_eq!(serde_json::to_string(&value).unwrap(), r#"{"value":""}"#);
    let bindings = declarations::<CustomContainer>();
    assert!(bindings.schemas.contains("export const LegacyForm: z.ZodObject<{ \"value\": z.ZodCodec<z.ZodOptional<z.ZodNullable<z.ZodString>>, z.ZodNullable<z.ZodString>> }> = /* @__PURE__ */ object({ \"value\": wire(z.string().nullable().optional(), z.string().nullable()) }).meta({\nid: \"LegacyForm\" });"));
    assert!(bindings.schemas.contains("export const CustomContainer: z.ZodCodec<typeof LegacyForm, z.ZodObject<{ \"value\": z.ZodString }>> = /* @__PURE__ */ wire(LegacyForm, object({ \"value\": z.string() })).meta({\nid: \"CustomContainer\" });"));
    assert!(bindings.types.contains("export type LegacyFormInput ="));
    let earlier = bindings.schemas.find("export const LegacyForm").unwrap();
    assert!(
        earlier
            < bindings
                .schemas
                .find("export const CustomContainer")
                .unwrap()
    );
}

#[test]
fn explicit_input_alias_avoids_a_real_output_name_collision() {
    let bindings = export_namespace("directional_ts", "directions", SCHEMAS).unwrap();
    assert!(bindings
        .types
        .contains("export type Task = z.output<typeof schemas.Task>;"));
    assert!(bindings
        .types
        .contains("export type TaskParams = z.input<typeof schemas.Task>;"));
    assert!(bindings
        .types
        .contains("export type TaskInput = z.output<typeof schemas.TaskInput>;"));
    assert!(bindings
        .types
        .contains("export type TaskInputInput = z.input<typeof schemas.TaskInput>;"));
    assert!(bindings
        .schemas
        .contains("\"input\": wire(TaskInput.nullable().optional(), TaskInput.nullable())"));
}
