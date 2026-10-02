use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{DataEnum, DeriveInput, Fields, Ident};

use crate::attrs;
use crate::helpers::{
    get_field_rename, get_rename_all_attribute, get_rename_attribute, get_variant_rename,
    is_skipped,
};

fn generics_for_visit(generics: &syn::Generics) -> (TokenStream, TokenStream, TokenStream) {
    let mut with_visitor = generics.clone();
    with_visitor.params.push(syn::parse_quote!(__visit_rs__V));
    let (impl_generics, _, _) = with_visitor.split_for_impl();
    let impl_generics = quote!(#impl_generics);
    let (_, ty_generics, where_clause) = generics.split_for_impl();
    let ty_generics = quote!(#ty_generics);
    let where_preds = match where_clause {
        Some(w) => {
            let preds = w.predicates.iter();
            quote!(#(#preds,)*)
        }
        None => quote!(),
    };
    (impl_generics, ty_generics, where_preds)
}

pub fn derive_all_variant_traits(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let enum_info = derive_enum_info(ast, data)?;
    let visit_variant = derive_visit_variant(ast, data)?;
    let visit_variants_static = derive_visit_variants_static(ast, data)?;
    let visit_variant_fields = derive_visit_variant_fields(ast, data)?;
    let visit_variant_fields_covered = derive_visit_variant_fields_covered(ast, data)?;
    let visit_variant_fields_static = derive_visit_variant_fields_static(ast, data)?;
    let visit_variant_fields_named = derive_visit_variant_fields_named(ast, data)?;
    let visit_variant_fields_static_named = derive_visit_variant_fields_static_named(ast, data)?;
    let visit_variant_fields_async = derive_visit_variant_fields_async(ast, data)?;
    let visit_variant_fields_covered_async = derive_visit_variant_fields_covered_async(ast, data)?;
    let visit_variant_fields_static_async = derive_visit_variant_fields_static_async(ast, data)?;
    let visit_variant_fields_named_async = derive_visit_variant_fields_named_async(ast, data)?;
    let visit_variant_fields_static_named_async =
        derive_visit_variant_fields_static_named_async(ast, data)?;

    Ok(quote! {
        const _: () = {
            #[allow(unused_imports)]
            use visit_rs::{EnumInfo as _, Visit as _, VisitAsync as _};
            #enum_info
            #visit_variant
            #visit_variants_static
            #visit_variant_fields
            #visit_variant_fields_covered
            #visit_variant_fields_static
            #visit_variant_fields_named
            #visit_variant_fields_static_named
            #visit_variant_fields_async
            #visit_variant_fields_covered_async
            #visit_variant_fields_static_async
            #visit_variant_fields_named_async
            #visit_variant_fields_static_named_async
        };
    })
}

fn derive_enum_info(ast: &DeriveInput, data: &DataEnum) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let name = get_rename_attribute(ast).unwrap_or_else(|| ident.to_string());
    let variant_count = data.variants.len();

    let enum_meta = attrs::extract_all_meta(&ast.attrs);

    let enum_meta_ref = if !enum_meta.is_empty() {
        let count = enum_meta.len();
        quote! {
            { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#enum_meta),*]; &META }
        }
    } else {
        quote! { &[] }
    };
    let enum_meta_ref = attrs::member(enum_meta_ref);

    let rename_all_rule = get_rename_all_attribute(ast);

    let renamed_variants: Vec<_> = data
        .variants
        .iter()
        .map(|variant| get_variant_rename(variant, rename_all_rule))
        .collect();

    let mut seen_names = HashSet::new();
    for (idx, renamed_name) in renamed_variants.iter().enumerate() {
        if !seen_names.insert(renamed_name) {
            let original_name = &data.variants[idx].ident;
            return Err(syn::Error::new_spanned(
                original_name,
                format!(
                    "Variant name '{}' is not unique after applying rename rules",
                    renamed_name
                ),
            ));
        }
    }

    let variant_metas: Vec<_> = data
        .variants
        .iter()
        .map(|variant| attrs::extract_all_meta(&variant.attrs))
        .collect();

    let variant_meta_refs: Vec<_> = variant_metas
        .iter()
        .map(|metas| {
            if !metas.is_empty() {
                let count = metas.len();
                quote! {
                    { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                }
            } else {
                quote! { &[] }
            }
        })
        .collect();

    let variant_infos = data.variants.iter().enumerate().zip(&renamed_variants).map(
        |((idx, _variant), renamed_name)| {
            let variant = &data.variants[idx];
            let named_fields = matches!(variant.fields, Fields::Named(_));
            let field_count = variant.fields.iter().filter(|f| !is_skipped(f)).count();
            let metadata_ref = attrs::member(variant_meta_refs[idx].clone());

            quote! {
                visit_rs::StructInfoData {
                    name: #renamed_name,
                    named_fields: #named_fields,
                    field_count: #field_count,
                    #metadata_ref
                }
            }
        },
    );

    let variant_info_arms = data.variants.iter().enumerate().zip(&renamed_variants).map(
        |((idx, variant), renamed_name)| {
            let variant_name = &variant.ident;
            let named_fields = matches!(variant.fields, Fields::Named(_));
            let field_count = variant.fields.iter().filter(|f| !is_skipped(f)).count();

            let pattern = match &variant.fields {
                Fields::Named(_) => quote! { Self::#variant_name { .. } },
                Fields::Unnamed(_) => {
                    let placeholders = (0..variant.fields.len()).map(|_| quote! { _ });
                    quote! { Self::#variant_name(#(#placeholders),*) }
                }
                Fields::Unit => quote! { Self::#variant_name },
            };

            let metadata_ref = attrs::member(variant_meta_refs[idx].clone());

            quote! {
                #pattern => visit_rs::StructInfoData {
                    name: #renamed_name,
                    named_fields: #named_fields,
                    field_count: #field_count,
                    #metadata_ref
                }
            }
        },
    );

    let variant_by_name_arms = renamed_variants
        .iter()
        .enumerate()
        .map(|(idx, renamed_name)| {
            let variant = &data.variants[idx];
            let named_fields = matches!(variant.fields, Fields::Named(_));
            let field_count = variant.fields.iter().filter(|f| !is_skipped(f)).count();
            let metadata_ref = attrs::member(variant_meta_refs[idx].clone());

            quote! {
                #renamed_name => Some(visit_rs::StructInfoData {
                    name: #renamed_name,
                    named_fields: #named_fields,
                    field_count: #field_count,
                    #metadata_ref
                })
            }
        });

    Ok(quote! {
        impl #impl_generics visit_rs::EnumInfo for #ident #ty_generics #where_clause {
            const DATA: visit_rs::EnumInfoData = visit_rs::EnumInfoData {
                name: #name,
                variant_count: #variant_count,
                #enum_meta_ref
            };

            fn variants() -> impl IntoIterator<Item = visit_rs::StructInfoData> + Send + Sync + 'static {
                [#(#variant_infos),*]
            }

            fn variant_info(&self) -> visit_rs::StructInfoData {
                match self {
                    #(#variant_info_arms),*
                }
            }

            fn variant_info_by_name(name: &str) -> Option<visit_rs::StructInfoData> {
                match name {
                    #(#variant_by_name_arms,)*
                    _ => None
                }
            }
        }
    })
}

fn derive_visit_variant(ast: &DeriveInput, _data: &DataEnum) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(&ast.generics);

    Ok(quote! {
        impl #impl_generics visit_rs::VisitVariant<__visit_rs__V> for #ident #ty_generics
        where
            #where_preds
            Self: 'static,
            __visit_rs__V: visit_rs::Visitor,
            for<'__visit_rs__a> visit_rs::Variant<'__visit_rs__a, Self>: visit_rs::Visit<__visit_rs__V>,
        {
            fn visit_variant(&self, visitor: &mut __visit_rs__V) -> <__visit_rs__V as visit_rs::Visitor>::Result {
                visit_rs::Variant {
                    info: self.variant_info(),
                    value: self,
                }
                .visit(visitor)
            }
        }
    })
}

fn derive_visit_variants_static(
    ast: &DeriveInput,
    _data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(&ast.generics);

    Ok(quote! {
        impl #impl_generics visit_rs::VisitVariantsStatic<__visit_rs__V> for #ident #ty_generics
        where
            #where_preds
            Self: 'static,
            __visit_rs__V: visit_rs::Visitor,
            for<'__visit_rs__a> visit_rs::Variant<'__visit_rs__a, visit_rs::Static<Self>>: visit_rs::Visit<__visit_rs__V>,
        {
            fn visit_variants_static<'__visit_rs__a>(visitor: &'__visit_rs__a mut __visit_rs__V) -> impl Iterator<Item = <__visit_rs__V as visit_rs::Visitor>::Result> {
                Self::variants().into_iter().map(|info| {
                    visit_rs::Variant {
                        info,
                        value: visit_rs::Static::new_ref(),
                    }
                    .visit(visitor)
                })
            }
        }
    })
}

fn derive_visit_variant_fields(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(&ast.generics);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { #ty: visit_rs::Visit<__visit_rs__V> });
            }
        }
    }

    let variant_arms = data.variants.iter().map(|variant| {
        let variant_name = &variant.ident;

        match &variant.fields {
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields
                    .named
                    .iter()
                    .filter(|f| !is_skipped(f))
                    .map(|f| &f.ident)
                    .collect();
                let field_matches = (0..field_names.len()).map(|idx| {
                    let field_name = &field_names[idx];
                    quote! {
                        #idx => Some(#field_name.visit(visitor))
                    }
                });

                quote! {
                    Self::#variant_name { #(#field_names),*, .. } => match position {
                        #(#field_matches,)*
                        _ => None,
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_idents: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| Ident::new(&format!("_tup_{}", i), Span::call_site()))
                    .collect();
                let field_matches = (0..field_idents.len())
                    .filter(|idx| !is_skipped(&fields.unnamed[*idx]))
                    .enumerate()
                    .map(|(position, idx)| {
                        let field_ident = &field_idents[idx];
                        quote! {
                            #position => Some(#field_ident.visit(visitor))
                        }
                    });

                quote! {
                    Self::#variant_name(#(#field_idents),*) => match position {
                        #(#field_matches,)*
                        _ => None,
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    Self::#variant_name => match position {
                        _ => None,
                    }
                }
            }
        }
    });

    Ok(quote! {
        impl #impl_generics visit_rs::VisitVariantFields<__visit_rs__V> for #ident #ty_generics
        where
            #where_preds
            Self: 'static,
            __visit_rs__V: visit_rs::Visitor,
            #(#field_predicates),*
        {
            fn visit_variant_fields<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut __visit_rs__V,
            ) -> impl Iterator<Item = <__visit_rs__V as visit_rs::Visitor>::Result> {
                let mut i = 0;
                std::iter::from_fn(move || {
                    let position = i;
                    i += 1;
                    match self {
                        #(#variant_arms),*
                    }
                })
            }
        }
    })
}

