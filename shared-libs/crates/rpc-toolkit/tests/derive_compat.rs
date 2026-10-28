#![cfg(feature = "ts")]
#![allow(dead_code)]

mod common;

use common::{declarations, SCHEMAS};
use rpc_toolkit::ts::{TSVisitor, TS};
use rpc_toolkit::{impl_ts_shape, reflect_ts};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
struct Borrowed<'a> {
    value: &'a str,
}
reflect_ts!(impl ['a] for Borrowed<'a>);
#[derive(visit_rs::VisitFields)]
struct Generic<'a, T, const N: usize> {
    payload: T,
    fixed: [u32; N],
    #[visit(
        opaque,
        input_wire = "&'a [T; N]",
        output_wire = "&'a T",
        type_attributes(visit::input_wire, visit::output_wire)
    )]
    private: Unvisited<'a, T>,
}
struct Unvisited<'a, T>(&'a T);
reflect_ts!(impl ['a, T, const N: usize] for Generic<'a, T, N> where [T: TS]);

#[test]
fn borrowed_payloads_and_const_metadata_targets_keep_the_owner_context() {
    fn borrowed<'a>(value: &'a str) {
        let fixture = Borrowed { value };
        assert_eq!(
            serde_json::to_value(&fixture).unwrap(),
            serde_json::json!({"value":value})
        );
        assert!(declarations::<Borrowed<'a>>()
            .schemas
            .contains("\"value\": z.string()"));
        let bindings = common::module::<Generic<'a, &'a str, 2>>();
        assert!(bindings.schemas.contains(
            "wire(object({ \"payload\": z.string(), \"fixed\": z.tuple([z.int(), z.int()]), \"private\": z.tuple([z.string(), z.string()]) }), object({ \"payload\": z.string(), \"fixed\": z.tuple([z.int(), z.int()]), \"private\": z.string() }))"
        ), "{}", bindings.schemas);
    }
    let owned = String::from("borrowed locally");
    borrowed(&owned);
}

#[derive(visit_rs::VisitFields)]
struct Recursive<T> {
    payload: T,
    next: Option<Box<Recursive<T>>>,
}
impl_ts_shape!(impl [T] for Recursive<T> where [T: TS]);
#[test]
fn concrete_generic_reservations_are_pending_before_recursive_children() {
    let mut visitor = TSVisitor::new();
    visitor.declare::<Recursive<String>>("StringNode");
    visitor.declare::<Recursive<u32>>("NumberNode");
    let bindings = visitor.into_bindings(None, SCHEMAS).unwrap();
    assert!(bindings.schemas.contains("get \"next\"(): z.ZodCodec<z.ZodOptional<z.ZodNullable<typeof StringNode>>, z.ZodNullable<typeof StringNode>> { return wire(StringNode.nullable().optional(), StringNode.nullable()); }"));
    assert!(bindings.schemas.contains("get \"next\"(): z.ZodCodec<z.ZodOptional<z.ZodNullable<typeof NumberNode>>, z.ZodNullable<typeof NumberNode>> { return wire(NumberNode.nullable().optional(), NumberNode.nullable()); }"));
    assert_eq!(bindings.schemas.matches("export const ").count(), 2);
    common::typecheck(
        &bindings,
        "const node: StringNode = { payload: 'a', next: { payload: 'b', next: null } };\nconst input: NumberNodeInput = { payload: 1 };\n// @ts-expect-error\nconst wrong: NumberNode = { payload: 'a', next: null };",
    );
}
