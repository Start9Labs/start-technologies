#![cfg(test)]
#![allow(dead_code, non_camel_case_types, non_upper_case_globals)]
use visit_rs::reflection::{
    DeclarationKind, EnumKind, FieldsKind, GenericParameter, MetadataPosition, Position,
    StructKind, TypeAttributeInfo,
};
use visit_rs::{
    EnumInfo, Named, Opaque, Static, TypeAttribute, TypeInfo, Visit, VisitFields,
    VisitFieldsStaticNamed, VisitTypeAttributes, VisitVariantFieldsStaticNamed, VisitVariants,
    Visitor,
};

type TypeFunction = fn() -> &'static str;
#[derive(Default)]
struct Recorder {
    optional: Vec<bool>,
    types: Vec<TypeFunction>,
    selected: Vec<(MetadataPosition, &'static str)>,
    attribute_infos: Vec<TypeAttributeInfo>,
}
impl Visitor for Recorder {
    type Result = &'static str;
}
trait Fact {
    fn name() -> &'static str;
    fn optional() -> bool {
        false
    }
}
impl Fact for u32 {
    fn name() -> &'static str {
        "u32"
    }
}
impl Fact for str {
    fn name() -> &'static str {
        "str"
    }
}
impl<T: Fact + ?Sized> Fact for &T {
    fn name() -> &'static str {
        T::name()
    }
}
impl<T: Fact, const N: usize> Fact for [T; N] {
    fn name() -> &'static str {
        "array"
    }
}
impl<T: Fact + ?Sized> Visit<Recorder> for Static<T> {
    fn visit(&self, v: &mut Recorder) -> &'static str {
        v.types.push(T::name);
        v.optional.push(T::optional());
        T::name()
    }
}
impl<T: ?Sized> Visit<Recorder> for Static<Opaque<T>> {
    fn visit(&self, _: &mut Recorder) -> &'static str {
        "opaque"
    }
}
impl<T: ?Sized> Visit<Recorder> for Named<'_, Static<T>>
where
    Static<T>: Visit<Recorder>,
{
    fn visit(&self, v: &mut Recorder) -> &'static str {
        self.value.visit(v)
    }
}
impl<T: ?Sized> Visit<Recorder> for TypeAttribute<T>
where
    Static<T>: Visit<Recorder>,
{
    fn visit(&self, v: &mut Recorder) -> &'static str {
        v.selected.push((self.info.position, self.info.value));
        v.attribute_infos.push(self.info);
        self.marker.visit(v)
    }
}
struct Hidden;
type Local = u32;
/// Public facts documentation.
#[derive(VisitFields)]
#[visit(label = "literal", target = "Local", type_attributes(visit::target))]
struct Sample<'a, T, const N: usize> {
    #[serde(skip)]
    regular: T,
    borrowed: &'a str,
    array: [u32; N],
    #[visit(opaque, target = "Local", type_attributes(visit::target))]
    hidden: Hidden,
    #[visit(skip)]
    omitted: Hidden,
}
#[derive(VisitVariants)]
enum Layouts<'a, T, const N: usize> {
    Unit,
    EmptyTuple(),
    EmptyNamed {},
    Tuple(T, &'a str),
    Named {
        array: [u32; N],
        #[visit(opaque)]
        hidden: Hidden,
    },
    #[visit(opaque, target = "Local", type_attributes(visit::target))]
    Opaque(Hidden),
}
#[derive(VisitFields)]
#[visit(opaque)]
struct RootOpaque<T> {
    value: T,
}
#[derive(VisitFields)]
struct Recursive<T> {
    next: Option<Box<Recursive<T>>>,
    value: T,
}
impl<T: Fact> Fact for Recursive<T> {
    fn name() -> &'static str {
        "recursive"
    }
}
impl<T: Fact> Fact for Option<Box<Recursive<T>>> {
    fn name() -> &'static str {
        "lazy recursive"
    }
}
#[derive(VisitVariants)]
enum RecursiveEnum<T> {
    Next(Box<RecursiveEnum<T>>),
    Value(T),
}
impl<T: Fact> Fact for Box<RecursiveEnum<T>> {
    fn name() -> &'static str {
        "lazy enum"
    }
}

