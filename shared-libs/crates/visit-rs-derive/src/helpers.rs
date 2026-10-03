pub use serde_derive_internals::attr::RenameRule;
use syn::punctuated::Punctuated;
use syn::{Attribute, DeriveInput, Lit, Meta, Token, Variant};

fn meta_items(attr: &Attribute) -> Vec<Meta> {
    if !(attr.path().is_ident("visit") || attr.path().is_ident("serde")) {
        return Vec::new();
    }
    let Ok(list) = attr.meta.require_list() else {
        return Vec::new();
    };
    list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .map(|p| p.into_iter().collect())
        .unwrap_or_default()
}

fn rename_value(meta: &Meta) -> Option<String> {
    match meta {
        Meta::NameValue(nv) => {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: Lit::Str(s), ..
            }) = &nv.value
            {
                return Some(s.value());
            }
            None
        }
        Meta::List(list) => {
            let nested = list
                .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .ok()?;
            let mut serialize = None;
            let mut deserialize = None;
            for item in &nested {
                if let Meta::NameValue(nv) = item
                    && let syn::Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(s), ..
                    }) = &nv.value
                {
                    if nv.path.is_ident("serialize") {
                        serialize = Some(s.value());
                    } else if nv.path.is_ident("deserialize") {
                        deserialize = Some(s.value());
                    }
                }
            }
            serialize.or(deserialize)
        }
        Meta::Path(_) => None,
    }
}

fn find_rename(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        for meta in meta_items(attr) {
            if meta.path().is_ident("rename")
                && let Some(name) = rename_value(&meta)
            {
                return Some(name);
            }
        }
    }
    None
}

fn find_rename_all(attrs: &[Attribute]) -> RenameRule {
    for attr in attrs {
        for meta in meta_items(attr) {
            if meta.path().is_ident("rename_all")
                && let Some(rule) = rename_value(&meta).and_then(|s| RenameRule::from_str(&s).ok())
            {
                return rule;
            }
        }
    }
    RenameRule::None
}

pub fn get_rename_attribute(ast: &DeriveInput) -> Option<String> {
    find_rename(&ast.attrs)
}

pub fn get_rename_all_attribute(ast: &DeriveInput) -> RenameRule {
    find_rename_all(&ast.attrs)
}

pub fn get_variant_rename(variant: &Variant, default_rule: RenameRule) -> String {
    find_rename(&variant.attrs)
        .unwrap_or_else(|| default_rule.apply_to_variant(&variant.ident.to_string()))
}

pub fn get_field_rename(field: &syn::Field, default_rule: RenameRule) -> Option<String> {
    let field_name = field.ident.as_ref()?.to_string();
    Some(find_rename(&field.attrs).unwrap_or_else(|| default_rule.apply_to_field(&field_name)))
}

pub fn is_skipped(field: &syn::Field) -> bool {
    field.attrs.iter().any(|attr| {
        meta_items(attr)
            .iter()
            .any(|m| m.path().is_ident("skip") || m.path().is_ident("skip_serializing"))
    })
}
