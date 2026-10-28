use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{DataEnum, DeriveInput, Fields, Ident};

use crate::attrs;
use crate::helpers::{
    get_field_rename, get_rename_all_attribute, get_rename_attribute, get_variant_rename,
    is_skipped,
};

fn generics_for_visit(ast: &DeriveInput) -> (TokenStream, TokenStream, TokenStream) {
    let visitor_type = crate::visitor_identifier(ast);
    let generics = &ast.generics;
    let mut with_visitor = generics.clone();
    with_visitor.params.push(syn::parse_quote!(#visitor_type));
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
    let runtime = crate::runtime();
    let static_ast = crate::reflection::static_input(ast)?;
    let syn::Data::Enum(static_data) = &static_ast.data else {
        unreachable!()
    };
    let enum_info = derive_enum_info(ast, data)?;
    let visit_variant = derive_visit_variant(ast, data)?;
    let visit_variants_static = derive_visit_variants_static(ast, data)?;
    let visit_variant_fields = derive_visit_variant_fields(&static_ast, static_data)?;
    let visit_variant_fields_covered =
        derive_visit_variant_fields_covered(&static_ast, static_data)?;
    let visit_variant_fields_static = derive_visit_variant_fields_static(&static_ast, static_data)?;
    let visit_variant_fields_named = derive_visit_variant_fields_named(&static_ast, static_data)?;
    let visit_variant_fields_static_named =
        derive_visit_variant_fields_static_named(&static_ast, static_data)?;
    let visit_variant_fields_async = derive_visit_variant_fields_async(&static_ast, static_data)?;
    let visit_variant_fields_covered_async =
        derive_visit_variant_fields_covered_async(&static_ast, static_data)?;
    let visit_variant_fields_static_async =
        derive_visit_variant_fields_static_async(&static_ast, static_data)?;
    let visit_variant_fields_named_async =
        derive_visit_variant_fields_named_async(&static_ast, static_data)?;
    let visit_variant_fields_static_named_async =
        derive_visit_variant_fields_static_named_async(&static_ast, static_data)?;

    Ok(quote! {
        const _: () = {
            #[allow(unused_imports)]
            use #runtime::{EnumInfo as _, Visit as _, VisitAsync as _};
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

fn enum_receiver(data: &DataEnum) -> TokenStream {
    if data.variants.is_empty() {
        quote!(*self)
    } else {
        quote!(self)
    }
}

fn derive_enum_info(ast: &DeriveInput, data: &DataEnum) -> Result<TokenStream, syn::Error> {
    let receiver = enum_receiver(data);
    let runtime = crate::runtime();
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let name = get_rename_attribute(ast).unwrap_or_else(|| ident.to_string());
    let variant_count = data.variants.len();

    let enum_meta = attrs::extract_all_meta(&ast.attrs);

    let enum_meta_ref = if !enum_meta.is_empty() {
        let count = enum_meta.len();
        quote! {
            { const META: [#runtime::metadata::AttributeMeta; #count] = [#(#enum_meta),*]; &META }
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
                    { const META: [#runtime::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
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
                #runtime::StructInfoData {
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
                #pattern => #runtime::StructInfoData {
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
                #renamed_name => Some(#runtime::StructInfoData {
                    name: #renamed_name,
                    named_fields: #named_fields,
                    field_count: #field_count,
                    #metadata_ref
                })
            }
        });

    Ok(quote! {
        impl #impl_generics #runtime::EnumInfo for #ident #ty_generics #where_clause {
            const DATA: #runtime::EnumInfoData = #runtime::EnumInfoData {
                name: #name,
                variant_count: #variant_count,
                #enum_meta_ref
            };

            fn variants() -> impl IntoIterator<Item = #runtime::StructInfoData> + Send + Sync + 'static {
                [#(#variant_infos),*]
            }

            fn variant_info(&self) -> #runtime::StructInfoData {
                match #receiver {
                    #(#variant_info_arms),*
                }
            }

            fn variant_info_by_name(name: &str) -> Option<#runtime::StructInfoData> {
                match name {
                    #(#variant_by_name_arms,)*
                    _ => None
                }
            }
        }
    })
}

fn derive_visit_variant(ast: &DeriveInput, _data: &DataEnum) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(ast);

    Ok(quote! {
        impl #impl_generics #runtime::VisitVariant<#visitor_type> for #ident #ty_generics
        where
            #where_preds
            #visitor_type: #runtime::Visitor,
            for<'__visit_rs__a> #runtime::Variant<'__visit_rs__a, Self>: #runtime::Visit<#visitor_type>,
        {
            fn visit_variant(&self, visitor: &mut #visitor_type) -> <#visitor_type as #runtime::Visitor>::Result {
                #runtime::Variant {
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
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(ast);

    Ok(quote! {
        impl #impl_generics #runtime::VisitVariantsStatic<#visitor_type> for #ident #ty_generics
        where
            #where_preds
            #visitor_type: #runtime::Visitor,
            for<'__visit_rs__a> #runtime::Variant<'__visit_rs__a, #runtime::Static<Self>>: #runtime::Visit<#visitor_type>,
        {
            fn visit_variants_static<'__visit_rs__a>(visitor: &'__visit_rs__a mut #visitor_type) -> impl Iterator<Item = <#visitor_type as #runtime::Visitor>::Result> + '__visit_rs__a {
                let iter = Self::variants().into_iter().map(|info| {
                    #runtime::Variant {
                        info,
                        value: &#runtime::Static::new(),
                    }
                    .visit(visitor)
                });
                iter
            }
        }
    })
}

fn derive_visit_variant_fields(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let receiver = enum_receiver(data);
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(ast);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { #ty: #runtime::Visit<#visitor_type> });
            }
        }
    }

    let variant_arms = data.variants.iter().map(|variant| {
        let pattern = value_variant_pattern(ast, variant);

        match &variant.fields {
            Fields::Named(fields) => {
                let field_matches = fields
                    .named
                    .iter()
                    .enumerate()
                    .filter(|(_, field)| !is_skipped(field))
                    .enumerate()
                    .map(|(position, (index, _))| {
                        let value = field_value(ast, variant, index);
                        quote! {
                            #position => Some((#value).visit(visitor))
                        }
                    });

                quote! {
                    #pattern => match position {
                        #(#field_matches,)*
                        _ => None,
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_matches = (0..fields.unnamed.len())
                    .filter(|idx| !is_skipped(&fields.unnamed[*idx]))
                    .enumerate()
                    .map(|(position, idx)| {
                        let value = field_value(ast, variant, idx);
                        quote! {
                            #position => Some((#value).visit(visitor))
                        }
                    });

                quote! {
                    #pattern => match position {
                        #(#field_matches,)*
                        _ => None,
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    #pattern => match position {
                        _ => None,
                    }
                }
            }
        }
    });

    Ok(quote! {
        impl #impl_generics #runtime::VisitVariantFields<#visitor_type> for #ident #ty_generics
        where
            #where_preds
            #visitor_type: #runtime::Visitor,
            #(#field_predicates),*
        {
            fn visit_variant_fields<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut #visitor_type,
            ) -> impl Iterator<Item = <#visitor_type as #runtime::Visitor>::Result> {
                let mut i = 0;
                std::iter::from_fn(move || {
                    let position = i;
                    i += 1;
                    match #receiver {
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
    let receiver = enum_receiver(data);
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(ast);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { for<'__visit_rs__covered> #runtime::Covered<'__visit_rs__covered, #ty>: #runtime::Visit<#visitor_type> });
            }
        }
    }

    let variant_arms = data.variants.iter().map(|variant| {
        let pattern = value_variant_pattern(ast, variant);

        match &variant.fields {
            Fields::Named(fields) => {
                let field_matches = fields
                    .named
                    .iter()
                    .enumerate()
                    .filter(|(_, field)| !is_skipped(field))
                    .enumerate()
                    .map(|(position, (index, _))| {
                        let value = field_value(ast, variant, index);
                        quote! {
                            #position => Some(#runtime::Covered(#value).visit(visitor))
                        }
                    });

                quote! {
                    #pattern => match position {
                        #(#field_matches,)*
                        _ => None,
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_matches = (0..fields.unnamed.len())
                    .filter(|idx| !is_skipped(&fields.unnamed[*idx]))
                    .enumerate()
                    .map(|(position, idx)| {
                        let value = field_value(ast, variant, idx);
                        quote! {
                            #position => Some(#runtime::Covered(#value).visit(visitor))
                        }
                    });

                quote! {
                    #pattern => match position {
                        #(#field_matches,)*
                        _ => None,
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    #pattern => match position {
                        _ => None,
                    }
                }
            }
        }
    });

    Ok(quote! {
        impl #impl_generics #runtime::VisitVariantFieldsCovered<#visitor_type> for #ident #ty_generics
        where
            #where_preds
            #visitor_type: #runtime::Visitor,
            #(#field_predicates),*
        {
            fn visit_variant_fields_covered<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut #visitor_type
            ) -> impl Iterator<Item = <#visitor_type as #runtime::Visitor>::Result> {
                let mut i = 0;
                std::iter::from_fn(move || {
                    let position = i;
                    i += 1;
                    match #receiver {
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
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(ast);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates
                    .push(quote! { #runtime::Static<#ty>: #runtime::Visit<#visitor_type> });
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
                    #idx => Some(#runtime::Static::<#ty>::new().visit(visitor))
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
        impl #impl_generics #runtime::VisitVariantFieldsStatic<#visitor_type> for #ident #ty_generics
        where
            #where_preds
            #visitor_type: #runtime::Visitor,
            #(#field_predicates),*
        {
            fn visit_variant_fields_static<'__visit_rs__a>(
                info: &'__visit_rs__a #runtime::StructInfoData,
                visitor: &'__visit_rs__a mut #visitor_type,
            ) -> impl Iterator<Item = <#visitor_type as #runtime::Visitor>::Result> + '__visit_rs__a {
                let mut i = 0;
                let iter = std::iter::from_fn(move || {
                    let position = i;
                    i += 1;
                    match info.name {
                        #(#variant_arms,)*
                        x => {
                            debug_assert!(false, "UNREACHABLE: unknown variant {}", x);
                            return None;
                        }
                    }
                });
                iter
            }
        }
    })
}

fn derive_visit_variant_fields_named(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let receiver = enum_receiver(data);
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(ast);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { for<'__visit_rs__named> #runtime::Named<'__visit_rs__named, #ty>: #runtime::Visit<#visitor_type> });
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
        let pattern = value_variant_pattern(ast, variant);

        match &variant.fields {
            Fields::Named(fields) => {
                let field_matches = fields.named.iter().enumerate().filter(|(_, f)| !is_skipped(f)).enumerate().map(|(position, (idx, field))| {
                    let field_name = field.ident.as_ref().unwrap();
                    let value = field_value(ast, variant, idx);
                    let renamed_field = get_field_rename(field, rename_all_rule).unwrap_or_else(|| field_name.to_string());
                    let metadata_ref = if !variant_field_metas[variant_idx][idx].is_empty() {
                        let metas = &variant_field_metas[variant_idx][idx];
                        let count = metas.len();
                        quote! {
                            { const META: [#runtime::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        #position => {
                            let named = #runtime::Named {
                                name: Some(#renamed_field),
                                #metadata_ref
                                value: #value,
                            };
                            Some(named.visit(visitor))
                        }
                    }
                });

                quote! {
                    #pattern => match position {
                        #(#field_matches,)*
                        _ => None,
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_matches = (0..fields.unnamed.len()).filter(|idx| !is_skipped(&fields.unnamed[*idx])).enumerate().map(|(position, idx)| {
                    let value = field_value(ast, variant, idx);
                    let metadata_ref = if !variant_field_metas[variant_idx][idx].is_empty() {
                        let metas = &variant_field_metas[variant_idx][idx];
                        let count = metas.len();
                        quote! {
                            { const META: [#runtime::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        #position => {
                            let named = #runtime::Named {
                                name: None,
                                #metadata_ref
                                value: #value,
                            };
                            Some(named.visit(visitor))
                        }
                    }
                });

                quote! {
                    #pattern => match position {
                        #(#field_matches,)*
                        _ => None,
                    }
                }
            }
            Fields::Unit => {
                quote! {
                    #pattern => match position {
                        _ => None,
                    }
                }
            }
        }
    });

    Ok(quote! {
        impl #impl_generics #runtime::VisitVariantFieldsNamed<#visitor_type> for #ident #ty_generics
        where
            #where_preds
            #visitor_type: #runtime::Visitor,
            #(#field_predicates),*
        {
            fn visit_variant_fields_named<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut #visitor_type
            ) -> impl Iterator<Item = <#visitor_type as #runtime::Visitor>::Result> {
                let mut i = 0;
                std::iter::from_fn(move || {
                    let position = i;
                    i += 1;
                    match #receiver {
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
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(ast);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { for<'__visit_rs__named> #runtime::Named<'__visit_rs__named, #runtime::Static<#ty>>: #runtime::Visit<#visitor_type> });
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
                            { const META: [#runtime::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        #position => {
                            let named = #runtime::Named {
                                name: Some(#renamed_field),
                                #metadata_ref
                                value: &#runtime::Static::<#ty>::new(),
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
                            { const META: [#runtime::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        #position => {
                            let named = #runtime::Named {
                                name: None,
                                #metadata_ref
                                value: &#runtime::Static::<#ty>::new(),
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
        impl #impl_generics #runtime::VisitVariantFieldsStaticNamed<#visitor_type> for #ident #ty_generics
        where
            #where_preds
            #visitor_type: #runtime::Visitor,
            #(#field_predicates),*
        {
            fn visit_variant_fields_static_named<'__visit_rs__a>(
                info: &'__visit_rs__a #runtime::StructInfoData,
                visitor: &'__visit_rs__a mut #visitor_type
            ) -> impl Iterator<Item = <#visitor_type as #runtime::Visitor>::Result> + '__visit_rs__a {
                let mut i = 0;
                let iter = std::iter::from_fn(move || {
                    let position = i;
                    i += 1;
                    match info.name {
                        #(#variant_match_arms,)*
                        x => {
                            debug_assert!(false, "UNREACHABLE: unknown variant {}", x);
                            return None;
                        }
                    }
                });
                iter
            }
        }
    })
}

fn derive_visit_variant_fields_async(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(ast);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { #ty: #runtime::VisitAsync<#visitor_type> });
                let capture = crate::shared_capture_predicate(&visitor_type, ty);
                field_predicates.push(quote!(#capture));
            }
        }
    }

    let body = value_async_variants(ast, data, crate::plain_async_callback)?;

    Ok(quote! {
        impl #impl_generics #runtime::VisitVariantFieldsAsync<#visitor_type> for #ident #ty_generics
        where
            #where_preds
            #visitor_type: #runtime::Visitor + Send,
            <#visitor_type as #runtime::Visitor>::Result: Send,
            #(#field_predicates),*
        {
            fn visit_variant_fields_async<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut #visitor_type
            ) -> impl #runtime::lib::futures::Stream<Item = <#visitor_type as #runtime::Visitor>::Result> + Send + '__visit_rs__a
            where
                #visitor_type: Send,
                <#visitor_type as #runtime::Visitor>::Result: Send,
            {
                #body
            }
        }
    })
}

fn derive_visit_variant_fields_covered_async(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(ast);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { for<'__visit_rs__covered> #runtime::Covered<'__visit_rs__covered, #ty>: #runtime::VisitAsync<#visitor_type> });
                let capture = crate::shared_capture_predicate(&visitor_type, ty);
                field_predicates.push(quote!(#capture));
            }
        }
    }

    let body = value_async_variants(ast, data, crate::covered_async_callback)?;

    Ok(quote! {
        impl #impl_generics #runtime::VisitVariantFieldsCoveredAsync<#visitor_type> for #ident #ty_generics
        where
            #where_preds
            #visitor_type: #runtime::Visitor + Send,
            <#visitor_type as #runtime::Visitor>::Result: Send,
            #(#field_predicates),*
        {
            fn visit_variant_fields_covered_async<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut #visitor_type
            ) -> impl #runtime::lib::futures::Stream<Item = <#visitor_type as #runtime::Visitor>::Result> + Send + '__visit_rs__a
            where
                #visitor_type: Send,
                <#visitor_type as #runtime::Visitor>::Result: Send,
            {
                #body
            }
        }
    })
}

fn derive_visit_variant_fields_static_async(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(ast);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates
                    .push(quote! { #runtime::Static<#ty>: #runtime::VisitAsync<#visitor_type> });
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
                    quote! { yield #runtime::VisitAsync::visit_async(&#runtime::Static::<#ty>::new(), visitor).await; }
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
                    quote! { yield #runtime::VisitAsync::visit_async(&#runtime::Static::<#ty>::new(), visitor).await; }
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
        impl #impl_generics #runtime::VisitVariantFieldsStaticAsync<#visitor_type> for #ident #ty_generics
        where
            #where_preds
            #visitor_type: #runtime::Visitor + Send,
            <#visitor_type as #runtime::Visitor>::Result: Send,
            #(#field_predicates),*
        {
            fn visit_variant_fields_static_async<'__visit_rs__a>(
                info: &'__visit_rs__a #runtime::StructInfoData,
                visitor: &'__visit_rs__a mut #visitor_type
            ) -> impl #runtime::lib::futures::Stream<Item = <#visitor_type as #runtime::Visitor>::Result> + Send + '__visit_rs__a {
                #runtime::lib::async_stream::stream! {
                    match info.name {
                        #(#variant_match_arms,)*
                        x => {
                            debug_assert!(false, "UNREACHABLE: unknown variant {}", x);
                        }
                    }
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <#visitor_type as #runtime::Visitor>::Result
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
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(ast);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { for<'__visit_rs__named> #runtime::Named<'__visit_rs__named, #ty>: #runtime::VisitAsync<#visitor_type> });
                let capture = crate::shared_capture_predicate(&visitor_type, ty);
                field_predicates.push(quote!(#capture));
            }
        }
    }

    let body = value_async_variants(ast, data, crate::named_async_callback)?;

    Ok(quote! {
        impl #impl_generics #runtime::VisitVariantFieldsNamedAsync<#visitor_type> for #ident #ty_generics
        where
            #where_preds
            #visitor_type: #runtime::Visitor + Send,
            <#visitor_type as #runtime::Visitor>::Result: Send,
            #(#field_predicates),*
        {
            fn visit_variant_fields_named_async<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut #visitor_type
            ) -> impl #runtime::lib::futures::Stream<Item = <#visitor_type as #runtime::Visitor>::Result> + Send + '__visit_rs__a
            where
                #visitor_type: Send,
                <#visitor_type as #runtime::Visitor>::Result: Send,
            {
                #body
            }
        }
    })
}

fn derive_visit_variant_fields_static_named_async(
    ast: &DeriveInput,
    data: &DataEnum,
) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_preds) = generics_for_visit(ast);

    let mut ty_set = HashSet::new();
    let mut field_predicates = Vec::new();
    for variant in &data.variants {
        for field in variant.fields.iter().filter(|f| !is_skipped(f)) {
            let ty = &field.ty;
            if ty_set.insert(ty) {
                field_predicates.push(quote! { for<'__visit_rs__named> #runtime::Named<'__visit_rs__named, #runtime::Static<#ty>>: #runtime::VisitAsync<#visitor_type> });
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
                            { const META: [#runtime::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        {
                            let named = #runtime::Named {
                                name: Some(#renamed_field),
                                #metadata_ref
                                value: &#runtime::Static::<#ty>::new(),
                            };
                            yield #runtime::VisitAsync::visit_async(&named, visitor).await;
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
                            { const META: [#runtime::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                        }
                    } else {
                        quote! { &[] }
                    };
                    let metadata_ref = attrs::member(metadata_ref);
                    quote! {
                        {
                            let named = #runtime::Named {
                                name: None,
                                #metadata_ref
                                value: &#runtime::Static::<#ty>::new(),
                            };
                            yield #runtime::VisitAsync::visit_async(&named, visitor).await;
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
        impl #impl_generics #runtime::VisitVariantFieldsStaticNamedAsync<#visitor_type> for #ident #ty_generics
        where
            #where_preds
            #visitor_type: #runtime::Visitor + Send,
            <#visitor_type as #runtime::Visitor>::Result: Send,
            #(#field_predicates),*
        {
            fn visit_variant_fields_static_named_async<'__visit_rs__a>(
                info: &'__visit_rs__a #runtime::StructInfoData,
                visitor: &'__visit_rs__a mut #visitor_type
            ) -> impl #runtime::lib::futures::Stream<Item = <#visitor_type as #runtime::Visitor>::Result> + Send + '__visit_rs__a {
                #runtime::lib::async_stream::stream! {
                    match info.name {
                        #(#variant_match_arms,)*
                        x => {
                            debug_assert!(false, "UNREACHABLE: unknown variant {}", x);
                        }
                    }
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <#visitor_type as #runtime::Visitor>::Result
                    }
                }
            }
        }
    })
}

fn value_async_variants(
    ast: &DeriveInput,
    data: &DataEnum,
    callback: crate::AsyncCallback,
) -> syn::Result<TokenStream> {
    let runtime = crate::runtime();
    let count = data.variants.len();
    if count == 0 {
        let visitor_type = crate::visitor_identifier(ast);
        return Ok(quote!(#runtime::lib::futures::stream::empty::<#visitor_type::Result>()));
    }
    let mut arms = Vec::new();
    for (index, variant) in data.variants.iter().enumerate() {
        let values = std::cell::RefCell::new(
            variant
                .fields
                .iter()
                .enumerate()
                .filter(|(_, field)| value_field_is_captured(ast, variant, field))
                .map(|(index, _)| field_value(ast, variant, index)),
        );
        let pattern = value_variant_pattern(ast, variant);
        let expression =
            |_: TokenStream| values.borrow_mut().next().expect("captured source field");
        let mut callback_ast = ast.clone();
        if crate::reflection::variant_is_opaque(ast, variant)? {
            callback_ast.attrs.push(syn::parse_quote!(#[visit(opaque)]));
        }
        let body = crate::value_async_body(&callback_ast, &variant.fields, callback, &expression)?;
        let mut body = quote!({ #body });
        if index + 1 < count {
            body = quote!(#runtime::lib::futures::future::Either::Left(#body));
        }
        for _ in 0..index {
            body = quote!(#runtime::lib::futures::future::Either::Right(#body));
        }
        arms.push(quote!(#pattern => #body));
    }
    Ok(quote!(match self { #(#arms),* }))
}

fn value_field_binding(ast: &DeriveInput, index: usize) -> Ident {
    crate::fresh_identifier(ast.to_token_stream(), &format!("__visit_rs_field_{index}_"))
}

fn field_value(ast: &DeriveInput, variant: &syn::Variant, index: usize) -> TokenStream {
    let field = variant
        .fields
        .iter()
        .nth(index)
        .expect("source field index");
    if crate::reflection::storage_is_opaque(
        crate::reflection::variant_is_opaque(ast, variant).expect("validated controls"),
        field,
    )
    .expect("validated controls")
    {
        let marker = crate::reflection::storage_marker(&field.ty);
        quote!(&#marker)
    } else {
        let binding = value_field_binding(ast, index);
        quote!(#binding)
    }
}

fn value_field_is_captured(ast: &DeriveInput, variant: &syn::Variant, field: &syn::Field) -> bool {
    !is_skipped(field)
        && !crate::reflection::storage_is_opaque(
            crate::reflection::variant_is_opaque(ast, variant).expect("validated controls"),
            field,
        )
        .expect("validated controls")
}

fn value_variant_pattern(ast: &DeriveInput, variant: &syn::Variant) -> TokenStream {
    let ident = &variant.ident;
    let names = variant.fields.iter().enumerate().map(|(index, field)| {
        let binding = if value_field_is_captured(ast, variant, field) {
            let binding = value_field_binding(ast, index);
            quote!(#binding)
        } else {
            quote!(_)
        };
        match &field.ident {
            Some(name) => quote!(#name: #binding),
            None => binding,
        }
    });
    match variant.fields {
        Fields::Unit => quote!(Self::#ident),
        Fields::Named(_) => quote!(Self::#ident { #(#names),* }),
        Fields::Unnamed(_) => quote!(Self::#ident(#(#names),*)),
    }
}