fn derive_visit_variant_fields_covered(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(&ast.generics);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { for<'__visit_rs__covered> visit_rs::Covered<'__visit_rs__covered, #ty>: visit_rs::Visit<__visit_rs__V> });
            }
        }
    }

    let variant_arms = data.variants.iter().map(|variant| {
        let variant_name = &variant.ident;

        match &variant.fields {
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields
                    .named
                    .iter()
                    .filter(|f| !is_skipped(f))
                    .map(|f| &f.ident)
                    .collect();
                let field_matches = (0..field_names.len()).map(|idx| {
                    let field_name = &field_names[idx];
                    quote! {
                        #idx => Some(visit_rs::Covered(#field_name).visit(visitor))
                    }
                });

                quote! {
                    Self::#variant_name { #(#field_names),*, .. } => match position {
                        #(#field_matches,)*
                        _ => None,
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_idents: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| Ident::new(&format!("_tup_{}", i), Span::call_site()))
                    .collect();
                let field_matches = (0..field_idents.len())
                    .filter(|idx| !is_skipped(&fields.unnamed[*idx]))
                    .enumerate()
                    .map(|(position, idx)| {
                        let field_ident = &field_idents[idx];
                        quote! {
                            #position => Some(visit_rs::Covered(#field_ident).visit(visitor))
                        }
                    });

                quote! {
                    Self::#variant_name(#(#field_idents),*) => match position {
                        #(#field_matches,)*
                        _ => None,
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    Self::#variant_name => match position {
                        _ => None,
                    }
                }
            }
        }
    });

    Ok(quote! {
        impl #impl_generics visit_rs::VisitVariantFieldsCovered<__visit_rs__V> for #ident #ty_generics
        where
            #where_preds
            Self: 'static,
            __visit_rs__V: visit_rs::Visitor,
            #(#field_predicates),*
        {
            fn visit_variant_fields_covered<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut __visit_rs__V
            ) -> impl Iterator<Item = <__visit_rs__V as visit_rs::Visitor>::Result> {
                let mut i = 0;
                std::iter::from_fn(move || {
                    let position = i;
                    i += 1;
                    match self {
                        #(#variant_arms),*
                    }
                })
            }
        }
    })
}

