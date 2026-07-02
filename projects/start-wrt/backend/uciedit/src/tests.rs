use super::*; // Access items from lib.rs
use inpt::split::SingleQuoted;
use uciedit_macros::TypedSection; // Import the derive macro

// Helper to normalize whitespace for comparisons
fn normalize(s: String) -> String {
    s.replace("\t", "    ")
}

#[derive(TypedSection, PartialEq, Eq, Debug, Default)]
struct Bar {
    always: i32,
    yes: Option<i32>,
    no: Option<i32>,
    many: Vec<i32>,
    #[uci(default)] // Test explicit default for non-Option/Vec
    num_default: i32,
    #[uci(default_value = "Some(\"default_val\".to_string())")]
    opt_str_default_val: Option<String>,
    #[uci(default_value = "vec![\"a\".to_string(), \"b\".to_string()]")]
    vec_str_default_val: Vec<String>,
}

#[test]
fn test_read_section() {
    let original = r"config bar
    option always 0
    option yes 1
    list many 2
    list many 3
    list many 4
";

    let expected = Bar {
        always: 0,
        yes: Some(1),
        no: None,
        many: vec![2, 3, 4],
        num_default: 0, // Default for i32
        opt_str_default_val: Some("default_val".to_string()),
        vec_str_default_val: vec!["a".to_string(), "b".to_string()],
    };
    let arena = Arena::new();
    let parsed: Bar = Config::parse_str(&arena, original).unwrap().sections[0]
        .get()
        .unwrap();

    println!(
        "===Original===\n{original}\n===Parsed===\n{parsed:#?}\n===Expected===\n{expected:#?}\n====="
    );
    assert_eq!(parsed, expected);
}

#[test]
fn test_append_section() {
    let original = r"config foo
    option hello world
    # a comment here
";

    let expected = r"config foo
    option hello world
    # a comment here

config bar appended
    option always '0'
    option yes '1'
    list many '2'
    list many '3'
    list many '4'
    option num_default '0'
    option opt_str_default_val 'default_val'
    list vec_str_default_val 'a'
    list vec_str_default_val 'b'
";

    let arena = Arena::new();
    let mut config = Config::parse_str(&arena, original).unwrap();
    config
        .append(
            &Bar {
                always: 0,
                yes: Some(1),
                no: None,
                many: vec![2, 3, 4],
                num_default: 0,
                opt_str_default_val: Some("default_val".to_string()),
                vec_str_default_val: vec!["a".to_string(), "b".to_string()],
            },
            Some("appended"),
        )
        .unwrap();
    let edited = config.dump_str();

    println!(
        "===Original===\n{original}\n===Edited===\n{edited}\n===Expected===\n{expected}\n====="
    );
    assert_eq!(normalize(edited), normalize(expected.to_string()));
}

#[derive(TypedSection, PartialEq, Eq, Debug)]
#[uci(ty = "bar")]
struct EditBar {
    always: i32,
    yes: Option<i32>,
    no: Option<i32>,
    many: Vec<i32>,
    few: Vec<i32>,
}

#[test]
fn test_edit_section() {
    let original = r"# top comment
config bar named
    # always comment
    option always 0

    # no comment
    option no 1

    # many comment
    list many 2

    # few comment
    list few 3
    list few 4
    list few 5

    # ignored comment
    option ignored 6

# bottom comment
config other
    option something here
";

    let expected = r"# top comment
config bar named
    # always comment
    option always '0'

    # no comment

    # many comment
    list many '2'

    # few comment
    list few '5'

    # ignored comment
    option ignored 6
    option yes '1'
    list many '3'
    list many '4'

# bottom comment
config other
    option something here
";

    let arena = Arena::new();
    let mut config = Config::parse_str(&arena, original).unwrap();
    for s in &mut config.sections {
        if s.ty() == "bar" {
            s.set(&EditBar {
                always: 0,
                yes: Some(1),
                no: None,
                many: vec![2, 3, 4],
                few: vec![5],
            })
            .unwrap();
        }
    }
    let edited = config.dump_str();
    println!(
        "===Original===\n{original}\n===Edited===\n{edited}\n===Expected===\n{expected}\n====="
    );
    assert_eq!(normalize(edited), normalize(expected.to_string()));
}

