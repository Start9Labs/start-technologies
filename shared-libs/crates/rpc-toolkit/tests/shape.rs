#![cfg(feature = "ts")]
#![allow(dead_code)]

use rpc_toolkit::reflect_ts;
use rpc_toolkit::ts::{Direction, TSVisitor, TS};
use serde::{Deserialize, Serialize};
use visit_rs::reflection::TypeInfo;
use visit_rs::{Named, Static, Visit, VisitFieldsStaticNamed, Visitor};

#[derive(Default)]
struct RawRecorder(Vec<Option<&'static str>>);
impl Visitor for RawRecorder {
    type Result = ();
}
impl<T: ?Sized> Visit<RawRecorder> for Static<T> {
    fn visit(&self, _: &mut RawRecorder) {}
}
impl<T: ?Sized> Visit<RawRecorder> for Named<'_, Static<T>> {
    fn visit(&self, visitor: &mut RawRecorder) {
        visitor.0.push(self.name);
    }
}
#[derive(Serialize, Deserialize, visit_rs::VisitFields)]
struct Original {
    #[serde(rename = "wire", alias = "old")]
    original: u32,
    #[serde(skip)]
    skipped: bool,
    #[serde(skip)]
    #[visit(skip)]
    presentation_skipped: bool,
}
reflect_ts!(Original);
fn module<T: TS>(direction: Direction) -> String {
    let mut visitor = TSVisitor::new();
    visitor.with_direction(direction, |v| v.append_type::<T>());
    visitor.into_module("Fixture").unwrap()
}
#[test]
fn raw_inventory_does_not_apply_consumer_renames_or_skips() {
    let mut raw = RawRecorder::default();
    Original::visit_fields_static_named(&mut raw).for_each(drop);
    assert_eq!(raw.0, [Some("original"), Some("skipped")]);
    let facts = Original::DECLARATION.fields;
    assert_eq!(facts.len(), 3);
    assert_eq!(
        facts
            .iter()
            .map(|field| field.visit_index)
            .collect::<Vec<_>>(),
        [Some(0), Some(1), None]
    );
    assert_eq!(facts[2].position.field, 2);
    assert_eq!(facts[2].name, Some("presentation_skipped"));
    assert!(facts[0].attributes.iter().any(|a| a.contains("rename")));
    assert_eq!(
        serde_json::to_value(Original {
            original: 7,
            skipped: true,
            presentation_skipped: true,
        })
        .unwrap(),
        serde_json::json!({"wire":7})
    );
    let decoded: Original = serde_json::from_str(r#"{"old":7}"#).unwrap();
    assert_eq!(decoded.original, 7);
    let input = module::<Original>(Direction::Input);
    assert!(input.contains("\"wire\"") && input.contains("\"old\""));
    let output = module::<Original>(Direction::Output);
    assert!(output.contains("\"wire\"") && !output.contains("\"old\""));
    assert!(!input.contains("original") && !input.contains("skipped"));
}
struct Hidden;
#[derive(visit_rs::VisitFields)]
struct OpaqueRequired {
    #[visit(opaque)]
    hidden: Hidden,
}
reflect_ts!(OpaqueRequired);
#[derive(visit_rs::VisitFields)]
#[visit(input_wire = "String", opaque)]
struct MissingSelector(Hidden);
reflect_ts!(MissingSelector);
#[derive(visit_rs::VisitFields)]
#[serde(transparent)]
struct Invalid {
    a: u32,
    b: u32,
}
reflect_ts!(Invalid);
#[derive(visit_rs::VisitFields)]
#[serde(transparent)]
#[visit(wire = "String", type_attributes(visit::wire), opaque)]
struct Bypass {
    a: Hidden,
    b: Hidden,
}
reflect_ts!(Bypass);
fn rejection<T: TS>(direction: Direction) -> String {
    let mut visitor = TSVisitor::new();
    visitor.with_direction(direction, |v| v.append_type::<T>());
    visitor.into_declarations().unwrap_err().to_string()
}
#[test]
fn unsupported_shapes_fail_without_unknown_fallback() {
    for direction in [Direction::Input, Direction::Output] {
        let opaque = rejection::<OpaqueLiteral>(direction);
        assert!(opaque.contains("Opaque field value requires a typed optionality fact"));
        assert!(opaque.contains("Opaque field option requires a typed optionality fact"));
        assert!(rejection::<UnselectedLiteral>(direction)
            .contains("Missing selected type metadata callback"));
        assert!(module::<OpaqueContainerLiteral>(direction).contains(" = number;"));
    }
    assert!(rejection::<OpaqueRequired>(Direction::Output).contains("Opaque field hidden"));
    assert!(rejection::<MissingSelector>(Direction::Input)
        .contains("Missing selected type metadata callback"));
    assert!(rejection::<Invalid>(Direction::Output).contains("transparent"));
    assert!(module::<Bypass>(Direction::Input).contains("BypassInput = string"));
}

#[derive(visit_rs::VisitFields)]
struct OpaqueLiteral {
    #[visit(opaque, ts(type = "number|null"))]
    value: Hidden,
    #[visit(opaque, ts(type = "null"))]
    option: Option<Hidden>,
}
reflect_ts!(OpaqueLiteral);
#[derive(visit_rs::VisitFields)]
struct UnselectedLiteral {
    #[visit(wire = "Maybe", ts(type = "number|null"))]
    value: bool,
}
reflect_ts!(UnselectedLiteral);
#[derive(visit_rs::VisitFields)]
#[visit(opaque, ts(type = "number"))]
struct OpaqueContainerLiteral(Hidden);
reflect_ts!(OpaqueContainerLiteral);

mod owner {
    use super::*;
    type InputAlias = Input;
    #[derive(Deserialize, visit_rs::VisitFields)]
    struct Input {
        count: u32,
    }
    reflect_ts!(Input);
    #[derive(Clone, Serialize, visit_rs::VisitFields)]
    struct Output {
        text: String,
    }
    reflect_ts!(Output);
    #[derive(Clone, Deserialize, Serialize, visit_rs::VisitFields)]
    #[serde(from = "InputAlias", into = "Output")]
    #[visit(opaque, type_attributes(serde::from, serde::into))]
    pub struct Root {
        value: u32,
    }
    impl From<Input> for Root {
        fn from(value: Input) -> Self {
            Self { value: value.count }
        }
    }
    impl From<Root> for Output {
        fn from(value: Root) -> Self {
            Self {
                text: value.value.to_string(),
            }
        }
    }
    reflect_ts!(Root);
}
#[test]
fn private_targets_and_mixed_conversion_roles_use_compiler_selected_callbacks() {
    let root: owner::Root = serde_json::from_str(r#"{"count":17}"#).unwrap();
    assert_eq!(
        serde_json::to_value(root).unwrap(),
        serde_json::json!({"text":"17"})
    );
    let input = module::<owner::Root>(Direction::Input);
    let output = module::<owner::Root>(Direction::Output);
    assert!(input.contains("\"count\":(number)") && !input.contains("\"text\""));
    assert!(output.contains("\"text\":(string)") && !output.contains("\"count\""));
    assert!(!<owner::Root as TS>::IS_OPTION);
}

type Maybe = Option<String>;
#[derive(visit_rs::VisitFields)]
struct WrappedOptions<'a> {
    alias: Maybe,
    reference: &'a Maybe,
    boxed: Box<Maybe>,
    arc: std::sync::Arc<Maybe>,
    rc: std::rc::Rc<Maybe>,
    cow: std::borrow::Cow<'a, Maybe>,
}
reflect_ts!(impl ['a] for WrappedOptions<'a>);
#[test]
fn typed_option_identity_survives_aliases_and_pointer_wrappers() {
    let input = module::<WrappedOptions<'_>>(Direction::Input);
    let output = module::<WrappedOptions<'_>>(Direction::Output);
    for name in ["alias", "reference", "boxed", "arc", "rc", "cow"] {
        assert!(input.contains(&format!("\"{name}\"?:")), "{}", input);
        assert!(output.contains(&format!("\"{name}\":")), "{}", output);
    }
}

#[derive(Deserialize, Serialize, visit_rs::VisitFields)]
struct HookOption {
    #[serde(deserialize_with = "decode_option")]
    #[visit(input_wire = "Maybe", type_attributes(visit::input_wire))]
    value: bool,
}
fn decode_option<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<bool, D::Error> {
    Ok(Maybe::deserialize(decoder)?.is_some())
}
reflect_ts!(HookOption);
#[derive(Deserialize, Serialize, visit_rs::VisitFields)]
struct DefaultHookOption {
    #[serde(default, deserialize_with = "decode_option")]
    #[visit(input_wire = "Maybe", type_attributes(visit::input_wire))]
    value: bool,
}
reflect_ts!(DefaultHookOption);
#[test]
fn selected_option_alias_combines_with_hook_and_default_rules() {
    assert!(serde_json::from_str::<HookOption>("{}").is_err());
    let value: HookOption = serde_json::from_str(r#"{"value":null}"#).unwrap();
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::json!({"value":false})
    );
    assert!(serde_json::from_str::<DefaultHookOption>("{}").is_ok());
    let required = module::<HookOption>(Direction::Input);
    assert!(required.contains("\"value\":((string|null))"));
    assert!(!required.contains("\"value\"?:"));
    assert!(module::<DefaultHookOption>(Direction::Input).contains("\"value\"?:((string|null))"));
    assert!(module::<HookOption>(Direction::Output).contains("\"value\":(boolean)"));
}
#[derive(visit_rs::VisitFields)]
struct Unselected {
    nested: String,
}
reflect_ts!(Unselected);
#[derive(visit_rs::VisitFields)]
struct LazyTargets {
    #[visit(input_wire = "Unselected", type_attributes(visit::input_wire))]
    #[serde(skip_deserializing)]
    skipped: bool,
    #[visit(
        wire = "Option<Unselected>",
        type_attributes(visit::wire),
        ts(type = "number|null")
    )]
    replaced: u32,
    #[visit(ts(type = "number|null"))]
    alias_literal: Maybe,
}
reflect_ts!(LazyTargets);
#[test]
fn unselected_and_literal_replaced_targets_never_register_definitions() {
    for direction in [Direction::Input, Direction::Output] {
        let module = module::<LazyTargets>(direction);
        assert!(!module.contains("Unselected"));
        assert!(module.contains(if direction == Direction::Input {
            "\"alias_literal\"?:(number|null)"
        } else {
            "\"alias_literal\":(number|null)"
        }));
        assert!(module.contains(if direction == Direction::Input {
            "\"replaced\"?:(number|null)"
        } else {
            "\"replaced\":(number|null)"
        }));
    }
}
