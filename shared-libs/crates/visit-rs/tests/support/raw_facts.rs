use super::*;

#[derive(VisitFields)]
#[serde(
    rename = "NotRust",
    rename_all = "not a rename rule",
    tag = "tag",
    untagged
)]
#[visit(rename = "Presentation")]
struct Inventory {
    #[serde(
        skip,
        skip_serializing,
        skip_deserializing,
        rename(serialize = "out", deserialize = "in")
    )]
    original: u32,
    #[visit(skip, rename = "PresentationField")]
    still_present: u32,
    #[serde(flatten, default = "MissingFunction", with = "MissingModule")]
    another: u32,
}
impl Visit<Recorder> for visit_rs::Named<'_, u32> {
    fn visit(&self, _: &mut Recorder) -> &'static str {
        self.name.unwrap()
    }
}
#[test]
fn raw_inventory_and_named_callbacks_preserve_distinct_coordinates() {
    use visit_rs::VisitFieldsNamed;
    let info = Inventory::DECLARATION;
    assert_eq!(info.name, "Inventory");
    assert_eq!(info.kind, DeclarationKind::Struct(FieldsKind::Named));
    assert_eq!(
        info.fields.iter().map(|f| f.name).collect::<Vec<_>>(),
        [Some("original"), Some("still_present"), Some("another")]
    );
    assert_eq!(
        info.fields
            .iter()
            .map(|f| f.position.field)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(
        info.fields
            .iter()
            .map(|f| f.visit_index)
            .collect::<Vec<_>>(),
        [Some(0), None, Some(1)]
    );
    assert!(info.source.contains("MissingModule"));
    assert!(info.fields[0].attributes[0].contains("skip_deserializing"));
    assert_eq!(
        <Inventory as visit_rs::StructInfo>::DATA.name,
        "Presentation"
    );
    assert_eq!(<Inventory as visit_rs::StructInfo>::DATA.field_count, 2);
    let mut recorder = Recorder::default();
    assert_eq!(
        Inventory::visit_fields_static_named(&mut recorder).collect::<Vec<_>>(),
        ["u32", "u32"]
    );
    let value = Inventory {
        original: 1,
        still_present: 2,
        another: 3,
    };
    assert_eq!(
        value.visit_fields_named(&mut recorder).collect::<Vec<_>>(),
        ["original", "another"]
    );
}

#[derive(VisitFields)]
#[serde(from = "u32", into = "NotSelected")]
#[visit(opaque, type_attributes(serde::from))]
struct ArbitraryNamespace(Hidden);
#[test]
fn caller_selected_namespaces_have_no_builtin_policy() {
    let mut recorder = Recorder::default();
    assert_eq!(
        ArbitraryNamespace::visit_type_attributes(&mut recorder).collect::<Vec<_>>(),
        ["u32"]
    );
    assert_eq!(recorder.attribute_infos[0].path, ["serde", "from"]);
    assert!(ArbitraryNamespace::DECLARATION.attributes[0].contains("NotSelected"));
}

#[derive(VisitFields)]
struct Unit;
#[derive(VisitFields)]
struct EmptyTuple();
#[derive(VisitFields)]
struct EmptyNamed {}
#[derive(VisitVariants)]
enum EmptyEnum {}
#[derive(VisitVariants)]
#[serde(rename_all = "invalid")]
enum Variants {
    Unit,
    EmptyTuple(),
    EmptyNamed {},
    #[serde(skip, rename = "Other")]
    Tuple(#[serde(skip)] u32, #[visit(skip)] u32),
    Named {
        #[serde(skip)]
        rust_name: u32,
    },
}
#[test]
fn layouts_and_variant_positions_preserve_rust_syntax() {
    fn struct_category<T: TypeInfo<Kind = StructKind>>() {}
    fn enum_category<T: TypeInfo<Kind = EnumKind>>() {}
    struct_category::<Unit>();
    enum_category::<Variants>();
    assert_eq!(
        Unit::DECLARATION.kind,
        DeclarationKind::Struct(FieldsKind::Unit)
    );
    assert_eq!(
        EmptyTuple::DECLARATION.kind,
        DeclarationKind::Struct(FieldsKind::Tuple)
    );
    assert_eq!(
        EmptyNamed::DECLARATION.kind,
        DeclarationKind::Struct(FieldsKind::Named)
    );
    assert!(EmptyEnum::DECLARATION.variants.is_empty());
    let variants = Variants::DECLARATION.variants;
    assert_eq!(
        variants.iter().map(|v| v.fields_kind).collect::<Vec<_>>(),
        [
            FieldsKind::Unit,
            FieldsKind::Tuple,
            FieldsKind::Named,
            FieldsKind::Tuple,
            FieldsKind::Named
        ]
    );
    assert_eq!(variants[3].name, "Tuple");
    assert_eq!(variants[3].fields.len(), 2);
    assert_eq!(
        variants[3].fields[1].position,
        Position {
            variant: Some(3),
            field: 1
        }
    );
    assert_eq!(variants[3].fields[1].visit_index, None);
    assert_eq!(variants[3].fields[0].name, None);
    assert_eq!(variants[4].fields[0].name, Some("rust_name"));
    let mut recorder = Recorder::default();
    assert_eq!(
        Variants::variants()
            .into_iter()
            .map(
                |info| Variants::visit_variant_fields_static_named(&info, &mut recorder)
                    .collect::<Vec<_>>()
            )
            .collect::<Vec<_>>(),
        [vec![], vec![], vec![], vec!["u32"], vec!["u32"]]
    );
}

mod private_scope {
    use super::*;
    type PrivateAlias = u32;
    type __visit_rs_Visitor0 = u32;
    #[derive(VisitFields)]
    #[visit(target = "__visit_rs_Visitor0", type_attributes(visit::target))]
    pub struct AliasCollision;
    #[derive(VisitFields)]
    #[doc = r#"  doc with spaces
and newline  "#]
    #[visit(extra(nested(type = r#"PrivateAlias"#, type = "u32")), unknown = "Unresolvable<> !", number = 12_3u32, tokens(@ raw), type_attributes(visit::extra::nested::type))]
    #[serde(from = "UnselectedMissingType", nonsense = "accepted")]
    pub struct PrivateRoot {
        #[visit(
            extra(nested(type = "&'static str")),
            type_attributes(visit::extra::nested::type)
        )]
        value: u32,
    }
}
#[test]
fn raw_docs_literals_unknown_tokens_and_repeated_selected_coordinates() {
    let info = private_scope::PrivateRoot::DECLARATION;
    assert_eq!(info.docs, &["  doc with spaces\nand newline  "]);
    assert_eq!(
        info.metadata[0],
        visit_rs::metadata::AttributeMeta::NameValue {
            path: "doc",
            name: "doc",
            value: visit_rs::metadata::MetaValue::Str("  doc with spaces\nand newline  "),
        }
    );
    assert!(info.attributes[0].contains("r#\"  doc with spaces\nand newline  \"#"));
    assert!(info.source.contains("12_3u32"));
    assert!(info.source.contains("Unresolvable<> !"));
    assert!(info.source.contains("tokens"));
    assert!(info.source.contains("@ raw"));
    let mut recorder = Recorder::default();
    assert_eq!(
        private_scope::PrivateRoot::visit_type_attributes(&mut recorder).collect::<Vec<_>>(),
        ["u32", "u32", "str"]
    );
    assert_eq!(
        private_scope::AliasCollision::visit_type_attributes(&mut Recorder::default())
            .collect::<Vec<_>>(),
        ["u32"]
    );
    assert_eq!(recorder.attribute_infos.len(), 3);
    assert_eq!(
        recorder.attribute_infos[0].path,
        ["visit", "extra", "nested", "type"]
    );
    assert_eq!(
        recorder.attribute_infos[0].literal_tokens,
        "r#\"PrivateAlias\"#"
    );
    assert_eq!(recorder.attribute_infos[0].value, "PrivateAlias");
    assert_eq!(
        recorder.attribute_infos[0].position,
        MetadataPosition {
            variant: None,
            field: None,
            attribute: 1,
            occurrence: 3
        }
    );
    assert_eq!(recorder.attribute_infos[1].position.occurrence, 4);
    assert_eq!(recorder.attribute_infos[2].position.field, Some(0));
}

