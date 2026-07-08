use ts_rs::TS;

use crate::InternedString;

impl TS for InternedString {
    type WithoutGenerics = Self;
    fn name() -> String {
        "string".to_owned()
    }
    fn inline() -> String {
        Self::name()
    }
    fn decl() -> String {
        panic!("InternedString cannot be declared")
    }
    fn decl_concrete() -> String {
        Self::decl()
    }
    fn inline_flattened() -> String {
        panic!("InternedString cannot be flattened")
    }
}

#[cfg(test)]
mod test {
    use ts_rs::TS;

    use crate::InternedString;

    #[derive(TS)]
    struct HasString {
        #[allow(dead_code)]
        s: InternedString,
    }

    #[test]
    fn test_export() {
        HasString::export_to_string().unwrap();
    }
}
