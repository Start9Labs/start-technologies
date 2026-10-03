use visit_rs::*;

// Test that Static, Named<Static<T>>, and Named work with unsized types

struct UnsizedVisitor;

impl Visitor for UnsizedVisitor {
    type Result = String;
}

// Implement Visit for Named<str>
impl<'a> Visit<UnsizedVisitor> for Named<'a, str> {
    fn visit(&self, _visitor: &mut UnsizedVisitor) -> String {
        format!(
            "Named({}, value='{}')",
            self.name.unwrap_or("UNDATA.nameD"),
            self.value
        )
    }
}

// Implement Visit for Named<[i32]>
impl<'a> Visit<UnsizedVisitor> for Named<'a, [i32]> {
    fn visit(&self, _visitor: &mut UnsizedVisitor) -> String {
        format!(
            "Named({}, slice len={})",
            self.name.unwrap_or("UNDATA.nameD"),
            self.value.len()
        )
    }
}

// Implement Visit for Static<str> (unsized type)
impl Visit<UnsizedVisitor> for Static<str> {
    fn visit(&self, _visitor: &mut UnsizedVisitor) -> String {
        "str (unsized)".to_string()
    }
}

// Implement Visit for Static<[i32]> (unsized slice)
impl Visit<UnsizedVisitor> for Static<[i32]> {
    fn visit(&self, _visitor: &mut UnsizedVisitor) -> String {
        "[i32] (unsized slice)".to_string()
    }
}

// Implement Visit for Named<Static<str>>
impl<'a> Visit<UnsizedVisitor> for Named<'a, Static<str>> {
    fn visit(&self, _visitor: &mut UnsizedVisitor) -> String {
        format!("Named({}, str)", self.name.unwrap_or("UNDATA.nameD"))
    }
}

// Implement Visit for Named<Static<[i32]>>
impl<'a> Visit<UnsizedVisitor> for Named<'a, Static<[i32]>> {
    fn visit(&self, _visitor: &mut UnsizedVisitor) -> String {
        format!("Named({}, [i32])", self.name.unwrap_or("UNDATA.nameD"))
    }
}

#[test]
fn test_static_with_unsized_str() {
    let mut visitor = UnsizedVisitor;
    let static_str = Static::<str>::new();
    let result = static_str.visit(&mut visitor);
    assert_eq!(result, "str (unsized)");
}

#[test]
fn test_static_with_unsized_slice() {
    let mut visitor = UnsizedVisitor;
    let static_slice = Static::<[i32]>::new();
    let result = static_slice.visit(&mut visitor);
    assert_eq!(result, "[i32] (unsized slice)");
}

#[test]
fn test_named_static_with_unsized_str() {
    let mut visitor = UnsizedVisitor;
    const STATIC_STR: Static<str> = Static::new();
    let named_static = Named {
        name: Some("my_string"),
        #[cfg(feature = "meta")]
        #[cfg(feature = "meta")]
        metadata: &[],
        value: &STATIC_STR,
    };
    let result = named_static.visit(&mut visitor);
    assert_eq!(result, "Named(my_string, str)");
}

#[test]
fn test_named_static_with_unsized_slice() {
    let mut visitor = UnsizedVisitor;
    const STATIC_SLICE: Static<[i32]> = Static::new();
    let named_static = Named {
        name: Some("my_slice"),
        #[cfg(feature = "meta")]
        #[cfg(feature = "meta")]
        metadata: &[],
        value: &STATIC_SLICE,
    };
    let result = named_static.visit(&mut visitor);
    assert_eq!(result, "Named(my_slice, [i32])");
}

#[test]
fn test_named_with_unsized_str() {
    let mut visitor = UnsizedVisitor;
    let hello = "hello";
    let named = Named {
        name: Some("greeting"),
        #[cfg(feature = "meta")]
        #[cfg(feature = "meta")]
        metadata: &[],
        value: hello as &str,
    };
    let result = named.visit(&mut visitor);
    assert_eq!(result, "Named(greeting, value='hello')");
}

#[test]
fn test_named_with_unsized_slice() {
    let mut visitor = UnsizedVisitor;
    let slice: &[i32] = &[1, 2, 3, 4, 5];
    let named = Named {
        name: Some("numbers"),
        #[cfg(feature = "meta")]
        #[cfg(feature = "meta")]
        metadata: &[],
        value: slice,
    };
    let result = named.visit(&mut visitor);
    assert_eq!(result, "Named(numbers, slice len=5)");
}

#[test]
fn test_static_send_sync_with_unsized() {
    // Verify that Static<T: ?Sized> still implements Send + Sync
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    assert_send::<Static<str>>();
    assert_sync::<Static<str>>();

    assert_send::<Static<[i32]>>();
    assert_sync::<Static<[i32]>>();

    assert_send::<Named<'static, Static<str>>>();
    assert_sync::<Named<'static, Static<str>>>();

    assert_send::<Named<'static, Static<[i32]>>>();
    assert_sync::<Named<'static, Static<[i32]>>>();

    // Named doesn't need Send/Sync because it contains a reference
}