#[derive(VisitFields)]
#[visit(opaque, local(type = "u32"), type_attributes(visit::local::type))]
struct OpaqueContainer {
    hidden: Hidden,
    #[visit(local(type = "u32"))]
    other: Hidden,
}
#[derive(VisitVariants)]
enum OpaqueVariant {
    #[visit(opaque, selected(type = "u32"), type_attributes(visit::selected::type))]
    Hidden(Hidden, Hidden),
    Visible(u32, #[visit(opaque)] Hidden),
}
#[derive(VisitFields)]
struct OpaqueField {
    #[visit(opaque, target = "u32", type_attributes(visit::target))]
    hidden: Hidden,
}
#[test]
fn local_selectors_and_opacity_preserve_complete_inventory() {
    let mut recorder = Recorder::default();
    assert_eq!(
        OpaqueContainer::visit_fields_static_named(&mut recorder).collect::<Vec<_>>(),
        ["opaque", "opaque"]
    );
    assert_eq!(
        OpaqueContainer::visit_type_attributes(&mut recorder).collect::<Vec<_>>(),
        ["u32"]
    );
    assert_eq!(OpaqueContainer::DECLARATION.fields.len(), 2);
    assert!(OpaqueContainer::DECLARATION.fields[1].attributes[0].contains("local"));
    assert_eq!(
        OpaqueVariant::variants()
            .into_iter()
            .map(
                |info| OpaqueVariant::visit_variant_fields_static_named(&info, &mut recorder)
                    .collect::<Vec<_>>()
            )
            .collect::<Vec<_>>(),
        [vec!["opaque", "opaque"], vec!["u32", "opaque"]]
    );
    assert_eq!(
        OpaqueVariant::visit_type_attributes(&mut recorder).collect::<Vec<_>>(),
        ["u32"]
    );
    assert_eq!(
        OpaqueField::visit_fields_static_named(&mut recorder).collect::<Vec<_>>(),
        ["opaque"]
    );
    assert_eq!(
        OpaqueField::visit_type_attributes(&mut recorder).collect::<Vec<_>>(),
        ["u32"]
    );
    assert_eq!(recorder.attribute_infos.len(), 3);
}

#[derive(VisitFields)]
#[visit(target = "&'a [T; N]", type_attributes(visit::target))]
struct Scoped<'a, T: ?Sized, const N: usize>
where
    T: Sized,
{
    borrowed: &'a T,
    array: &'a [T; N],
}
#[derive(VisitFields)]
#[visit(target = "[u32; __visit_rs_visitor0]", type_attributes(visit::target))]
struct ConstCollision<const __visit_rs_visitor0: usize> {
    array: [u32; __visit_rs_visitor0],
}
#[test]
fn original_generic_bounds_and_const_identifier_scope() {
    fn borrowed<'a>(value: &'a u32) {
        let _value = Scoped::<u32, 2> {
            borrowed: value,
            array: &[1, 2],
        };
        let mut recorder = Recorder::default();
        assert_eq!(
            Scoped::<'a, u32, 2>::visit_fields_static_named(&mut recorder).collect::<Vec<_>>(),
            ["u32", "array"]
        );
        assert_eq!(
            Scoped::<'a, u32, 2>::visit_type_attributes(&mut recorder).collect::<Vec<_>>(),
            ["array"]
        );
    }
    let value = 42;
    borrowed(&value);
    let info = Scoped::<u32, 2>::DECLARATION;
    assert_eq!(
        info.parameters,
        [
            GenericParameter::Lifetime { name: "'a" },
            GenericParameter::Type { name: "T" },
            GenericParameter::Const {
                name: "N",
                ty: "usize"
            }
        ]
    );
    assert!(info.source.contains("T : Sized"));
    assert_eq!(RootOpaque::<Hidden>::DECLARATION.fields[0].type_syntax, "T");
    assert_eq!(Recursive::<Hidden>::DECLARATION.fields.len(), 2);
    let mut recorder = Recorder::default();
    assert_eq!(
        ConstCollision::<2>::visit_type_attributes(&mut recorder).collect::<Vec<_>>(),
        ["array"]
    );
    assert_eq!(
        ConstCollision::<2>::visit_fields_static_named(&mut recorder).collect::<Vec<_>>(),
        ["array"]
    );
}

