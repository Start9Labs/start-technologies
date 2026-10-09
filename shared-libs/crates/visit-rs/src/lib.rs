extern crate self as visit_rs;

use std::marker::PhantomData;

use futures::Stream;
pub use visit_rs_derive::*;
pub mod reflection;
pub use reflection::{Opaque, TypeAttribute, TypeInfo};

/// Visits compiler-resolved type-valued metadata independently of storage.
///
/// Selected targets require the visitor's ordinary typed-marker support.
///
/// ```compile_fail
/// use visit_rs::{Static, TypeAttribute, Visit, VisitTypeAttributes, Visitor};
/// struct Recorder;
/// impl Visitor for Recorder { type Result = (); }
/// impl Visit<Recorder> for Static<u32> { fn visit(&self, _: &mut Recorder) {} }
/// impl<T: ?Sized> Visit<Recorder> for TypeAttribute<T>
/// where Static<T>: Visit<Recorder> {
///     fn visit(&self, visitor: &mut Recorder) { self.marker.visit(visitor); }
/// }
/// struct Hidden;
/// #[derive(visit_rs::VisitFields)]
/// #[visit(opaque, target = "Hidden", type_attributes(visit::target))]
/// struct Root(Hidden);
/// Root::visit_type_attributes(&mut Recorder).for_each(drop);
/// ```
///
/// Selected literals must contain valid Rust type syntax.
///
/// ```compile_fail
/// #[derive(visit_rs::VisitFields)]
/// #[visit(target = 42, type_attributes(visit::target))]
/// struct Root;
/// ```
///
/// ```compile_fail
/// #[derive(visit_rs::VisitFields)]
/// #[visit(target = "not a type!", type_attributes(visit::target))]
/// struct Root;
/// ```
///
/// Malformed selected paths or descendants reject the declaration.
///
/// ```compile_fail
/// #[derive(visit_rs::VisitVariants)]
/// #[visit(type_attributes(visit::))]
/// enum Root { Value(u32) }
/// ```
///
/// ```compile_fail
/// #[derive(visit_rs::VisitVariants)]
/// #[visit(target(type = "u32", @ raw), type_attributes(visit::target::type))]
/// enum Root { Value(u32) }
/// ```
pub trait VisitTypeAttributes<V: Visitor>: TypeInfo {
    fn visit_type_attributes<'a>(visitor: &'a mut V) -> impl Iterator<Item = V::Result> + 'a;
}

pub mod metadata;

pub mod lib {
    pub use async_stream;
    pub use futures;

    /// The shared value of a capture context.
    pub trait SharedCapture {
        type Value: Sync + ?Sized;
    }

    impl<Context, Value: Sync + ?Sized> SharedCapture for (Context, Value) {
        type Value = Value;
    }
}

pub trait Visitor {
    type Result;
}

pub trait WrapperVisitor<'a>: Visitor {
    type Inner: Visitor<Result = Self::Result>;
    fn as_inner(&mut self) -> &mut Self::Inner;
    fn wrap(visitor: &'a mut Self::Inner) -> Self;
}

pub trait Visit<V: Visitor> {
    fn visit(&self, visitor: &mut V) -> V::Result;
}

pub trait VisitAsync<V: Visitor> {
    fn visit_async<'a>(&'a self, visitor: &'a mut V) -> impl Future<Output = V::Result> + Send + 'a
    where
        V: Send,
        V::Result: Send;
}

pub trait StructInfo {
    const DATA: StructInfoData;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructInfoData {
    pub name: &'static str,
    pub named_fields: bool,
    pub field_count: usize,
    #[cfg(feature = "meta")]
    pub metadata: &'static [metadata::AttributeMeta],
}

pub trait VisitFields<V: Visitor>: StructInfo {
    fn visit_fields<'a>(&'a self, visitor: &'a mut V) -> impl Iterator<Item = V::Result> + 'a;
}

pub trait VisitFieldsCovered<V: Visitor>: StructInfo {
    fn visit_fields_covered<'a>(
        &'a self,
        visitor: &'a mut V,
    ) -> impl Iterator<Item = V::Result> + 'a;
}

