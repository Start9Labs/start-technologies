#![cfg(feature = "meta")]
#![cfg(feature = "meta")]

use visit_rs::metadata::{AttributeMeta, MetaValue};
use visit_rs::{EnumInfo, Visit, VisitVariants};

#[derive(VisitVariants)]
#[visit(tag = "type", content = "data")]
enum Adjacent {
    A(u32),
    B { x: String },
}

fn find_list<'a>(metas: &'a [AttributeMeta], path: &str) -> Option<&'a [AttributeMeta]> {
    metas.iter().find_map(|m| match m {
        AttributeMeta::List { path: p, items } if *p == path => Some(*items),
        _ => None,
    })
}

fn find_str(metas: &[AttributeMeta], name: &str) -> Option<&'static str> {
    metas.iter().find_map(|m| match m {
        AttributeMeta::NameValue {
            name: n,
            value: MetaValue::Str(s),
            ..
        } if *n == name => Some(*s),
        _ => None,
    })
}

// Multi-item attributes (e.g. adjacently-tagged enums) must survive as structured
// metadata rather than collapsing to AttributeMeta::Unparsed.
#[test]
fn tag_and_content_survive_as_structured_metadata() {
    let meta = <Adjacent as EnumInfo>::DATA.metadata;
    assert!(
        !meta
            .iter()
            .any(|m| matches!(m, AttributeMeta::Unparsed { .. })),
        "multi-item attribute should not be Unparsed: {meta:?}"
    );
    let items = find_list(meta, "visit").expect("visit(...) list present");
    assert_eq!(find_str(items, "tag"), Some("type"));
    assert_eq!(find_str(items, "content"), Some("data"));
}

// A `rename` combined with another item in the same attribute list must still be honored.
#[derive(VisitVariants)]
enum Combined {
    #[visit(rename = "renamed", alias = "legacy")]
    Foo,
}

#[test]
fn rename_honored_in_multi_item_list() {
    assert_eq!(
        Combined::variants().into_iter().next().unwrap().name,
        "renamed"
    );
}
