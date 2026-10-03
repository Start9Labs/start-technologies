use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{DataStruct, DeriveInput, Fields, Path, WhereClause, WherePredicate, parse_quote};

mod attrs;
mod helpers;
mod shape;
mod ts;

#[proc_macro_derive(TS, attributes(serde, ts, visit))]
pub fn derive_ts(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as DeriveInput);
    ts::derive(&ast)
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

#[proc_macro_derive(SerdeShape, attributes(serde, visit))]
pub fn derive_serde_shape(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as DeriveInput);
    shape::derive(&ast)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
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
    let ident = &input.ident;

    let (_, ty_generics, _) = &input.generics.split_for_impl();

    let mut generics = input.generics.clone();

    generics.params.push(syn::parse_quote! { __visit_rs__V });

    let predicates = &mut generics
        .where_clause
        .get_or_insert(WhereClause {
            predicates: Default::default(),
            where_token: Default::default(),
        })
        .predicates;

    predicates.push(syn::parse_quote! { __visit_rs__V: visit_rs::Visitor });
    if sync {
        predicates.extend(
            field_iter(fields)
                .map(|(_, f)| &f.ty)
                .map(|t| -> WherePredicate {
                    parse_quote! { #t: Sync }
                }),
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
                    syn::parse_quote! { for<'__visit_rs__named> #named <'__visit_rs__named, visit_rs::Static<#ty>>: #trait_path<__visit_rs__V> },
                );
            } else {
                predicates.push(syn::parse_quote! { for<'__visit_rs__named> #named <'__visit_rs__named, #ty>: #trait_path<__visit_rs__V> });
            }
        } else {
            if is_static {
                predicates
                    .push(syn::parse_quote! { visit_rs::Static<#ty>: #trait_path<__visit_rs__V> });
            } else {
                predicates.push(syn::parse_quote! { #ty: #trait_path<__visit_rs__V> });
            }
        }
    }

    let (impl_generics, _, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics #trait_path_fields<__visit_rs__V> for #ident #ty_generics #where_clause
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

#[proc_macro_derive(VisitFields, attributes(visit))]
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

    let all_impls = match (|| {
        Ok::<_, syn::Error>([
            derive_struct_info(&ast, data)?,
            derive_visit_fields(&ast, data)?,
            derive_visit_fields_covered(&ast, data)?,
            derive_visit_fields_async(&ast, data)?,
            derive_visit_fields_covered_async(&ast, data)?,
            derive_visit_fields_named(&ast, data)?,
            derive_visit_fields_named_async(&ast, data)?,
            derive_visit_fields_static(&ast, data)?,
            derive_visit_fields_static_async(&ast, data)?,
            derive_visit_fields_static_named(&ast, data)?,
            derive_visit_fields_static_named_async(&ast, data)?,
        ])
    })() {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };

    proc_macro::TokenStream::from(quote! {
        #(#all_impls)*
    })
}

fn derive_struct_info(ast: &DeriveInput, data: &DataStruct) -> Result<TokenStream, syn::Error> {
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let named_fields = matches!(data.fields, Fields::Named(_));
    let field_count = field_iter(&data.fields).count();

    let name = get_rename_attribute(ast).unwrap_or_else(|| ident.to_string());

    let struct_meta = attrs::extract_all_meta(&ast.attrs);

    let struct_meta_ref = if !struct_meta.is_empty() {
        let count = struct_meta.len();
        quote! {
            { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#struct_meta),*]; &META }
        }
    } else {
        quote! { &[] }
    };
    let struct_meta_ref = attrs::member(struct_meta_ref);

    Ok(quote! {
        impl #impl_generics visit_rs::StructInfo for #ident #ty_generics #where_clause {
            const DATA: visit_rs::StructInfoData = visit_rs::StructInfoData {
                name: #name,
                named_fields: #named_fields,
                field_count: #field_count,
                #struct_meta_ref
            };
        }
    })
}

fn derive_visit_fields(ast: &DeriveInput, data: &DataStruct) -> Result<TokenStream, syn::Error> {
    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { visit_rs::VisitFields },
        &syn::parse_quote! { visit_rs::Visit },
        None,
        false,
        false,
    );

    let visit_fields_impl = field_idx_iter(&data.fields).enumerate().map(|(num, idx)| {
        quote! {
            #num => {
                pos += 1;
                Some(visit_rs::Visit::visit(&self.#idx, visitor))
            }
        }
    });

    Ok(quote! {
        #impl_t {
            fn visit_fields<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut __visit_rs__V
            ) -> impl Iterator<Item = <__visit_rs__V as visit_rs::Visitor>::Result> {
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
    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { visit_rs::VisitFieldsCovered },
        &syn::parse_quote! { visit_rs::Visit },
        Some(&syn::parse_quote! { visit_rs::Covered }),
        false,
        false,
    );

    let visit_fields_impl = field_idx_iter(&data.fields).enumerate().map(|(num, idx)| {
        quote! {
            #num => {
                pos += 1;
                Some(visit_rs::Visit::visit(&visit_rs::Covered(&self.#idx), visitor))
            }
        }
    });

    Ok(quote! {
        #impl_t {
            fn visit_fields_covered<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut __visit_rs__V
            ) -> impl Iterator<Item = <__visit_rs__V as visit_rs::Visitor>::Result> {
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
    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { visit_rs::VisitFieldsAsync },
        &syn::parse_quote! { visit_rs::VisitAsync },
        None,
        true,
        false,
    );

    let visit_fields_impl = field_idx_iter(&data.fields).map(|idx| {
        quote! {
            yield visit_rs::VisitAsync::visit_async(&self.#idx, visitor).await;
        }
    });

    Ok(quote! {
        #impl_t {
            fn visit_fields_async<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut __visit_rs__V,
            ) -> impl visit_rs::lib::futures::Stream<Item = <__visit_rs__V as visit_rs::Visitor>::Result> + Send + '__visit_rs__a
            where
                __visit_rs__V: Send,
                <__visit_rs__V as visit_rs::Visitor>::Result: Send,
            {
                visit_rs::lib::async_stream::stream! {
                    #(#visit_fields_impl)*
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <__visit_rs__V as visit_rs::Visitor>::Result
                    }
                }
            }
        }
    })
}

fn derive_visit_fields_covered_async(
    ast: &DeriveInput,
    data: &DataStruct,
) -> Result<TokenStream, syn::Error> {
    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { visit_rs::VisitFieldsCoveredAsync },
        &syn::parse_quote! { visit_rs::VisitAsync },
        Some(&syn::parse_quote! { visit_rs::Covered }),
        true,
        false,
    );

    let visit_fields_impl = field_idx_iter(&data.fields).map(|idx| {
        quote! {
            yield visit_rs::VisitAsync::visit_async(&visit_rs::Covered(&self.#idx), visitor).await;
        }
    });

    Ok(quote! {
        #impl_t {
            fn visit_fields_covered_async<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut __visit_rs__V,
            ) -> impl visit_rs::lib::futures::Stream<Item = <__visit_rs__V as visit_rs::Visitor>::Result> + Send + '__visit_rs__a
            where
                __visit_rs__V: Send,
                <__visit_rs__V as visit_rs::Visitor>::Result: Send,
            {
                visit_rs::lib::async_stream::stream! {
                    #(#visit_fields_impl)*
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <__visit_rs__V as visit_rs::Visitor>::Result
                    }
                }
            }
        }
    })
}

fn derive_visit_fields_named(
    ast: &DeriveInput,
    data: &DataStruct,
) -> Result<TokenStream, syn::Error> {
    let _ident = &ast.ident;

    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { visit_rs::VisitFieldsNamed },
        &syn::parse_quote! { visit_rs::Visit },
        Some(&syn::parse_quote! { visit_rs::Named }),
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
                let metadata_ref = if !field_metas[num].is_empty() {
                    let metas = &field_metas[num];
                    let count = metas.len();
                    quote! {
                        { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                    }
                } else {
                    quote! { &[] }
                };
                let metadata_ref = attrs::member(metadata_ref);

                quote! {
                    #num => {
                        pos += 1;
                        Some({
                            let named = visit_rs::Named {
                                name: #name,
                                #metadata_ref
                                value: &self.#idx,
                            };
                            visit_rs::Visit::visit(&named, visitor)
                        })
                    }
                }
            });

    Ok(quote! {
        #impl_t {
            fn visit_fields_named<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut __visit_rs__V
            ) -> impl Iterator<Item = <__visit_rs__V as visit_rs::Visitor>::Result> + '__visit_rs__a {
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
    let _ident = &ast.ident;

    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { visit_rs::VisitFieldsNamedAsync },
        &syn::parse_quote! { visit_rs::VisitAsync },
        Some(&syn::parse_quote! { visit_rs::Named }),
        true,
        false,
    );

    let field_metas: Vec<_> = field_iter(&data.fields)
        .map(|(_, field)| attrs::extract_all_meta(&field.attrs))
        .collect();

    let visit_fields_named_impl =
        field_name_idx_iter(ast, &data.fields)
            .enumerate()
            .map(|(num, (name, idx))| {
                let metadata_ref = if !field_metas[num].is_empty() {
                    let metas = &field_metas[num];
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
                            name: #name,
                            #metadata_ref
                            value: &self.#idx,
                        };
                        yield visit_rs::VisitAsync::visit_async(&named, visitor).await;
                    }
                }
            });

    Ok(quote! {
        #impl_t {
            fn visit_fields_named_async<'__visit_rs__a>(
                &'__visit_rs__a self,
                visitor: &'__visit_rs__a mut __visit_rs__V,
            ) -> impl visit_rs::lib::futures::Stream<Item = <__visit_rs__V as visit_rs::Visitor>::Result> + Send + '__visit_rs__a
            where
                __visit_rs__V: Send,
                <__visit_rs__V as visit_rs::Visitor>::Result: Send,
            {
                visit_rs::lib::async_stream::stream! {
                    #(#visit_fields_named_impl)*
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <__visit_rs__V as visit_rs::Visitor>::Result
                    }
                }
            }
        }
    })
}

