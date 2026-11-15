use std::collections::BTreeMap;

use heck::*;
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::{Comma, Paren, Pub};
use syn::{
    Attribute, Data, DataEnum, DataStruct, DeriveInput, Error, Fields, GenericArgument, Ident, Lit,
    LitInt, LitStr, Meta, MetaNameValue, Path, PathArguments, Type, TypePath, VisRestricted,
    Visibility,
};

pub fn build_model(item: &DeriveInput) -> TokenStream {
    let res = match &item.data {
        Data::Struct(struct_ast) => build_model_struct(item, struct_ast),
        Data::Enum(enum_ast) => build_model_enum(item, enum_ast),
        _ => panic!("Models can only be created for Structs and Enums"),
    };
    if let Some(dbg) = item.attrs.iter().find(|a| a.path.is_ident("macro_debug")) {
        return Error::new_spanned(dbg, format!("{}", res)).to_compile_error();
    } else {
        res
    }
}

fn serde_meta(attrs: &[Attribute]) -> impl Iterator<Item = Meta> + '_ {
    attrs
        .iter()
        .filter(|attr| attr.path.is_ident("serde"))
        .filter_map(|attr| {
            attr.parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)
                .ok()
        })
        .flatten()
}

fn serde_name_values(attrs: &[Attribute]) -> impl Iterator<Item = MetaNameValue> + '_ {
    serde_meta(attrs).filter_map(|meta| match meta {
        Meta::NameValue(value) => Some(value),
        _ => None,
    })
}

fn serde_flag(attrs: &[Attribute], name: &str) -> Option<Path> {
    serde_meta(attrs).find_map(|meta| match meta {
        Meta::Path(path) if path.is_ident(name) => Some(path),
        _ => None,
    })
}

fn serde_string(attrs: &[Attribute], name: &str) -> Option<LitStr> {
    serde_name_values(attrs)
        .filter(|value| value.path.is_ident(name))
        .find_map(|value| match value.lit {
            Lit::Str(value) => Some(value),
            _ => None,
        })
}

fn get_accessor(serde_rename_all: &Option<String>, attrs: &[Attribute], ident: &Ident) -> LitStr {
    if let Some(serde_rename) = serde_string(attrs, "rename") {
        return serde_rename;
    }
    let ident_string = ident.to_string();
    let ident_str = ident_string.as_str();
    match serde_rename_all.as_deref() {
        Some("lowercase") => LitStr::new(
            &ident_str.to_lower_camel_case().to_lowercase(),
            ident.span(),
        ),
        Some("UPPERCASE") => LitStr::new(
            &ident_str.to_lower_camel_case().to_uppercase(),
            ident.span(),
        ),
        Some("PascalCase") => LitStr::new(&ident_str.to_pascal_case(), ident.span()),
        Some("camelCase") => LitStr::new(&ident_str.to_lower_camel_case(), ident.span()),
        Some("SCREAMING_SNAKE_CASE") => {
            LitStr::new(&ident_str.to_shouty_snake_case(), ident.span())
        }
        Some("kebab-case") => LitStr::new(&ident_str.to_kebab_case(), ident.span()),
        Some("SCREAMING-KEBAB-CASE") => {
            LitStr::new(&ident_str.to_shouty_kebab_case(), ident.span())
        }
        _ => LitStr::new(&ident.to_string(), ident.span()),
    }
}

struct ChildInfo {
    vis: Visibility,
    name: Ident,
    accessor: Option<Lit>,
    ty: Type,
}
impl ChildInfo {
    fn from_fields(serde_rename_all: &Option<String>, fields: &Fields) -> Vec<Self> {
        let mut children = Vec::new();
        match fields {
            Fields::Named(f) => {
                for field in &f.named {
                    let ident = field.ident.clone().unwrap();
                    let ty = field.ty.clone();
                    let accessor = if serde_flag(&field.attrs, "flatten").is_some() {
                        None
                    } else {
                        Some(Lit::Str(get_accessor(
                            serde_rename_all,
                            &field.attrs,
                            field.ident.as_ref().unwrap(),
                        )))
                    };
                    children.push(ChildInfo {
                        vis: field.vis.clone(),
                        name: ident,
                        accessor,
                        ty,
                    })
                }
            }
            Fields::Unnamed(f) => {
                for (i, field) in f.unnamed.iter().enumerate() {
                    let ident = Ident::new(&format!("idx_{i}"), field.span());
                    let ty = field.ty.clone();
                    let accessor = if f.unnamed.len() > 1 {
                        Some(Lit::Int(LitInt::new(
                            &format!("{}", i),
                            proc_macro2::Span::call_site(),
                        )))
                    } else {
                        None // newtype wrapper
                    };
                    children.push(ChildInfo {
                        vis: field.vis.clone(),
                        name: ident,
                        accessor,
                        ty,
                    })
                }
            }
            Fields::Unit => (),
        }
        children
    }
}

