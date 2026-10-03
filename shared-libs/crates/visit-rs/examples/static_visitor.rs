use visit_rs::*;

struct TypeNameVisitor;

impl Visitor for TypeNameVisitor {
    type Result = &'static str;
}

impl Visit<TypeNameVisitor> for Static<String> {
    fn visit(&self, _visitor: &mut TypeNameVisitor) -> &'static str {
        "String"
    }
}

impl Visit<TypeNameVisitor> for Static<i32> {
    fn visit(&self, _visitor: &mut TypeNameVisitor) -> &'static str {
        "i32"
    }
}

impl Visit<TypeNameVisitor> for Static<bool> {
    fn visit(&self, _visitor: &mut TypeNameVisitor) -> &'static str {
        "bool"
    }
}

#[derive(VisitFields)]
struct MyStruct {
    name: String,
    age: i32,
    active: bool,
}

fn main() {
    let mut visitor = TypeNameVisitor;

    let field_types: Vec<_> = MyStruct::visit_fields_static(&mut visitor).collect();

    println!("Field types:");
    for (i, ty) in field_types.iter().enumerate() {
        println!("  Field {}: {}", i, ty);
    }

    assert_eq!(field_types, vec!["String", "i32", "bool"]);
    println!("\nStatic visitor works!");
}
