use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;

use serde::de::{MapAccess, Visitor};
use serde::Deserialize;

use crate::InOMap;

pub struct MyVisitor<K, V>
where
    K: Clone + Eq,
    V: Clone,
{
    marker: PhantomData<fn() -> InOMap<K, V>>,
}

impl<K, V> MyVisitor<K, V>
where
    K: Clone + Eq,
    V: Clone,
{
    pub fn new() -> Self {
        MyVisitor {
            marker: PhantomData,
        }
    }
}

// This is the trait that Deserializers are going to be driving. There
// is one method for each type of data that our type knows how to
// deserialize from. There are many other methods that are not
// implemented here, for example deserializing from integers or strings.
// By default those methods will return an error, which makes sense
// because we cannot deserialize a MyMap from an integer or string.
impl<'de, K, V> Visitor<'de> for MyVisitor<K, V>
where
    K: Deserialize<'de> + Clone + Eq + Deref,
    V: Deserialize<'de> + Clone,
    <K as Deref>::Target: Eq,
{
    // The type that our Visitor is going to produce.
    type Value = InOMap<K, V>;

    // Format a message stating what data this Visitor expects to receive.
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a very special map")
    }

    // Deserialize MyMap from an abstract "map" provided by the
    // Deserializer. The MapAccess input is a callback provided by
    // the Deserializer to let us see each entry in the map.
    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut map = InOMap::new();

        // While there are entries remaining in the input, add them
        // into our map.
        while let Some((key, value)) = access.next_entry()? {
            map.insert(key, value);
        }

        Ok(map)
    }
}
