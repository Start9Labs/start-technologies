#![cfg(feature = "ts")]
#![allow(dead_code)]

mod common;

use common::{declarations, module, rejection};
use rpc_toolkit::reflect_ts;
use rpc_toolkit::ts::TS;
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
    let bindings = module::<Original>();
    let schemas = &bindings.schemas;
    assert!(schemas.contains("export const TestType: z.ZodCodec<z.ZodIntersection<z.ZodObject<{ \"old\": z.ZodOptional<z.ZodInt>; \"wire\": z.ZodOptional<z.ZodInt> }>, z.ZodUnion<readonly [z.ZodObject<{ \"old\": z.ZodInt }>, z.ZodObject<{ \"wire\": z.ZodInt }>]>>, z.ZodObject<{ \"wire\": z.ZodInt }>> = /* @__PURE__ */ wire(z.intersection(object({ \"old\": z.int().optional(), \"wire\": z.int().optional() }), z.union([object({ \"old\": z.int() }), object({ \"wire\": z.int() })])), object({ \"wire\": z.int() })).meta({\nid: \"TestType\" });"), "{}", schemas);
    assert!(!schemas.contains("original") && !schemas.contains("skipped"));
    common::typecheck(
        &bindings,
        "const canonical: TestTypeInput = { wire: 7 };\nconst legacy: TestTypeInput = { old: 7 };\n// @ts-expect-error\nconst missing: TestTypeInput = {};\n// @ts-expect-error\nconst output: TestType = { old: 7 };",
    );
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
#[test]
fn unsupported_shapes_fail_without_unknown_fallback() {
    let opaque = rejection::<OpaqueLiteral>();
    assert!(opaque.contains("Opaque field value requires a typed optionality fact"));
    assert!(opaque.contains("Opaque field option requires a typed optionality fact"));
    assert!(rejection::<UnselectedLiteral>().contains("Missing selected type metadata callback"));
    assert!(declarations::<OpaqueContainerLiteral>()
        .schemas
        .contains("const literal_0 = /* @__PURE__ */ z.number();\nexport const OpaqueContainerLiteral: typeof literal_0 = /* @__PURE__ */ literal_0.meta({\nid: \"OpaqueContainerLiteral\" });"));
    assert!(rejection::<OpaqueRequired>().contains("Opaque field hidden"));
    assert!(rejection::<MissingSelector>().contains("Missing selected type metadata callback"));
    assert!(rejection::<Invalid>().contains("transparent"));
    let bypass = declarations::<Bypass>();
    assert!(bypass.schemas.contains(
        "export const Bypass: z.ZodString = /* @__PURE__ */ z.string().meta({\nid: \"Bypass\" });"
    ));
    assert!(bypass
        .types
        .contains("export type BypassInput = z.input<typeof schemas.Bypass>;"));
}

#[derive(visit_rs::VisitFields)]
struct OpaqueLiteral {
    #[visit(opaque, ts(type = "z.number().nullable()"))]
    value: Hidden,
    #[visit(opaque, ts(type = "z.null()"))]
    option: Option<Hidden>,
}
reflect_ts!(OpaqueLiteral);
#[derive(visit_rs::VisitFields)]
struct UnselectedLiteral {
    #[visit(wire = "Maybe", ts(type = "z.number().nullable()"))]
    value: bool,
}
reflect_ts!(UnselectedLiteral);
#[derive(visit_rs::VisitFields)]
#[visit(opaque, ts(type = "z.number()"))]
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
    let bindings = declarations::<owner::Root>();
    let schemas = &bindings.schemas;
    assert!(schemas.contains(
        "export const Input: z.ZodObject<{ \"count\": z.ZodInt }> = /* @__PURE__ */ object({ \"count\": z.int() }).meta({\nid: \"Input\" });"
    ));
    assert!(schemas.contains(
        "export const Output: z.ZodObject<{ \"text\": z.ZodString }> = /* @__PURE__ */ object({ \"text\": z.string() }).meta({\nid: \"Output\" });"
    ));
    assert!(schemas.contains("export const Root: z.ZodCodec<typeof Input, typeof Output> = /* @__PURE__ */ wire(Input, Output).meta({\nid: \"Root\" });"));
    assert!(!<owner::Root as TS>::IS_OPTION);
    common::typecheck(
        &bindings,
        "const input: RootInput = { count: 1 };\nconst output: Root = { text: '1' };\n// @ts-expect-error\nconst wrong: RootInput = { text: '1' };",
    );
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
    let bindings = declarations::<WrappedOptions<'_>>();
    for name in ["alias", "reference", "boxed", "arc", "rc", "cow"] {
        assert!(
            bindings.schemas.contains(&format!(
                "\"{name}\": wire(z.string().nullable().optional(), z.string().nullable())"
            )),
            "{}",
            bindings.schemas
        );
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
    let required = declarations::<HookOption>().schemas;
    assert!(required.contains("wire(object({ \"value\": z.string().nullable() }), object({ \"value\": z.boolean() }))"), "{}", required);
    let defaulted = declarations::<DefaultHookOption>().schemas;
    assert!(defaulted.contains("wire(object({ \"value\": z.string().nullable().optional() }), object({ \"value\": z.boolean() }))"), "{}", defaulted);
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
        ts(type = "z.number().nullable()")
    )]
    replaced: u32,
    #[visit(ts(type = "z.number().nullable()"))]
    alias_literal: Maybe,
}
reflect_ts!(LazyTargets);
#[test]
fn unselected_and_literal_replaced_targets_never_register_definitions() {
    let schemas = declarations::<LazyTargets>().schemas;
    assert!(!schemas.contains("Unselected"), "{}", schemas);
    assert!(
        schemas.contains("const literal_0 = /* @__PURE__ */ z.number().nullable();\n"),
        "{}",
        schemas
    );
    assert!(schemas.contains("wire(object({ \"replaced\": literal_0.optional(), \"alias_literal\": literal_0.optional() }), object({ \"skipped\": z.boolean(), \"replaced\": literal_0, \"alias_literal\": literal_0 }))"), "{}", schemas);
}
