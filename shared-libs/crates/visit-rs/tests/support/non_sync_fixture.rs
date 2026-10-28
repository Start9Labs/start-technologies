#![allow(dead_code)]
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use ::visit_rs::*;

pub type Maybe = Option<String>;
pub struct LocalStorage {
    pub cell: Cell<u32>,
    pub refcell: RefCell<String>,
}
#[derive(VisitFields)]
pub struct WrappedOptions<'a> {
    pub alias: Maybe,
    pub borrowed: &'a Maybe,
    pub boxed: Box<Maybe>,
    pub arc: Arc<Maybe>,
    pub rc: Rc<Maybe>,
    pub cow: Cow<'a, Maybe>,
    pub local: LocalStorage,
}
#[derive(VisitFields)]
pub struct GenericRc<T>(pub Rc<T>);
#[derive(VisitFields)]
pub struct LocalRoot(pub LocalStorage);
#[derive(VisitVariants)]
pub enum LocalEnum<'a> {
    Rc(Rc<Maybe>),
    Borrowed(&'a Maybe),
    Local { storage: LocalStorage },
}

pub struct Legacy;
impl Visitor for Legacy {
    type Result = usize;
}
macro_rules! callbacks {
    ($($ty:ty),*) => { $(
        impl Visit<Legacy> for $ty { fn visit(&self, _: &mut Legacy) -> usize { 1 } }
        impl VisitAsync<Legacy> for $ty {
            fn visit_async<'a>(&'a self, _: &'a mut Legacy) -> impl Future<Output=usize> + Send + 'a {
                std::future::ready(1)
            }
        }
    )* };
}
callbacks!(
    Maybe,
    &Maybe,
    Box<Maybe>,
    Arc<Maybe>,
    Cow<'_, Maybe>,
    LocalStorage
);
impl<T> Visit<Legacy> for Rc<T> {
    fn visit(&self, _: &mut Legacy) -> usize {
        1
    }
}
impl<T> VisitAsync<Legacy> for Rc<T> {
    fn visit_async<'a>(&'a self, _: &'a mut Legacy) -> impl Future<Output = usize> + Send + 'a {
        std::future::ready(1)
    }
}
impl<T: ?Sized + Visit<Legacy>> Visit<Legacy> for Named<'_, T> {
    fn visit(&self, v: &mut Legacy) -> usize {
        self.value.visit(v)
    }
}
impl<T: ?Sized + Visit<Legacy>> Visit<Legacy> for Covered<'_, T> {
    fn visit(&self, v: &mut Legacy) -> usize {
        self.0.visit(v)
    }
}
impl<T: ?Sized> VisitAsync<Legacy> for Named<'_, T> {
    fn visit_async<'a>(&'a self, _: &'a mut Legacy) -> impl Future<Output = usize> + Send + 'a {
        std::future::ready(1)
    }
}
impl<T: ?Sized> VisitAsync<Legacy> for Covered<'_, T> {
    fn visit_async<'a>(&'a self, _: &'a mut Legacy) -> impl Future<Output = usize> + Send + 'a {
        std::future::ready(1)
    }
}

trait TypedFact {
    fn optional() -> bool;
}
impl TypedFact for Maybe {
    fn optional() -> bool {
        true
    }
}
impl<T: TypedFact> TypedFact for &T {
    fn optional() -> bool {
        T::optional()
    }
}
macro_rules! wrapped_facts {
    ($($wrapper:ident),*) => { $(impl<T: TypedFact> TypedFact for $wrapper<T> { fn optional() -> bool { T::optional() } })* };
}
wrapped_facts!(Box, Arc, Rc);
impl<T: TypedFact + Clone> TypedFact for Cow<'_, T> {
    fn optional() -> bool {
        T::optional()
    }
}
impl TypedFact for LocalStorage {
    fn optional() -> bool {
        false
    }
}
struct TypedTraitRecorder;
impl Visitor for TypedTraitRecorder {
    type Result = bool;
}
impl<T: TypedFact> Visit<TypedTraitRecorder> for Static<T> {
    fn visit(&self, _: &mut TypedTraitRecorder) -> bool {
        T::optional()
    }
}
impl<T> Visit<TypedTraitRecorder> for Named<'_, Static<T>>
where
    Static<T>: Visit<TypedTraitRecorder>,
{
    fn visit(&self, v: &mut TypedTraitRecorder) -> bool {
        self.value.visit(v)
    }
}

pub fn check() {
    let maybe = Some(String::from("value"));
    let root = WrappedOptions {
        alias: maybe.clone(),
        borrowed: &maybe,
        boxed: Box::new(maybe.clone()),
        arc: Arc::new(maybe.clone()),
        rc: Rc::new(maybe.clone()),
        cow: Cow::Borrowed(&maybe),
        local: LocalStorage {
            cell: Cell::new(1),
            refcell: RefCell::new(String::new()),
        },
    };
    let mut v = Legacy;
    assert_eq!(root.visit_fields(&mut v).collect::<Vec<_>>(), [1; 7]);
    assert_eq!(root.visit_fields_named(&mut v).collect::<Vec<_>>(), [1; 7]);
    assert_eq!(
        root.visit_fields_covered(&mut v).collect::<Vec<_>>(),
        [1; 7]
    );
    assert_eq!(
        WrappedOptions::visit_fields_static_named(&mut TypedTraitRecorder).collect::<Vec<_>>(),
        [true, true, true, true, true, true, false]
    );
    let local = LocalRoot(LocalStorage {
        cell: Cell::new(0),
        refcell: RefCell::new(String::new()),
    });
    assert_eq!(local.visit_fields(&mut v).count(), 1);
    assert_eq!(local.visit_fields_named(&mut v).count(), 1);
    assert_eq!(local.visit_fields_covered(&mut v).count(), 1);
    assert_eq!(
        LocalRoot::visit_fields_static_named(&mut TypedTraitRecorder).collect::<Vec<_>>(),
        [false]
    );
    let generic = GenericRc(Rc::new(maybe.clone()));
    assert_eq!(generic.visit_fields(&mut v).count(), 1);
    assert_eq!(
        GenericRc::<Maybe>::visit_fields_static_named(&mut TypedTraitRecorder).collect::<Vec<_>>(),
        [true]
    );
    for root in [
        LocalEnum::Rc(Rc::new(maybe.clone())),
        LocalEnum::Borrowed(root.borrowed),
        LocalEnum::Local {
            storage: root.local,
        },
    ] {
        assert_eq!(root.visit_variant_fields(&mut v).collect::<Vec<_>>(), [1]);
        assert_eq!(
            root.visit_variant_fields_named(&mut v).collect::<Vec<_>>(),
            [1]
        );
        assert_eq!(
            root.visit_variant_fields_covered(&mut v)
                .collect::<Vec<_>>(),
            [1]
        );
    }
    for info in LocalEnum::variants() {
        let values = LocalEnum::visit_variant_fields_static_named(&info, &mut TypedTraitRecorder)
            .collect::<Vec<_>>();
        assert_eq!(values, [info.name != "Local"]);
    }
}