#[test]
fn test_remove_sections() {
    let original = r"# section 1
config retain
    option foo bar
    # comment 1

# section 2
config remove
    option foo bar
    # comment 2

# section 3
config remove
    option foo bar
    # comment 3

# section 4
config retain
    option foo bar
    # comment 4

# section 5
config remove
config retain
";

    let expected = r"# section 1
config retain
    option foo bar
    # comment 1

# section 4
config retain
    option foo bar
    # comment 4

config retain
";

    let arena = Arena::new();
    let mut config = Config::parse_str(&arena, original).unwrap();
    config.sections.retain(|s| s.ty() == "retain");
    let edited = config.dump_str();

    println!(
        "===Original===\n{original}\n===Edited===\n{edited}\n===Expected===\n{expected}\n====="
    );
    assert_eq!(normalize(edited), normalize(expected.to_string()));
}

#[test]
fn test_line_parse_errors() {
    assert!(matches!(
        Line::parse("config foo 'invalid name", 0),
        Err(Error::BadSection { .. })
    ));
    assert!(matches!(
        Line::parse("config ", 0),
        Err(Error::BadSection { .. })
    ));
    assert!(matches!(
        Line::parse("\toption name_only", 0),
        Err(Error::BadOption { .. })
    ));
    assert!(matches!(
        Line::parse("\tlist name_only", 0),
        Err(Error::BadList { .. })
    ));
    assert!(matches!(
        Line::parse("unknown_keyword foo bar", 0),
        Err(Error::UnknownKeyword { .. })
    ));
}

#[test]
fn test_token_functionality() {
    let arena = Arena::new();
    let token_w = Token::from_str("word", &arena);
    assert_eq!(token_w.as_str(), "word");
    assert_eq!(format!("{}", token_w), "word");

    let token_sq_needed = Token::from_string("hello world".to_string(), &arena);
    assert_eq!(token_sq_needed.as_str(), "hello world");
    assert_eq!(format!("{}", token_sq_needed), "'hello world'");

    let token_sq_manual = Token::Sq(SingleQuoted { inner: "sq word" });
    assert_eq!(token_sq_manual.as_str(), "sq word");
    assert_eq!(format!("{}", token_sq_manual), "'sq word'");

    assert_eq!(Token::from_bool(true).as_str(), "1");
    assert_eq!(Token::from_bool(false).as_str(), "0");

    assert!(Token::from_str("true", &arena).parse_bool(0).unwrap());
    assert!(!Token::from_str("0", &arena).parse_bool(0).unwrap());
    assert!(matches!(
        Token::from_str("maybe", &arena).parse_bool(0),
        Err(Error::ValueBoolean { .. })
    ));

    assert_eq!(
        Token::from_str("123", &arena)
            .parse_fromstr::<i32>(0)
            .unwrap(),
        123
    );
    assert!(matches!(
        Token::from_str("abc", &arena).parse_fromstr::<i32>(0),
        Err(Error::ValueMsg { .. })
    ));
}

#[derive(TypedSection, PartialEq, Eq, Debug)]
#[uci(ty = "custom_type")]
struct RenamedTypeSection {
    #[uci(rename = "old_name")]
    new_name: String,
    val: bool,
}

#[test]
fn test_uci_section_rename_and_type_override() {
    assert_eq!(RenamedTypeSection::TY, "custom_type");
    let original = r"config custom_type 'my_section'
    option old_name 'test_value'
    option val true
";
    let expected_obj = RenamedTypeSection {
        new_name: "test_value".to_string(),
        val: true,
    };

    let arena = Arena::new();
    let config = Config::parse_str(&arena, original).unwrap();
    assert_eq!(config.sections[0].ty(), "custom_type");
    assert_eq!(config.sections[0].name().unwrap(), "my_section");
    let parsed: RenamedTypeSection = config.sections[0].get().unwrap();
    assert_eq!(parsed, expected_obj);

    let arena2 = Arena::new();
    let mut config2 = Config::parse_str(&arena2, "").unwrap();
    config2.append(&expected_obj, Some("my_section")).unwrap();
    let written = config2.dump_str();
    let expected_written = r"config custom_type my_section
    option old_name 'test_value'
    option val '1'
";
    assert_eq!(normalize(written), normalize(expected_written.to_string()));
}

#[derive(TypedSection, PartialEq, Eq, Debug)]
struct DefaultDemo {
    #[uci(default)]
    my_int: i32, // uses i32::default()
    my_opt_int: Option<i32>, // uses None
    my_vec_int: Vec<i32>,    // uses vec![]
    #[uci(default_value = r#"Some("hello".to_string())"#)]
    my_opt_str: Option<String>,
    #[uci(default_value = r#"vec![10, 20]"#)]
    my_default_vec: Vec<i32>,
}

