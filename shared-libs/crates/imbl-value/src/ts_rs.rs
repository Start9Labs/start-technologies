use ts_rs::{TypeVisitor, TS};

use crate::{InOMap, Value};

impl TS for Value {
    type WithoutGenerics = Self;
    fn name() -> String {
        "unknown".to_string()
    }
    fn inline() -> String {
        Self::name()
    }
    fn decl() -> String {
        panic!("Value cannot be declared")
    }
    fn decl_concrete() -> String {
        panic!("Value cannot be declared")
    }
    fn inline_flattened() -> String {
        panic!("Value cannot be flattened")
    }
}

impl<K: TS + Eq + Clone, V: TS + Clone> TS for InOMap<K, V> {
    type WithoutGenerics = InOMap<(), ()>;

    fn ident() -> String {
        panic!()
    }

    fn name() -> String {
        format!("{{ [key in {}]?: {} }}", K::name(), V::name())
    }

    fn inline() -> String {
        format!("{{ [key in {}]?: {} }}", K::inline(), V::inline())
    }

    fn visit_dependencies(v: &mut impl TypeVisitor)
    where
        Self: 'static,
    {
        K::visit_dependencies(v);

        V::visit_dependencies(v);
    }

    fn visit_generics(v: &mut impl TypeVisitor)
    where
        Self: 'static,
    {
        K::visit_generics(v);

        v.visit::<K>();

        V::visit_generics(v);

        v.visit::<V>();
    }

    fn decl() -> String {
        panic!("{} cannot be declared", Self::name())
    }

    fn decl_concrete() -> String {
        panic!("{} cannot be declared", Self::name())
    }

    fn inline_flattened() -> String {
        format!("({{ [key in {}]?: {} }})", K::inline(), V::inline())
    }
}
