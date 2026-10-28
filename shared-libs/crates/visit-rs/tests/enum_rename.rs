use visit_rs::*;

// Test basic variant rename
#[derive(VisitVariants)]
enum BasicEnum {
    #[visit(rename = "custom_variant")]
    VariantOne,
    VariantTwo,
}

// Test rename_all on enum
#[derive(VisitVariants)]
#[visit(rename_all = "snake_case")]
enum SnakeCaseEnum {
    VariantOne,
    VariantTwo,
    SomeLongVariantName,
}

// Test rename_all with individual overrides
#[derive(VisitVariants)]
#[visit(rename_all = "SCREAMING_SNAKE_CASE")]
enum MixedRenameEnum {
    FirstVariant,
    #[visit(rename = "CustomName")]
    SecondVariant,
    ThirdVariant,
}

// Test field rename in enum variants
#[derive(VisitVariants)]
enum EnumWithFields {
    Struct {
        #[visit(rename = "renamed_field")]
        original_name: String,
        other_field: i32,
    },
}

// Note: Serde compatibility can be tested when serde is available
// For now, we test visit's own rename_all attribute

#[test]
fn test_basic_variant_rename() {
    let variants: Vec<_> = BasicEnum::variants().into_iter().collect();
    assert_eq!(variants[0].name, "custom_variant");
    assert_eq!(variants[1].name, "VariantTwo");
}

#[test]
fn test_rename_all_snake_case() {
    let variants: Vec<_> = SnakeCaseEnum::variants().into_iter().collect();
    assert_eq!(variants[0].name, "variant_one");
    assert_eq!(variants[1].name, "variant_two");
    assert_eq!(variants[2].name, "some_long_variant_name");
}

#[test]
fn test_rename_all_screaming_snake_case() {
    let variants: Vec<_> = MixedRenameEnum::variants().into_iter().collect();
    assert_eq!(variants[0].name, "FIRST_VARIANT");
    assert_eq!(variants[1].name, "CustomName"); // override
    assert_eq!(variants[2].name, "THIRD_VARIANT");
}

#[test]
fn test_variant_info_by_name_with_rename() {
    let info = SnakeCaseEnum::variant_info_by_name("variant_one");
    assert!(info.is_some());
    assert_eq!(info.unwrap().name, "variant_one");

    // Original name should not work
    let info = SnakeCaseEnum::variant_info_by_name("VariantOne");
    assert!(info.is_none());
}

#[test]
fn test_case_conversions() {
    #[allow(non_camel_case_types)]
    #[derive(VisitVariants)]
    #[visit(rename_all = "PascalCase")]
    enum PascalCase {
        test_variant,
    }

    #[derive(VisitVariants)]
    #[visit(rename_all = "camelCase")]
    enum CamelCase {
        TestVariant,
    }

    #[derive(VisitVariants)]
    #[visit(rename_all = "kebab-case")]
    enum KebabCase {
        TestVariant,
    }

    #[derive(VisitVariants)]
    #[visit(rename_all = "SCREAMING-KEBAB-CASE")]
    enum ScreamingKebabCase {
        TestVariant,
    }

    // serde's `apply_to_variant` treats PascalCase as identity (variants are assumed
    // already PascalCase), so `test_variant` is left unchanged — matching serde_json.
    assert_eq!(
        PascalCase::variants().into_iter().next().unwrap().name,
        "test_variant"
    );
    assert_eq!(
        CamelCase::variants().into_iter().next().unwrap().name,
        "testVariant"
    );
    assert_eq!(
        KebabCase::variants().into_iter().next().unwrap().name,
        "test-variant"
    );
    assert_eq!(
        ScreamingKebabCase::variants()
            .into_iter()
            .next()
            .unwrap()
            .name,
        "TEST-VARIANT"
    );
}

// Variants with acronyms / consecutive capitals must match serde_json's wire output,
// which inserts a separator before every interior uppercase letter.
#[test]
fn test_acronym_variant_conversions() {
    #[derive(VisitVariants)]
    #[visit(rename_all = "snake_case")]
    enum Snake {
        IOError,
        JSONData,
        HttpResponse,
    }
    let names: Vec<_> = Snake::variants().into_iter().map(|v| v.name).collect();
    assert_eq!(names, ["i_o_error", "j_s_o_n_data", "http_response"]);

    #[derive(VisitVariants)]
    #[visit(rename_all = "kebab-case")]
    enum Kebab {
        IOError,
    }
    assert_eq!(
        Kebab::variants().into_iter().next().unwrap().name,
        "i-o-error"
    );
}
