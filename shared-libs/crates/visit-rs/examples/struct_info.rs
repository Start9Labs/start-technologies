use visit_rs::*;

#[derive(VisitFields)]
struct NamedStruct {
    field1: String,
    field2: i32,
    field3: bool,
}

#[derive(VisitFields)]
struct TupleStruct(String, i32, bool);

fn main() {
    println!("NamedStruct:");
    println!("  DATA.name: {}", NamedStruct::DATA.name);
    println!("  DATA.named_fields: {}", NamedStruct::DATA.named_fields);
    println!("  DATA.field_count: {}", NamedStruct::DATA.field_count);

    println!("\nTupleStruct:");
    println!("  DATA.name: {}", TupleStruct::DATA.name);
    println!("  DATA.named_fields: {}", TupleStruct::DATA.named_fields);
    println!("  DATA.field_count: {}", TupleStruct::DATA.field_count);

    assert_eq!(NamedStruct::DATA.name, "NamedStruct");
    assert!(NamedStruct::DATA.named_fields);
    assert_eq!(NamedStruct::DATA.field_count, 3);

    assert_eq!(TupleStruct::DATA.name, "TupleStruct");
    assert!(!TupleStruct::DATA.named_fields);
    assert_eq!(TupleStruct::DATA.field_count, 3);

    println!("\nStructInfo trait works correctly!");
}