#[test]
fn ordinary_callbacks_and_raw_facts() {
    fn borrowed<'a>(_: &'a str) {
        let mut v = Recorder::default();
        let results: Vec<_> = Sample::<'a, u32, 2>::visit_fields_static_named(&mut v).collect();
        assert_eq!(results, ["u32", "str", "array", "opaque"]);
        assert_eq!(
            v.types.iter().map(|f| f()).collect::<Vec<_>>(),
            ["u32", "str", "array"]
        );
        let metadata: Vec<_> = Sample::<'a, u32, 2>::visit_type_attributes(&mut v).collect();
        assert_eq!(metadata, ["u32", "u32"]);
        assert_eq!(v.selected[0].0.field, None);
        assert_eq!(v.selected[1].0.field, Some(3));
        let facts = Sample::<'a, u32, 2>::DECLARATION;
        assert_eq!(facts.fields.len(), 5);
        assert_eq!(facts.fields[3].name, Some("hidden"));
        assert_eq!(facts.docs, [" Public facts documentation."]);
        assert!(facts.source.contains("literal"));
        assert_eq!(facts.parameters.len(), 3);
    }
    let text = String::new();
    borrowed(&text);
}
#[test]
fn enum_layouts_and_opacity() {
    fn borrowed<'a>(_: &'a str) {
        let mut v = Recorder::default();
        let facts = Layouts::<'a, u32, 3>::DECLARATION;
        assert_eq!(facts.kind, DeclarationKind::Enum);
        assert_eq!(
            facts.variants[..3]
                .iter()
                .map(|f| f.fields_kind)
                .collect::<Vec<_>>(),
            [FieldsKind::Unit, FieldsKind::Tuple, FieldsKind::Named]
        );
        let result: Vec<_> = Layouts::<'a, u32, 3>::variants()
            .into_iter()
            .map(|info| {
                Layouts::<'a, u32, 3>::visit_variant_fields_static_named(&info, &mut v)
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(
            result,
            [
                vec![],
                vec![],
                vec![],
                vec!["u32", "str"],
                vec!["array", "opaque"],
                vec!["opaque"]
            ]
        );
        assert_eq!(
            Layouts::<'a, u32, 3>::visit_type_attributes(&mut v).collect::<Vec<_>>(),
            ["u32"]
        );
        assert_eq!(v.selected[0].0.variant, Some(5));
    }
    let text = String::new();
    borrowed(&text);
    assert_eq!(
        RootOpaque::<Hidden>::visit_fields_static_named(&mut Recorder::default())
            .collect::<Vec<_>>(),
        ["opaque"]
    );
}
#[test]
fn recursive_payload_bounds() {
    assert_eq!(
        Recursive::<u32>::visit_fields_static_named(&mut Recorder::default()).collect::<Vec<_>>(),
        ["lazy recursive", "u32"]
    );
    let info = RecursiveEnum::<u32>::variants().into_iter().next().unwrap();
    assert_eq!(
        RecursiveEnum::<u32>::visit_variant_fields_static_named(&info, &mut Recorder::default())
            .collect::<Vec<_>>(),
        ["lazy enum"]
    );
}

#[allow(non_camel_case_types)]
#[derive(VisitFields)]
struct Hygiene<__visit_rs_Visitor0, __visit_rs__V> {
    first: __visit_rs_Visitor0,
    second: __visit_rs__V,
}
#[allow(non_camel_case_types)]
#[derive(VisitVariants)]
enum EnumHygiene<__visit_rs_Visitor0> {
    Value(__visit_rs_Visitor0),
}

struct Manual;
impl visit_rs::StructInfo for Manual {
    const DATA: visit_rs::StructInfoData = visit_rs::StructInfoData {
        name: "Manual",
        named_fields: false,
        field_count: 0,
        #[cfg(feature = "meta")]
        metadata: &[],
    };
}
impl VisitFieldsStaticNamed<Recorder> for Manual {
    fn visit_fields_static_named<'a>(
        _: &'a mut Recorder,
    ) -> impl Iterator<Item = &'static str> + 'a {
        std::iter::empty()
    }
}
#[test]
fn fresh_identifiers_and_manual_trait() {
    let mut recorder = Recorder::default();
    assert_eq!(
        Hygiene::<u32, u32>::visit_fields_static_named(&mut recorder).count(),
        2
    );
    assert_eq!(Manual::visit_fields_static_named(&mut recorder).count(), 0);
    let info = EnumHygiene::<u32>::variants().into_iter().next().unwrap();
    assert_eq!(
        EnumHygiene::<u32>::visit_variant_fields_static_named(&info, &mut recorder).count(),
        1
    );
    let facts = Sample::<'_, u32, 2>::DECLARATION;
    assert_eq!(
        facts
            .fields
            .iter()
            .map(|f| f.visit_index)
            .collect::<Vec<_>>(),
        [Some(0), Some(1), Some(2), Some(3), None]
    );
}

