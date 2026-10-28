use std::fmt::Write;
use std::future::Future;

use visit_rs::{
    EnumInfo, Named, Static, Variant, Visit, VisitAsync, VisitVariant, VisitVariantFields,
    VisitVariantFieldsAsync, VisitVariantFieldsCovered, VisitVariantFieldsCoveredAsync,
    VisitVariantFieldsNamed, VisitVariantFieldsNamedAsync, VisitVariantFieldsStatic,
    VisitVariantFieldsStaticAsync, VisitVariantFieldsStaticNamed,
    VisitVariantFieldsStaticNamedAsync, VisitVariants, VisitVariantsStatic, Visitor,
};

fn verify_traits<T, V>()
where
    V: Visitor,
    T: EnumInfo
        + VisitVariant<V>
        + VisitVariantsStatic<V>
        + VisitVariantFields<V>
        + VisitVariantFieldsAsync<V>
        + VisitVariantFieldsCovered<V>
        + VisitVariantFieldsCoveredAsync<V>
        + VisitVariantFieldsNamed<V>
        + VisitVariantFieldsNamedAsync<V>
        + VisitVariantFieldsStatic<V>
        + VisitVariantFieldsStaticAsync<V>
        + VisitVariantFieldsStaticNamed<V>
        + VisitVariantFieldsStaticNamedAsync<V>,
{
}

struct FmtVisitor(String);

impl Visitor for FmtVisitor {
    type Result = std::fmt::Result;
}

#[derive(VisitVariants)]
enum TestEnum {
    Unit,
    Single(String),
    Tuple(i32, f32),
    Struct { foo: String, bar: Vec<u8> },
}

impl Visit<FmtVisitor> for String {
    fn visit(&self, visitor: &mut FmtVisitor) -> <FmtVisitor as Visitor>::Result {
        write!(&mut visitor.0, "{:?}", self)
    }
}
impl Visit<FmtVisitor> for i32 {
    fn visit(&self, visitor: &mut FmtVisitor) -> <FmtVisitor as Visitor>::Result {
        write!(&mut visitor.0, "{:?}", self)
    }
}
impl Visit<FmtVisitor> for f32 {
    fn visit(&self, visitor: &mut FmtVisitor) -> <FmtVisitor as Visitor>::Result {
        write!(&mut visitor.0, "{:?}", self)
    }
}
impl Visit<FmtVisitor> for Vec<u8> {
    fn visit(&self, visitor: &mut FmtVisitor) -> <FmtVisitor as Visitor>::Result {
        visitor.0.push('[');
        for (i, item) in self.iter().enumerate() {
            if i > 0 {
                visitor.0.push(',');
            }
            write!(&mut visitor.0, "{}", item)?;
        }
        visitor.0.push(']');
        Ok(())
    }
}

impl VisitAsync<FmtVisitor> for String {
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut FmtVisitor,
    ) -> impl Future<Output = <FmtVisitor as Visitor>::Result> + Send + 'a {
        async move { self.visit(visitor) }
    }
}

impl VisitAsync<FmtVisitor> for i32 {
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut FmtVisitor,
    ) -> impl Future<Output = <FmtVisitor as Visitor>::Result> + Send + 'a {
        async move { self.visit(visitor) }
    }
}

impl VisitAsync<FmtVisitor> for f32 {
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut FmtVisitor,
    ) -> impl Future<Output = <FmtVisitor as Visitor>::Result> + Send + 'a {
        async move { self.visit(visitor) }
    }
}

impl VisitAsync<FmtVisitor> for Vec<u8> {
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut FmtVisitor,
    ) -> impl Future<Output = <FmtVisitor as Visitor>::Result> + Send + 'a {
        async move { self.visit(visitor) }
    }
}

impl Visit<FmtVisitor> for TestEnum {
    fn visit(&self, visitor: &mut FmtVisitor) -> <FmtVisitor as Visitor>::Result {
        self.visit_variant(visitor)
    }
}

