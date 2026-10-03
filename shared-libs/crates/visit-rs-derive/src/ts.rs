use std::collections::BTreeMap;

use proc_macro2::{TokenStream, TokenTree};
use quote::{ToTokens, format_ident, quote};
use syn::{Attribute, Data, DeriveInput, GenericParam, Ident, LitStr, Token, Type, parse_quote};

#[derive(Default)]
struct Config {
    literal: Option<LitStr>,
    wire: Option<Type>,
    name: Option<LitStr>,
    input_name: Option<LitStr>,
    export: bool,
    skip: bool,
    namespaces: Vec<LitStr>,
    concrete: BTreeMap<String, Type>,
}

impl Config {
    fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut config = Self::default();
        for attr in attrs.iter().filter(|attr| attr.path().is_ident("ts")) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("type") {
                    config.literal = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("wire") {
                    config.wire = Some(meta.value()?.parse::<LitStr>()?.parse()?);
                } else if meta.path.is_ident("rename") {
                    config.name = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("input_rename") {
                    config.input_name = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("skip") {
                    config.skip = true;
                } else if meta.path.is_ident("export") {
                    config.export = true;
                } else if meta.path.is_ident("namespace") {
                    let value = meta.value()?;
                    if value.peek(syn::token::Bracket) {
                        let content;
                        syn::bracketed!(content in value);
                        config.namespaces.extend(
                            content.parse_terminated(|input| input.parse::<LitStr>(), Token![,])?,
                        );
                    } else {
                        config.namespaces.push(value.parse()?);
                    }
                } else if meta.path.is_ident("concrete") {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    while !content.is_empty() {
                        let name: Ident = content.parse()?;
                        content.parse::<Token![=]>()?;
                        config.concrete.insert(name.to_string(), content.parse()?);
                        if !content.is_empty() {
                            content.parse::<Token![,]>()?;
                        }
                    }
                } else {
                    return Err(meta.error("unsupported TypeScript attribute"));
                }
                Ok(())
            })?;
        }
        if config.literal.is_some() && config.wire.is_some() {
            return Err(syn::Error::new_spanned(
                &attrs[0],
                "type and wire are mutually exclusive",
            ));
        }
        Ok(config)
    }
}

fn contains_ident(tokens: TokenStream, name: &str) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => ident == name,
        TokenTree::Group(group) => contains_ident(group.stream(), name),
        _ => false,
    })
}