impl<T: ?Sized> visit_rs::VisitAsync<Recorder> for Static<T>
where
    Self: Visit<Recorder>,
{
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut Recorder,
    ) -> impl Future<Output = &'static str> + Send + 'a {
        async move { self.visit(visitor) }
    }
}
impl<T: ?Sized> visit_rs::VisitAsync<Recorder> for Named<'_, Static<T>>
where
    Static<T>: Visit<Recorder>,
{
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut Recorder,
    ) -> impl Future<Output = &'static str> + Send + 'a {
        async move { self.value.visit(visitor) }
    }
}
#[test]
fn borrowed_static_async_callbacks() {
    use visit_rs::lib::futures::StreamExt;
    use visit_rs::lib::futures::executor::block_on;
    use visit_rs::{VisitFieldsStaticNamedAsync, VisitVariantFieldsStaticNamedAsync};
    fn borrowed<'a>(_: &'a str) {
        let mut recorder = Recorder::default();
        let results = block_on(
            Sample::<'a, u32, 2>::visit_fields_static_named_async(&mut recorder)
                .collect::<Vec<_>>(),
        );
        assert_eq!(results, ["u32", "str", "array", "opaque"]);
        let info = Layouts::<'a, u32, 2>::variants()
            .into_iter()
            .nth(3)
            .unwrap();
        let results = block_on(
            Layouts::<'a, u32, 2>::visit_variant_fields_static_named_async(&info, &mut recorder)
                .collect::<Vec<_>>(),
        );
        assert_eq!(results, ["u32", "str"]);
    }
    let text = String::new();
    borrowed(&text);
}

impl Fact for Option<u32> {
    fn name() -> &'static str {
        "optional"
    }
    fn optional() -> bool {
        true
    }
}
type OptionalAlias = Option<u32>;
#[derive(VisitFields)]
struct LiteralReplaced {
    #[visit(
        opaque,
        constant = "null",
        details(hint = "OptionalAlias"),
        type_attributes(visit::details::hint)
    )]
    storage: Hidden,
}
impl<T: EnumInfo> Visit<Recorder> for visit_rs::Variant<'_, Static<T>> {
    fn visit(&self, _: &mut Recorder) -> &'static str {
        self.info.name
    }
}
#[test]
fn typed_literal_metadata_and_borrowed_variants() {
    let mut recorder = Recorder::default();
    assert_eq!(
        LiteralReplaced::visit_fields_static_named(&mut recorder).collect::<Vec<_>>(),
        ["opaque"]
    );
    assert_eq!(
        LiteralReplaced::visit_type_attributes(&mut recorder).collect::<Vec<_>>(),
        ["optional"]
    );
    assert_eq!(recorder.optional, [true]);
    assert_eq!(recorder.selected[0].0.field, Some(0));
    fn borrowed<'a>(_: &'a str) {
        use visit_rs::VisitVariantsStatic;
        let mut recorder = Recorder::default();
        assert_eq!(
            Layouts::<'a, u32, 2>::visit_variants_static(&mut recorder).count(),
            6
        );
    }
    let text = String::new();
    borrowed(&text);
}

#[derive(VisitFields)]
#[visit(
    opaque,
    shape = "[T; N]",
    borrowed = "&'a str",
    type_attributes(visit::shape, visit::borrowed)
)]
struct GenericSelected<'a, T, const N: usize> {
    hidden: Hidden,
    borrowed: &'a T,
}
#[test]
fn compiler_resolves_selected_generics() {
    fn borrowed<'a>(_: &'a str) {
        let mut recorder = Recorder::default();
        assert_eq!(
            GenericSelected::<'a, u32, 4>::visit_type_attributes(&mut recorder).collect::<Vec<_>>(),
            ["array", "str"]
        );
    }
    let text = String::new();
    borrowed(&text);
}