impl<'a> Visit<FmtVisitor> for Variant<'a, TestEnum> {
    fn visit(&self, visitor: &mut FmtVisitor) -> <FmtVisitor as Visitor>::Result {
        visitor.0.push_str(self.info.name);
        if self.info.field_count == 0 {
            return Ok(());
        }
        if self.info.named_fields {
            visitor.0.push('{');
        } else {
            visitor.0.push('(');
        }
        self.value
            .visit_variant_fields_named(visitor)
            .collect::<std::fmt::Result>()?;
        // Remove trailing comma
        if visitor.0.ends_with(',') {
            visitor.0.pop();
        }
        if self.info.named_fields {
            visitor.0.push('}');
        } else {
            visitor.0.push(')');
        }

        Ok(())
    }
}

impl<'a, T> Visit<FmtVisitor> for Named<'a, T>
where
    T: Visit<FmtVisitor>,
{
    fn visit(&self, visitor: &mut FmtVisitor) -> <FmtVisitor as Visitor>::Result {
        if let Some(name) = self.name {
            write!(&mut visitor.0, "{name}:")?;
        }
        self.value.visit(visitor)?;
        visitor.0.push(',');

        Ok(())
    }
}

impl<'a, T> VisitAsync<FmtVisitor> for Named<'a, T>
where
    T: VisitAsync<FmtVisitor> + Sync,
{
    fn visit_async<'b>(
        &'b self,
        visitor: &'b mut FmtVisitor,
    ) -> impl Future<Output = <FmtVisitor as Visitor>::Result> + Send + 'b {
        async move {
            if let Some(name) = self.name {
                write!(&mut visitor.0, "{name}:")?;
            }
            VisitAsync::visit_async(self.value, visitor).await?;
            visitor.0.push(',');
            Ok(())
        }
    }
}

impl<'a, T> Visit<FmtVisitor> for visit_rs::Covered<'a, T>
where
    T: Visit<FmtVisitor> + ?Sized,
{
    fn visit(&self, visitor: &mut FmtVisitor) -> <FmtVisitor as Visitor>::Result {
        self.0.visit(visitor)
    }
}

impl<'a, T> VisitAsync<FmtVisitor> for visit_rs::Covered<'a, T>
where
    T: VisitAsync<FmtVisitor> + Sync + ?Sized,
{
    fn visit_async<'b>(
        &'b self,
        visitor: &'b mut FmtVisitor,
    ) -> impl Future<Output = <FmtVisitor as Visitor>::Result> + Send + 'b {
        async move { VisitAsync::visit_async(self.0, visitor).await }
    }
}

impl<T> VisitAsync<FmtVisitor> for Static<T>
where
    T: VisitAsync<FmtVisitor>,
{
    fn visit_async<'a>(
        &'a self,
        _visitor: &'a mut FmtVisitor,
    ) -> impl Future<Output = <FmtVisitor as Visitor>::Result> + Send + 'a {
        async move { Ok(()) }
    }
}

impl<T> Visit<FmtVisitor> for Static<T> {
    fn visit(&self, _visitor: &mut FmtVisitor) -> <FmtVisitor as Visitor>::Result {
        Ok(())
    }
}

impl<'a> Visit<FmtVisitor> for Variant<'a, Static<TestEnum>> {
    fn visit(&self, visitor: &mut FmtVisitor) -> <FmtVisitor as Visitor>::Result {
        visitor.0.push_str(self.info.name);
        Ok(())
    }
}

fn fmt<T: Visit<FmtVisitor>>(value: &T) -> String {
    let mut visitor = FmtVisitor(String::new());
    value.visit(&mut visitor).unwrap();
    visitor.0
}

#[test]
fn test_test_enum() {
    assert_eq!(&fmt(&TestEnum::Unit), "Unit");
    assert_eq!(&fmt(&TestEnum::Single("hi".into())), "Single(\"hi\")");
    assert_eq!(&fmt(&TestEnum::Tuple(1, 2.1)), "Tuple(1,2.1)");
    assert_eq!(
        &fmt(&TestEnum::Struct {
            foo: "hello".into(),
            bar: vec![1, 2, 3]
        }),
        "Struct{foo:\"hello\",bar:[1,2,3]}"
    )
}

