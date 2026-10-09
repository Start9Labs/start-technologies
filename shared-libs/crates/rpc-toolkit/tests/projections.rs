#![cfg(feature = "ts")]
#![allow(dead_code)]

use rpc_toolkit::ts::{Direction, TSVisitor, TS};

struct Hidden;

#[derive(visit_rs::VisitFields)]
struct Nested {
    #[visit(type_attributes(visit::wire))]
    #[visit(ts(skip), wire = "rpc_toolkit::ts::Unknown")]
    #[visit(opaque)]
    context: Hidden,
    value: String,
}
rpc_toolkit::reflect_ts!(Nested);
#[derive(visit_rs::VisitFields)]
struct Projection {
    nested: Nested,
    required: u32,
    #[visit(type_attributes(visit::wire))]
    #[visit(ts(type = "number|null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[visit(opaque)]
    nullable: Option<Hidden>,
    #[visit(type_attributes(visit::wire))]
    #[visit(ts(skip), wire = "rpc_toolkit::ts::Unknown")]
    #[visit(opaque)]
    omitted: Hidden,
    last: String,
}
rpc_toolkit::reflect_ts!(Projection);
#[derive(visit_rs::VisitFields)]
struct Tuple(
    #[visit(type_attributes(visit::wire))]
    #[visit(opaque, ts(skip), wire = "rpc_toolkit::ts::Unknown")]
    Hidden,
    String,
);
rpc_toolkit::reflect_ts!(Tuple);
#[derive(visit_rs::VisitFields)]
#[visit(opaque)]
struct Newtype(#[visit(ts(skip), wire = "rpc_toolkit::ts::Unknown")] Hidden);
rpc_toolkit::reflect_ts!(Newtype);

#[test]
fn omission_and_optional_state_are_scoped_to_each_field() {
    for direction in [Direction::Input, Direction::Output] {
        let mut v = TSVisitor::new();
        v.with_direction(direction, |v| v.append_type::<Projection>());
        let output = v.into_declarations().unwrap();
        assert!(!output.contains("context"));
        assert!(!output.contains("omitted"));
        assert!(output.contains("\"nested\":(Nested"));
        assert!(output.contains("\"value\":(string)"));
        assert!(output.contains("\"required\":(number)"));
        assert!(output.contains("\"last\":(string)"));
        assert!(output.contains(if direction == Direction::Input {
            "\"nullable\"?:(number|null)"
        } else {
            "\"nullable\":(number|null)"
        }));
    }
}
fn rejects<T: TS>() {
    let mut v = TSVisitor::new();
    v.append_type::<T>();
    assert!(v
        .into_module("Rejected")
        .unwrap_err()
        .to_string()
        .contains("Omission requires a named field"));
}
#[test]
fn omission_rejects_tuple_and_newtype_fields() {
    rejects::<Tuple>();
    rejects::<Newtype>();
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
    #[visit(ts(type = "boolean"))]
    overridden: Nested,
    #[visit(ts(type = "number|null"))]
    option: Option<Nested>,
    required: String,
}
rpc_toolkit::reflect_ts!(LiteralFields);

#[derive(visit_rs::VisitFields)]
#[visit(ts(type = "number"))]
struct LiteralContainer(Nested);
rpc_toolkit::reflect_ts!(LiteralContainer);

#[test]
fn literals_do_not_register_replaced_definitions() {
    for direction in [Direction::Input, Direction::Output] {
        let mut v = TSVisitor::new();
        v.with_direction(direction, |v| v.append_type::<LiteralFields>());
        let module = v.into_declarations().unwrap();
        assert!(!module.contains("Nested"));
        assert!(module.contains("\"overridden\":(boolean)"));
        assert!(module.contains("\"required\":(string)"));
        assert!(module.contains(if direction == Direction::Input {
            "\"option\"?:(number|null)"
        } else {
            "\"option\":(number|null)"
        }));
        let mut v = TSVisitor::new();
        v.append_type::<LiteralContainer>();
        assert_eq!(
            v.into_declarations().unwrap(),
            "export type LiteralContainer = number;\n"
        );
    }
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
bad_hint!(Duplicate, visit(ts(type = "string", type = "number")));
bad_hint!(BadSkip, visit(ts(skip = true)));
bad_hint!(BadName, visit(ts(rename = "invalid-name")));
bad_hint!(BadNested, visit(ts(type("string"))));
bad_hint!(BadOuter, visit(ts(type = "string"), @ raw));
bad_hint!(UnknownOuter, visit(@ raw));

#[derive(visit_rs::VisitFields)]
struct BadOuterField {
    #[visit(ts(type = "string"), @ raw)]
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
    let mut v = TSVisitor::new();
    v.append_type::<ManualEmpty>();
    assert_eq!(
        v.into_module("EmptyRoot").unwrap(),
        "export type EmptyRoot = null;\n"
    );
    let mut v = TSVisitor::new();
    v.with_direction(Direction::Input, |v| v.append_type::<ManualNamed>());
    assert_eq!(
        v.into_declarations().unwrap(),
        "export type ManualParams = string;\n"
    );
}

fn rejects_hint<T: TS>(message: &str) {
    let mut v = TSVisitor::new();
    v.append_type::<T>();
    assert!(v
        .into_declarations()
        .unwrap_err()
        .to_string()
        .contains(message));
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
    let mut visitor = TSVisitor::new();
    visitor.append_type::<UnknownOuter>();
    assert_eq!(
        visitor.into_declarations().unwrap(),
        "export type UnknownOuter = string;\n"
    );
}

#[test]
fn precise_generic_bridges_preserve_docs_and_aliases() {
    let mut v = TSVisitor::new();
    v.with_direction(Direction::Input, |v| v.append_type::<Borrowed<'_>>());
    let output = v.into_declarations().unwrap();
    assert!(output.contains("Borrowed documentation.\n */\nexport type BorrowedParams = string"));
    let mut v = TSVisitor::new();
    v.append_type::<Array<String, 2>>();
    assert!(v
        .into_module("ArrayRoot")
        .unwrap()
        .contains("[string,string]"));
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
    for direction in [Direction::Input, Direction::Output] {
        let mut visitor = TSVisitor::new();
        visitor.with_direction(direction, |v| v.append_type::<ConstantIdentity>());
        let module = visitor.into_declarations().unwrap();
        assert!(module.contains(if direction == Direction::Input {
            "export type ExplicitInput = null;"
        } else {
            "export type ExplicitAlias = null;"
        }));
    }
}