#[derive(Default)]
struct ValueRecorder;
impl Visitor for ValueRecorder {
    type Result = usize;
}
impl<T: ?Sized> Visit<ValueRecorder> for Opaque<T> {
    fn visit(&self, _: &mut ValueRecorder) -> usize {
        1
    }
}
impl<T: ?Sized + Visit<ValueRecorder>> Visit<ValueRecorder> for Named<'_, T> {
    fn visit(&self, visitor: &mut ValueRecorder) -> usize {
        self.value.visit(visitor)
    }
}
impl<T: ?Sized + Visit<ValueRecorder>> Visit<ValueRecorder> for visit_rs::Covered<'_, T> {
    fn visit(&self, visitor: &mut ValueRecorder) -> usize {
        self.0.visit(visitor)
    }
}
impl<T: ?Sized> visit_rs::VisitAsync<ValueRecorder> for Opaque<T> {
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut ValueRecorder,
    ) -> impl Future<Output = usize> + Send + 'a {
        async move { self.visit(visitor) }
    }
}
impl<T: ?Sized + Sync + Visit<ValueRecorder>> visit_rs::VisitAsync<ValueRecorder> for Named<'_, T> {
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut ValueRecorder,
    ) -> impl Future<Output = usize> + Send + 'a {
        async move { self.visit(visitor) }
    }
}
impl<T: ?Sized + Sync + Visit<ValueRecorder>> visit_rs::VisitAsync<ValueRecorder>
    for visit_rs::Covered<'_, T>
{
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut ValueRecorder,
    ) -> impl Future<Output = usize> + Send + 'a {
        async move { self.visit(visitor) }
    }
}
struct HiddenNoSync(std::cell::Cell<u32>);
#[derive(VisitFields)]
#[visit(opaque)]
struct OpaqueValue<'a> {
    hidden: &'a HiddenNoSync,
}
#[derive(VisitVariants)]
enum OpaqueEnumValue<'a> {
    #[visit(opaque)]
    Tuple(&'a HiddenNoSync),
    Named {
        #[visit(opaque)]
        hidden: &'a HiddenNoSync,
    },
}
#[test]
fn opaque_value_callbacks_without_storage_sync() {
    use ::visit_rs::lib::futures::StreamExt;
    use ::visit_rs::lib::futures::executor::block_on;
    use ::visit_rs::*;
    let hidden = HiddenNoSync(std::cell::Cell::new(0));
    let root = OpaqueValue { hidden: &hidden };
    let mut visitor = ValueRecorder;
    assert_eq!(root.visit_fields(&mut visitor).collect::<Vec<_>>(), [1]);
    assert_eq!(
        root.visit_fields_named(&mut visitor).collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(
        root.visit_fields_covered(&mut visitor).collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(
        block_on(root.visit_fields_async(&mut visitor).collect::<Vec<_>>()),
        [1]
    );
    assert_eq!(
        block_on(
            root.visit_fields_named_async(&mut visitor)
                .collect::<Vec<_>>()
        ),
        [1]
    );
    assert_eq!(
        block_on(
            root.visit_fields_covered_async(&mut visitor)
                .collect::<Vec<_>>()
        ),
        [1]
    );
    for root in [
        OpaqueEnumValue::Tuple(&hidden),
        OpaqueEnumValue::Named { hidden: &hidden },
    ] {
        assert_eq!(
            root.visit_variant_fields(&mut visitor).collect::<Vec<_>>(),
            [1]
        );
        assert_eq!(
            root.visit_variant_fields_named(&mut visitor)
                .collect::<Vec<_>>(),
            [1]
        );
        assert_eq!(
            root.visit_variant_fields_covered(&mut visitor)
                .collect::<Vec<_>>(),
            [1]
        );
        assert_eq!(
            block_on(
                root.visit_variant_fields_async(&mut visitor)
                    .collect::<Vec<_>>()
            ),
            [1]
        );
        assert_eq!(
            block_on(
                root.visit_variant_fields_named_async(&mut visitor)
                    .collect::<Vec<_>>()
            ),
            [1]
        );
        assert_eq!(
            block_on(
                root.visit_variant_fields_covered_async(&mut visitor)
                    .collect::<Vec<_>>()
            ),
            [1]
        );
    }
}
struct BorrowWrapper<T>(T);
#[derive(VisitFields)]
#[visit(target = "&'a u32", type_attributes(visit::target))]
struct NestedBorrowed<'a> {
    tuple: (u32, &'a u32),
    option: Option<&'a u32>,
    cow: std::borrow::Cow<'a, str>,
    wrapper: BorrowWrapper<&'a u32>,
}
#[derive(VisitVariants)]
enum NestedBorrowedEnum<'a> {
    Tuple((u32, &'a u32), Option<&'a u32>),
    Named {
        cow: std::borrow::Cow<'a, str>,
        wrapper: BorrowWrapper<&'a u32>,
    },
}

impl Visit<ValueRecorder> for (u32, &u32) {
    fn visit(&self, _: &mut ValueRecorder) -> usize {
        1
    }
}
impl visit_rs::VisitAsync<ValueRecorder> for (u32, &u32) {
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut ValueRecorder,
    ) -> impl Future<Output = usize> + Send + 'a {
        async move { self.visit(visitor) }
    }
}

