use futures::StreamExt;
use visit_rs::*;

struct Numbers;
impl Visitor for Numbers {
    type Result = u32;
}
impl Visit<Numbers> for u32 {
    fn visit(&self, _: &mut Numbers) -> u32 {
        *self
    }
}
impl VisitAsync<Numbers> for u32 {
    async fn visit_async(&self, v: &mut Numbers) -> u32 {
        self.visit(v)
    }
}
impl Visit<Numbers> for Static<u32> {
    fn visit(&self, _: &mut Numbers) -> u32 {
        1
    }
}
impl VisitAsync<Numbers> for Static<u32> {
    async fn visit_async(&self, v: &mut Numbers) -> u32 {
        self.visit(v)
    }
}
impl<T: Visit<Numbers> + ?Sized> Visit<Numbers> for Named<'_, T> {
    fn visit(&self, v: &mut Numbers) -> u32 {
        self.value.visit(v)
    }
}
impl<T: VisitAsync<Numbers> + Sync + ?Sized> VisitAsync<Numbers> for Named<'_, T> {
    async fn visit_async(&self, v: &mut Numbers) -> u32 {
        self.value.visit_async(v).await
    }
}
impl<T: Visit<Numbers> + ?Sized> Visit<Numbers> for Covered<'_, T> {
    fn visit(&self, v: &mut Numbers) -> u32 {
        self.0.visit(v)
    }
}
impl<T: VisitAsync<Numbers> + Sync + ?Sized> VisitAsync<Numbers> for Covered<'_, T> {
    async fn visit_async(&self, v: &mut Numbers) -> u32 {
        self.0.visit_async(v).await
    }
}

struct NoVisitor;
#[derive(VisitVariants)]
enum Skipped {
    Tuple(#[visit(skip)] NoVisitor, u32, #[visit(skip)] NoVisitor, u32),
    Named {
        #[visit(skip)]
        ignored: NoVisitor,
        a: u32,
        b: u32,
    },
    Unit,
}

#[tokio::test]
async fn skips_preserve_field_indices_and_all_visiting_paths() {
    let values = [
        Skipped::Tuple(NoVisitor, 2, NoVisitor, 3),
        Skipped::Named {
            ignored: NoVisitor,
            a: 2,
            b: 3,
        },
    ];
    for value in values {
        let info = value.variant_info();
        assert_eq!(info.field_count, 2);
        let mut v = Numbers;
        assert_eq!(value.visit_variant_fields(&mut v).sum::<u32>(), 5);
        assert_eq!(value.visit_variant_fields_covered(&mut v).sum::<u32>(), 5);
        assert_eq!(value.visit_variant_fields_named(&mut v).sum::<u32>(), 5);
        assert_eq!(
            Skipped::visit_variant_fields_static(&info, &mut v).sum::<u32>(),
            2
        );
        assert_eq!(
            Skipped::visit_variant_fields_static_named(&info, &mut v).sum::<u32>(),
            2
        );
        assert_eq!(
            value
                .visit_variant_fields_async(&mut v)
                .collect::<Vec<_>>()
                .await,
            vec![2, 3]
        );
        assert_eq!(
            value
                .visit_variant_fields_covered_async(&mut v)
                .collect::<Vec<_>>()
                .await,
            vec![2, 3]
        );
        assert_eq!(
            value
                .visit_variant_fields_named_async(&mut v)
                .collect::<Vec<_>>()
                .await,
            vec![2, 3]
        );
        assert_eq!(
            Skipped::visit_variant_fields_static_async(&info, &mut v)
                .collect::<Vec<_>>()
                .await,
            vec![1, 1]
        );
        assert_eq!(
            Skipped::visit_variant_fields_static_named_async(&info, &mut v)
                .collect::<Vec<_>>()
                .await,
            vec![1, 1]
        );
    }
    assert_eq!(Skipped::Unit.visit_variant_fields(&mut Numbers).count(), 0);
}