pub trait VisitFieldsStatic<V: Visitor>: StructInfo {
    fn visit_fields_static<'a>(visitor: &'a mut V) -> impl Iterator<Item = V::Result> + 'a;
}

pub trait VisitFieldsAsync<V: Visitor>: StructInfo {
    fn visit_fields_async<'a>(
        &'a self,
        visitor: &'a mut V,
    ) -> impl Stream<Item = V::Result> + Send + 'a
    where
        V: Send,
        V::Result: Send;
}

pub trait VisitFieldsCoveredAsync<V: Visitor>: StructInfo {
    fn visit_fields_covered_async<'a>(
        &'a self,
        visitor: &'a mut V,
    ) -> impl Stream<Item = V::Result> + Send + 'a
    where
        V: Send,
        V::Result: Send;
}

pub trait VisitFieldsStaticAsync<V: Visitor>: StructInfo {
    fn visit_fields_static_async<'a>(visitor: &'a mut V) -> impl Stream<Item = V::Result> + 'a
    where
        V: Send,
        V::Result: Send;
}

pub trait VisitFieldsNamed<V: Visitor>: StructInfo {
    fn visit_fields_named<'a>(&'a self, visitor: &'a mut V)
    -> impl Iterator<Item = V::Result> + 'a;
}

pub trait VisitFieldsStaticNamed<V: Visitor>: StructInfo {
    fn visit_fields_static_named<'a>(visitor: &'a mut V) -> impl Iterator<Item = V::Result> + 'a;
}

pub trait VisitFieldsNamedAsync<V: Visitor>: StructInfo {
    fn visit_fields_named_async<'a>(
        &'a self,
        visitor: &'a mut V,
    ) -> impl Stream<Item = V::Result> + Send + 'a
    where
        V: Send,
        V::Result: Send;
}

pub trait VisitFieldsStaticNamedAsync<V: Visitor>: StructInfo {
    fn visit_fields_static_named_async<'a>(
        visitor: &'a mut V,
    ) -> impl Stream<Item = V::Result> + Send + 'a
    where
        V: Send,
        V::Result: Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Named<'a, T: ?Sized> {
    pub name: Option<&'static str>,
    #[cfg(feature = "meta")]
    pub metadata: &'static [metadata::AttributeMeta],
    pub value: &'a T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Covered<'a, T: ?Sized>(pub &'a T);

pub struct Static<T: ?Sized> {
    _phantom: PhantomData<T>,
}
impl<T: ?Sized> std::fmt::Debug for Static<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(std::any::type_name::<Self>()).finish()
    }
}
unsafe impl<T: ?Sized> Send for Static<T> {}
unsafe impl<T: ?Sized> Sync for Static<T> {}
impl<T: ?Sized> Default for Static<T> {
    fn default() -> Self {
        Static::new()
    }
}
impl<T: ?Sized> Clone for Static<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: ?Sized> Copy for Static<T> {}
impl<T: ?Sized + 'static, U: ?Sized + 'static> PartialEq<Static<U>> for Static<T> {
    fn eq(&self, _: &Static<U>) -> bool {
        std::any::TypeId::of::<T>() == std::any::TypeId::of::<U>()
    }
}
impl<T: ?Sized + 'static> Eq for Static<T> {}

impl<T: ?Sized> Static<T> {
    pub const fn new() -> Self {
        Static {
            _phantom: PhantomData,
        }
    }
    pub const fn new_ref() -> &'static Self {
        const STATIC: Static<()> = Static::new();
        // SAFETY: Safe to transmute because it is always just a phantom
        unsafe { std::mem::transmute(&STATIC) }
    }
}