struct TestVisitor;

impl Visitor for TestVisitor {
    type Result = String;
}

impl<'a> Visit<TestVisitor> for Variant<'a, TestEnum> {
    fn visit(&self, _visitor: &mut TestVisitor) -> String {
        format!("Variant({})", self.info.name)
    }
}

impl<'a> Visit<TestVisitor> for Variant<'a, Static<TestEnum>> {
    fn visit(&self, _visitor: &mut TestVisitor) -> String {
        format!("Variant(Static, {})", self.info.name)
    }
}

impl Visit<TestVisitor> for String {
    fn visit(&self, _visitor: &mut TestVisitor) -> String {
        format!("String(\"{}\")", self)
    }
}

impl Visit<TestVisitor> for i32 {
    fn visit(&self, _visitor: &mut TestVisitor) -> String {
        format!("i32({})", self)
    }
}

impl Visit<TestVisitor> for f32 {
    fn visit(&self, _visitor: &mut TestVisitor) -> String {
        format!("f32({})", self)
    }
}

impl Visit<TestVisitor> for Vec<u8> {
    fn visit(&self, _visitor: &mut TestVisitor) -> String {
        format!("Vec<u8>(len={})", self.len())
    }
}

impl Visit<TestVisitor> for Static<String> {
    fn visit(&self, _visitor: &mut TestVisitor) -> String {
        "Static<String>".to_string()
    }
}

impl Visit<TestVisitor> for Static<i32> {
    fn visit(&self, _visitor: &mut TestVisitor) -> String {
        "Static<i32>".to_string()
    }
}

impl Visit<TestVisitor> for Static<f32> {
    fn visit(&self, _visitor: &mut TestVisitor) -> String {
        "Static<f32>".to_string()
    }
}

impl Visit<TestVisitor> for Static<Vec<u8>> {
    fn visit(&self, _visitor: &mut TestVisitor) -> String {
        "Static<Vec<u8>>".to_string()
    }
}

#[test]
fn test_trait_bounds() {
    verify_traits::<TestEnum, FmtVisitor>();
}

#[test]
fn test_enum_info() {
    assert_eq!(TestEnum::DATA.name, "TestEnum");
    assert_eq!(TestEnum::DATA.variant_count, 4);

    let variants: Vec<_> = TestEnum::variants().into_iter().collect();
    assert_eq!(variants.len(), 4);
    assert_eq!(variants[0].name, "Unit");
    assert_eq!(variants[0].field_count, 0);
    assert_eq!(variants[1].name, "Single");
    assert_eq!(variants[1].field_count, 1);
    assert_eq!(variants[2].name, "Tuple");
    assert_eq!(variants[2].field_count, 2);
    assert_eq!(variants[3].name, "Struct");
    assert_eq!(variants[3].field_count, 2);
}

#[test]
fn test_visit_variant() {
    let mut visitor = TestVisitor;

    let unit = TestEnum::Unit;
    let result = unit.visit_variant(&mut visitor);
    assert_eq!(result, "Variant(Unit)");

    let single = TestEnum::Single("test".to_string());
    let result = single.visit_variant(&mut visitor);
    assert_eq!(result, "Variant(Single)");
}

#[test]
fn test_visit_variants_static() {
    let mut visitor = TestVisitor;
    let results: Vec<_> = TestEnum::visit_variants_static(&mut visitor).collect();

    assert_eq!(results.len(), 4);
    assert_eq!(results[0], "Variant(Static, Unit)");
    assert_eq!(results[1], "Variant(Static, Single)");
    assert_eq!(results[2], "Variant(Static, Tuple)");
    assert_eq!(results[3], "Variant(Static, Struct)");
}