struct Fns {
    from_parts: TokenStream,
    impl_fns: TokenStream,
    impl_ref_fns: TokenStream,
    impl_mut_fns: TokenStream,
    impl_mut_destructure: TokenStream,
}

fn impl_fns(model_ty: &Type, children: &[ChildInfo], name: &Ident) -> Fns {
    let mut parts_args = TokenStream::new();
    let mut parts_assignments = TokenStream::new();
    let mut impl_fns = TokenStream::new();
    let mut impl_ref_fns = TokenStream::new();
    let mut impl_mut_fns = TokenStream::new();
    let mut destructure_members = TokenStream::new();
    let mut init_destructure_members = TokenStream::new();
    let mut mkdestructure_members = TokenStream::new();
    let mut destructure_member_idents = TokenStream::new();
    for ChildInfo {
        vis,
        name,
        accessor,
        ty,
    } in children
    {
        let name_owned = Ident::new(&format!("into_{name}"), name.span());
        let name_ref = Ident::new(&format!("as_{name}"), name.span());
        let name_mut = Ident::new(&format!("as_{name}_mut"), name.span());
        let vis = match vis {
            Visibility::Inherited => Visibility::Restricted(VisRestricted {
                pub_token: Pub::default(),
                paren_token: Paren::default(),
                in_token: None,
                path: Box::new(Path::from(Ident::new("super", Span::call_site()))),
            }),
            Visibility::Restricted(VisRestricted {
                path: orig_path, ..
            }) if orig_path
                .segments
                .first()
                .map(|s| s.ident == "super")
                .unwrap_or(false) =>
            {
                Visibility::Restricted(VisRestricted {
                    pub_token: Pub::default(),
                    paren_token: Paren::default(),
                    in_token: None,
                    path: Box::new({
                        let mut path = Path::from(Ident::new("super", Span::call_site()));
                        path.segments.extend(orig_path.segments.iter().cloned());
                        path
                    }),
                })
            }
            a => a.clone(),
        };
        let accessor_owned = if let Some(accessor) = accessor {
            quote! {
                {
                    #[allow(unused_imports)]
                    use patch_db::value::index::Index;
                    #accessor.index_into_owned(v).unwrap_or_default()
                }
            }
        } else {
            quote! { v }
        };
        let accessor_ref = if let Some(accessor) = accessor {
            quote! {
                {
                    #[allow(unused_imports)]
                    use patch_db::value::index::Index;
                    #accessor.index_into(v).unwrap_or(&patch_db::value::NULL)
                }
            }
        } else {
            quote! { v }
        };
        let accessor_mut = if let Some(accessor) = accessor {
            quote! {
                {
                    #[allow(unused_imports)]
                    use patch_db::value::index::Index;
                    #accessor.index_or_insert(v)
                }
            }
        } else {
            quote! { v }
        };

        let child_model_ty = replace_self(model_ty.clone(), &ty);
        parts_args.extend(quote_spanned! { name.span() =>
            #name: #child_model_ty,
        });
        parts_assignments.extend(quote_spanned! { name.span() =>
            *#accessor_mut = #name.into_value();
        });
        impl_fns.extend(quote_spanned! { name.span() =>
            #vis fn #name_owned (self) -> #child_model_ty {
                use patch_db::ModelExt;
                self.transmute(|v| #accessor_owned)
            }
        });
        impl_ref_fns.extend(quote_spanned! { name.span() =>
            #vis fn #name_ref (&self) -> &#child_model_ty {
                use patch_db::ModelExt;
                self.transmute_ref(|v| #accessor_ref)
            }
        });
        impl_mut_fns.extend(quote_spanned! { name.span() =>
            #vis fn #name_mut (&mut self) -> &mut #child_model_ty {
                use patch_db::ModelExt;
                self.transmute_mut(|v| #accessor_mut)
            }
        });
        if let Some(accessor) = accessor {
            destructure_members.extend(quote_spanned! { name.span() =>
                #vis #name: &'a mut #child_model_ty,
            });
            init_destructure_members.extend(quote_spanned! { name.span() =>
                {
                    #[allow(unused_imports)]
                    use patch_db::value::index::Index;
                    #accessor.index_or_insert(self.as_value_mut())
                }
            });
            mkdestructure_members.extend(quote_spanned! { name.span() =>
                let #name = <#child_model_ty>::value_as_mut(__patch_db__destructure_map.remove(#accessor).unwrap());
            });
            destructure_member_idents.extend(quote_spanned! { name.span() => #name, });
        }
    }

    let mod_name = Ident::new(&format!("__patch_db__destructure_{name}"), name.span());
    let explicit_model_ty = replace_self(
        model_ty.clone(),
        &Type::Path(TypePath {
            qself: None,
            path: name.clone().into(),
        }),
    );

    Fns {
        from_parts: quote! {
            pub fn from_parts(#parts_args) -> Self {
                use patch_db::ModelExt;
                let mut res = patch_db::value::json!({});
                let v = &mut res;
                #parts_assignments
                Self::from_value(res)
            }
        },
        impl_fns,
        impl_ref_fns,
        impl_mut_fns,
        impl_mut_destructure: quote! {
            #[allow(non_snake_case)]
            mod #mod_name {
                use super::*;
                pub struct DestructuredMut<'a> {
                    __patch_db__destructure_phantom: std::marker::PhantomData<&'a ()>,
                    #destructure_members
                }
                impl patch_db::DestructureMut for #explicit_model_ty {
                    type Destructured<'a> = DestructuredMut<'a>;
                    fn destructure_mut<'a>(&'a mut self) -> Self::Destructured<'a> {
                        use patch_db::ModelExt;
                        let mut __patch_db__destructure_map: std::collections::BTreeMap<_, _> = self.children_mut().into_iter().collect();
                        #mkdestructure_members
                        DestructuredMut {
                            __patch_db__destructure_phantom: std::marker::PhantomData,
                            #destructure_member_idents
                        }
                    }
                }
            }
        },
    }
}

