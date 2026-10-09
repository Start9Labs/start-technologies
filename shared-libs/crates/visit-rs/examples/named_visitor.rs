use visit_rs::*;

struct FieldInfoVisitor;

impl Visitor for FieldInfoVisitor {
    type Result = String;
}

impl<T> Visit<FieldInfoVisitor> for Named<'_, T> {
    fn visit(&self, _visitor: &mut FieldInfoVisitor) -> String {
        format!(
            "Field: {}, Type: {}",
            self.name.unwrap_or("UNDATA.nameD"),
            std::any::type_name::<T>()
        )
    }
}

#[derive(VisitFields)]
struct Person {
    name: String,
    age: i32,
    email: String,
}

fn main() {
    let person = Person {
        name: "Bob".to_string(),
        age: 25,
        email: "bob@example.com".to_string(),
    };

    let mut visitor = FieldInfoVisitor;

    let field_info: Vec<_> = person.visit_fields_named(&mut visitor).collect();

    println!("Person struct field information:");
    for (i, info) in field_info.iter().enumerate() {
        println!("  Field {}: {}", i, info);
    }

    assert_eq!(field_info.len(), 3);
    println!("\nNamed visitor works!");
}
