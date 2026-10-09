#![cfg(feature = "ts")]
#![allow(dead_code)]

use rpc_toolkit::ts::{Direction, TSVisitor, TS};
use serde::Serialize;
use visit_rs::reflection::{Opaque, TypeInfo};
use visit_rs::{Named, Static, Visit, VisitFieldsStaticNamed, Visitor};

struct Names;
impl Visitor for Names {
    type Result = Option<&'static str>;
}
impl<T: ?Sized> Visit<Names> for Named<'_, Static<T>> {
    fn visit(&self, _: &mut Names) -> Option<&'static str> {
        self.name
    }
}
struct Unsupported(std::cell::Cell<u32>);
#[derive(visit_rs::VisitFields)]
#[visit(wire = "Unsupported", type_attributes(visit::wire))]
struct LegacySelected {
    scalar: u32,
    #[visit(opaque)]
    hidden: Unsupported,
}

#[test]
fn storage_support_is_independent_of_selected_metadata_support() {
    let callbacks: Vec<_> = LegacySelected::visit_fields_static_named(&mut Names).collect();
    assert_eq!(callbacks, [Some("scalar"), Some("hidden")]);
    assert_eq!(LegacySelected::DECLARATION.fields.len(), 2);
}

#[test]
fn opaque_callbacks_require_neither_ts_nor_sync() {
    let marker = Static::<Opaque<Unsupported>>::new();
    let named = Named {
        name: Some("hidden"),
        metadata: &[],
        value: &marker,
    };
    assert!(named
        .visit(&mut rpc_toolkit::ts::StorageCollector)
        .is_none());
}

#[derive(Serialize, visit_rs::VisitVariants)]
#[serde(
    tag = "kind",
    content = "value",
    bound(serialize = "T: Serialize, [u32; N]: Serialize")
)]
enum BorrowedTree<'a, T, const N: usize> {
    Borrowed(&'a T),
    Fixed([u32; N]),
    Branch(Box<BorrowedTree<'a, T, N>>),
}
rpc_toolkit::impl_ts_shape!(impl ['a, T, const N: usize] for BorrowedTree<'a, T, N> where [T: TS]);

#[test]
fn borrowed_generic_enum_recursion_uses_payload_bounds() {
    fn check<'a>(value: &'a str) {
        let tree = BorrowedTree::<_, 2>::Branch(Box::new(BorrowedTree::Borrowed(&value)));
        assert_eq!(
            serde_json::to_value(tree).unwrap(),
            serde_json::json!({
                "kind": "Branch", "value": {"kind": "Borrowed", "value": value}
            })
        );
        for direction in [Direction::Input, Direction::Output] {
            let mut visitor = TSVisitor::new();
            visitor.with_direction(direction, |visitor| {
                visitor.declare::<BorrowedTree<'a, &'a str, 2>>("BorrowedTree")
            });
            let declarations = visitor.into_declarations().unwrap();
            let root = if direction == Direction::Input {
                "BorrowedTreeInput"
            } else {
                "BorrowedTree"
            };
            assert!(
                declarations.contains(&format!("\"value\":({root})")),
                "{}",
                declarations
            );
            assert!(declarations.contains("[number,number]"), "{}", declarations);
            assert!(declarations.contains("\"Borrowed\""), "{}", declarations);
            assert_eq!(declarations.matches("export type ").count(), 1);
        }
    }
    let owned = String::from("local borrow");
    check(&owned);
}

#[derive(visit_rs::VisitVariants)]
enum OriginalVariantIndices {
    #[serde(skip)]
    Hidden(#[visit(opaque)] Unsupported),
    Tuple(
        #[visit(skip)]
        #[serde(skip)]
        Unsupported,
        u32,
    ),
    Named {
        #[visit(rename = "presentation")]
        original: String,
    },
}
rpc_toolkit::reflect_ts!(OriginalVariantIndices);

#[test]
fn enum_callbacks_follow_original_variant_and_filtered_field_coordinates() {
    let facts = OriginalVariantIndices::DECLARATION;
    assert_eq!(facts.variants[1].fields[0].visit_index, None);
    assert_eq!(facts.variants[1].fields[1].visit_index, Some(0));
    assert_eq!(facts.variants[1].fields[1].position.variant, Some(1));
    for direction in [Direction::Input, Direction::Output] {
        let mut visitor = TSVisitor::new();
        visitor.with_direction(direction, |visitor| {
            visitor.append_type::<OriginalVariantIndices>()
        });
        let module = visitor.into_declarations().unwrap();
        assert!(module.contains("number"), "{}", module);
        assert!(module.contains("\"original\":(string)"), "{}", module);
        assert!(!module.contains("presentation"), "{}", module);
        assert!(!module.contains("Hidden"), "{}", module);
    }
}