fn replace_self(ty: Type, replace: &Type) -> Type {
    match ty {
        Type::Path(mut a) => Type::Path({
            a.path.segments = a
                .path
                .segments
                .into_iter()
                .map(|mut s| {
                    s.arguments = match s.arguments {
                        PathArguments::AngleBracketed(mut a) => PathArguments::AngleBracketed({
                            a.args = a
                                .args
                                .into_iter()
                                .map(|a| match a {
                                    GenericArgument::Type(Type::Path(a)) => {
                                        GenericArgument::Type({
                                            if a.path.is_ident("Self") {
                                                replace.clone()
                                            } else {
                                                Type::Path(a)
                                            }
                                        })
                                    }
                                    a => a,
                                })
                                .collect();
                            a
                        }),
                        a => a,
                    };
                    s
                })
                .collect();
            a
        }),
        a => a,
    }
}

fn build_model_struct(base: &DeriveInput, ast: &DataStruct) -> TokenStream {
    let model_ty = match base
        .attrs
        .iter()
        .filter(|attr| attr.path.is_ident("model"))
        .filter_map(|attr| attr.parse_meta().ok())
        .find_map(|meta| {
            if let Meta::NameValue(a) = meta {
                Some(a)
            } else {
                None
            }
        })
        .ok_or_else(|| {
            Error::new(
                base.ident.span(),
                "could not determine model type\n#[model = \"...\"] is required",
            )
        })
        .and_then(|meta| {
            if let Lit::Str(a) = meta.lit {
                Ok(a)
            } else {
                Err(Error::new(
                    meta.lit.span(),
                    "syntax error: expected string literal",
                ))
            }
        })
        .and_then(|s| syn::parse_str::<Type>(&s.value()))
    {
        Ok(a) => a,
        Err(e) => return e.into_compile_error(),
    };
    let model_ty_name = replace_self(
        model_ty.clone(),
        &Type::Path(TypePath {
            qself: None,
            path: base.ident.clone().into(),
        }),
    );
    let serde_rename_all = serde_string(&base.attrs, "rename_all").map(|value| value.value());
    let children = ChildInfo::from_fields(&serde_rename_all, &ast.fields);
    let name = &base.ident;
    let Fns {
        from_parts,
        impl_fns,
        impl_ref_fns,
        impl_mut_fns,
        impl_mut_destructure,
    } = impl_fns(&model_ty, &children, name);
    quote! {
        impl patch_db::HasModel for #name {
            type Model = #model_ty;
        }
        impl #model_ty_name {
            #from_parts
            #impl_fns
            #impl_ref_fns
            #impl_mut_fns
        }
        #impl_mut_destructure
    }
}