fn derive_visit_variant_fields_static(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(&ast.generics);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates
                    .push(quote! { visit_rs::Static<#ty>: visit_rs::Visit<__visit_rs__V> });
            }
        }
    }

    let rename_all_rule = get_rename_all_attribute(ast);

    let variant_field_types: Vec<_> = data
        .variants
        .iter()
        .map(|variant| {
            let variant_name_str = get_variant_rename(variant, rename_all_rule);
            let field_types: Vec<_> = variant
                .fields
                .iter()
                .filter(|f| !is_skipped(f))
                .map(|f| &f.ty)
                .collect();
            (variant_name_str, field_types)
        })
        .collect();

    let variant_arms = variant_field_types
        .iter()
        .map(|(variant_name, field_types)| {
            let field_matches = (0..field_types.len()).map(|idx| {
                let ty = &field_types[idx];
                quote! {
                    #idx => Some(visit_rs::Static::<#ty>::new().visit(visitor))
                }
            });

            quote! {
                #variant_name => match position {
                    #(#field_matches,)*
                    _ => None,
                }
            }
        });

    Ok(quote! {
        impl #impl_generics visit_rs::VisitVariantFieldsStatic<__visit_rs__V> for #ident #ty_generics
        where
            #where_preds
            Self: 'static,
            __visit_rs__V: visit_rs::Visitor,
            #(#field_predicates),*
        {
            fn visit_variant_fields_static<'__visit_rs__a>(
                info: &'__visit_rs__a visit_rs::StructInfoData,
                visitor: &'__visit_rs__a mut __visit_rs__V,
            ) -> impl Iterator<Item = <__visit_rs__V as visit_rs::Visitor>::Result> + '__visit_rs__a {
                let mut i = 0;
                std::iter::from_fn(move || {
                    let position = i;
                    i += 1;
                    match info.name {
                        #(#variant_arms,)*
                        x => {
                            debug_assert!(false, "UNREACHABLE: unknown variant {}", x);
                            return None;
                        }
                    }
                })
            }
        }
    })
}

