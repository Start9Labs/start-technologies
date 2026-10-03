//! Static JSON-shape traversal using serde's directional wire rules.
//!
//! A consumer implements `ShapeVisitor` and static field visiting. Derived
//! `SerdeShape` implementations supply metadata and invoke nested field callbacks
//! in declaration order, without constructing a value. Unsupported representations
//! reach `ShapeVisitor::unsupported`; the consumer decides how to report them.

use crate::{Static, Visit, Visitor};

/// Deserialization accepts input shapes; serialization produces output shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Direction {
    Input,
    Output,
}

/// Preserves the distinction between unit, newtype, tuple and named-field payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Struct,
    Tuple,
    Newtype,
    Unit,
}

/// Enum discriminator layout, including tag and content keys where applicable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    External,
    Internal(&'static str),
    Adjacent(&'static str, &'static str),
    Untagged,
}

/// A selected field's wire name, accepted aliases and omission/flattening rules.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub optional: bool,
    pub option_default: bool,
    pub flatten: bool,
}

/// A selected variant's wire representation, including unsupported hook markers.
#[derive(Debug, Clone, Copy)]
pub struct Variant {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub style: Style,
    pub tag: Tag,
    pub custom: bool,
    pub other: bool,
}

/// Receives nested JSON payloads and statically visits their selected field types.
pub trait ShapeVisitor: Visitor + Sized {
    fn unsupported(&mut self, reason: &'static str);
    fn structure(&mut self, style: Style, fields: impl FnOnce(&mut Self));
    fn enumeration(&mut self, variants: impl FnOnce(&mut Self));
    fn variant(&mut self, variant: Variant, fields: impl FnOnce(&mut Self));
    fn field<T: ?Sized>(&mut self, field: Field)
    where
        Static<T>: Visit<Self>;
}

/// Visits the JSON shape selected by serde's input or output rules.
pub trait SerdeShape<V: ShapeVisitor> {
    fn visit_shape(visitor: &mut V, direction: Direction);
}