fn derive_visit_fields_static(
    ast: &DeriveInput,
    data: &DataStruct,
) -> Result<TokenStream, syn::Error> {
    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { visit_rs::VisitFieldsStatic },
        &syn::parse_quote! { visit_rs::Visit },
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
                Some(visit_rs::Visit::visit(&visit_rs::Static::<#ty>::new(), visitor))
            }
        }
    });

    Ok(quote! {
        #impl_t {
            fn visit_fields_static<'__visit_rs__a>(
                visitor: &'__visit_rs__a mut __visit_rs__V
            ) -> impl Iterator<Item = <__visit_rs__V as visit_rs::Visitor>::Result> + '__visit_rs__a {
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
    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { visit_rs::VisitFieldsStaticAsync },
        &syn::parse_quote! { visit_rs::VisitAsync },
        None,
        true,
        true,
    );

    let field_types: Vec<_> = field_iter(&data.fields)
        .map(|(_, field)| &field.ty)
        .collect();
    let visit_fields_impl = field_types.iter().map(|ty| {
        quote! {
            yield visit_rs::VisitAsync::visit_async(&visit_rs::Static::<#ty>::new(), visitor).await;
        }
    });

    Ok(quote! {
        #impl_t {
            fn visit_fields_static_async<'__visit_rs__a>(
                visitor: &'__visit_rs__a mut __visit_rs__V,
            ) -> impl visit_rs::lib::futures::Stream<Item = <__visit_rs__V as visit_rs::Visitor>::Result> + Send + '__visit_rs__a
            where
                __visit_rs__V: Send,
                <__visit_rs__V as visit_rs::Visitor>::Result: Send,
            {
                visit_rs::lib::async_stream::stream! {
                    #(#visit_fields_impl)*
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <__visit_rs__V as visit_rs::Visitor>::Result
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
    let _ident = &ast.ident;

    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { visit_rs::VisitFieldsStaticNamed },
        &syn::parse_quote! { visit_rs::Visit },
        Some(&syn::parse_quote! { visit_rs::Named }),
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
                    { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
                }
            } else {
                quote! { &[] }
            };
            let metadata_ref = attrs::member(metadata_ref);

            (name, ty, metadata_ref)
        });

    let visit_fields_named_impl = field_name_type_iter.enumerate().map(|(num, (name, ty, metadata_ref))| {
        quote! {
            #num => {
                pos += 1;
                {
                    static __VISIT_RS_STATIC: visit_rs::Static<()> = visit_rs::Static::new();
                    let named = visit_rs::Named {
                        name: #name,
                        #metadata_ref
                        value: unsafe {
                            &*(&__VISIT_RS_STATIC as *const visit_rs::Static<()> as *const visit_rs::Static<#ty>)
                        },
                    };
                    Some(visit_rs::Visit::visit(&named, visitor))
                }
            }
        }
    });

    Ok(quote! {
        #impl_t {
            fn visit_fields_static_named<'__visit_rs__a>(
                visitor: &'__visit_rs__a mut __visit_rs__V
            ) -> impl Iterator<Item = <__visit_rs__V as visit_rs::Visitor>::Result> + '__visit_rs__a {
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
    let _ident = &ast.ident;

    let impl_t = make_impl(
        ast,
        &data.fields,
        &syn::parse_quote! { visit_rs::VisitFieldsStaticNamedAsync },
        &syn::parse_quote! { visit_rs::VisitAsync },
        Some(&syn::parse_quote! { visit_rs::Named }),
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
                    { const META: [visit_rs::metadata::AttributeMeta; #count] = [#(#metas),*]; &META }
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
                static __VISIT_RS_STATIC: visit_rs::Static<()> = visit_rs::Static::new();
                let named = visit_rs::Named {
                    name: #name,
                    #metadata_ref
                    value: unsafe {
                        &*(&__VISIT_RS_STATIC as *const visit_rs::Static<()> as *const visit_rs::Static<#ty>)
                    },
                };
                yield visit_rs::VisitAsync::visit_async(&named, visitor).await;
            }
        }
    });

    Ok(quote! {
        #impl_t {
            fn visit_fields_static_named_async<'__visit_rs__a>(
                visitor: &'__visit_rs__a mut __visit_rs__V,
            ) -> impl visit_rs::lib::futures::Stream<Item = <__visit_rs__V as visit_rs::Visitor>::Result> + Send + '__visit_rs__a
            where
                __visit_rs__V: Send,
                <__visit_rs__V as visit_rs::Visitor>::Result: Send,
            {
                visit_rs::lib::async_stream::stream! {
                    #(#visit_fields_named_impl)*
                    #[allow(unreachable_code)]
                    if false {
                        yield unreachable!() as <__visit_rs__V as visit_rs::Visitor>::Result
                    }
                }
            }
        }
    })
}

mod enum_variants;

#[proc_macro_derive(VisitVariants, attributes(visit))]
pub fn derive_visit_variants(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();

    let syn::Data::Enum(data) = &ast.data else {
        return syn::Error::new_spanned(&ast.ident, "VisitVariants can only be used on enums")
            .to_compile_error()
            .into();
    };

    match enum_variants::derive_all_variant_traits(&ast, data) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