fn derive_visit_variant_fields_named(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(&ast.generics);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { for<'__visit_rs__named> visit_rs::Named<'__visit_rs__named, #ty>: visit_rs::Visit<__visit_rs__V> });
            }
        }
    }

    let rename_all_rule = get_rename_all_attribute(ast);

    let variant_field_metas: Vec<Vec<Vec<TokenStream>>> = data
        .variants
        .iter()
        .map(|variant| {
            variant
                .fields
                .iter()
                .map(|field| attrs::extract_all_meta(&field.attrs))
                .collect()
        })
        .collect();

    let variant_arms = data.variants.iter().enumerate().map(|(variant_idx, variant)| {
        let variant_name = &variant.ident;

        match &variant.fields {
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields.named.iter().filter(|f| !is_skipped(f)).map(|f| &f.ident).collect();
                let field_matches = fields.named.iter().enumerate().filter(|(_, f)| !is_skipped(f)).enumerate().map(|(position, (idx, field))| {
                    let field_name = field.ident.as_ref().unwrap();
                    let renamed_field = get_field_rename(field, rename_all_rule).unwrap_or_else(|| field_name.to_string());
                    let metadata_ref = if !variant_field_metas[variant_idx][idx].is_empty() {
                        let metas = &variant_field_metas[variant_idx][idx];
                        let count = metas.len();
                        quote! {
                            { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        #position => {
                            let named = visit_rs::Named {
                                name: Some(#renamed_field),
                                #metadata_ref
                                value: #field_name,
                            };
                            Some(named.visit(visitor))
                        }
                    }
                });

                quote! {
                    Self::#variant_name { #(#field_names),*, .. } => match position {
                        #(#field_matches,)*
                        _ => None,
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_idents: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| Ident::new(&format!("_tup_{}", i), Span::call_site()))
                    .collect();
                let field_matches = (0..field_idents.len()).filter(|idx| !is_skipped(&fields.unnamed[*idx])).enumerate().map(|(position, idx)| {
                    let field_ident = &field_idents[idx];
                    let metadata_ref = if !variant_field_metas[variant_idx][idx].is_empty() {
                        let metas = &variant_field_metas[variant_idx][idx];
                        let count = metas.len();
                        quote! {
                            { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        #position => {
                            let named = visit_rs::Named {
                                name: None,
                                #metadata_ref
                                value: #field_ident,
                            };
                            Some(named.visit(visitor))
                        }
                    }
                });

                quote! {
                    Self::#variant_name(#(#field_idents),*) => match position {
                        #(#field_matches,)*
                        _ => None,
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    Self::#variant_name => match position {
                        _ => None,
                    }
                }
            }
        }
    });

    Ok(quote! {
        impl #impl_generics visit_rs::VisitVariantFieldsNamed<__visit_rs__V> for #ident #ty_generics
        where
            #where_preds
            Self: 'static,
            __visit_rs__V: visit_rs::Visitor,
            #(#field_predicates),*
        {
            fn visit_variant_fields_named<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut __visit_rs__V
            ) -> impl Iterator<Item = <__visit_rs__V as visit_rs::Visitor>::Result> {
                let mut i = 0;
                std::iter::from_fn(move || {
                    let position = i;
                    i += 1;
                    match self {
                        #(#variant_arms),*
                    }
                })
            }
        }
    })
}