fn concrete_type(ast: &DeriveInput, config: &Config) -> syn::Result<Type> {
    let name = &ast.ident;
    let mut args = Vec::new();
    for param in &ast.generics.params {
        args.push(match param {
            GenericParam::Lifetime(_) => quote!('static),
            GenericParam::Type(param) => {
                let ty = config
                    .concrete
                    .get(&param.ident.to_string())
                    .or(param.default.as_ref().map(|(_, ty)| ty))
                    .ok_or_else(|| {
                        syn::Error::new_spanned(param, "export requires a concrete type argument")
                    })?;
                quote!(#ty)
            }
            GenericParam::Const(param) => {
                let value = param
                    .default
                    .as_ref()
                    .map(|(_, value)| value)
                    .ok_or_else(|| {
                        syn::Error::new_spanned(param, "export requires a default const argument")
                    })?;
                quote!(#value)
            }
        });
    }
    syn::parse2(if args.is_empty() {
        quote!(#name)
    } else {
        quote!(#name<#(#args),*>)
    })
}

pub fn derive(ast: &DeriveInput) -> syn::Result<TokenStream> {
    let config = Config::parse(&ast.attrs)?;
    let name = &ast.ident;
    let declaration = config
        .name
        .clone()
        .unwrap_or_else(|| LitStr::new(&name.to_string(), name.span()));
    let mut generics = ast.generics.clone();
    let (_, ty_generics, _) = ast.generics.split_for_impl();
    let mut markers = Vec::new();
    let mut shape_ast = ast.clone();
    let mut marker_index = 0usize;
    let mut rewrite = |fields: &mut syn::Fields| -> syn::Result<()> {
        if let syn::Fields::Named(fields) = fields {
            fields.named = std::mem::take(&mut fields.named)
                .into_iter()
                .filter_map(|field| match Config::parse(&field.attrs) {
                    Ok(config) if config.skip => None,
                    _ => Some(field),
                })
                .collect();
        }
        for field in fields {
            let config = Config::parse(&field.attrs)?;
            if config.skip {
                return Err(syn::Error::new_spanned(
                    field,
                    "skip requires a named field; use a tuple type override",
                ));
            }
            let wire = if let Some(literal) = config.literal {
                let marker = format_ident!("__VisitTsWire{}{}", name, marker_index);
                marker_index += 1;
                let option = match &field.ty {
                    Type::Path(path) => path
                        .path
                        .segments
                        .last()
                        .is_some_and(|segment| segment.ident == "Option"),
                    _ => false,
                };
                markers.push(quote! {
                    struct #marker;
                    impl visit_rs::ts::TS for #marker {
                        const IS_OPTION: bool = #option;
                        fn visit_ts(visitor: &mut visit_rs::ts::TSVisitor) {
                            visitor.ts.push_str(#literal);
                        }
                    }
                });
                Some(parse_quote!(#marker))
            } else {
                config.wire
            };
            if let Some(wire) = wire {
                let wire = LitStr::new(&wire.to_token_stream().to_string(), name.span());
                field.attrs.push(parse_quote!(#[visit(wire = #wire)]));
            }
        }
        Ok(())
    };
    let body = if let Some(literal) = &config.literal {
        quote!(visitor.ts.push_str(#literal);)
    } else if let Some(wire) = &config.wire {
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#wire: visit_rs::ts::TS));
        quote!(visitor.append_type::<#wire>();)
    } else {
        match &mut shape_ast.data {
            Data::Struct(data) => rewrite(&mut data.fields)?,
            Data::Enum(data) => {
                for variant in &mut data.variants {
                    rewrite(&mut variant.fields)?;
                }
            }
            Data::Union(data) => {
                return Err(syn::Error::new_spanned(
                    data.union_token,
                    "expected a struct or enum",
                ));
            }
        }
        let (input, mut types) = crate::shape::direction(&shape_ast, true)?;
        let (output, output_types) = crate::shape::direction(&shape_ast, false)?;
        types.extend(output_types);
        let generic_names: Vec<_> = ast
            .generics
            .type_params()
            .map(|param| param.ident.to_string())
            .collect();
        for ty in types {
            let tokens = ty.to_token_stream();
            if !contains_ident(tokens.clone(), &name.to_string())
                && generic_names
                    .iter()
                    .any(|name| contains_ident(tokens.clone(), name))
            {
                let bound = parse_quote!(#ty: visit_rs::ts::TS);
                if !generics
                    .make_where_clause()
                    .predicates
                    .iter()
                    .any(|existing| existing == &bound)
                {
                    generics.make_where_clause().predicates.push(bound);
                }
            }
        }
        quote! {
            use visit_rs::shape::ShapeVisitor;
            match visitor.direction() {
                visit_rs::shape::Direction::Input => { #input }
                visit_rs::shape::Direction::Output => { #output }
            }
        }
    };
    let definition = if ast.generics.type_params().next().is_none()
        && ast.generics.const_params().next().is_none()
    {
        quote!(const DEFINE: Option<&'static str> = Some(#declaration);)
    } else {
        quote!()
    };
    let input_definition = config
        .input_name
        .as_ref()
        .map(|name| quote!(const INPUT_DEFINE: Option<&'static str> = Some(#name);));
    let export = if config.export {
        let concrete = concrete_type(ast, &config)?;
        let namespaces = if config.namespaces.is_empty() {
            vec![LitStr::new("", name.span())]
        } else {
            config.namespaces
        };
        quote! {
            #(visit_rs::ts::inventory::submit! {
                visit_rs::ts::Export {
                    module: module_path!(),
                    namespace: #namespaces,
                    name: #declaration,
                    register: |visitor| visitor.declare::<#concrete>(#declaration),
                }
            })*
        }
    } else {
        quote!()
    };
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    Ok(quote! {
        #(#markers)*
        impl #impl_generics visit_rs::ts::TS for #name #ty_generics #where_clause {
            #definition
            #input_definition
            fn visit_ts(visitor: &mut visit_rs::ts::TSVisitor) { #body }
        }
        #export
    })
}
