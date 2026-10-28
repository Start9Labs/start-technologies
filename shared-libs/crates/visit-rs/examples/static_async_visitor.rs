use futures::StreamExt;
use visit_rs::*;

struct AsyncTypeVisitor;

impl Visitor for AsyncTypeVisitor {
    type Result = String;
}

impl VisitAsync<AsyncTypeVisitor> for Static<String> {
    async fn visit_async(&self, _visitor: &mut AsyncTypeVisitor) -> String {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        "String".to_string()
    }
}

impl VisitAsync<AsyncTypeVisitor> for Static<i32> {
    async fn visit_async(&self, _visitor: &mut AsyncTypeVisitor) -> String {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        "i32".to_string()
    }
}

impl VisitAsync<AsyncTypeVisitor> for Static<bool> {
    async fn visit_async(&self, _visitor: &mut AsyncTypeVisitor) -> String {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        "bool".to_string()
    }
}

impl<'a> VisitAsync<AsyncTypeVisitor> for Named<'a, Static<String>> {
    async fn visit_async(&self, _visitor: &mut AsyncTypeVisitor) -> String {
        format!("Named({}, String)", self.name.unwrap_or("UNDATA.nameD"))
    }
}

impl<'a> VisitAsync<AsyncTypeVisitor> for Named<'a, Static<i32>> {
    async fn visit_async(&self, _visitor: &mut AsyncTypeVisitor) -> String {
        format!("Named({}, i32)", self.name.unwrap_or("UNDATA.nameD"))
    }
}

impl<'a> VisitAsync<AsyncTypeVisitor> for Named<'a, Static<bool>> {
    async fn visit_async(&self, _visitor: &mut AsyncTypeVisitor) -> String {
        format!("Named({}, bool)", self.name.unwrap_or("UNDATA.nameD"))
    }
}

#[derive(VisitFields)]
struct Config {
    host: String,
    port: i32,
    enabled: bool,
}

#[derive(VisitFields)]
#[visit(rename = "EmptyStructure")]
#[allow(unused)]
struct Empty {}

#[derive(VisitFields)]
#[visit(rename = "ServerSettings")]
struct Settings {
    timeout: i32,
}

#[tokio::main]
async fn main() {
    let mut visitor = AsyncTypeVisitor;

    println!("Testing VisitFieldsStaticAsync:");
    let field_types: Vec<_> = Config::visit_fields_static_async(&mut visitor)
        .collect()
        .await;
    for (i, ty) in field_types.iter().enumerate() {
        println!("  Field {}: {}", i, ty);
    }
    assert_eq!(field_types, vec!["String", "i32", "bool"]);

    println!("\nTesting VisitFieldsStaticNamedAsync:");
    let field_types_named: Vec<_> = Config::visit_fields_static_named_async(&mut visitor)
        .collect()
        .await;
    for (i, ty) in field_types_named.iter().enumerate() {
        println!("  Field {}: {}", i, ty);
    }
    assert_eq!(
        field_types_named,
        vec![
            "Named(host, String)",
            "Named(port, i32)",
            "Named(enabled, bool)"
        ]
    );

    println!("\nTesting StructInfo:");
    println!("  Config::DATA.name = {}", Config::DATA.name);
    println!(
        "  Config::DATA.named_fields = {}",
        Config::DATA.named_fields
    );
    println!("  Config::DATA.field_count = {}", Config::DATA.field_count);
    assert_eq!(Config::DATA.name, "Config");
    assert_eq!(Config::DATA.named_fields, true);
    assert_eq!(Config::DATA.field_count, 3);

    println!("\n  Empty::DATA.name = {}", Empty::DATA.name);
    println!("  Empty::DATA.named_fields = {}", Empty::DATA.named_fields);
    println!("  Empty::DATA.field_count = {}", Empty::DATA.field_count);
    assert_eq!(Empty::DATA.name, "EmptyStructure");
    assert_eq!(Empty::DATA.named_fields, true);
    assert_eq!(Empty::DATA.field_count, 0);

    println!("\n  Settings::DATA.name = {}", Settings::DATA.name);
    println!(
        "  Settings::DATA.named_fields = {}",
        Settings::DATA.named_fields
    );
    println!(
        "  Settings::DATA.field_count = {}",
        Settings::DATA.field_count
    );
    assert_eq!(Settings::DATA.name, "ServerSettings");
    assert_eq!(Settings::DATA.named_fields, true);
    assert_eq!(Settings::DATA.field_count, 1);

    println!("\nAll async static visitors and StructInfo work!");
}
