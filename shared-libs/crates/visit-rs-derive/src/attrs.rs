use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;
use syn::{Attribute, Expr, Lit, Meta, Token};

pub(crate) fn metadata_path(input: syn::parse::ParseStream) -> syn::Result<syn::Path> {
    use syn::ext::IdentExt;
    let mut path = syn::Path::from(input.call(syn::Ident::parse_any)?);
    while input.peek(Token![::]) {
        input.parse::<Token![::]>()?;
        path.segments
            .push(syn::PathSegment::from(input.call(syn::Ident::parse_any)?));
    }
    Ok(path)
}

pub(crate) fn children(list: &syn::MetaList) -> syn::Result<Punctuated<Meta, Token![,]>> {
    list.parse_args_with(|input: syn::parse::ParseStream| {
        Punctuated::<Meta, Token![,]>::parse_terminated_with(input, nested_meta)
    })
}

fn nested_meta(input: syn::parse::ParseStream) -> syn::Result<Meta> {
    let path = metadata_path(input)?;
    if input.peek(Token![=]) {
        Ok(Meta::NameValue(syn::MetaNameValue {
            path,
            eq_token: input.parse()?,
            value: input.parse()?,
        }))
    } else if input.peek(syn::token::Paren) {
        let content;
        let delimiter = syn::MacroDelimiter::Paren(syn::parenthesized!(content in input));
        Ok(Meta::List(syn::MetaList {
            path,
            delimiter,
            tokens: content.parse()?,
        }))
    } else {
        Ok(Meta::Path(path))
    }
}

fn parse_meta_to_attribute_meta(meta: &Meta, runtime: &TokenStream) -> TokenStream {
    match meta {
        Meta::Path(path) => {
            let path_str = path.to_token_stream().to_string();
            quote! {
                #runtime::metadata::AttributeMeta::Path {
                    path: #path_str,
                }
            }
        }
        Meta::List(list) => {
            let path_str = list.path.to_token_stream().to_string();

            match children(list) {
                Ok(nested) => {
                    let items = nested
                        .iter()
                        .map(|meta| parse_meta_to_attribute_meta(meta, runtime));
                    quote! {
                        #runtime::metadata::AttributeMeta::List {
                            path: #path_str,
                            items: &[#(#items),*],
                        }
                    }
                }
                Err(_) => {
                    let tokens_str = list.tokens.to_string();
                    quote! {
                        #runtime::metadata::AttributeMeta::Unparsed {
                            path: #path_str,
                            tokens: #tokens_str,
                        }
                    }
                }
            }
        }
        Meta::NameValue(nv) => {
            let path_str = nv.path.to_token_stream().to_string();
            let name_str = nv
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();

            let value = match &nv.value {
                Expr::Lit(expr_lit) => match &expr_lit.lit {
                    Lit::Str(s) => {
                        let val = s.value();
                        quote! { #runtime::metadata::MetaValue::Str(#val) }
                    }
                    Lit::Bool(b) => {
                        let val = b.value;
                        quote! { #runtime::metadata::MetaValue::Bool(#val) }
                    }
                    Lit::Int(i) => {
                        if let Ok(val) = i.base10_parse::<i64>() {
                            quote! { #runtime::metadata::MetaValue::Int(#val) }
                        } else {
                            let s = i.to_string();
                            quote! { #runtime::metadata::MetaValue::Unparsed(#s) }
                        }
                    }
                    Lit::Float(f) => {
                        let s = f.to_string();
                        quote! { #runtime::metadata::MetaValue::Float(#s) }
                    }
                    _ => {
                        let s = expr_lit.to_token_stream().to_string();
                        quote! { #runtime::metadata::MetaValue::Unparsed(#s) }
                    }
                },
                Expr::Path(path) => {
                    let s = path.to_token_stream().to_string();
                    quote! { #runtime::metadata::MetaValue::Path(#s) }
                }
                _ => {
                    let s = nv.value.to_token_stream().to_string();
                    quote! { #runtime::metadata::MetaValue::Unparsed(#s) }
                }
            };

            quote! {
                #runtime::metadata::AttributeMeta::NameValue {
                    path: #path_str,
                    name: #name_str,
                    value: #value,
                }
            }
        }
    }
}

pub fn extract_all_meta(attrs: &[Attribute]) -> Vec<TokenStream> {
    extract_all_meta_with_runtime(attrs, &crate::runtime())
}

pub(crate) fn extract_all_meta_with_runtime(
    attrs: &[Attribute],
    runtime: &TokenStream,
) -> Vec<TokenStream> {
    attrs
        .iter()
        .map(|attr| parse_meta_to_attribute_meta(&attr.meta, runtime))
        .collect()
}

pub(crate) fn extract_docs(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|attr| match &attr.meta {
            Meta::NameValue(value) if value.path.is_ident("doc") => match &value.value {
                Expr::Lit(expr) => match &expr.lit {
                    Lit::Str(value) => Some(value.value()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .collect()
}

pub fn member(value: TokenStream) -> TokenStream {
    if cfg!(feature = "meta") {
        quote!(metadata: #value,)
    } else {
        quote!()
    }
}