impl Visit<ValueRecorder> for Option<&u32> {
    fn visit(&self, _: &mut ValueRecorder) -> usize {
        1
    }
}
impl visit_rs::VisitAsync<ValueRecorder> for Option<&u32> {
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut ValueRecorder,
    ) -> impl Future<Output = usize> + Send + 'a {
        async move { self.visit(visitor) }
    }
}

impl Visit<ValueRecorder> for std::borrow::Cow<'_, str> {
    fn visit(&self, _: &mut ValueRecorder) -> usize {
        1
    }
}
impl visit_rs::VisitAsync<ValueRecorder> for std::borrow::Cow<'_, str> {
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut ValueRecorder,
    ) -> impl Future<Output = usize> + Send + 'a {
        async move { self.visit(visitor) }
    }
}

impl Visit<ValueRecorder> for BorrowWrapper<&u32> {
    fn visit(&self, _: &mut ValueRecorder) -> usize {
        1
    }
}
impl visit_rs::VisitAsync<ValueRecorder> for BorrowWrapper<&u32> {
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut ValueRecorder,
    ) -> impl Future<Output = usize> + Send + 'a {
        async move { self.visit(visitor) }
    }
}

#[test]
fn nested_borrowed_value_callbacks() {
    use ::visit_rs::lib::futures::StreamExt;
    use ::visit_rs::lib::futures::executor::block_on;
    use ::visit_rs::*;
    let value = 4;
    let text = String::from("borrowed");
    let root = NestedBorrowed {
        tuple: (1, &value),
        option: Some(&value),
        cow: std::borrow::Cow::Borrowed(&text),
        wrapper: BorrowWrapper(&value),
    };
    let mut visitor = ValueRecorder;
    assert_eq!(root.visit_fields(&mut visitor).count(), 4);
    assert_eq!(root.visit_fields_named(&mut visitor).count(), 4);
    assert_eq!(root.visit_fields_covered(&mut visitor).count(), 4);
    assert_eq!(
        block_on(root.visit_fields_async(&mut visitor).collect::<Vec<_>>()),
        [1; 4]
    );
    assert_eq!(
        block_on(
            root.visit_fields_named_async(&mut visitor)
                .collect::<Vec<_>>()
        ),
        [1; 4]
    );
    assert_eq!(
        block_on(
            root.visit_fields_covered_async(&mut visitor)
                .collect::<Vec<_>>()
        ),
        [1; 4]
    );
    assert_eq!(
        NestedBorrowed::visit_type_attributes(&mut Recorder::default()).collect::<Vec<_>>(),
        ["u32"]
    );
    for root in [
        NestedBorrowedEnum::Tuple((1, &value), Some(&value)),
        NestedBorrowedEnum::Named {
            cow: std::borrow::Cow::Borrowed(&text),
            wrapper: BorrowWrapper(&value),
        },
    ] {
        assert_eq!(root.visit_variant_fields(&mut visitor).count(), 2);
        assert_eq!(root.visit_variant_fields_named(&mut visitor).count(), 2);
        assert_eq!(root.visit_variant_fields_covered(&mut visitor).count(), 2);
        assert_eq!(
            block_on(
                root.visit_variant_fields_async(&mut visitor)
                    .collect::<Vec<_>>()
            ),
            [1; 2]
        );
        assert_eq!(
            block_on(
                root.visit_variant_fields_named_async(&mut visitor)
                    .collect::<Vec<_>>()
            ),
            [1; 2]
        );
        assert_eq!(
            block_on(
                root.visit_variant_fields_covered_async(&mut visitor)
                    .collect::<Vec<_>>()
            ),
            [1; 2]
        );
    }
}

#[path = "support/hygiene.rs"]
mod hygiene;

#[path = "support/raw_facts.rs"]
mod raw_facts;

#[path = "support/non_sync_fixture.rs"]
mod non_sync_fixture;
#[test]
fn non_sync_ordinary_fields_remain_visitable() {
    non_sync_fixture::check();
}
