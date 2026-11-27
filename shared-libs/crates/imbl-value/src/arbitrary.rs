use proptest::prelude::*;
use proptest_derive::Arbitrary;
use yasi::InternedString;

use crate::{GenericValue, ObjectKind};

impl<K: ObjectKind> Arbitrary for GenericValue<K> {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;
    fn arbitrary() -> Self::Strategy {
        value_strategy()
    }
    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        value_strategy()
    }
}

pub fn value_strategy<K: ObjectKind>() -> BoxedStrategy<GenericValue<K>> {
    let leaf = proptest::prop_oneof![
        Just(GenericValue::Null),
        any::<bool>().prop_map(GenericValue::Bool),
        number_strategy(),
        ".*".prop_map(|s| GenericValue::from(s)),
    ];
    leaf.prop_recursive(8, 256, 10, |inner| {
        prop_oneof![array_strategy(inner.clone()), object_strategy(inner)]
    })
    .boxed()
}

pub fn number_strategy<K: ObjectKind>() -> BoxedStrategy<GenericValue<K>> {
    #[derive(Debug, Arbitrary)]
    enum Number {
        PosInt(u64),
        NegInt(i64),
        Float(f64),
    }
    Number::arbitrary()
        .prop_map::<serde_json::Number, _>(|n| match n {
            Number::PosInt(a) => a.into(),
            Number::NegInt(a) => a.into(),
            Number::Float(a) => {
                serde_json::Number::from_f64(a).unwrap_or(serde_json::Number::from(0))
            }
        })
        .prop_map(GenericValue::Number)
        .boxed()
}

pub fn array_strategy<K: ObjectKind>(
    inner: impl Strategy<Value = GenericValue<K>> + 'static,
) -> BoxedStrategy<GenericValue<K>> {
    imbl::proptest::vector(inner, 0..10)
        .prop_map(GenericValue::Array)
        .boxed()
}

pub fn object_strategy<K: ObjectKind>(
    inner: impl Strategy<Value = GenericValue<K>> + 'static,
) -> BoxedStrategy<GenericValue<K>> {
    prop::collection::hash_map(".*", inner, 0..10)
        .prop_map(|map| {
            map.into_iter()
                .map(|(k, v)| (InternedString::intern(k), v))
                .collect()
        })
        .prop_map(GenericValue::Object)
        .boxed()
}
