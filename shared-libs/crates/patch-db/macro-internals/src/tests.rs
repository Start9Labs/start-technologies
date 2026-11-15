use super::*;

#[test]
fn untagged_models_are_rejected_with_separate_or_combined_attributes() {
    for serde in [
        "#[serde(untagged)]",
        "#[serde(untagged, rename_all = \"camelCase\")]",
    ] {
        let input: DeriveInput = syn::parse_str(&format!(
            "{serde} #[model = \"Model<Self>\"] enum Unsupported {{ Value(String) }}"
        ))
        .unwrap();
        let output = build_model(&input).to_string();
        assert!(output.contains("compile_error"));
        assert!(output.contains("Cannot derive HasModel for untagged enum"));
    }
}

#[test]
fn flattened_fields_preserve_combined_serde_flags() {
    let input: DeriveInput = syn::parse_quote! {
        #[model = "Model<Self>"]
        struct Wrapper {
            #[serde(default, flatten)]
            fields: Fields,
        }
    };
    let Data::Struct(data) = input.data else {
        panic!("Expected struct");
    };
    let fields = ChildInfo::from_fields(&None, &data.fields);
    assert!(fields[0].accessor.is_none());
}
