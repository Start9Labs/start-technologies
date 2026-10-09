use visit_rs::*;

struct FieldInfoVisitor;

impl Visitor for FieldInfoVisitor {
    type Result = String;
}

impl<'a, T> Visit<FieldInfoVisitor> for Named<'a, Static<T>> {
    fn visit(&self, _visitor: &mut FieldInfoVisitor) -> String {
        format!("Type: {}", std::any::type_name::<T>())
    }
}

#[derive(VisitFields)]
struct Person {
    name: String,
    age: i32,
    email: String,
}

fn main() {
    let mut visitor = FieldInfoVisitor;

    let field_info: Vec<_> = Person::visit_fields_static_named(&mut visitor).collect();

    println!("Person struct field information:");
    for (i, info) in field_info.iter().enumerate() {
        println!("  Field {}: {}", i, info);
    }

    assert_eq!(field_info.len(), 3);
    println!("\nStatic named visitor works!");
}