#[test]
fn test_uci_section_defaults() {
    let original_empty_section = "config defaultdemo 'test'";
    let arena = Arena::new();
    let parsed: DefaultDemo = Config::parse_str(&arena, original_empty_section)
        .unwrap()
        .sections[0]
        .get()
        .unwrap();

    assert_eq!(parsed.my_int, 0);
    assert_eq!(parsed.my_opt_int, None);
    assert_eq!(parsed.my_vec_int, Vec::<i32>::new());
    assert_eq!(parsed.my_opt_str, Some("hello".to_string()));
    assert_eq!(parsed.my_default_vec, vec![10, 20]);

    let original_partial = r"config defaultdemo 'test'
    option my_int 5
    list my_vec_int 1
";
    let arena2 = Arena::new();
    let parsed_partial: DefaultDemo = Config::parse_str(&arena2, original_partial)
        .unwrap()
        .sections[0]
        .get()
        .unwrap();
    assert_eq!(parsed_partial.my_int, 5);
    assert_eq!(parsed_partial.my_opt_int, None);
    assert_eq!(parsed_partial.my_vec_int, vec![1]);
    assert_eq!(parsed_partial.my_opt_str, Some("hello".to_string()));
    assert_eq!(parsed_partial.my_default_vec, vec![10, 20]);
}

#[derive(TypedSection, PartialEq, Eq, Debug)]
struct BoolFieldTest {
    is_enabled: bool,
    is_active: bool,
    is_present: Option<bool>,
    bits: Vec<bool>,
}

#[test]
fn test_uci_section_bool_field() {
    let original = r"config boolfieldtest
    option is_enabled yes
    option is_active 0
    option is_present true
    list bits off
    list bits yes
    list bits on
";
    let arena = Arena::new();
    let parsed: BoolFieldTest = Config::parse_str(&arena, original).unwrap().sections[0]
        .get()
        .unwrap();
    assert!(parsed.is_enabled);
    assert!(!parsed.is_active);
    assert_eq!(parsed.is_present, Some(true));

    let arena2 = Arena::new();
    let mut config2 = Config::parse_str(&arena2, "").unwrap();
    config2.append(&parsed, None).unwrap();
    let written = config2.dump_str();
    let expected_written = r"config boolfieldtest
    option is_enabled '1'
    option is_active '0'
    option is_present '1'
    list bits '0'
    list bits '1'
    list bits '1'
";
    assert_eq!(normalize(written), normalize(expected_written.to_string()));
}

#[derive(TypedSection, Debug)]
struct RequiredField {
    must_exist: String,
}

#[test]
fn test_uci_section_missing_required_field() {
    let original = "config requiredfield 'test'"; // must_exist is missing
    let arena = Arena::new();
    let config = Config::parse_str(&arena, original).unwrap();
    let result: Result<RequiredField, Error> = config.sections[0].get();
    assert!(matches!(
        result,
        Err(Error::MissingOption { missing, .. }) if missing == "must_exist"
    ));
}

#[test]
fn test_sections_mut_push_to_empty_config() {
    let expected = r"config bar
    option always '1'
    option num_default '0'
";
    let arena = Arena::new();
    let mut config = Config::parse_str(&arena, "").unwrap();
    config
        .append(
            &Bar {
                always: 1,
                yes: None,
                no: None,
                many: vec![],
                num_default: 0,
                opt_str_default_val: None, // Test with None for default_value field
                vec_str_default_val: vec![], // Test with empty for default_value field
            },
            None,
        )
        .unwrap();
    let edited = config.dump_str();
    assert_eq!(normalize(edited), normalize(expected.to_string()));
}

#[derive(TypedSection, Debug, PartialEq)]
struct AnotherType {
    value: String,
}

#[test]
fn test_sections_mut_set_non_matching_type() {
    let original = r"config bar 'mybar'
    option always 123
";
    let arena = Arena::new();
    let mut config = Config::parse_str(&arena, original).unwrap();
    assert_eq!(config.sections[0].ty(), "bar");
    // Try to set it with a struct of a different TypedSection type
    let result = config.sections[0].set(&AnotherType {
        value: "test".to_string(),
    });

    assert!(matches!(
        result,
        Err(Error::ExpectedSectionType { expected, found, .. })
        if expected == AnotherType::TY && found == Bar::TY
    ));
}

#[test]
fn test_parse_empty_config() {
    let original = "";
    let arena = Arena::new();
    let config = Config::parse_str(&arena, original).unwrap();
    assert!(config.sections.is_empty()); // No sections

    let arena2 = Arena::new();
    let config2 = Config::parse_str(&arena2, original).unwrap();
    let edited = config2.dump_str();
    assert_eq!(edited, "");
}