struct StorageOnlyRecorder;
impl Visitor for StorageOnlyRecorder {
    type Result = usize;
}
impl Visit<StorageOnlyRecorder> for Static<u32> {
    fn visit(&self, _: &mut StorageOnlyRecorder) -> usize {
        1
    }
}
impl Visit<StorageOnlyRecorder> for Named<'_, Static<u32>> {
    fn visit(&self, visitor: &mut StorageOnlyRecorder) -> usize {
        self.value.visit(visitor)
    }
}
#[derive(VisitFields)]
#[visit(target = "Hidden", type_attributes(visit::target))]
struct SelectedWithoutCallback {
    value: u32,
}
#[test]
fn selected_metadata_does_not_constrain_storage_capabilities() {
    assert_eq!(
        SelectedWithoutCallback::visit_fields_static_named(&mut StorageOnlyRecorder)
            .collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(SelectedWithoutCallback::DECLARATION.fields.len(), 1);
}

#[derive(VisitFields)]
#[visit(target = "str", type_attributes(visit::target))]
struct UnsizedTail {
    count: u32,
    tail: str,
}
#[test]
fn unsized_markers_compile_without_values() {
    let mut recorder = Recorder::default();
    assert_eq!(
        UnsizedTail::visit_fields_static_named(&mut recorder).collect::<Vec<_>>(),
        ["u32", "str"]
    );
    assert_eq!(
        UnsizedTail::visit_type_attributes(&mut recorder).collect::<Vec<_>>(),
        ["str"]
    );
}
