// The `VisitVariants` derive must compile without the caller importing `Visit` / `EnumInfo`
// (the generated impls bring the traits into scope themselves).
#[derive(visit_rs::VisitVariants)]
enum E {
    A(u32),
    B { x: String },
    Unit,
}

#[test]
fn derives_without_trait_imports() {
    // Accessing trait items here does require the trait in scope — that's the test's choice,
    // not the derive's requirement. The derive itself compiled above with no imports.
    use visit_rs::EnumInfo;
    assert_eq!(<E as EnumInfo>::DATA.variant_count, 3);
}
