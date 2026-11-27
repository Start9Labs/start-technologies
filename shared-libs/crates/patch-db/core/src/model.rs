use std::marker::PhantomData;

use imbl_value::{InternedString, ObjectMap, OrdValue};
use json_ptr::JsonPointer;

pub struct Pointer<T> {
    ptr: JsonPointer,
    phantom: PhantomData<T>,
}
impl<T> Default for Pointer<T> {
    fn default() -> Self {
        Self {
            ptr: JsonPointer::default(),
            phantom: PhantomData,
        }
    }
}
impl<T> Pointer<T> {
    pub fn unwrap(self) -> JsonPointer {
        self.ptr
    }
}
impl<T> std::ops::Deref for Pointer<T> {
    type Target = JsonPointer;
    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

/// Derived accessors preserve explicit `serde(rename = "...")` field and variant names.
pub trait HasModel: Sized {
    type Model: Model<Self>;
}

mod sealed {
    use super::*;
    pub trait ModelMarker {
        fn into_value(self) -> OrdValue;
        fn from_value(value: OrdValue) -> Self;
        fn as_value<'a>(&'a self) -> &'a OrdValue;
        fn value_as<'a>(value: &'a OrdValue) -> &'a Self;
        fn as_value_mut<'a>(&'a mut self) -> &'a mut OrdValue;
        fn value_as_mut<'a>(value: &'a mut OrdValue) -> &'a mut Self;
    }
    impl<T> ModelMarker for T
    where
        T: From<OrdValue> + Into<OrdValue> + Sized,
        for<'a> &'a T: From<&'a OrdValue> + Into<&'a OrdValue>,
        for<'a> &'a mut T: From<&'a mut OrdValue> + Into<&'a mut OrdValue>,
    {
        fn into_value(self) -> OrdValue {
            self.into()
        }
        fn from_value(value: OrdValue) -> Self {
            value.into()
        }
        fn as_value<'a>(&'a self) -> &'a OrdValue {
            self.into()
        }
        fn value_as<'a>(value: &'a OrdValue) -> &'a Self {
            value.into()
        }
        fn as_value_mut<'a>(&'a mut self) -> &'a mut OrdValue {
            self.into()
        }
        fn value_as_mut<'a>(value: &'a mut OrdValue) -> &'a mut Self {
            value.into()
        }
    }
}

pub trait Model<T>: sealed::ModelMarker + Sized {
    type Model<U>: Model<U>;
}
pub trait ModelExt<T>: Model<T> {
    fn into_value(self) -> OrdValue {
        <Self as sealed::ModelMarker>::into_value(self)
    }
    fn from_value(value: OrdValue) -> Self {
        <Self as sealed::ModelMarker>::from_value(value)
    }
    fn as_value<'a>(&'a self) -> &'a OrdValue {
        <Self as sealed::ModelMarker>::as_value(self)
    }
    fn value_as<'a>(value: &'a OrdValue) -> &'a Self {
        <Self as sealed::ModelMarker>::value_as(value)
    }
    fn as_value_mut<'a>(&'a mut self) -> &'a mut OrdValue {
        <Self as sealed::ModelMarker>::as_value_mut(self)
    }
    fn value_as_mut<'a>(value: &'a mut OrdValue) -> &'a mut Self {
        <Self as sealed::ModelMarker>::value_as_mut(value)
    }
    fn transmute<U>(self, f: impl FnOnce(OrdValue) -> OrdValue) -> Self::Model<U> {
        Self::Model::<U>::from_value(f(<Self as sealed::ModelMarker>::into_value(self)))
    }
    fn transmute_ref<'a, U>(
        &'a self,
        f: impl for<'b> FnOnce(&'b OrdValue) -> &'b OrdValue,
    ) -> &'a Self::Model<U> {
        Self::Model::<U>::value_as(f(<Self as sealed::ModelMarker>::as_value(self)))
    }
    fn transmute_mut<'a, U>(
        &'a mut self,
        f: impl for<'b> FnOnce(&'b mut OrdValue) -> &'b mut OrdValue,
    ) -> &'a mut Self::Model<U> {
        Self::Model::<U>::value_as_mut(f(<Self as sealed::ModelMarker>::as_value_mut(self)))
    }
    fn children_mut<'a>(
        &'a mut self,
    ) -> impl IntoIterator<Item = (&'a InternedString, &'a mut OrdValue)> + Send + Sync {
        ModelExt::<T>::as_value_mut(self)
            .as_object_mut()
            .into_iter()
            .flat_map(|o| o.entries_mut())
    }
}
impl<T, M: Model<T>> ModelExt<T> for M {}

pub trait DestructureMut {
    type Destructured<'a>
    where
        Self: 'a;
    fn destructure_mut<'a>(&'a mut self) -> Self::Destructured<'a>;
}

#[cfg(test)]
mod test {
    use std::marker::PhantomData;

