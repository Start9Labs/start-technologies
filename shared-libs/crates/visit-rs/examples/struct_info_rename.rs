use visit_rs::*;

#[derive(VisitFields)]
struct DefaultName {
    field: String,
}

#[derive(VisitFields)]
#[visit(rename = "CustomName")]
struct RenamedWithVisit {
    field: String,
}

#[derive(VisitFields)]
#[visit(rename = "AnotherCustomName")]
struct AnotherRenamed {
    field1: i32,
    field2: bool,
}

fn main() {
    println!("Testing StructInfo DATA.name constant:");

    println!("\nDefaultName (no rename):");
    println!("  DATA.name: {}", DefaultName::DATA.name);
    println!("  DATA.named_fields: {}", DefaultName::DATA.named_fields);
    println!("  DATA.field_count: {}", DefaultName::DATA.field_count);
    assert_eq!(DefaultName::DATA.name, "DefaultName");
    assert_eq!(DefaultName::DATA.named_fields, true);
    assert_eq!(DefaultName::DATA.field_count, 1);

    println!("\nRenamedWithVisit:");
    println!("  DATA.name: {}", RenamedWithVisit::DATA.name);
    println!(
        "  DATA.named_fields: {}",
        RenamedWithVisit::DATA.named_fields
    );
    println!("  DATA.field_count: {}", RenamedWithVisit::DATA.field_count);
    assert_eq!(RenamedWithVisit::DATA.name, "CustomName");
    assert_eq!(RenamedWithVisit::DATA.named_fields, true);
    assert_eq!(RenamedWithVisit::DATA.field_count, 1);

    println!("\nAnotherRenamed:");
    println!("  DATA.name: {}", AnotherRenamed::DATA.name);
    println!("  DATA.named_fields: {}", AnotherRenamed::DATA.named_fields);
    println!("  DATA.field_count: {}", AnotherRenamed::DATA.field_count);
    assert_eq!(AnotherRenamed::DATA.name, "AnotherCustomName");
    assert_eq!(AnotherRenamed::DATA.named_fields, true);
    assert_eq!(AnotherRenamed::DATA.field_count, 2);

    println!("\nAll rename tests passed!");
}
