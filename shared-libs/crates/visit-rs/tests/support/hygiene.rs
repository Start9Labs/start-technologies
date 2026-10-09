use visit_rs::lib::futures::StreamExt;
use visit_rs::lib::futures::executor::block_on;
use visit_rs::{
    VisitVariantFieldsAsync, VisitVariantFieldsCoveredAsync, VisitVariantFieldsNamedAsync,
};

use super::*;

#[allow(non_camel_case_types)]
type __visit_rs_Visitor0 = u32;

#[derive(VisitFields)]
#[visit(target = "__visit_rs_Visitor0", type_attributes(visit::target))]
struct SelectedStruct {
    value: u32,
}

#[derive(VisitVariants)]
#[visit(target = "__visit_rs_Visitor0", type_attributes(visit::target))]
enum SelectedEnum {
    Value(u32),
}

#[test]
fn selected_only_aliases_keep_owner_resolution() {
    let mut visitor = Recorder::default();
    assert_eq!(
        SelectedStruct::visit_type_attributes(&mut visitor).collect::<Vec<_>>(),
        ["u32"]
    );
    assert_eq!(
        SelectedEnum::visit_type_attributes(&mut visitor).collect::<Vec<_>>(),
        ["u32"]
    );
}

impl Visit<ValueRecorder> for u32 {
    fn visit(&self, _: &mut ValueRecorder) -> usize {
        1
    }
}
impl visit_rs::VisitAsync<ValueRecorder> for u32 {
    fn visit_async<'a>(
        &'a self,
        _: &'a mut ValueRecorder,
    ) -> impl Future<Output = usize> + Send + 'a {
        std::future::ready(1)
    }
}

#[derive(VisitVariants)]
enum HelperNames {
    Stream { stream: u32 },
    Fresh { __visit_rs_stream0: u32 },
    Visitor { visitor: u32 },
    Locals { position: u32, i: u32 },
    Binding { __visit_rs_field_0_0: u32 },
}

#[allow(non_upper_case_globals)]
#[derive(VisitVariants)]
enum ConstTuple<const _tup_0: u32> {
    Value(#[visit(skip)] (), u32, u32),
    First(u32),
}

struct ExactValues;

impl Visitor for ExactValues {
    type Result = u32;
}

impl Visit<ExactValues> for u32 {
    fn visit(&self, _: &mut ExactValues) -> u32 {
        *self
    }
}

impl visit_rs::VisitAsync<ExactValues> for u32 {
    fn visit_async<'a>(&'a self, _: &'a mut ExactValues) -> impl Future<Output = u32> + Send + 'a {
        std::future::ready(*self)
    }
}

impl Visit<ExactValues> for visit_rs::Covered<'_, u32> {
    fn visit(&self, visitor: &mut ExactValues) -> u32 {
        self.0.visit(visitor)
    }
}

impl Visit<ExactValues> for Named<'_, u32> {
    fn visit(&self, visitor: &mut ExactValues) -> u32 {
        self.value.visit(visitor)
    }
}

impl visit_rs::VisitAsync<ExactValues> for visit_rs::Covered<'_, u32> {
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut ExactValues,
    ) -> impl Future<Output = u32> + Send + 'a {
        self.0.visit_async(visitor)
    }
}

impl visit_rs::VisitAsync<ExactValues> for Named<'_, u32> {
    fn visit_async<'a>(
        &'a self,
        visitor: &'a mut ExactValues,
    ) -> impl Future<Output = u32> + Send + 'a {
        self.value.visit_async(visitor)
    }
}

fn assert_exact_fields<T>(value: &T, expected: &[u32])
where
    T: visit_rs::VisitVariantFields<ExactValues>
        + visit_rs::VisitVariantFieldsCovered<ExactValues>
        + visit_rs::VisitVariantFieldsNamed<ExactValues>
        + VisitVariantFieldsAsync<ExactValues>
        + VisitVariantFieldsCoveredAsync<ExactValues>
        + VisitVariantFieldsNamedAsync<ExactValues>,
{
    let mut visitor = ExactValues;
    assert_eq!(
        value.visit_variant_fields(&mut visitor).collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        value
            .visit_variant_fields_covered(&mut visitor)
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        value
            .visit_variant_fields_named(&mut visitor)
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        block_on(
            value
                .visit_variant_fields_async(&mut visitor)
                .collect::<Vec<_>>()
        ),
        expected
    );
    assert_eq!(
        block_on(
            value
                .visit_variant_fields_covered_async(&mut visitor)
                .collect::<Vec<_>>()
        ),
        expected
    );
    assert_eq!(
        block_on(
            value
                .visit_variant_fields_named_async(&mut visitor)
                .collect::<Vec<_>>()
        ),
        expected
    );
}

#[test]
fn enum_visitor_field_keeps_exact_value() {
    assert_exact_fields(&HelperNames::Visitor { visitor: 17 }, &[17]);
}

#[test]
fn enum_iterator_local_fields_keep_exact_values() {
    assert_exact_fields(
        &HelperNames::Locals {
            position: 23,
            i: 31,
        },
        &[23, 31],
    );
    assert_exact_fields(
        &HelperNames::Binding {
            __visit_rs_field_0_0: 37,
        },
        &[37],
    );
}

#[test]
fn enum_const_tuple_keeps_original_indices() {
    assert_exact_fields(&ConstTuple::<41>::First(43), &[43]);
    assert_exact_fields(&ConstTuple::<41>::Value((), 47, 53), &[47, 53]);
}

#[test]
fn enum_fields_keep_their_bindings_in_async_helpers() {
    for value in [
        HelperNames::Stream { stream: 7 },
        HelperNames::Fresh {
            __visit_rs_stream0: 9,
        },
    ] {
        let mut visitor = ValueRecorder;
        assert_eq!(
            block_on(
                value
                    .visit_variant_fields_async(&mut visitor)
                    .collect::<Vec<_>>()
            ),
            [1]
        );
        assert_eq!(
            block_on(
                value
                    .visit_variant_fields_covered_async(&mut visitor)
                    .collect::<Vec<_>>()
            ),
            [1]
        );
        assert_eq!(
            block_on(
                value
                    .visit_variant_fields_named_async(&mut visitor)
                    .collect::<Vec<_>>()
            ),
            [1]
        );
    }
}