pub trait EnumInfo {
    const DATA: EnumInfoData;
    fn variants() -> impl IntoIterator<Item = StructInfoData> + Send + Sync + 'static;
    fn variant_info(&self) -> StructInfoData;
    fn variant_info_by_name(name: &str) -> Option<StructInfoData>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumInfoData {
    pub name: &'static str,
    pub variant_count: usize,
    #[cfg(feature = "meta")]
    pub metadata: &'static [metadata::AttributeMeta],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Variant<'a, T: ?Sized> {
    pub info: StructInfoData,
    pub value: &'a T,
}

pub trait VisitVariant<V: Visitor>: EnumInfo {
    fn visit_variant(&self, visitor: &mut V) -> V::Result;
}

pub trait VisitVariantsStatic<V: Visitor>: EnumInfo {
    fn visit_variants_static<'a>(visitor: &'a mut V) -> impl Iterator<Item = V::Result> + 'a;
}

pub trait VisitVariantFields<V: Visitor>: EnumInfo {
    fn visit_variant_fields<'a>(
        &'a self,
        visitor: &'a mut V,
    ) -> impl Iterator<Item = V::Result> + 'a;
}

pub trait VisitVariantFieldsCovered<V: Visitor>: EnumInfo {
    fn visit_variant_fields_covered<'a>(
        &'a self,
        visitor: &'a mut V,
    ) -> impl Iterator<Item = V::Result> + 'a;
}

pub trait VisitVariantFieldsStatic<V: Visitor>: EnumInfo {
    fn visit_variant_fields_static<'a>(
        info: &'a StructInfoData,
        visitor: &'a mut V,
    ) -> impl Iterator<Item = V::Result> + 'a;
}

pub trait VisitVariantFieldsAsync<V: Visitor>: EnumInfo {
    fn visit_variant_fields_async<'a>(
        &'a self,
        visitor: &'a mut V,
    ) -> impl Stream<Item = V::Result> + Send + 'a
    where
        V: Send,
        V::Result: Send;
}

pub trait VisitVariantFieldsCoveredAsync<V: Visitor>: EnumInfo {
    fn visit_variant_fields_covered_async<'a>(
        &'a self,
        visitor: &'a mut V,
    ) -> impl Stream<Item = V::Result> + Send + 'a
    where
        V: Send,
        V::Result: Send;
}

pub trait VisitVariantFieldsStaticAsync<V: Visitor>: EnumInfo {
    fn visit_variant_fields_static_async<'a>(
        info: &'a StructInfoData,
        visitor: &'a mut V,
    ) -> impl Stream<Item = V::Result> + 'a
    where
        V: Send,
        V::Result: Send;
}

pub trait VisitVariantFieldsNamed<V: Visitor>: EnumInfo {
    fn visit_variant_fields_named<'a>(
        &'a self,
        visitor: &'a mut V,
    ) -> impl Iterator<Item = V::Result> + 'a;
}

pub trait VisitVariantFieldsStaticNamed<V: Visitor>: EnumInfo {
    fn visit_variant_fields_static_named<'a>(
        info: &'a StructInfoData,
        visitor: &'a mut V,
    ) -> impl Iterator<Item = V::Result> + 'a;
}

pub trait VisitVariantFieldsNamedAsync<V: Visitor>: EnumInfo {
    fn visit_variant_fields_named_async<'a>(
        &'a self,
        visitor: &'a mut V,
    ) -> impl Stream<Item = V::Result> + Send + 'a
    where
        V: Send,
        V::Result: Send;
}

pub trait VisitVariantFieldsStaticNamedAsync<V: Visitor>: EnumInfo {
    fn visit_variant_fields_static_named_async<'a>(
        info: &'a StructInfoData,
        visitor: &'a mut V,
    ) -> impl Stream<Item = V::Result> + Send + 'a
    where
        V: Send,
        V::Result: Send;
}
