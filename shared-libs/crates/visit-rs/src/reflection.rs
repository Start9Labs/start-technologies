//! Unconditional Rust declaration facts and compiler-resolved metadata traversal.

use crate::Static;

/// The syntactic field layout, including empty tuple and named layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldsKind {
    Unit,
    Tuple,
    Named,
}

/// The original Rust declaration category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclarationKind {
    Struct(FieldsKind),
    Enum,
    Union,
}

/// A declared generic parameter in source order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenericParameter {
    Lifetime {
        name: &'static str,
    },
    Type {
        name: &'static str,
    },
    Const {
        name: &'static str,
        ty: &'static str,
    },
}

/// Original indices within the effective derive input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    pub variant: Option<usize>,
    pub field: usize,
}

/// An original Rust field, including fields omitted by other derives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldInfo {
    pub position: Position,
    /// Ordinal in the legacy field iterator, absent for `visit(skip)`.
    pub visit_index: Option<usize>,
    pub name: Option<&'static str>,
    pub type_syntax: &'static str,
    pub attributes: &'static [&'static str],
}

/// An original Rust enum variant and its complete field inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VariantInfo {
    pub index: usize,
    pub name: &'static str,
    pub fields_kind: FieldsKind,
    pub attributes: &'static [&'static str],
    pub fields: &'static [FieldInfo],
}

/// Effective Rust declaration tokens and unconditional structural facts.
///
/// Token text preserves literal spelling and order, not source whitespace or
/// declarations removed before macro expansion. Attributes remain uninterpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeclarationInfo {
    pub name: &'static str,
    pub module: &'static str,
    pub kind: DeclarationKind,
    pub source: &'static str,
    pub parameters: &'static [GenericParameter],
    pub attributes: &'static [&'static str],
    /// Structured attribute values in source order, independent of the `meta` feature.
    pub metadata: &'static [crate::metadata::AttributeMeta],
    /// String-valued Rust doc attributes, preserving meaningful whitespace.
    pub docs: &'static [&'static str],
    pub fields: &'static [FieldInfo],
    pub variants: &'static [VariantInfo],
}

/// A metadata node's pre-order coordinate within its original attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MetadataPosition {
    pub variant: Option<usize>,
    pub field: Option<usize>,
    pub attribute: usize,
    pub occurrence: usize,
}

/// A locally selected string literal resolved by the compiler as a Rust type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeAttributeInfo {
    pub position: MetadataPosition,
    pub path: &'static [&'static str],
    pub literal_tokens: &'static str,
    pub value: &'static str,
}

/// Supplies declaration facts independently of visiting bounds.
///
/// `Kind` identifies the syntactic root category without encoding visitor policy.
pub trait TypeInfo {
    /// The syntactic root category emitted by the declaration's derive.
    type Kind;
    const DECLARATION: DeclarationInfo;
}

/// Bound-free storage marker retaining the original type.
pub struct Opaque<T: ?Sized>(pub Static<T>);

/// Compiler-resolved selected metadata paired with its source coordinate.
pub struct TypeAttribute<T: ?Sized> {
    pub info: TypeAttributeInfo,
    pub marker: Static<T>,
}

/// A struct declaration category with no traversal behavior.
pub struct StructKind;

/// An enum declaration category with no traversal behavior.
pub struct EnumKind;
