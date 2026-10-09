#![cfg(feature = "ts")]
#![allow(dead_code)]

use rpc_toolkit::ts::{Direction, TSVisitor, TS};
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
        let mut visitor = TSVisitor::new();
        visitor.append_type::<Borrowed<'a>>();
        assert!(visitor
            .into_declarations()
            .unwrap()
            .contains("\"value\":(string)"));
        for direction in [Direction::Input, Direction::Output] {
            let mut visitor = TSVisitor::new();
            visitor.with_direction(direction, |v| v.append_type::<Generic<'a, &'a str, 2>>());
            let module = visitor.into_module("Fixture").unwrap();
            assert!(module.contains(if direction == Direction::Input {
                "\"private\":([string,string])"
            } else {
                "\"private\":(string)"
            }));
        }
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
    let declarations = visitor.into_declarations().unwrap();
    assert!(declarations.contains("\"next\":((StringNode|null))"));
    assert!(declarations.contains("\"next\":((NumberNode|null))"));
    assert_eq!(declarations.matches("export type ").count(), 2);
}