fn derive_visit_variant_fields_static_named(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(&ast.generics);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { for<'__visit_rs__named> visit_rs::Named<'__visit_rs__named, visit_rs::Static<#ty>>: visit_rs::Visit<__visit_rs__V> });
            }
        }
    }

    let rename_all_rule = get_rename_all_attribute(ast);

    let variant_field_metas: Vec<Vec<Vec<TokenStream>>> = data
        .variants
        .iter()
        .map(|variant| {
            variant
                .fields
                .iter()
                .map(|field| attrs::extract_all_meta(&field.attrs))
                .collect()
        })
        .collect();

    let variant_match_arms = data.variants.iter().enumerate().map(|(variant_idx, variant)| {
        let _variant_name = &variant.ident;
        let variant_name_str = get_variant_rename(variant, rename_all_rule);

        match &variant.fields {
            Fields::Named(fields) => {
                let field_visits = fields.named.iter().enumerate().filter(|(_, f)| !is_skipped(f)).enumerate().map(|(position, (idx, field))| {
                    let ty = &field.ty;
                    let field_name = field.ident.as_ref().unwrap();
                    let renamed_field = get_field_rename(field, rename_all_rule).unwrap_or_else(|| field_name.to_string());
                    let metadata_ref = if !variant_field_metas[variant_idx][idx].is_empty() {
                        let metas = &variant_field_metas[variant_idx][idx];
                        let count = metas.len();
                        quote! {
                            { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        #position => {
                            let named = visit_rs::Named {
                                name: Some(#renamed_field),
                                #metadata_ref
                                value: &visit_rs::Static::<#ty>::new(),
                            };
                            Some(named.visit(visitor))
                        }
                    }
                });

                quote! {
                    #variant_name_str => match position {
                        #(#field_visits,)*
                        _ => None,
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_visits = fields.unnamed.iter().enumerate().filter(|(_, f)| !is_skipped(f)).enumerate().map(|(position, (idx, field))| {
                    let ty = &field.ty;
                    let metadata_ref = if !variant_field_metas[variant_idx][idx].is_empty() {
                        let metas = &variant_field_metas[variant_idx][idx];
                        let count = metas.len();
                        quote! {
                            { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        #position => {
                            let named = visit_rs::Named {
                                name: None,
                                #metadata_ref
                                value: &visit_rs::Static::<#ty>::new(),
                            };
                            Some(named.visit(visitor))
                        }
                    }
                });

                quote! {
                    #variant_name_str => match position {
                        #(#field_visits,)*
                        _ => None,
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    #variant_name_str => match position {
                        _ => None,
                    }
                }
            }
        }
    });

    Ok(quote! {
        impl #impl_generics visit_rs::VisitVariantFieldsStaticNamed<__visit_rs__V> for #ident #ty_generics
        where
            #where_preds
            Self: 'static,
            __visit_rs__V: visit_rs::Visitor,
            #(#field_predicates),*
        {
            fn visit_variant_fields_static_named<'__visit_rs__a>(
                info: &'__visit_rs__a visit_rs::StructInfoData,
                visitor: &'__visit_rs__a mut __visit_rs__V
            ) -> impl Iterator<Item = <__visit_rs__V as visit_rs::Visitor>::Result> + '__visit_rs__a {
                let mut i = 0;
                std::iter::from_fn(move || {
                    let position = i;
                    i += 1;
                    match info.name {
                        #(#variant_match_arms,)*
                        x => {
                            debug_assert!(false, "UNREACHABLE: unknown variant {}", x);
                            return None;
                        }
                    }
                })
            }
        }
    })
}

