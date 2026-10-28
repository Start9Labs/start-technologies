#![cfg(feature = "meta")]
#[cfg(feature = "meta")]
use visit_rs::{StructInfo, VisitFields, Visitor, metadata::AttributeMeta};

#[cfg(feature = "meta")]
#[derive(VisitFields)]
#[visit(some_attr = "value")]
#[visit(another_attr)]
struct MyStruct {
    #[visit(field_attr = "field_value")]
    field1: String,
    field2: i32,
}

#[cfg(feature = "meta")]
struct MetadataCollector {
    metadata: Vec<&'static [AttributeMeta]>,
}

#[cfg(feature = "meta")]
impl Visitor for MetadataCollector {
    type Result = ();
}

#[cfg(feature = "meta")]
impl<'a, T> visit_rs::Visit<MetadataCollector> for visit_rs::Named<'a, T> {
    fn visit(&self, visitor: &mut MetadataCollector) -> () {
        visitor.metadata.push(self.metadata);
    }
}

#[cfg(feature = "meta")]
#[test]
fn test_struct_metadata() {
    let attrs = &MyStruct::DATA.metadata;
    assert_eq!(attrs.len(), 2);

    // Check first attribute: #[visit(some_attr = "value")]
    match &attrs[0] {
        AttributeMeta::List { path, items } => {
            // It's parsed as a List containing a NameValue
            assert_eq!(*path, "visit");
            assert_eq!(items.len(), 1);
            match &items[0] {
                AttributeMeta::NameValue { name, value, .. } => {
                    assert_eq!(*name, "some_attr");
                    match value {
                        visit_rs::metadata::MetaValue::Str(s) => assert_eq!(*s, "value"),
                        _ => panic!("Expected Str value"),
                    }
                }
                _ => panic!("Expected NameValue in list"),
            }
        }
        _ => panic!("Expected List attribute, got {:?}", attrs[0]),
    }

    // Check second attribute: #[visit(another_attr)]
    match &attrs[1] {
        AttributeMeta::List { path, items } => {
            assert_eq!(*path, "visit");
            assert_eq!(items.len(), 1);
            match &items[0] {
                AttributeMeta::Path { path } => {
                    assert_eq!(*path, "another_attr");
                }
                _ => panic!("Expected Path in list"),
            }
        }
        _ => panic!("Expected List attribute"),
    }
}

#[cfg(feature = "meta")]
#[test]
fn test_field_metadata() {
    use visit_rs::VisitFieldsNamed;

    let my_struct = MyStruct {
        field1: "test".to_string(),
        field2: 42,
    };

    let mut visitor = MetadataCollector {
        metadata: Vec::new(),
    };
    let _: Vec<()> = my_struct.visit_fields_named(&mut visitor).collect();

    // field1 should have one attribute
    assert_eq!(visitor.metadata[0].len(), 1);
    match &visitor.metadata[0][0] {
        AttributeMeta::List { path, items } => {
            assert_eq!(*path, "visit");
            assert_eq!(items.len(), 1);
            match &items[0] {
                AttributeMeta::NameValue { name, value, .. } => {
                    assert_eq!(*name, "field_attr");
                    match value {
                        visit_rs::metadata::MetaValue::Str(s) => assert_eq!(*s, "field_value"),
                        _ => panic!("Expected Str value"),
                    }
                }
                _ => panic!("Expected NameValue in list"),
            }
        }
        _ => panic!("Expected List attribute, got {:?}", visitor.metadata[0][0]),
    }

    // field2 should have no attributes
    assert_eq!(visitor.metadata[1].len(), 0);
}
