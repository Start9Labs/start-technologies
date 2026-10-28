use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;
use syn::{Attribute, Data, DeriveInput, Expr, Fields, GenericParam, Lit, Meta, Path, Token, Type};

use crate::attrs::{children, metadata_path};

fn segments(path: &Path) -> Vec<String> {
    path.segments.iter().map(|s| s.ident.to_string()).collect()
}

#[derive(Default)]
struct Controls {
    selectors: Vec<Vec<String>>,
    opaque: bool,
}

fn controls(attrs: &[Attribute]) -> syn::Result<Controls> {
    let mut result = Controls::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("visit")) {
        let Meta::List(list) = &attr.meta else {
            continue;
        };
        let items = match children(list) {
            Ok(items) => items,
            Err(error) => {
                if list.tokens.clone().into_iter().any(|token| {
                    matches!(token, proc_macro2::TokenTree::Ident(i) if i == "type_attributes" || i == "opaque")
                }) {
                    return Err(error);
                }
                continue;
            }
        };
        for item in items {
            match item {
                Meta::Path(p) if p.is_ident("opaque") => result.opaque = true,
                Meta::List(l) if l.path.is_ident("type_attributes") => {
                    let paths = l.parse_args_with(|input: syn::parse::ParseStream| {
                        Punctuated::<Path, Token![,]>::parse_terminated_with(input, metadata_path)
                    })?;
                    result.selectors.extend(paths.iter().map(segments));
                }
                item if item.path().is_ident("type_attributes")
                    || item.path().is_ident("opaque") =>
                {
                    return Err(syn::Error::new_spanned(
                        item,
                        "expected opaque or type_attributes(path, ...)",
                    ));
                }
                _ => {}
            }
        }
    }
    Ok(result)
}