fn derive_visit_variant_fields_async(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(&ast.generics);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { #ty: visit_rs::VisitAsync<__visit_rs__V> });
                field_predicates.push(quote! { #ty: Sync });
            }
        }
    }

    let variant_arms = data.variants.iter().map(|variant| {
        let variant_name = &variant.ident;

        match &variant.fields {
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields.named.iter().filter(|f| !is_skipped(f)).map(|f| &f.ident).collect();
                let field_visits = field_names.iter().map(|field_name| {
                    quote! { yield visit_rs::VisitAsync::visit_async(#field_name, visitor).await; }
                });

                quote! {
                    Self::#variant_name { #(#field_names),*, .. } => {
                        #(#field_visits)*
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_idents: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| Ident::new(&format!("_tup_{}", i), Span::call_site()))
                    .collect();
                let field_visits = field_idents.iter().zip(fields.unnamed.iter()).filter(|(_, f)| !is_skipped(f)).map(|(field_ident, _)| {
                    quote! { yield visit_rs::VisitAsync::visit_async(#field_ident, visitor).await; }
                });

                quote! {
                    Self::#variant_name(#(#field_idents),*) => {
                        #(#field_visits)*
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    Self::#variant_name => {}
                }
            }
        }
    });

    Ok(quote! {
        impl #impl_generics visit_rs::VisitVariantFieldsAsync<__visit_rs__V> for #ident #ty_generics
        where
            #where_preds
            Self: 'static,
            __visit_rs__V: visit_rs::Visitor + Send,
            <__visit_rs__V as visit_rs::Visitor>::Result: Send,
            #(#field_predicates),*
        {
            fn visit_variant_fields_async<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut __visit_rs__V
            ) -> impl visit_rs::lib::futures::Stream<Item = <__visit_rs__V as visit_rs::Visitor>::Result> + Send + '__visit_rs__a
            where
                __visit_rs__V: Send,
                <__visit_rs__V as visit_rs::Visitor>::Result: Send,
            {
                visit_rs::lib::async_stream::stream! {
                    match self {
                        #(#variant_arms)*
                    }
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <__visit_rs__V as visit_rs::Visitor>::Result
                    }
                }
            }
        }
    })
}

fn derive_visit_variant_fields_covered_async(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(&ast.generics);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { for<'__visit_rs__covered> visit_rs::Covered<'__visit_rs__covered, #ty>: visit_rs::VisitAsync<__visit_rs__V> });
                field_predicates.push(quote! { #ty: Sync });
            }
        }
    }

    let variant_arms = data.variants.iter().map(|variant| {
        let variant_name = &variant.ident;

        match &variant.fields {
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields.named.iter().filter(|f| !is_skipped(f)).map(|f| &f.ident).collect();
                let field_visits = field_names.iter().map(|field_name| {
                    quote! { yield visit_rs::VisitAsync::visit_async(&visit_rs::Covered(#field_name), visitor).await; }
                });

                quote! {
                    Self::#variant_name { #(#field_names),*, .. } => {
                        #(#field_visits)*
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_idents: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| Ident::new(&format!("_tup_{}", i), Span::call_site()))
                    .collect();
                let field_visits = field_idents.iter().zip(fields.unnamed.iter()).filter(|(_, f)| !is_skipped(f)).map(|(field_ident, _)| {
                    quote! { yield visit_rs::VisitAsync::visit_async(&visit_rs::Covered(#field_ident), visitor).await; }
                });

                quote! {
                    Self::#variant_name(#(#field_idents),*) => {
                        #(#field_visits)*
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    Self::#variant_name => {}
                }
            }
        }
    });

    Ok(quote! {
        impl #impl_generics visit_rs::VisitVariantFieldsCoveredAsync<__visit_rs__V> for #ident #ty_generics
        where
            #where_preds
            Self: 'static,
            __visit_rs__V: visit_rs::Visitor + Send,
            <__visit_rs__V as visit_rs::Visitor>::Result: Send,
            #(#field_predicates),*
        {
            fn visit_variant_fields_covered_async<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut __visit_rs__V
            ) -> impl visit_rs::lib::futures::Stream<Item = <__visit_rs__V as visit_rs::Visitor>::Result> + Send + '__visit_rs__a
            where
                __visit_rs__V: Send,
                <__visit_rs__V as visit_rs::Visitor>::Result: Send,
            {
                visit_rs::lib::async_stream::stream! {
                    match self {
                        #(#variant_arms)*
                    }
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <__visit_rs__V as visit_rs::Visitor>::Result
                    }
                }
            }
        }
    })
}

fn derive_visit_variant_fields_static_async(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(&ast.generics);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates
                    .push(quote! { visit_rs::Static<#ty>: visit_rs::VisitAsync<__visit_rs__V> });
            }
        }
    }

    let rename_all_rule = get_rename_all_attribute(ast);

    let variant_match_arms = data.variants.iter().map(|variant| {
        let _variant_name = &variant.ident;
        let variant_name_str = get_variant_rename(variant, rename_all_rule);

        match &variant.fields {
            Fields::Named(fields) => {
                let field_visits = fields.named.iter().filter(|f| !is_skipped(f)).map(|field| {
                    let ty = &field.ty;
                    quote! { yield visit_rs::VisitAsync::visit_async(&visit_rs::Static::<#ty>::new(), visitor).await; }
                });

                quote! {
                    #variant_name_str => {
                        #(#field_visits)*
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_visits = fields.unnamed.iter().filter(|f| !is_skipped(f)).map(|field| {
                    let ty = &field.ty;
                    quote! { yield visit_rs::VisitAsync::visit_async(&visit_rs::Static::<#ty>::new(), visitor).await; }
                });

                quote! {
                    #variant_name_str => {
                        #(#field_visits)*
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    #variant_name_str => {}
                }
            }
        }
    });

    Ok(quote! {
        impl #impl_generics visit_rs::VisitVariantFieldsStaticAsync<__visit_rs__V> for #ident #ty_generics
        where
            #where_preds
            Self: 'static,
            __visit_rs__V: visit_rs::Visitor + Send,
            <__visit_rs__V as visit_rs::Visitor>::Result: Send,
            #(#field_predicates),*
        {
            fn visit_variant_fields_static_async<'__visit_rs__a>(
                info: &'__visit_rs__a visit_rs::StructInfoData,
                visitor: &'__visit_rs__a mut __visit_rs__V
            ) -> impl visit_rs::lib::futures::Stream<Item = <__visit_rs__V as visit_rs::Visitor>::Result> {
                visit_rs::lib::async_stream::stream! {
                    match info.name {
                        #(#variant_match_arms,)*
                        x => {
                            debug_assert!(false, "UNREACHABLE: unknown variant {}", x);
                        }
                    }
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <__visit_rs__V as visit_rs::Visitor>::Result
                    }
                }
            }
        }
    })
}

fn derive_visit_variant_fields_named_async(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(&ast.generics);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { for<'__visit_rs__named> visit_rs::Named<'__visit_rs__named, #ty>: visit_rs::VisitAsync<__visit_rs__V> });
                field_predicates.push(quote! { #ty: Sync });
            }
        }
    }

    let rename_all_rule = get_rename_all_attribute(ast);

    let variant_field_metas: Vec<Vec<Vec<TokenStream>>> = data
        .variants
        .iter()
        .map(|variant| {
            variant
                .fields
                .iter()
                .map(|field| attrs::extract_all_meta(&field.attrs))
                .collect()
        })
        .collect();

    let variant_arms = data.variants.iter().enumerate().map(|(variant_idx, variant)| {
        let variant_name = &variant.ident;

        match &variant.fields {
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields.named.iter().filter(|f| !is_skipped(f)).map(|f| &f.ident).collect();
                let field_visits = fields.named.iter().enumerate().filter(|(_, f)| !is_skipped(f)).map(|(idx, field)| {
                    let field_name = field.ident.as_ref().unwrap();
                    let renamed_field = get_field_rename(field, rename_all_rule).unwrap_or_else(|| field_name.to_string());
                    let metadata_ref = if !variant_field_metas[variant_idx][idx].is_empty() {
                        let metas = &variant_field_metas[variant_idx][idx];
                        let count = metas.len();
                        quote! {
                            { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        {
                            let named = visit_rs::Named {
                                name: Some(#renamed_field),
                                #metadata_ref
                                value: #field_name,
                            };
                            yield visit_rs::VisitAsync::visit_async(&named, visitor).await;
                        }
                    }
                });

                quote! {
                    Self::#variant_name { #(#field_names),*, .. } => {
                        #(#field_visits)*
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_idents: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| Ident::new(&format!("_tup_{}", i), Span::call_site()))
                    .collect();
                let field_visits = field_idents.iter().enumerate().filter(|(idx, _)| !is_skipped(&fields.unnamed[*idx])).map(|(idx, field_ident)| {
                    let metadata_ref = if !variant_field_metas[variant_idx][idx].is_empty() {
                        let metas = &variant_field_metas[variant_idx][idx];
                        let count = metas.len();
                        quote! {
                            { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        {
                            let named = visit_rs::Named {
                                name: None,
                                #metadata_ref
                                value: #field_ident,
                            };
                            yield visit_rs::VisitAsync::visit_async(&named, visitor).await;
                        }
                    }
                });

                quote! {
                    Self::#variant_name(#(#field_idents),*) => {
                        #(#field_visits)*
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    Self::#variant_name => {}
                }
            }
        }
    });

    Ok(quote! {
        impl #impl_generics visit_rs::VisitVariantFieldsNamedAsync<__visit_rs__V> for #ident #ty_generics
        where
            #where_preds
            Self: 'static,
            __visit_rs__V: visit_rs::Visitor + Send,
            <__visit_rs__V as visit_rs::Visitor>::Result: Send,
            #(#field_predicates),*
        {
            fn visit_variant_fields_named_async<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut __visit_rs__V
            ) -> impl visit_rs::lib::futures::Stream<Item = <__visit_rs__V as visit_rs::Visitor>::Result> + Send + '__visit_rs__a
            where
                __visit_rs__V: Send,
                <__visit_rs__V as visit_rs::Visitor>::Result: Send,
            {
                visit_rs::lib::async_stream::stream! {
                    match self {
                        #(#variant_arms)*
                    }
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <__visit_rs__V as visit_rs::Visitor>::Result
                    }
                }
            }
        }
    })
}

fn derive_visit_variant_fields_static_named_async(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(&ast.generics);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { for<'__visit_rs__named> visit_rs::Named<'__visit_rs__named, visit_rs::Static<#ty>>: visit_rs::VisitAsync<__visit_rs__V> });
            }
        }
    }

    let rename_all_rule = get_rename_all_attribute(ast);

    let variant_field_metas: Vec<Vec<Vec<TokenStream>>> = data
        .variants
        .iter()
        .map(|variant| {
            variant
                .fields
                .iter()
                .map(|field| attrs::extract_all_meta(&field.attrs))
                .collect()
        })
        .collect();

    let variant_match_arms = data.variants.iter().enumerate().map(|(variant_idx, variant)| {
        let _variant_name = &variant.ident;
        let variant_name_str = get_variant_rename(variant, rename_all_rule);

        match &variant.fields {
            Fields::Named(fields) => {
                let field_visits = fields.named.iter().enumerate().filter(|(_, f)| !is_skipped(f)).map(|(idx, field)| {
                    let ty = &field.ty;
                    let field_name = field.ident.as_ref().unwrap();
                    let renamed_field = get_field_rename(field, rename_all_rule).unwrap_or_else(|| field_name.to_string());
                    let metadata_ref = if !variant_field_metas[variant_idx][idx].is_empty() {
                        let metas = &variant_field_metas[variant_idx][idx];
                        let count = metas.len();
                        quote! {
                            { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        {
                            let named = visit_rs::Named {
                                name: Some(#renamed_field),
                                #metadata_ref
                                value: &visit_rs::Static::<#ty>::new(),
                            };
                            yield visit_rs::VisitAsync::visit_async(&named, visitor).await;
                        }
                    }
                });

                quote! {
                    #variant_name_str => {
                        #(#field_visits)*
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_visits = fields.unnamed.iter().enumerate().filter(|(_, f)| !is_skipped(f)).map(|(idx, field)| {
                    let ty = &field.ty;
                    let metadata_ref = if !variant_field_metas[variant_idx][idx].is_empty() {
                        let metas = &variant_field_metas[variant_idx][idx];
                        let count = metas.len();
                        quote! {
                            { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        {
                            let named = visit_rs::Named {
                                name: None,
                                #metadata_ref
                                value: &visit_rs::Static::<#ty>::new(),
                            };
                            yield visit_rs::VisitAsync::visit_async(&named, visitor).await;
                        }
                    }
                });

                quote! {
                    #variant_name_str => {
                        #(#field_visits)*
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    #variant_name_str => {}
                }
            }
        }
    });

    Ok(quote! {
        impl #impl_generics visit_rs::VisitVariantFieldsStaticNamedAsync<__visit_rs__V> for #ident #ty_generics
        where
            #where_preds
            Self: 'static,
            __visit_rs__V: visit_rs::Visitor + Send,
            <__visit_rs__V as visit_rs::Visitor>::Result: Send,
            #(#field_predicates),*
        {
            fn visit_variant_fields_static_named_async<'__visit_rs__a>(
                info: &'__visit_rs__a visit_rs::StructInfoData,
                visitor: &'__visit_rs__a mut __visit_rs__V
            ) -> impl visit_rs::lib::futures::Stream<Item = <__visit_rs__V as visit_rs::Visitor>::Result> {
                visit_rs::lib::async_stream::stream! {
                    match info.name {
                        #(#variant_match_arms,)*
                        x => {
                            debug_assert!(false, "UNREACHABLE: unknown variant {}", x);
                        }
                    }
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <__visit_rs__V as visit_rs::Visitor>::Result
                    }
                }
            }
        }
    })
}