#[test]
fn test_parse_config_with_only_comments_and_empty_lines() {
    let original = r"# This is a comment

# Another comment
";
    let arena = Arena::new();
    let config = Config::parse_str(&arena, original).unwrap();
    assert!(config.sections.is_empty()); // No sections

    let arena2 = Arena::new();
    let config2 = Config::parse_str(&arena2, original).unwrap();
    let edited = config2.dump_str();
    assert_eq!(normalize(edited), normalize(original.to_string()));
}

#[test]
fn test_round_trip_complex_config() {
    #[derive(TypedSection, PartialEq, Eq, Debug, Default)]
    struct ComplexSection {
        name: String,
        value: Option<i32>,
        items: Vec<String>,
        enabled: bool,
        #[uci(rename = "old_flag")]
        new_flag: Option<bool>,
        with_quotes: String,
    }

    let section1 = ComplexSection {
        name: "Section One".to_string(),
        value: Some(42),
        items: vec!["item1".to_string(), "item2 with spaces".to_string()],
        enabled: true,
        new_flag: Some(false),
        with_quotes: "\'custom\'".to_string(),
    };
    let section2 = ComplexSection {
        name: "Section Two".to_string(),
        value: None,
        items: vec![],
        enabled: false,
        new_flag: None,
        with_quotes: "\"custom\"".to_string(),
    };

    let arena = Arena::new();
    let mut config = Config::parse_str(&arena, "").unwrap();
    config.append(&section1, Some("first")).unwrap();
    config.append(&section2, Some("second")).unwrap();
    let original_config_str = config.dump_str();

    let expected_written_config = r#"config complexsection first
    option name 'Section One'
    option value '42'
    list items 'item1'
    list items 'item2 with spaces'
    option enabled '1'
    option old_flag '0'
    option with_quotes "'custom'"

config complexsection second
    option name 'Section Two'
    option enabled '0'
    option with_quotes '"custom"'
"#;
    println!(
        "===Generated Config===\n{}\n===Expected Config===\n{}\n===",
        original_config_str, expected_written_config
    );

    let arena2 = Arena::new();
    let config2 = Config::parse_str(&arena2, &original_config_str).unwrap();
    assert_eq!(config2.sections.len(), 2);
    let parsed_s1: ComplexSection = config2.sections[0].get().unwrap();
    let parsed_s2: ComplexSection = config2.sections[1].get().unwrap();

    assert_eq!(parsed_s1, section1);
    assert_eq!(parsed_s2, section2);

    // Also check the string output for consistency (order might vary for new fields)
    // This is a bit more fragile but good for a smoke test.
    // The order of options within a section is preserved for existing options,
    // but new options (from fields not in original config) are appended.
    assert_eq!(
        normalize(original_config_str),
        normalize(expected_written_config.to_string())
    );
}

#[test]
fn test_sections_mut_reset_applies_removals() {
    let original = r"config item one
config item two
config item three
";
    let expected_after_removal_then_reset = r"config item two
config item three
";
    let expected_after_removal_in_reset = r"config item one
config item three
";

    let arena = Arena::new();
    let mut config = Config::parse_str(&arena, original).unwrap();
    // Remove 'two' (index 1)
    config.sections.remove(1);
    let names: Vec<_> = config
        .sections
        .iter()
        .map(|s| s.name().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["one", "three"],
        "Only 'one' and 'three' should remain"
    );
    let edited = config.dump_str();
    assert_eq!(
        normalize(edited),
        normalize(expected_after_removal_in_reset.to_string())
    );

    let arena2 = Arena::new();
    let mut config2 = Config::parse_str(&arena2, original).unwrap();
    // Remove 'one' (index 0)
    config2.sections.remove(0);
    let names2: Vec<_> = config2
        .sections
        .iter()
        .map(|s| s.name().unwrap().to_string())
        .collect();
    assert_eq!(
        names2,
        vec!["two", "three"],
        "Only 'two' and 'three' should remain"
    );
    let edited_for_removal = config2.dump_str();
    assert_eq!(
        normalize(edited_for_removal),
        normalize(expected_after_removal_then_reset.to_string())
    );
}

#[test]
fn test_appended_section_readable_without_roundtrip() {
    let arena = Arena::new();
    let mut config = Config::parse_str(&arena, "").unwrap();

    let section = Bar {
        always: 42,
        yes: Some(7),
        no: None,
        many: vec![1, 2],
        num_default: 0,
        opt_str_default_val: Some("hello".into()),
        vec_str_default_val: vec!["x".into()],
    };
    config.append(&section, Some("test")).unwrap();

    // Read directly from the appended section — no dump+parse round-trip.
    let read_back: Bar = config.sections[0].get().unwrap();
    assert_eq!(read_back, section);
}
