use futures::StreamExt;
use visit_rs::*;

struct AsyncTypeVisitor;

impl Visitor for AsyncTypeVisitor {
    type Result = String;
}

impl VisitAsync<AsyncTypeVisitor> for String {
    async fn visit_async(&self, _visitor: &mut AsyncTypeVisitor) -> String {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        format!("String({})", self)
    }
}

impl VisitAsync<AsyncTypeVisitor> for i32 {
    async fn visit_async(&self, _visitor: &mut AsyncTypeVisitor) -> String {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        format!("i32({})", self)
    }
}

impl VisitAsync<AsyncTypeVisitor> for bool {
    async fn visit_async(&self, _visitor: &mut AsyncTypeVisitor) -> String {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        format!("bool({})", self)
    }
}

impl<'a, T> VisitAsync<AsyncTypeVisitor> for Named<'a, T>
where
    T: VisitAsync<AsyncTypeVisitor> + Sync,
{
    async fn visit_async(&self, visitor: &mut AsyncTypeVisitor) -> String {
        let value_str = self.value.visit_async(visitor).await;
        format!(
            "Named({}, {})",
            self.name.unwrap_or("UNDATA.nameD"),
            value_str
        )
    }
}

#[derive(VisitFields)]
struct Config {
    host: String,
    port: i32,
    enabled: bool,
}

#[tokio::main]
async fn main() {
    let config = Config {
        host: "localhost".to_string(),
        port: 8080,
        enabled: true,
    };

    let mut visitor = AsyncTypeVisitor;

    println!("Testing VisitFieldsAsync:");
    let field_values: Vec<_> = config.visit_fields_async(&mut visitor).collect().await;
    for (i, val) in field_values.iter().enumerate() {
        println!("  Field {}: {}", i, val);
    }
    assert_eq!(
        field_values,
        vec!["String(localhost)", "i32(8080)", "bool(true)"]
    );

    println!("\nTesting VisitFieldsNamedAsync:");
    let field_values_named: Vec<_> = config
        .visit_fields_named_async(&mut visitor)
        .collect()
        .await;
    for (i, val) in field_values_named.iter().enumerate() {
        println!("  Field {}: {}", i, val);
    }
    assert_eq!(
        field_values_named,
        vec![
            "Named(host, String(localhost))",
            "Named(port, i32(8080))",
            "Named(enabled, bool(true))"
        ]
    );

    println!("\nAll async visitors work!");
}
