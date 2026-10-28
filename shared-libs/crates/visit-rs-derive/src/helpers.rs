use syn::{Attribute, DeriveInput, Lit, Meta, Variant};

use crate::attrs::children;

fn meta_items(attr: &Attribute) -> Vec<Meta> {
    if !attr.path().is_ident("visit") {
        return Vec::new();
    }
    let Ok(list) = attr.meta.require_list() else {
        return Vec::new();
    };
    children(list)
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
        Meta::List(_) => None,
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
    field
        .attrs
        .iter()
        .any(|attr| meta_items(attr).iter().any(|m| m.path().is_ident("skip")))
}

fn lowercase_initial(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map(|c| c.to_ascii_lowercase().to_string())
        .unwrap_or_default()
        + chars.as_str()
}

#[derive(Clone, Copy)]
pub enum RenameRule {
    None,
    Lower,
    Upper,
    Pascal,
    Camel,
    Snake,
    ScreamingSnake,
    Kebab,
    ScreamingKebab,
}

impl RenameRule {
    fn from_str(rule: &str) -> Result<Self, ()> {
        Ok(match rule {
            "lowercase" => Self::Lower,
            "UPPERCASE" => Self::Upper,
            "PascalCase" => Self::Pascal,
            "camelCase" => Self::Camel,
            "snake_case" => Self::Snake,
            "SCREAMING_SNAKE_CASE" => Self::ScreamingSnake,
            "kebab-case" => Self::Kebab,
            "SCREAMING-KEBAB-CASE" => Self::ScreamingKebab,
            _ => return Err(()),
        })
    }

    pub fn apply_to_field(self, name: &str) -> String {
        match self {
            Self::None | Self::Lower | Self::Snake => name.to_owned(),
            Self::Upper | Self::ScreamingSnake => name.to_ascii_uppercase(),
            Self::Kebab => name.replace('_', "-"),
            Self::ScreamingKebab => name.replace('_', "-").to_ascii_uppercase(),
            Self::Camel => lowercase_initial(&Self::Pascal.apply_to_field(name)),
            Self::Pascal => {
                let mut result = String::new();
                let mut capitalize = true;
                for ch in name.chars() {
                    if ch == '_' {
                        capitalize = true;
                    } else {
                        result.push(if capitalize {
                            ch.to_ascii_uppercase()
                        } else {
                            ch
                        });
                        capitalize = false;
                    }
                }
                result
            }
        }
    }

    pub fn apply_to_variant(self, name: &str) -> String {
        match self {
            Self::None | Self::Pascal => name.to_owned(),
            Self::Lower => name.to_ascii_lowercase(),
            Self::Upper => name.to_ascii_uppercase(),
            Self::Camel => lowercase_initial(name),
            Self::Snake | Self::ScreamingSnake | Self::Kebab | Self::ScreamingKebab => {
                let mut result = String::new();
                let separator = if matches!(self, Self::Kebab | Self::ScreamingKebab) {
                    '-'
                } else {
                    '_'
                };
                for (index, ch) in name.chars().enumerate() {
                    if index > 0 && ch.is_uppercase() {
                        result.push(separator);
                    }
                    result.push(ch.to_ascii_lowercase());
                }
                if matches!(self, Self::ScreamingSnake | Self::ScreamingKebab) {
                    result.make_ascii_uppercase();
                }
                result
            }
        }
    }
}