#[test]
fn test_visit_variant_fields() {
    let mut visitor = TestVisitor;

    let unit = TestEnum::Unit;
    let fields: Vec<_> = unit.visit_variant_fields(&mut visitor).collect();
    assert_eq!(fields.len(), 0);

    let single = TestEnum::Single("hello".to_string());
    let fields: Vec<_> = single.visit_variant_fields(&mut visitor).collect();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0], "String(\"hello\")");

    let tuple = TestEnum::Tuple(42, 1.25);
    let fields: Vec<_> = tuple.visit_variant_fields(&mut visitor).collect();
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0], "i32(42)");
    assert_eq!(fields[1], "f32(1.25)");

    let struct_var = TestEnum::Struct {
        foo: "bar".to_string(),
        bar: vec![1, 2, 3],
    };
    let fields: Vec<_> = struct_var.visit_variant_fields(&mut visitor).collect();
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0], "String(\"bar\")");
    assert_eq!(fields[1], "Vec<u8>(len=3)");
}

#[test]
fn test_visit_variant_fields_static() {
    let mut visitor = TestVisitor;

    let unit_info = TestEnum::variant_info_by_name("Unit").unwrap();
    let fields: Vec<_> = TestEnum::visit_variant_fields_static(&unit_info, &mut visitor).collect();
    assert_eq!(fields.len(), 0);

    let single_info = TestEnum::variant_info_by_name("Single").unwrap();
    let fields: Vec<_> =
        TestEnum::visit_variant_fields_static(&single_info, &mut visitor).collect();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0], "Static<String>");

    let tuple_info = TestEnum::variant_info_by_name("Tuple").unwrap();
    let fields: Vec<_> = TestEnum::visit_variant_fields_static(&tuple_info, &mut visitor).collect();
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0], "Static<i32>");
    assert_eq!(fields[1], "Static<f32>");
}

// Generic / bounded enums must derive VisitVariants and impl every Visit* trait (for T: 'static).
#[derive(VisitVariants)]
enum GenEnum<T> {
    A(T),
    B { x: T },
    Unit,
}

#[derive(VisitVariants)]
#[visit(rename_all = "snake_case")]
enum Bounded<T: Clone>
where
    T: Send,
{
    FirstVariant(T),
    SecondVariant,
}

// The field-visiting traits (the ones that don't need a per-enum `Variant: Visit` impl) must
// exist for concrete instantiations of a generic enum. (`VisitVariant`/`VisitVariantsStatic`
// additionally need the user's `Variant<'_, _>: Visit<V>` impl, as for any enum.)
fn assert_field_traits<T, V>()
where
    V: Visitor,
    T: EnumInfo
        + VisitVariantFields<V>
        + VisitVariantFieldsAsync<V>
        + VisitVariantFieldsCovered<V>
        + VisitVariantFieldsCoveredAsync<V>
        + VisitVariantFieldsNamed<V>
        + VisitVariantFieldsNamedAsync<V>
        + VisitVariantFieldsStatic<V>
        + VisitVariantFieldsStaticAsync<V>
        + VisitVariantFieldsStaticNamed<V>
        + VisitVariantFieldsStaticNamedAsync<V>,
{
}

#[test]
fn test_generic_enum() {
    assert_field_traits::<GenEnum<String>, FmtVisitor>();
    assert_field_traits::<GenEnum<i32>, FmtVisitor>();
    assert_field_traits::<Bounded<String>, FmtVisitor>();

    assert_eq!(<GenEnum<String> as EnumInfo>::DATA.variant_count, 3);
    assert!(GenEnum::<String>::variant_info_by_name("A").is_some());

    // Runtime visit of a generic variant.
    let val = GenEnum::B {
        x: String::from("hi"),
    };
    let mut visitor = FmtVisitor(String::new());
    let _: Vec<_> = val.visit_variant_fields(&mut visitor).collect();
    assert!(visitor.0.contains("hi"));

    // where-clause + rename_all on a bounded generic enum.
    assert_eq!(
        Bounded::<i32>::variants()
            .into_iter()
            .map(|v| v.name)
            .collect::<Vec<_>>(),
        ["first_variant", "second_variant"]
    );
}
