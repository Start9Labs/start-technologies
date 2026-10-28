use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{DataStruct, DeriveInput, Fields, Path, WhereClause, WherePredicate, parse_quote};

mod attrs;
mod helpers;
mod reflection;

use helpers::{get_field_rename, get_rename_all_attribute, get_rename_attribute};

fn make_impl(
    input: &DeriveInput,
    fields: &Fields,
    trait_path_fields: &Path,
    trait_path: &Path,
    named: Option<&Path>,
    sync: bool,
    is_static: bool,
) -> TokenStream {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(input);
    let ident = &input.ident;

    let (_, ty_generics, _) = &input.generics.split_for_impl();

    let mut generics = input.generics.clone();

    generics.params.push(syn::parse_quote! { #visitor_type });

    let predicates = &mut generics
        .where_clause
        .get_or_insert(WhereClause {
            predicates: Default::default(),
            where_token: Default::default(),
        })
        .predicates;

    predicates.push(syn::parse_quote! { #visitor_type: #runtime::Visitor });
    if sync && !is_static {
        predicates.extend(
            field_iter(fields)
                .map(|(_, f)| &f.ty)
                .map(|t| -> WherePredicate { shared_capture_predicate(&visitor_type, t) }),
        );
    }

    let mut ty_set = HashSet::new();
    for (_, field) in field_iter(fields) {
        let ty = &field.ty;
        if !ty_set.insert(ty) {
            continue;
        }
        if let Some(named) = named {
            if is_static {
                predicates.push(
                    syn::parse_quote! { for<'__visit_rs__named> #named <'__visit_rs__named, #runtime::Static<#ty>>: #trait_path<#visitor_type> },
                );
            } else {
                predicates.push(syn::parse_quote! { for<'__visit_rs__named> #named <'__visit_rs__named, #ty>: #trait_path<#visitor_type> });
            }
        } else {
            if is_static {
                predicates
                    .push(syn::parse_quote! { #runtime::Static<#ty>: #trait_path<#visitor_type> });
            } else {
                predicates.push(syn::parse_quote! { #ty: #trait_path<#visitor_type> });
            }
        }
    }

    let (impl_generics, _, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics #trait_path_fields<#visitor_type> for #ident #ty_generics #where_clause
    }
}

fn field_iter(fields: &Fields) -> impl Iterator<Item = (usize, &syn::Field)> {
    fields
        .iter()
        .enumerate()
        .filter(|(_, field)| !helpers::is_skipped(field))
}

fn field_idx_iter(fields: &Fields) -> impl Iterator<Item = TokenStream> {
    field_iter(fields).map(|(index, field)| {
        let field_name = &field.ident;
        if let Some(name) = field_name {
            quote! { #name }
        } else {
            let index = syn::Index::from(index);
            quote! { #index }
        }
    })
}

fn field_name_idx_iter<'a>(
    ast: &'a DeriveInput,
    fields: &'a syn::Fields,
) -> impl Iterator<Item = (TokenStream, TokenStream)> + 'a {
    let rename_all_rule = get_rename_all_attribute(ast);

    field_iter(fields).map(move |(index, field)| {
        let field_name = &field.ident;
        let idx = if let Some(name) = field_name {
            quote! { #name }
        } else {
            let index = syn::Index::from(index);
            quote! { #index }
        };
        let name = if field_name.is_some() {
            let renamed = get_field_rename(field, rename_all_rule).unwrap();
            quote! { Some(#renamed) }
        } else {
            quote! { None }
        };
        (name, idx)
    })
}

#[proc_macro_derive(VisitFields, attributes(visit, serde))]
pub fn derive_visit_fields_(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();

    let syn::Data::Struct(data) = &ast.data else {
        let span = match &ast.data {
            syn::Data::Enum(data) => data.enum_token.span,
            syn::Data::Union(data) => data.union_token.span,
            _ => Span::call_site(),
        };
        return syn::Error::new(span, "VisitFields can only be derived for structs")
            .to_compile_error()
            .into();
    };

    let static_ast = match reflection::static_input(&ast) {
        Ok(ast) => ast,
        Err(e) => return e.to_compile_error().into(),
    };
    let syn::Data::Struct(static_data) = &static_ast.data else {
        unreachable!()
    };
    let facts = match reflection::derive(&ast) {
        Ok(facts) => facts,
        Err(e) => return e.to_compile_error().into(),
    };
    let all_impls = match (|| {
        Ok::<_, syn::Error>([
            derive_struct_info(&ast, data)?,
            derive_visit_fields(&static_ast, static_data)?,
            derive_visit_fields_covered(&static_ast, static_data)?,
            derive_visit_fields_async(&static_ast, static_data)?,
            derive_visit_fields_covered_async(&static_ast, static_data)?,
            derive_visit_fields_named(&static_ast, static_data)?,
            derive_visit_fields_named_async(&static_ast, static_data)?,
            derive_visit_fields_static(&static_ast, static_data)?,
            derive_visit_fields_static_async(&static_ast, static_data)?,
            derive_visit_fields_static_named(&static_ast, static_data)?,
            derive_visit_fields_static_named_async(&static_ast, static_data)?,
        ])
    })() {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };

    proc_macro::TokenStream::from(quote! {
        #facts
        #(#all_impls)*
    })
}

fn derive_struct_info(ast: &DeriveInput, data: &DataStruct) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let named_fields = matches!(data.fields, Fields::Named(_));
    let field_count = field_iter(&data.fields).count();

    let name = get_rename_attribute(ast).unwrap_or_else(|| ident.to_string());

    let struct_meta = attrs::extract_all_meta(&ast.attrs);

    let struct_meta_ref = if !struct_meta.is_empty() {
        let count = struct_meta.len();
        quote! {
            { const META: [#runtime::metadata::AttributeMeta; #count] = [#(#struct_meta),*]; &META }
        }
    } else {
        quote! { &[] }
    };
    let struct_meta_ref = attrs::member(struct_meta_ref);

    Ok(quote! {
        impl #impl_generics #runtime::StructInfo for #ident #ty_generics #where_clause {
            const DATA: #runtime::StructInfoData = #runtime::StructInfoData {
                name: #name,
                named_fields: #named_fields,
                field_count: #field_count,
                #struct_meta_ref
            };
        }
    })
}

fn derive_visit_fields(ast: &DeriveInput, data: &DataStruct) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { #runtime::VisitFields },
        &syn::parse_quote! { #runtime::Visit },
        None,
        false,
        false,
    );

    let visit_fields_impl = field_idx_iter(&data.fields).enumerate().map(|(num, idx)| {
        let value = struct_field_value(ast, &data.fields, &idx);
        quote! {
            #num => {
                pos += 1;
                Some(#runtime::Visit::visit(#value, visitor))
            }
        }
    });

    Ok(quote! {
        #impl_t {
            fn visit_fields<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut #visitor_type
            ) -> impl Iterator<Item = <#visitor_type as #runtime::Visitor>::Result> {
                std::iter::from_fn({
                    let mut pos = 0;
                    move || match pos {
                        #(#visit_fields_impl)*
                        _ => None,
                    }
                })
            }
        }
    })
}

fn derive_visit_fields_covered(
    ast: &DeriveInput,
    data: &DataStruct,
) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { #runtime::VisitFieldsCovered },
        &syn::parse_quote! { #runtime::Visit },
        Some(&syn::parse_quote! { #runtime::Covered }),
        false,
        false,
    );

    let visit_fields_impl = field_idx_iter(&data.fields).enumerate().map(|(num, idx)| {
        let value = struct_field_value(ast, &data.fields, &idx);
        quote! {
            #num => {
                pos += 1;
                Some(#runtime::Visit::visit(&#runtime::Covered(#value), visitor))
            }
        }
    });

    Ok(quote! {
        #impl_t {
            fn visit_fields_covered<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut #visitor_type
            ) -> impl Iterator<Item = <#visitor_type as #runtime::Visitor>::Result> {
                std::iter::from_fn({
                    let mut pos = 0;
                    move || match pos {
                        #(#visit_fields_impl)*
                        _ => None,
                    }
                })
            }
        }
    })
}

fn derive_visit_fields_async(
    ast: &DeriveInput,
    data: &DataStruct,
) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { #runtime::VisitFieldsAsync },
        &syn::parse_quote! { #runtime::VisitAsync },
        None,
        true,
        false,
    );

    let body = value_async_body(ast, &data.fields, plain_async_callback, &|idx| {
        struct_field_value(ast, &data.fields, &idx)
    })?;

    Ok(quote! {
        #impl_t {
            fn visit_fields_async<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut #visitor_type,
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

fn derive_visit_fields_covered_async(
    ast: &DeriveInput,
    data: &DataStruct,
) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { #runtime::VisitFieldsCoveredAsync },
        &syn::parse_quote! { #runtime::VisitAsync },
        Some(&syn::parse_quote! { #runtime::Covered }),
        true,
        false,
    );

    let body = value_async_body(ast, &data.fields, covered_async_callback, &|idx| {
        struct_field_value(ast, &data.fields, &idx)
    })?;

    Ok(quote! {
        #impl_t {
            fn visit_fields_covered_async<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut #visitor_type,
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

fn derive_visit_fields_named(
    ast: &DeriveInput,
    data: &DataStruct,
) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let _ident = &ast.ident;

    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { #runtime::VisitFieldsNamed },
        &syn::parse_quote! { #runtime::Visit },
        Some(&syn::parse_quote! { #runtime::Named }),
        false,
        false,
    );

    let field_metas: Vec<_> = field_iter(&data.fields)
        .map(|(_, field)| attrs::extract_all_meta(&field.attrs))
        .collect();

    let visit_fields_named_impl =
        field_name_idx_iter(ast, &data.fields)
            .enumerate()
            .map(|(num, (name, idx))| {
                let value = struct_field_value(ast, &data.fields, &idx);
                let metadata_ref = if !field_metas[num].is_empty() {
                    let metas = &field_metas[num];
                    let count = metas.len();
                    quote! {
                        { const META: [#runtime::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                    }
                } else {
                    quote! { &[] }
                };
                let metadata_ref = attrs::member(metadata_ref);

                quote! {
                    #num => {
                        pos += 1;
                        Some({
                            let named = #runtime::Named {
                                name: #name,
                                #metadata_ref
                                value: #value,
                            };
                            #runtime::Visit::visit(&named, visitor)
                        })
                    }
                }
            });

    Ok(quote! {
        #impl_t {
            fn visit_fields_named<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut #visitor_type
            ) -> impl Iterator<Item = <#visitor_type as #runtime::Visitor>::Result> + '__visit_rs__a {
                std::iter::from_fn({
                    let mut pos = 0;
                    move || match pos {
                        #(#visit_fields_named_impl)*
                        _ => None,
                    }
                })
            }
        }
    })
}

fn derive_visit_fields_named_async(
    ast: &DeriveInput,
    data: &DataStruct,
) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let _ident = &ast.ident;

    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { #runtime::VisitFieldsNamedAsync },
        &syn::parse_quote! { #runtime::VisitAsync },
        Some(&syn::parse_quote! { #runtime::Named }),
        true,
        false,
    );

    let body = value_async_body(ast, &data.fields, named_async_callback, &|idx| {
        struct_field_value(ast, &data.fields, &idx)
    })?;

    Ok(quote! {
        #impl_t {
            fn visit_fields_named_async<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut #visitor_type,
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

fn derive_visit_fields_static(
    ast: &DeriveInput,
    data: &DataStruct,
) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { #runtime::VisitFieldsStatic },
        &syn::parse_quote! { #runtime::Visit },
        None,
        false,
        true,
    );

    let field_types: Vec<_> = field_iter(&data.fields)
        .map(|(_, field)| &field.ty)
        .collect();
    let visit_fields_impl = field_types.iter().enumerate().map(|(num, ty)| {
        quote! {
            #num => {
                pos += 1;
                Some(#runtime::Visit::visit(&#runtime::Static::<#ty>::new(), visitor))
            }
        }
    });

    Ok(quote! {
        #impl_t {
            fn visit_fields_static<'__visit_rs__a>(
                visitor: &'__visit_rs__a mut #visitor_type
            ) -> impl Iterator<Item = <#visitor_type as #runtime::Visitor>::Result> + '__visit_rs__a {
                std::iter::from_fn({
                    let mut pos = 0;
                    move || match pos {
                        #(#visit_fields_impl)*
                        _ => None,
                    }
                })
            }
        }
    })
}

fn derive_visit_fields_static_async(
    ast: &DeriveInput,
    data: &DataStruct,
) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { #runtime::VisitFieldsStaticAsync },
        &syn::parse_quote! { #runtime::VisitAsync },
        None,
        true,
        true,
    );

    let field_types: Vec<_> = field_iter(&data.fields)
        .map(|(_, field)| &field.ty)
        .collect();
    let visit_fields_impl = field_types.iter().map(|ty| {
        quote! {
            yield #runtime::VisitAsync::visit_async(&#runtime::Static::<#ty>::new(), visitor).await;
        }
    });

    Ok(quote! {
        #impl_t {
            fn visit_fields_static_async<'__visit_rs__a>(
                visitor: &'__visit_rs__a mut #visitor_type,
            ) -> impl #runtime::lib::futures::Stream<Item = <#visitor_type as #runtime::Visitor>::Result> + Send + '__visit_rs__a
            where
                #visitor_type: Send,
                <#visitor_type as #runtime::Visitor>::Result: Send,
            {
                #runtime::lib::async_stream::stream! {
                    #(#visit_fields_impl)*
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <#visitor_type as #runtime::Visitor>::Result
                    }
                }
            }
        }
    })
}

fn derive_visit_fields_static_named(
    ast: &DeriveInput,
    data: &DataStruct,
) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let _ident = &ast.ident;

    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { #runtime::VisitFieldsStaticNamed },
        &syn::parse_quote! { #runtime::Visit },
        Some(&syn::parse_quote! { #runtime::Named }),
        false,
        true,
    );

    let rename_all_rule = get_rename_all_attribute(ast);

    let field_metas: Vec<_> = field_iter(&data.fields)
        .map(|(_, field)| attrs::extract_all_meta(&field.attrs))
        .collect();

    let field_name_type_iter = field_iter(&data.fields)
        .enumerate()
        .map(|(num, (_, field))| {
            let field_name = &field.ident;
            let ty = &field.ty;
            let name = if field_name.is_some() {
                let renamed = get_field_rename(field, rename_all_rule).unwrap();
                quote! { Some(#renamed) }
            } else {
                quote! { None }
            };

            let metadata_ref = if !field_metas[num].is_empty() {
                let metas = &field_metas[num];
                let count = metas.len();
                quote! {
                    { const META: [#runtime::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                }
            } else {
                quote! { &[] }
            };
            let metadata_ref = attrs::member(metadata_ref);

            (name, ty, metadata_ref)
        });

    let visit_fields_named_impl =
        field_name_type_iter
            .enumerate()
            .map(|(num, (name, ty, metadata_ref))| {
                quote! {
                    #num => {
                        pos += 1;
                        {
                            let named = #runtime::Named {
                                name: #name,
                                #metadata_ref
                                value: &#runtime::Static::<#ty>::new(),
                            };
                            Some(#runtime::Visit::visit(&named, visitor))
                        }
                    }
                }
            });

    Ok(quote! {
        #impl_t {
            fn visit_fields_static_named<'__visit_rs__a>(
                visitor: &'__visit_rs__a mut #visitor_type
            ) -> impl Iterator<Item = <#visitor_type as #runtime::Visitor>::Result> + '__visit_rs__a {
                std::iter::from_fn({
                    let mut pos = 0;
                    move || match pos {
                        #(#visit_fields_named_impl)*
                        _ => None,
                    }
                })
            }
        }
    })
}

fn derive_visit_fields_static_named_async(
    ast: &DeriveInput,
    data: &DataStruct,
) -> Result<TokenStream, syn::Error> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let _ident = &ast.ident;

    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { #runtime::VisitFieldsStaticNamedAsync },
        &syn::parse_quote! { #runtime::VisitAsync },
        Some(&syn::parse_quote! { #runtime::Named }),
        true,
        true,
    );

    let rename_all_rule = get_rename_all_attribute(ast);

    let field_metas: Vec<_> = field_iter(&data.fields)
        .map(|(_, field)| attrs::extract_all_meta(&field.attrs))
        .collect();

    let field_name_type_iter = field_iter(&data.fields)
        .enumerate()
        .map(|(num, (_, field))| {
            let field_name = &field.ident;
            let ty = &field.ty;
            let name = if field_name.is_some() {
                let renamed = get_field_rename(field, rename_all_rule).unwrap();
                quote! { Some(#renamed) }
            } else {
                quote! { None }
            };

            let metadata_ref = if !field_metas[num].is_empty() {
                let metas = &field_metas[num];
                let count = metas.len();
                quote! {
                    { const META: [#runtime::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                }
            } else {
                quote! { &[] }
            };
            let metadata_ref = attrs::member(metadata_ref);

            (name, ty, metadata_ref)
        });

    let visit_fields_named_impl = field_name_type_iter.map(|(name, ty, metadata_ref)| {
        quote! {
            {
                let named = #runtime::Named {
                    name: #name,
                    #metadata_ref
                    value: &#runtime::Static::<#ty>::new(),
                };
                yield #runtime::VisitAsync::visit_async(&named, visitor).await;
            }
        }
    });

    Ok(quote! {
        #impl_t {
            fn visit_fields_static_named_async<'__visit_rs__a>(
                visitor: &'__visit_rs__a mut #visitor_type,
            ) -> impl #runtime::lib::futures::Stream<Item = <#visitor_type as #runtime::Visitor>::Result> + Send + '__visit_rs__a
            where
                #visitor_type: Send,
                <#visitor_type as #runtime::Visitor>::Result: Send,
            {
                #runtime::lib::async_stream::stream! {
                    #(#visit_fields_named_impl)*
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <#visitor_type as #runtime::Visitor>::Result
                    }
                }
            }
        }
    })
}

mod enum_variants;

#[proc_macro_derive(VisitVariants, attributes(visit, serde))]
pub fn derive_visit_variants(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();

    let syn::Data::Enum(data) = &ast.data else {
        return syn::Error::new_spanned(&ast.ident, "VisitVariants can only be used on enums")
            .to_compile_error()
            .into();
    };

    match enum_variants::derive_all_variant_traits(&ast, data).and_then(|tokens| {
        let facts = reflection::derive(&ast)?;
        Ok(quote!(#facts #tokens))
    }) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

pub(crate) fn runtime() -> TokenStream {
    match proc_macro_crate::crate_name("visit-rs").expect("runtime dependency") {
        proc_macro_crate::FoundCrate::Itself => quote!(::visit_rs),
        proc_macro_crate::FoundCrate::Name(name) => {
            let name = syn::Ident::new(&name, Span::call_site());
            quote!(::#name)
        }
    }
}

pub(crate) fn visitor_identifier(ast: &DeriveInput) -> syn::Ident {
    use quote::ToTokens;
    fresh_identifier(ast.to_token_stream(), "__visit_rs_Visitor")
}

pub(crate) fn fresh_identifier(tokens: TokenStream, prefix: &str) -> syn::Ident {
    fn contains(tokens: TokenStream, name: &str) -> bool {
        tokens.into_iter().any(|token| match token {
            proc_macro2::TokenTree::Ident(i) => i == name,
            proc_macro2::TokenTree::Group(g) => contains(g.stream(), name),
            _ => false,
        })
    }
    for index in 0.. {
        let name = format!("{prefix}{index}");
        if !contains(tokens.clone(), &name) {
            return syn::Ident::new(&name, Span::mixed_site());
        }
    }
    unreachable!()
}

pub(crate) fn shared_capture_predicate(visitor: &syn::Ident, ty: &syn::Type) -> WherePredicate {
    let runtime = crate::runtime();
    parse_quote! { (#visitor, #ty): #runtime::lib::SharedCapture<Value = #ty> }
}

pub(crate) fn value_async_body(
    ast: &DeriveInput,
    fields: &syn::Fields,
    callback: AsyncCallback,
    field_value: &dyn Fn(TokenStream) -> TokenStream,
) -> syn::Result<TokenStream> {
    let runtime = crate::runtime();
    let visitor_type = crate::visitor_identifier(ast);
    let helper = fresh_identifier(quote!(#ast), "__visit_rs_stream");
    let mut parameters = Vec::new();
    let mut arguments = Vec::new();
    let mut types = Vec::new();
    let mut contexts = Vec::new();
    let mut predicates = Vec::new();
    let mut visits = Vec::new();
    for (index, (name, idx)) in field_name_idx_iter(ast, fields).enumerate() {
        let field = field_iter(fields).nth(index).unwrap().1;
        let opaque = reflection::storage_is_opaque(reflection::root_is_opaque(ast)?, field)?;
        let context = syn::Ident::new(&format!("Field{index}"), Span::mixed_site());
        let ty: syn::Type = parse_quote!(<#context as #runtime::lib::SharedCapture>::Value);
        let storage = &field.ty;
        let value = syn::Ident::new(&format!("field{index}"), Span::mixed_site());
        types.push(context.clone());
        contexts.push(quote!((#visitor_type, #storage)));
        predicates.push(quote!(#context: #runtime::lib::SharedCapture));
        predicates.push(quote!(#ty: '__visit_rs__a));
        let reference = if opaque {
            predicates.push(quote!(#ty: Send + Sized));
            parameters.push(quote!(#value: #ty));
            arguments.push(reflection::storage_marker(&field.ty));
            quote!(&#value)
        } else {
            parameters.push(quote!(#value: &'__visit_rs__a #ty));
            arguments.push(field_value(idx));
            quote!(#value)
        };
        let (predicate, visit) = callback(&runtime, &visitor_type, &ty, reference, name, field);
        predicates.push(predicate);
        visits.push(visit);
    }
    Ok(quote! {
        fn #helper<'__visit_rs__a, #visitor_type, #(#types: ?Sized),*>(
            visitor: &'__visit_rs__a mut #visitor_type, #(#parameters),*
        ) -> impl #runtime::lib::futures::Stream<Item = #visitor_type::Result> + Send + '__visit_rs__a
        where #visitor_type: #runtime::Visitor + Send, #visitor_type::Result: Send,
              #(#predicates,)*
        {
            #runtime::lib::async_stream::stream! {
                #(#visits)*
                #[allow(unreachable_code)]
                if false { yield unreachable!() as #visitor_type::Result }
            }
        }
        #helper::<#visitor_type, #(#contexts),*>(visitor, #(#arguments),*)
    })
}

fn struct_field_value(ast: &DeriveInput, fields: &Fields, idx: &TokenStream) -> TokenStream {
    let field = fields
        .iter()
        .enumerate()
        .find(|(index, field)| {
            field
                .ident
                .as_ref()
                .map(|i| quote!(#i).to_string())
                .unwrap_or_else(|| index.to_string())
                == idx.to_string()
        })
        .unwrap()
        .1;
    if reflection::storage_is_opaque(
        reflection::root_is_opaque(ast).expect("validated controls"),
        field,
    )
    .expect("validated controls")
    {
        let marker = reflection::storage_marker(&field.ty);
        quote!(&#marker)
    } else {
        quote!(&self.#idx)
    }
}

pub(crate) type AsyncCallback = fn(
    &TokenStream,
    &syn::Ident,
    &syn::Type,
    TokenStream,
    TokenStream,
    &syn::Field,
) -> (TokenStream, TokenStream);

pub(crate) fn plain_async_callback(
    runtime: &TokenStream,
    visitor: &syn::Ident,
    ty: &syn::Type,
    reference: TokenStream,
    _: TokenStream,
    _: &syn::Field,
) -> (TokenStream, TokenStream) {
    (
        quote!(#ty: #runtime::VisitAsync<#visitor>),
        quote!(yield #runtime::VisitAsync::visit_async(#reference, visitor).await;),
    )
}

pub(crate) fn covered_async_callback(
    runtime: &TokenStream,
    visitor: &syn::Ident,
    ty: &syn::Type,
    reference: TokenStream,
    _: TokenStream,
    _: &syn::Field,
) -> (TokenStream, TokenStream) {
    (
        quote!(for<'__visit_rs__callback> #runtime::Covered<'__visit_rs__callback, #ty>: #runtime::VisitAsync<#visitor>),
        quote!({ let callback = #runtime::Covered(#reference); yield #runtime::VisitAsync::visit_async(&callback, visitor).await; }),
    )
}

pub(crate) fn named_async_callback(
    runtime: &TokenStream,
    visitor: &syn::Ident,
    ty: &syn::Type,
    reference: TokenStream,
    name: TokenStream,
    field: &syn::Field,
) -> (TokenStream, TokenStream) {
    let meta = attrs::extract_all_meta(&field.attrs);
    let metadata = attrs::member(quote!(&[#(#meta),*]));
    (
        quote!(for<'__visit_rs__callback> #runtime::Named<'__visit_rs__callback, #ty>: #runtime::VisitAsync<#visitor>),
        quote!({ let callback = #runtime::Named { name: #name, #metadata value: #reference }; yield #runtime::VisitAsync::visit_async(&callback, visitor).await; }),
    )
}