fn build_model_enum(base: &DeriveInput, ast: &DataEnum) -> TokenStream {
    let model_ty = match base
        .attrs
        .iter()
        .filter(|attr| attr.path.is_ident("model"))
        .filter_map(|attr| attr.parse_meta().ok())
        .find_map(|meta| {
            if let Meta::NameValue(a) = meta {
                Some(a)
            } else {
                None
            }
        })
        .ok_or_else(|| {
            Error::new(
                base.ident.span(),
                "could not determine model type\n#[model = \"...\"] is required",
            )
        })
        .and_then(|meta| {
            if let Lit::Str(a) = meta.lit {
                Ok(a)
            } else {
                Err(Error::new(
                    meta.lit.span(),
                    "syntax error: expected string literal",
                ))
            }
        })
        .and_then(|s| syn::parse_str::<Type>(&s.value()))
    {
        Ok(a) => a,
        Err(e) => return e.into_compile_error(),
    };
    let model_ty_name = replace_self(
        model_ty.clone(),
        &Type::Path(TypePath {
            qself: None,
            path: base.ident.clone().into(),
        }),
    );
    let mut match_name = None;
    let mut match_name_ref = None;
    let mut match_name_mut = None;
    for arg in base
        .attrs
        .iter()
        .filter(|attr| attr.path.is_ident("model"))
        .filter_map(|attr| attr.parse_args::<MetaNameValue>().ok())
    {
        match arg {
            MetaNameValue {
                path,
                lit: Lit::Str(s),
                ..
            } if path.is_ident("match") => match_name = Some(s.parse().unwrap()),
            MetaNameValue {
                path,
                lit: Lit::Str(s),
                ..
            } if path.is_ident("match_ref") => match_name_ref = Some(s.parse().unwrap()),
            MetaNameValue {
                path,
                lit: Lit::Str(s),
                ..
            } if path.is_ident("match_mut") => match_name_mut = Some(s.parse().unwrap()),
            _ => (),
        }
    }
    let match_name = match_name
        .unwrap_or_else(|| Ident::new(&format!("{}MatchModel", base.ident), Span::call_site()));
    let match_name_ref = match_name_ref
        .unwrap_or_else(|| Ident::new(&format!("{}MatchModelRef", base.ident), Span::call_site()));
    let match_name_mut = match_name_mut
        .unwrap_or_else(|| Ident::new(&format!("{}MatchModelMut", base.ident), Span::call_site()));
    let serde_rename_all = serde_string(&base.attrs, "rename_all").map(|value| value.value());
    if let Some(untagged) = serde_flag(&base.attrs, "untagged") {
        return Error::new(untagged.span(), "Cannot derive HasModel for untagged enum")
            .into_compile_error();
    }
    let mut serde_tag: BTreeMap<&'static str, Lit> = serde_name_values(&base.attrs)
        .filter_map(|nv| {
            if nv.path.is_ident("tag") {
                Some(("tag", nv.lit))
            } else if nv.path.is_ident("content") {
                Some(("content", nv.lit))
            } else {
                None
            }
        })
        .collect();

    let mut model_variants = TokenStream::new();
    let mut ref_model_variants = TokenStream::new();
    let mut mut_model_variants = TokenStream::new();
    let (impl_new, impl_unmatch, impl_new_ref, impl_new_mut) = if let Some(Lit::Str(tag)) =
        serde_tag.remove("tag")
    {
        if let Some(Lit::Str(content)) = serde_tag.remove("content") {
            let mut tag_variants = TokenStream::new();
            let mut tag_variants_unmatch = TokenStream::new();
            let mut tag_variants_ref = TokenStream::new();
            let mut tag_variants_mut = TokenStream::new();
            for variant in &ast.variants {
                let variant_name = &variant.ident;
                let variant_accessor =
                    get_accessor(&serde_rename_all, &variant.attrs, variant_name);
                tag_variants.extend(quote_spanned! { variant_name.span() =>
                    Some(#variant_accessor) => #match_name::#variant_name(self.transmute(|v| {
                        #[allow(unused_imports)]
                        use patch_db::value::index::Index;
                        #content.index_into_owned(v).unwrap_or_default()
                    })),
                });
                tag_variants_unmatch.extend(quote_spanned! { variant_name.span() =>
                    #match_name::#variant_name(v) => Self::from_value({
                        let mut a = patch_db::value::json!({ #tag: #variant_accessor });
                        a[#content] = v.into_value();
                        a
                    }),
                });
                tag_variants_ref.extend(quote_spanned! { variant_name.span() =>
                    Some(#variant_accessor) => #match_name_ref::#variant_name(self.transmute_ref(|v| {
                        #[allow(unused_imports)]
                        use patch_db::value::index::Index;
                        #content.index_into(v).unwrap_or(&patch_db::value::NULL)
                    })),
                });
                tag_variants_mut.extend(quote_spanned! { variant_name.span() =>
                    Some(#variant_accessor) => #match_name_mut::#variant_name(self.transmute_mut(|v| {
                        #[allow(unused_imports)]
                        use patch_db::value::index::Index;
                        #content.index_or_insert(v)
                    })),
                });
            }
            (
                quote! {
                    use patch_db::ModelExt;
                    match self.as_value()[#tag].as_str() {
                        #tag_variants
                        _ => #match_name::Error(self.into_value()),
                    }
                },
                quote! {
                    use patch_db::ModelExt;
                    match value {
                        #tag_variants_unmatch
                        #match_name::Error(v) => Self::from_value(v),
                    }
                },
                quote! {
                    use patch_db::ModelExt;
                    match self.as_value()[#tag].as_str() {
                        #tag_variants_ref
                        _ => #match_name_ref::Error(self.as_value()),
                    }
                },
                quote! {
                    use patch_db::ModelExt;
                    match self.as_value()[#tag].as_str() {
                        #tag_variants_mut
                        _ => #match_name_mut::Error(self.as_value_mut()),
                    }
                },
            )
        } else {
            let mut tag_variants = TokenStream::new();
            let mut tag_variants_unmatch = TokenStream::new();
            let mut tag_variants_ref = TokenStream::new();
            let mut tag_variants_mut = TokenStream::new();
            for variant in &ast.variants {
                let variant_name = &variant.ident;
                let variant_accessor =
                    get_accessor(&serde_rename_all, &variant.attrs, variant_name);
                tag_variants.extend(quote_spanned! { variant_name.span() =>
                    Some(#variant_accessor) => #match_name::#variant_name(self.transmute(|v| v)),
                });
                tag_variants_unmatch.extend(quote_spanned! { variant_name.span() =>
                    #match_name::#variant_name(v) => Self::from_value({
                        let mut a = v.into_value();
                        a[#tag] = patch_db::Value::String(std::sync::Arc::new(#variant_accessor.to_owned()));
                        a
                    }),
                });
                tag_variants_ref.extend(quote_spanned! { variant_name.span() =>
                    Some(#variant_accessor) => #match_name_ref::#variant_name(self.transmute_ref(|v| v)),
                });
                tag_variants_mut.extend(quote_spanned! { variant_name.span() =>
                    Some(#variant_accessor) => #match_name_mut::#variant_name(self.transmute_mut(|v| v)),
                });
            }
            (
                quote! {
                    use patch_db::ModelExt;
                    match self.as_value()[#tag].as_str() {
                        #tag_variants
                        _ => #match_name::Error(self.into_value()),
                    }
                },
                quote! {
                    use patch_db::ModelExt;
                    match value {
                        #tag_variants_unmatch
                        #match_name::Error(v) => Self::from_value(v),
                    }
                },
                quote! {
                    use patch_db::ModelExt;
                    match self.as_value()[#tag].as_str() {
                        #tag_variants_ref
                        _ => #match_name_ref::Error(self.as_value()),
                    }
                },
                quote! {
                    use patch_db::ModelExt;
                    match self.as_value()[#tag].as_str() {
                        #tag_variants_mut
                        _ => #match_name_mut::Error(self.as_value_mut()),
                    }
                },
            )
        }
    } else {
        let mut tag_variants = TokenStream::new();
        let mut tag_variants_unmatch = TokenStream::new();
        let mut tag_variants_ref = TokenStream::new();
        let mut tag_variants_mut = TokenStream::new();
        for variant in &ast.variants {
            let variant_name = &variant.ident;
            let variant_accessor = get_accessor(&serde_rename_all, &variant.attrs, variant_name);
            tag_variants.extend(quote_spanned! { variant_name.span() =>
                if value.as_object().map(|o| o.contains_key(#variant_accessor)).unwrap_or(false) {
                    #match_name::#variant_name(self.transmute(|v| {
                        #[allow(unused_imports)]
                        use patch_db::value::index::Index;
                        #variant_accessor.index_into_owned(v).unwrap_or_default()
                    }))
                } else
            });
            tag_variants_unmatch.extend(quote_spanned! { variant_name.span() =>
                #match_name::#variant_name(v) => Self::from_value({
                    let mut a = patch_db::value::json!({});
                    a[#variant_accessor] = v.into_value();
                    a
                }),
            });
            tag_variants_ref.extend(quote_spanned! { variant_name.span() =>
                if value.as_object().map(|o| o.contains_key(#variant_accessor)).unwrap_or(false) {
                    #match_name_ref::#variant_name(self.transmute_ref(|v| {
                        #[allow(unused_imports)]
                        use patch_db::value::index::Index;
                        #variant_accessor.index_into(v).unwrap_or(&patch_db::value::NULL)
                    }))
                } else
            });
            tag_variants_mut.extend(quote_spanned! { variant_name.span() =>
                if value.as_object().map(|o| o.contains_key(#variant_accessor)).unwrap_or(false) {
                    #match_name_mut::#variant_name(self.transmute_mut(|v| {
                        #[allow(unused_imports)]
                        use patch_db::value::index::Index;
                        #variant_accessor.index_or_insert(v)
                    }))
                } else
            });
        }
        (
            quote! {
                use patch_db::ModelExt;
                #tag_variants {
                    #match_name::Error(self.into_value()),
                }
            },
            quote! {
                use patch_db::ModelExt;
                match value {
                    #tag_variants_unmatch
                    #match_name::Error(v) => Self::from_value(v),
                }
            },
            quote! {
                use patch_db::ModelExt;
                #tag_variants_ref {
                    #match_name_ref::Error(self.as_value()),
                }
            },
            quote! {
                use patch_db::ModelExt;
                #tag_variants_mut {
                    #match_name_mut::Error(self.as_value_mut()),
                }
            },
        )
    };
    for variant in &ast.variants {
        let name = &variant.ident;
        let ty = match &variant.fields {
            Fields::Unnamed(a) if a.unnamed.len() == 1 => &a.unnamed.first().unwrap().ty,
            a => {
                return Error::new(
                    a.span(),
                    "Can only derive HasModel for enums with newtype variants",
                )
                .into_compile_error()
            }
        };
        let model_variant_ty = replace_self(model_ty.clone(), &ty);
        model_variants.extend(quote_spanned! { name.span() =>
            #name(#model_variant_ty),
        });
        ref_model_variants.extend(quote_spanned! { name.span() =>
            #name(&'a #model_variant_ty),
        });
        mut_model_variants.extend(quote_spanned! { name.span() =>
            #name(&'a mut #model_variant_ty),
        });
    }
    let name = &base.ident;
    let vis = &base.vis;
    quote! {
        impl patch_db::HasModel for #name {
            type Model = #model_ty;
        }

        impl #model_ty_name {
            #vis fn into_match(self) -> #match_name {
                #impl_new
            }
            #vis fn from_match(value: #match_name) -> Self {
                #impl_unmatch
            }
        }

        #[derive(Debug)]
        #vis enum #match_name {
            #model_variants
            Error(patch_db::Value),
        }

        impl #model_ty_name {
            #vis fn as_match<'a>(&'a self) -> #match_name_ref<'a> {
                #impl_new_ref
            }
        }

        #[derive(Debug)]
        #vis enum #match_name_ref<'a> {
            #ref_model_variants
            Error(&'a patch_db::Value),
        }

        impl #model_ty_name {
            #vis fn as_match_mut<'a>(&'a mut self) -> #match_name_mut<'a> {
                #impl_new_mut
            }
        }

        #[derive(Debug)]
        #vis enum #match_name_mut<'a> {
            #mut_model_variants
            Error(&'a mut patch_db::Value),
        }
    }
}

#[cfg(test)]
mod tests;
