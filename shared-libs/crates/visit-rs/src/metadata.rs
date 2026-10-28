/// Rich metadata representation that can be stored as 'static and is Send + Sync
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MetaValue {
    /// A literal string value
    Str(&'static str),
    /// A literal boolean value
    Bool(bool),
    /// A literal integer value
    Int(i64),
    /// A literal float value (stored as string to maintain Eq/Hash)
    Float(&'static str),
    /// A path/identifier
    Path(&'static str),
    /// An unparsed value (fallback)
    Unparsed(&'static str),
}

/// Represents a parsed attribute
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AttributeMeta {
    /// A simple path attribute like `#[visit(skip)]`
    Path { path: &'static str },
    /// A name-value attribute like `#[visit(rename = "foo")]`
    NameValue {
        path: &'static str,
        name: &'static str,
        value: MetaValue,
    },
    /// A list attribute like `#[visit(rename_all = "snake_case")]`
    List {
        path: &'static str,
        items: &'static [AttributeMeta],
    },
    /// Fallback for unparseable attributes
    Unparsed {
        path: &'static str,
        tokens: &'static str,
    },
}
