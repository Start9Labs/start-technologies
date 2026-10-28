use visit_rs::*;

struct TypeVisitor;

impl Visitor for TypeVisitor {
    type Result = String;
}

// Sized types
impl Visit<TypeVisitor> for Static<String> {
    fn visit(&self, _visitor: &mut TypeVisitor) -> String {
        "String (sized)".to_string()
    }
}

impl Visit<TypeVisitor> for Static<i32> {
    fn visit(&self, _visitor: &mut TypeVisitor) -> String {
        "i32 (sized)".to_string()
    }
}

// Unsized types
impl Visit<TypeVisitor> for Static<str> {
    fn visit(&self, _visitor: &mut TypeVisitor) -> String {
        "str (unsized)".to_string()
    }
}

impl Visit<TypeVisitor> for Static<[i32]> {
    fn visit(&self, _visitor: &mut TypeVisitor) -> String {
        "[i32] (unsized slice)".to_string()
    }
}

// Named versions
impl<'a> Visit<TypeVisitor> for Named<'a, Static<String>> {
    fn visit(&self, _visitor: &mut TypeVisitor) -> String {
        format!("Named({}, String)", self.name.unwrap_or("UNDATA.nameD"))
    }
}

impl<'a> Visit<TypeVisitor> for Named<'a, Static<str>> {
    fn visit(&self, _visitor: &mut TypeVisitor) -> String {
        format!(
            "Named({}, str unsized)",
            self.name.unwrap_or("UNDATA.nameD")
        )
    }
}

impl<'a> Visit<TypeVisitor> for Named<'a, Static<[i32]>> {
    fn visit(&self, _visitor: &mut TypeVisitor) -> String {
        format!(
            "Named({}, [i32] unsized)",
            self.name.unwrap_or("UNDATA.nameD")
        )
    }
}

fn main() {
    let mut visitor = TypeVisitor;

    println!("Testing Static<T> with sized types:");
    println!("  {}", Static::<String>::new().visit(&mut visitor));
    println!("  {}", Static::<i32>::new().visit(&mut visitor));

    println!("\nTesting Static<T> with unsized types:");
    println!("  {}", Static::<str>::new().visit(&mut visitor));
    println!("  {}", Static::<[i32]>::new().visit(&mut visitor));

    println!("\nTesting Named<Static<T>> with sized types:");
    {
        const STATIC_STRING: Static<String> = Static::new();
        println!(
            "  {}",
            Named {
                name: Some("my_string"),
                #[cfg(feature = "meta")]
                #[cfg(feature = "meta")]
                metadata: &[],
                value: &STATIC_STRING
            }
            .visit(&mut visitor)
        );
    }

    println!("\nTesting Named<Static<T>> with unsized types:");
    {
        const STATIC_STR: Static<str> = Static::new();
        println!(
            "  {}",
            Named {
                name: Some("my_str"),
                #[cfg(feature = "meta")]
                #[cfg(feature = "meta")]
                metadata: &[],
                value: &STATIC_STR
            }
            .visit(&mut visitor)
        );
    }
    {
        const STATIC_SLICE: Static<[i32]> = Static::new();
        println!(
            "  {}",
            Named {
                name: Some("my_slice"),
                #[cfg(feature = "meta")]
                #[cfg(feature = "meta")]
                metadata: &[],
                value: &STATIC_SLICE
            }
            .visit(&mut visitor)
        );
    }

    println!("\nStatic and Named<Static<T>> support both sized and unsized types!");
}