    use imbl_value::{from_value, ord_json, to_ord_value, OrdValue};
    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Serialize};

    use crate as patch_db;

    /// &mut Model<T> <=> &mut Value
    #[repr(transparent)]
    #[derive(Debug)]
    pub struct Model<T> {
        value: OrdValue,
        phantom: PhantomData<T>,
    }
    impl<T: DeserializeOwned> Model<T> {
        pub fn de(self) -> Result<T, imbl_value::Error> {
            from_value(self.value)
        }
    }
    impl<T: Serialize> Model<T> {
        pub fn ser(&mut self, value: &T) -> Result<(), imbl_value::Error> {
            self.value = to_ord_value(value)?;
            Ok(())
        }
    }
    impl<T> Clone for Model<T> {
        fn clone(&self) -> Self {
            Self {
                value: self.value.clone(),
                phantom: PhantomData,
            }
        }
    }
    impl<T> From<OrdValue> for Model<T> {
        fn from(value: OrdValue) -> Self {
            Self {
                value,
                phantom: PhantomData,
            }
        }
    }
    impl<T> From<Model<T>> for OrdValue {
        fn from(value: Model<T>) -> Self {
            value.value
        }
    }
    impl<'a, T> From<&'a OrdValue> for &'a Model<T> {
        fn from(value: &'a OrdValue) -> Self {
            unsafe { std::mem::transmute(value) }
        }
    }
    impl<'a, T> From<&'a Model<T>> for &'a OrdValue {
        fn from(value: &'a Model<T>) -> Self {
            unsafe { std::mem::transmute(value) }
        }
    }
    impl<'a, T> From<&'a mut OrdValue> for &mut Model<T> {
        fn from(value: &'a mut OrdValue) -> Self {
            unsafe { std::mem::transmute(value) }
        }
    }
    impl<'a, T> From<&'a mut Model<T>> for &mut OrdValue {
        fn from(value: &'a mut Model<T>) -> Self {
            unsafe { std::mem::transmute(value) }
        }
    }
    impl<T> patch_db::Model<T> for Model<T> {
        type Model<U> = Model<U>;
    }

    #[derive(crate::HasModel)]
    #[model = "Model<Self>"]
    struct Foo {
        #[allow(dead_code)]
        a: Bar,
    }

    #[derive(crate::HasModel)]
    #[model = "Model<Self>"]
    struct Bar {
        #[allow(dead_code)]
        b: String,
    }

    #[derive(Deserialize, Serialize, crate::HasModel)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    #[model = "Model<Self>"]
    struct RenamedFields {
        #[serde(rename = "legacyName")]
        current_name: String,
        #[serde(default, rename = "legacyKey")]
        current_key: String,
        other_field: String,
    }

    #[test]
    fn accessors_preserve_serialized_names_with_combined_attributes() {
        let original = RenamedFields {
            current_name: "name".into(),
            current_key: "key".into(),
            other_field: "other".into(),
        };
        let mut model = Model::<RenamedFields>::from(to_ord_value(&original).unwrap());
        assert_eq!(model.as_current_name().clone().de().unwrap(), "name");
        assert_eq!(model.as_current_key().clone().de().unwrap(), "key");
        assert_eq!(model.as_other_field().clone().de().unwrap(), "other");
        model.as_current_key_mut().ser(&"changed".into()).unwrap();
        assert_eq!(
            OrdValue::from(model),
            ord_json!({"legacyName": "name", "legacyKey": "changed", "otherField": "other"})
        );
    }

    #[derive(Deserialize, Serialize, crate::HasModel)]
    #[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
    #[model = "Model<Self>"]
    enum RenamedVariant {
        #[serde(rename = "legacy")]
        CurrentName(String),
        DerivedName(String),
    }

    #[test]
    fn variant_accessors_preserve_explicit_and_container_names() {
        for variant in [
            RenamedVariant::CurrentName("key".into()),
            RenamedVariant::DerivedName("key".into()),
        ] {
            let encoded = to_ord_value(&variant).unwrap();
            let model = Model::<RenamedVariant>::from(encoded.clone());
            let matched = model.into_match();
            let key = match &matched {
                RenamedVariantMatchModel::CurrentName(key)
                | RenamedVariantMatchModel::DerivedName(key) => key.clone().de().unwrap(),
                RenamedVariantMatchModel::Error(_) => panic!("Serialized variant must match"),
            };
            assert_eq!(key, "key");
            assert_eq!(
                OrdValue::from(Model::<RenamedVariant>::from_match(matched)),
                encoded
            );
        }
    }

    #[test]
    fn unknown_variants_preserve_the_value_through_model_ext() {
        let unknown = ord_json!({"kind": "unknown", "value": "key"});
        let mut model = Model::<RenamedVariant>::from(unknown.clone());
        assert!(matches!(
            model.as_match(),
            RenamedVariantMatchModelRef::Error(value) if value == &unknown
        ));
        assert!(matches!(
            model.as_match_mut(),
            RenamedVariantMatchModelMut::Error(value) if &*value == &unknown
        ));
        assert!(matches!(
            model.into_match(),
            RenamedVariantMatchModel::Error(value) if value == unknown
        ));
    }

    fn mutate_fn(v: &mut Model<Foo>) {
        let a = v.as_a_mut();
        a.as_b_mut().ser(&"NotThis".into()).unwrap();
        a.as_b_mut().ser(&"Replaced".into()).unwrap();
    }

    #[test]
    fn test() {
        let mut model = Model::<Foo>::from(imbl_value::ord_json!({
            "a": {
                "b": "ReplaceMe"
            }
        }));
        mutate_fn(&mut model);
        mutate_fn(&mut model);
        assert_eq!(
            crate::model::sealed::ModelMarker::as_value(&model),
            &ord_json!({
                "a": {
                    "b": "Replaced"
                }
            })
        )
    }
}
