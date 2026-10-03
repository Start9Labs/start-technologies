use proc_macro2::TokenStream;
use quote::quote;
use serde_derive_internals::{Ctxt, Derive, ast, attr};
use syn::{DeriveInput, parse_quote};

fn style(style: ast::Style) -> TokenStream {
    let name = match style {
        ast::Style::Struct => quote!(Struct),
        ast::Style::Tuple => quote!(Tuple),
        ast::Style::Newtype => quote!(Newtype),
        ast::Style::Unit => quote!(Unit),
    };
    quote!(visit_rs::shape::Style::#name)
}

fn fields(
    fields: &[ast::Field],
    input: bool,
    transparent: bool,
    default: bool,
    style: ast::Style,
) -> (TokenStream, Vec<syn::Type>) {
    let mut types = Vec::new();
    let visits: Vec<_> = fields.iter().filter_map(|field| {
        let attrs = &field.attrs;
        if (!matches!(style, ast::Style::Newtype) && ((input && attrs.skip_deserializing()) || (!input && attrs.skip_serializing())))
            || (transparent && !attrs.transparent()) {
            return None;
        }
        let wire_type = match wire_type(&field.original.attrs, input) {
            Ok(wire_type) => wire_type,
            Err(error) => return Some(error.to_compile_error()),
        };
        let ty = wire_type.as_ref().unwrap_or(field.ty);
        let name = if input { attrs.name().deserialize_name() } else { attrs.name().serialize_name() };
        let name = &name.value;
        let aliases: Vec<_> = if input { attrs.aliases().iter().map(|n| &n.value).collect() } else { vec![] };
        let optional = if input { default || !attrs.default().is_none() } else { attrs.skip_serializing_if().is_some() };
        let flatten = attrs.flatten();
        let option_default = attrs.deserialize_with().is_none();
        let custom = if input { attrs.deserialize_with().is_some() } else { attrs.serialize_with().is_some() };
        if custom && wire_type.is_none() {
            return Some(quote!(visitor.unsupported(concat!("Custom serde field ", #name, " requires a wire type override"));));
        }
        types.push(ty.clone());
        Some(quote! {
            visitor.field::<#ty>(visit_rs::shape::Field {
                name: #name, aliases: &[#(#aliases),*], optional: #optional,
                option_default: #option_default, flatten: #flatten,
            });
        })
    }).collect();
    (quote!(#(#visits)*), types)
}

fn wire_type(attrs: &[syn::Attribute], input: bool) -> syn::Result<Option<syn::Type>> {
    use syn::punctuated::Punctuated;
    use syn::{Expr, Lit, Meta, Token};

    let mut common = None;
    let mut directional = None;
    let key = if input { "input_wire" } else { "output_wire" };
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("visit")) {
        let items = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
        for item in items {
            if let Meta::NameValue(value) = item
                && (value.path.is_ident("wire") || value.path.is_ident(key))
            {
                let ty = if let Expr::Lit(literal) = &value.value
                    && let Lit::Str(literal) = &literal.lit
                {
                    literal.parse()?
                } else {
                    return Err(syn::Error::new_spanned(
                        value,
                        "expected a wire type string",
                    ));
                };
                let target = if value.path.is_ident(key) {
                    &mut directional
                } else {
                    &mut common
                };
                if target.replace(ty).is_some() {
                    return Err(syn::Error::new_spanned(
                        value,
                        "duplicate wire type override",
                    ));
                }
            }
        }
    }
    Ok(directional.or(common))
}

fn tag(tag: &attr::TagType) -> TokenStream {
    match tag {
        attr::TagType::External => quote!(visit_rs::shape::Tag::External),
        attr::TagType::Internal { tag } => quote!(visit_rs::shape::Tag::Internal(#tag)),
        attr::TagType::Adjacent { tag, content } => {
            quote!(visit_rs::shape::Tag::Adjacent(#tag, #content))
        }
        attr::TagType::None => quote!(visit_rs::shape::Tag::Untagged),
    }
}

pub(crate) fn direction(
    ast: &DeriveInput,
    input: bool,
) -> syn::Result<(TokenStream, Vec<syn::Type>)> {
    if let Some(ty) = wire_type(&ast.attrs, input)? {
        return Ok((
            quote!(visit_rs::Visit::visit(visit_rs::Static::<#ty>::new_ref(), visitor);),
            vec![ty],
        ));
    }
    let cx = Ctxt::new();
    let container = ast::Container::from_ast(
        &cx,
        ast,
        if input {
            Derive::Deserialize
        } else {
            Derive::Serialize
        },
        &parse_quote!(__serde),
    );
    cx.check()?;
    let container =
        container.ok_or_else(|| syn::Error::new_spanned(ast, "expected a struct or enum"))?;
    if (input && !matches!(container.attrs.identifier(), attr::Identifier::No))
        || container.attrs.remote().is_some()
    {
        return Ok((
            quote!(visitor.unsupported("Serde identifier and remote derives require an explicit shape override");),
            vec![],
        ));
    }
    if matches!(container.data, ast::Data::Struct(..))
        && !matches!(container.attrs.tag(), attr::TagType::External)
    {
        return Ok((
            quote!(visitor.unsupported("Tagged structs require an explicit shape override");),
            vec![],
        ));
    }
    let converted = if input {
        container
            .attrs
            .type_from()
            .or(container.attrs.type_try_from())
    } else {
        container.attrs.type_into()
    };
    if let Some(ty) = converted {
        return Ok((
            quote!(visit_rs::Visit::visit(visit_rs::Static::<#ty>::new_ref(), visitor);),
            vec![ty.clone()],
        ));
    }
    let mut types = Vec::new();
    let body = match &container.data {
        ast::Data::Struct(struct_style, fs) => {
            let transparent = container.attrs.transparent();
            let s = style(if transparent {
                ast::Style::Newtype
            } else {
                *struct_style
            });
            let (fs, field_types) = fields(
                fs,
                input,
                transparent,
                !container.attrs.default().is_none(),
                *struct_style,
            );
            types.extend(field_types);
            quote!(visitor.structure(#s, |visitor| { #fs });)
        }
        ast::Data::Enum(variants) => {
            let visits: Vec<_> = variants
                .iter()
                .filter_map(|variant| {
                    let attrs = &variant.attrs;
                    if (input && attrs.skip_deserializing()) || (!input && attrs.skip_serializing())
                    {
                        return None;
                    }
                    let name = if input {
                        attrs.name().deserialize_name()
                    } else {
                        attrs.name().serialize_name()
                    };
                    let name = &name.value;
                    let aliases: Vec<_> = if input {
                        attrs.aliases().iter().map(|n| &n.value).collect()
                    } else {
                        vec![]
                    };
                    let variant_style = if matches!(variant.style, ast::Style::Newtype)
                        && ((input && variant.fields[0].attrs.skip_deserializing())
                            || (!input && variant.fields[0].attrs.skip_serializing()))
                    {
                        ast::Style::Unit
                    } else {
                        variant.style
                    };
                    let s = style(variant_style);
                    let tag = if attrs.untagged() {
                        quote!(visit_rs::shape::Tag::Untagged)
                    } else {
                        tag(container.attrs.tag())
                    };
                    let custom = if input {
                        attrs.deserialize_with().is_some()
                    } else {
                        attrs.serialize_with().is_some()
                    };
                    let other = input && attrs.other();
                    let fs = if custom {
                        quote!()
                    } else {
                        let (fs, field_types) =
                            fields(&variant.fields, input, false, false, variant_style);
                        types.extend(field_types);
                        fs
                    };
                    Some(quote! {
                        visitor.variant(visit_rs::shape::Variant {
                            name: #name, aliases: &[#(#aliases),*], style: #s, tag: #tag,
                            custom: #custom, other: #other,
                        }, |visitor| { #fs });
                    })
                })
                .collect();
            quote!(visitor.enumeration(|visitor| { #(#visits)* });)
        }
    };
    Ok((body, types))
}

pub fn derive(ast: &DeriveInput) -> syn::Result<TokenStream> {
    let (input, mut types) = direction(ast, true)?;
    let (output, output_types) = direction(ast, false)?;
    types.extend(output_types);
    let name = &ast.ident;
    let (_, ty_generics, _) = ast.generics.split_for_impl();
    let mut generics = ast.generics.clone();
    generics
        .params
        .push(parse_quote!(__visit_rs__V: visit_rs::shape::ShapeVisitor));
    for ty in types {
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(visit_rs::Static<#ty>: visit_rs::Visit<__visit_rs__V>));
    }
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics visit_rs::shape::SerdeShape<__visit_rs__V> for #name #ty_generics #where_clause {
            fn visit_shape(visitor: &mut __visit_rs__V, direction: visit_rs::shape::Direction) {
                match direction {
                    visit_rs::shape::Direction::Input => { #input }
                    visit_rs::shape::Direction::Output => { #output }
                }
            }
        }
    })
}