fn optional_index(index: Option<usize>) -> TokenStream {
    match index {
        Some(index) => quote!(::core::option::Option::Some(#index)),
        None => quote!(::core::option::Option::None),
    }
}

fn attributes(attrs: &[Attribute]) -> TokenStream {
    let tokens = attrs.iter().map(|a| a.to_token_stream().to_string());
    quote!(&[#(#tokens),*])
}

fn fields_kind(fields: &Fields, runtime: &TokenStream) -> TokenStream {
    match fields {
        Fields::Unit => quote!(#runtime::reflection::FieldsKind::Unit),
        Fields::Unnamed(_) => quote!(#runtime::reflection::FieldsKind::Tuple),
        Fields::Named(_) => quote!(#runtime::reflection::FieldsKind::Named),
    }
}

enum Callback {
    TypeAttribute(TokenStream, Type),
}

impl Callback {
    fn tokens(self, runtime: &TokenStream, visitor: &syn::Ident) -> TokenStream {
        let Self::TypeAttribute(info, ty) = self;
        quote!(#runtime::Visit::visit(&#runtime::TypeAttribute { info: #info, marker: #runtime::Static::<#ty>::new() }, #visitor))
    }
}

struct Emission {
    runtime: TokenStream,
    calls: Vec<Callback>,
    bounds: Vec<Type>,
}

impl Emission {
    fn attributes(
        &mut self,
        attrs: &[Attribute],
        variant: Option<usize>,
        field: Option<usize>,
    ) -> syn::Result<Controls> {
        let controls = controls(attrs)?;
        for (attribute, attr) in attrs.iter().enumerate() {
            let mut occurrence = 0;
            self.walk(
                &attr.meta,
                Vec::new(),
                &controls.selectors,
                variant,
                field,
                attribute,
                &mut occurrence,
            )?;
        }
        Ok(controls)
    }

    fn walk(
        &mut self,
        item: &Meta,
        prefix: Vec<String>,
        selectors: &[Vec<String>],
        variant: Option<usize>,
        field: Option<usize>,
        attribute: usize,
        occurrence: &mut usize,
    ) -> syn::Result<()> {
        let mut full = prefix;
        full.extend(segments(item.path()));
        if selectors.contains(&full) {
            let literal = match item {
                Meta::NameValue(nv) => match &nv.value {
                    Expr::Lit(expr) => match &expr.lit {
                        Lit::Str(literal) => Some(literal),
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            }
            .ok_or_else(|| {
                syn::Error::new_spanned(item, "selected type metadata must be a string literal")
            })?;
            let ty = literal.parse::<Type>().map_err(|e| {
                syn::Error::new_spanned(
                    literal,
                    format!("selected metadata is not Rust type syntax: {e}"),
                )
            })?;
            let literal_tokens = literal.to_token_stream().to_string();
            let value = literal.value();
            let ordinal = *occurrence;
            let variant = optional_index(variant);
            let field = optional_index(field);
            let runtime = &self.runtime;
            self.bounds.push(ty.clone());
            self.calls.push(Callback::TypeAttribute(quote! {
                #runtime::reflection::TypeAttributeInfo {
                    position: #runtime::reflection::MetadataPosition { variant: #variant, field: #field, attribute: #attribute, occurrence: #ordinal },
                    path: &[#(#full),*], literal_tokens: #literal_tokens, value: #value,
                }
            }, ty));
        }
        *occurrence += 1;
        if let Meta::List(list) = item {
            match children(list) {
                Ok(items) => {
                    for child in items {
                        self.walk(
                            &child,
                            full.clone(),
                            selectors,
                            variant,
                            field,
                            attribute,
                            occurrence,
                        )?;
                    }
                }
                Err(error) if selectors.iter().any(|s| s.starts_with(&full)) => return Err(error),
                Err(_) => {}
            }
        }
        Ok(())
    }

    fn fields<'a>(
        &mut self,
        fields: impl IntoIterator<Item = &'a syn::Field>,
        variant: Option<usize>,
    ) -> syn::Result<Vec<TokenStream>> {
        let mut infos = Vec::new();
        let mut next_visit = 0;
        for (index, field) in fields.into_iter().enumerate() {
            self.attributes(&field.attrs, variant, Some(index))?;
            let runtime = &self.runtime;
            let ty = &field.ty;
            let syntax = ty.to_token_stream().to_string();
            let name = match &field.ident {
                Some(name) => {
                    let name = name.to_string();
                    quote!(::core::option::Option::Some(#name))
                }
                None => quote!(::core::option::Option::None),
            };
            let attrs = attributes(&field.attrs);
            let variant = optional_index(variant);
            let visit_index = if crate::helpers::is_skipped(field) {
                optional_index(None)
            } else {
                let index = next_visit;
                next_visit += 1;
                optional_index(Some(index))
            };
            let info = quote!(#runtime::reflection::FieldInfo {
                position: #runtime::reflection::Position { variant: #variant, field: #index },
                visit_index: #visit_index,
                name: #name, type_syntax: #syntax, attributes: #attrs,
            });
            infos.push(info.clone());
        }
        Ok(infos)
    }
}

fn collect(input: &DeriveInput) -> syn::Result<(TokenStream, Emission)> {
    let runtime = match proc_macro_crate::crate_name("visit-rs")
        .map_err(|e| syn::Error::new_spanned(&input.ident, e))?
    {
        proc_macro_crate::FoundCrate::Itself => quote!(::visit_rs),
        proc_macro_crate::FoundCrate::Name(name) => {
            let ident = syn::Ident::new(&name, Span::call_site());
            quote!(::#ident)
        }
    };
    let mut emission = Emission {
        runtime: runtime.clone(),
        calls: Vec::new(),
        bounds: Vec::new(),
    };
    emission.attributes(&input.attrs, None, None)?;
    let mut fields = Vec::new();
    let mut variants = Vec::new();
    let kind = match &input.data {
        Data::Struct(data) => {
            fields = emission.fields(&data.fields, None)?;
            let kind = fields_kind(&data.fields, &runtime);
            quote!(#runtime::reflection::DeclarationKind::Struct(#kind))
        }
        Data::Enum(data) => {
            for (index, variant) in data.variants.iter().enumerate() {
                emission.attributes(&variant.attrs, Some(index), None)?;
                let fields = emission.fields(&variant.fields, Some(index))?;
                let name = variant.ident.to_string();
                let kind = fields_kind(&variant.fields, &runtime);
                let attrs = attributes(&variant.attrs);
                variants.push(quote!(#runtime::reflection::VariantInfo {
                    index: #index, name: #name, fields_kind: #kind, attributes: #attrs, fields: &[#(#fields),*],
                }));
            }
            quote!(#runtime::reflection::DeclarationKind::Enum)
        }
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                input,
                "union traversal is unsupported",
            ));
        }
    };
    let parameters = input.generics.params.iter().map(|p| match p {
        GenericParam::Lifetime(p) => {
            let name = p.lifetime.to_token_stream().to_string();
            quote!(#runtime::reflection::GenericParameter::Lifetime { name: #name })
        }
        GenericParam::Type(p) => {
            let name = p.ident.to_string();
            quote!(#runtime::reflection::GenericParameter::Type { name: #name })
        }
        GenericParam::Const(p) => {
            let name = p.ident.to_string();
            let ty = p.ty.to_token_stream().to_string();
            quote!(#runtime::reflection::GenericParameter::Const { name: #name, ty: #ty })
        }
    });
    let ident = &input.ident;
    let name = ident.to_string();
    let source = input.to_token_stream().to_string();
    let attrs = attributes(&input.attrs);
    let metadata = crate::attrs::extract_all_meta_with_runtime(&input.attrs, &runtime);
    let docs = crate::attrs::extract_docs(&input.attrs);
    let (original_impl, type_generics, original_where) = input.generics.split_for_impl();
    let category = match input.data {
        Data::Struct(_) => quote!(#runtime::reflection::StructKind),
        Data::Enum(_) => quote!(#runtime::reflection::EnumKind),
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                input,
                "union traversal is unsupported",
            ));
        }
    };
    let facts = quote! {
        impl #original_impl #runtime::TypeInfo for #ident #type_generics #original_where {
            type Kind = #category;
            const DECLARATION: #runtime::reflection::DeclarationInfo = #runtime::reflection::DeclarationInfo {
                name: #name, module: ::core::module_path!(), kind: #kind, source: #source,
                parameters: &[#(#parameters),*], attributes: #attrs, metadata: &[#(#metadata),*], docs: &[#(#docs),*], fields: &[#(#fields),*], variants: &[#(#variants),*],
            };
        }
    };
    Ok((facts, emission))
}

pub(crate) fn derive(input: &DeriveInput) -> syn::Result<TokenStream> {
    let (facts, emission) = collect(input)?;
    let runtime = emission.runtime.clone();
    let selected = &emission.bounds;
    let visitor_type = crate::fresh_identifier(quote!(#input #(#selected)*), "__visit_rs_Visitor");
    let ident = &input.ident;
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let mut generics = input.generics.clone();
    generics
        .params
        .push(syn::parse_quote!(#visitor_type: #runtime::Visitor));
    let (bounds, method) = metadata_emission(&visitor_type, emission);
    generics.make_where_clause().predicates.extend(bounds);
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    Ok(quote! {
        #facts
        impl #impl_generics #runtime::VisitTypeAttributes<#visitor_type> for #ident #ty_generics #where_clause {
            #method
        }
    })
}

fn metadata_emission(
    visitor_type: &syn::Ident,
    emission: Emission,
) -> (Vec<syn::WherePredicate>, TokenStream) {
    let runtime = &emission.runtime;
    let visitor = syn::Ident::new("visitor", Span::call_site());
    let bounds = emission
        .bounds
        .iter()
        .flat_map(|ty| {
            [
                syn::parse_quote!(#runtime::TypeAttribute<#ty>: #runtime::Visit<#visitor_type>),
                syn::parse_quote!(#runtime::Static<#ty>: #runtime::Visit<#visitor_type>),
            ]
        })
        .collect();
    let calls = emission.calls.into_iter().enumerate().map(|(index, call)| {
        let call = call.tokens(runtime, &visitor);
        quote!(#index => Some(#call),)
    });
    (
        bounds,
        quote! {
            fn visit_type_attributes<'__visit_rs__a>(visitor: &'__visit_rs__a mut #visitor_type) -> impl Iterator<Item = <#visitor_type as #runtime::Visitor>::Result> + '__visit_rs__a {
                let mut position = 0;
                std::iter::from_fn(move || {
                    let current = position;
                    position += 1;
                    match current { #(#calls)* _ => None }
                })
            }
        },
    )
}

pub(crate) fn static_input(input: &DeriveInput) -> syn::Result<DeriveInput> {
    let mut result = input.clone();
    let runtime = crate::runtime();
    let opaque = controls(&input.attrs)?.opaque;
    let apply = |fields: &mut Fields, parent: bool| -> syn::Result<()> {
        for field in fields {
            if storage_is_opaque(parent, field)? {
                let ty = &field.ty;
                field.ty = syn::parse_quote!(#runtime::Opaque<#ty>);
            }
        }
        Ok(())
    };
    match &mut result.data {
        Data::Struct(data) => apply(&mut data.fields, opaque)?,
        Data::Enum(data) => {
            for variant in &mut data.variants {
                let parent = variant_is_opaque(input, variant)?;
                apply(&mut variant.fields, parent)?;
            }
        }
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "union traversal is unsupported",
            ));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selected(input: DeriveInput) -> syn::Result<Controls> {
        Emission {
            runtime: quote!(::visit_rs),
            calls: Vec::new(),
            bounds: Vec::new(),
        }
        .attributes(&input.attrs, None, None)
    }

    #[test]
    fn selected_non_string_is_rejected() {
        let error = selected(syn::parse_quote!(
            #[visit(target = 42, type_attributes(visit::target))]
            struct Example;
        ))
        .err()
        .unwrap();
        assert!(
            error
                .to_string()
                .contains("selected type metadata must be a string literal")
        );
    }

    #[test]
    fn selected_invalid_type_is_rejected() {
        let error = selected(syn::parse_quote!(
            #[visit(target = "not Rust !", type_attributes(visit::target))]
            struct Example;
        ))
        .err()
        .unwrap();
        assert!(
            error
                .to_string()
                .contains("selected metadata is not Rust type syntax")
        );
    }

    #[test]
    fn malformed_selected_descendants_are_rejected() {
        let error = selected(syn::parse_quote!(
            #[visit(target(@ raw), type_attributes(visit::target::type))]
            struct Example;
        ))
        .err()
        .unwrap();
        assert_eq!(error.to_string(), "expected ident");
    }

    #[test]
    fn malformed_selector_is_rejected() {
        assert!(
            selected(syn::parse_quote!(
                #[visit(type_attributes(visit::target = "u32"))]
                struct Example;
            ))
            .is_err()
        );
    }

    #[test]
    fn unresolved_unselected_text_and_unparsed_attributes_are_accepted() {
        assert!(
            selected(syn::parse_quote!(
                #[visit(target = "Unresolved !", unknown(@ raw))]
                #[serde(tag = "x", untagged, strange(option = 12))]
                struct Example;
            ))
            .is_ok()
        );
    }
}

pub(crate) fn storage_is_opaque(parent: bool, field: &syn::Field) -> syn::Result<bool> {
    Ok(parent || controls(&field.attrs)?.opaque)
}

pub(crate) fn root_is_opaque(ast: &DeriveInput) -> syn::Result<bool> {
    Ok(controls(&ast.attrs)?.opaque)
}

pub(crate) fn variant_is_opaque(ast: &DeriveInput, variant: &syn::Variant) -> syn::Result<bool> {
    Ok(root_is_opaque(ast)? || controls(&variant.attrs)?.opaque)
}

pub(crate) fn storage_marker(ty: &Type) -> TokenStream {
    let runtime = crate::runtime();
    let Type::Path(path) = ty else { unreachable!() };
    let syn::PathArguments::AngleBracketed(arguments) =
        &path.path.segments.last().unwrap().arguments
    else {
        unreachable!()
    };
    let original = arguments.args.first().unwrap();
    quote!(#runtime::Opaque::<#original>(#runtime::Static::new()))
}
