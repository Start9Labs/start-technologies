use uciedit_macros::TypedSection;

use super::*; // Access items from lib.rs // Import the derive macro

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

#[derive(TypedSection)]
#[uci(ty = "replace")]
struct ReplaceSection {
    value: String,
}

#[test]
fn test_remove_append_does_not_accumulate_separators() {
    for count in [1, 2] {
        let arena = Arena::new();
        let mut config = Config::parse_str(
            &arena,
            "# retained header\nconfig retain\n\toption value 'original'\n\t# retained body\n",
        )
        .unwrap();
        for _ in 0..count {
            config
                .append(
                    &ReplaceSection {
                        value: "test".into(),
                    },
                    None,
                )
                .unwrap();
        }
        let expected = config.dump_str();
        for cycle in 0..16 {
            let text = arena.alloc(config.dump_str());
            config = Config::parse_str(&arena, text).unwrap();
            config.sections.retain(|section| section.ty() != "replace");
            for _ in 0..count {
                config
                    .append(
                        &ReplaceSection {
                            value: "test".into(),
                        },
                        None,
                    )
                    .unwrap();
            }
            assert_eq!(config.dump_str(), expected, "count {count}, cycle {cycle}");
        }
    }
}

#[test]
fn test_remove_section_keeps_retained_body_and_header_comments() {
    let arena = Arena::new();
    let original = "# first header\nconfig retain first\n\toption value 'first'\n\t# first body\n\n# removed header\n\nconfig remove\n\toption value 'removed'\n\t# removed body\n\n# last header\n\nconfig retain last\n\toption value 'last'\n\t# last body\n";
    let expected = "# first header\nconfig retain first\n\toption value 'first'\n\t# first body\n\n# last header\n\nconfig retain last\n\toption value 'last'\n\t# last body\n";
    let mut config = Config::parse_str(&arena, original).unwrap();
    config.sections.retain(|section| section.ty() == "retain");
    assert_eq!(config.dump_str(), expected);

    let mut config =
        Config::parse_str(&arena, "config retain\n\t# retained body\nconfig remove\n").unwrap();
    config.sections.retain(|section| section.ty() == "retain");
    assert_eq!(config.dump_str(), "config retain\n\t# retained body\n");
}

#[test]
fn test_parse_dump_preserves_separators_and_comments() {
    let arena = Arena::new();
    for original in [
        "",
        "\n\n# file comment\n\n",
        "\n# header\nconfig retain\n\toption value 'test'\n\n",
        "# first\nconfig retain first\n\t# body\n\n\n# second\n\nconfig retain second\n\tlist values 'one' #inline\n\t# final body\n\n",
        "config retain\n\t# body without separator\nconfig retain\n",
    ] {
        let config = Config::parse_str(&arena, original).unwrap();
        assert_eq!(config.dump_str(), original);
        let mut bytes = Vec::new();
        config.dump_io(&mut bytes).unwrap();
        assert_eq!(bytes, original.as_bytes());
    }
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
        Line::parse("\toption", 0),
        Err(Error::BadOption { .. })
    ));
    assert!(matches!(
        Line::parse("\toption a 'b' 'c'", 0),
        Err(Error::BadOption { .. })
    ));
    assert!(matches!(
        Line::parse("\tlist a \"unterminated", 0),
        Err(Error::BadList { .. })
    ));
    assert!(matches!(
        Line::parse("package", 0),
        Err(Error::BadPackage { .. })
    ));
    assert!(matches!(
        Line::parse("unknown_keyword foo bar", 0),
        Err(Error::UnknownKeyword { .. })
    ));
}

#[test]
fn test_missing_value_is_empty() {
    for line in ["\toption name_only", "\tlist name_only"] {
        match Line::parse(line, 0).unwrap() {
            Line::Option { value: v, .. } | Line::List { item: v, .. } => {
                assert_eq!(v.as_str(), "", "{line}");
                assert_eq!(v.to_string(), "''", "{line}");
            }
            other => panic!("{line}: {other:?}"),
        }
    }
}

/// Each value as libuci's `parse_str` decodes it (`uci/file.c`).
#[test]
fn test_libuci_values() {
    let cases = [
        (r"'Rich'\''s Wi-Fi'", "Rich's Wi-Fi"),
        (r#""Rich's Wi-Fi""#, "Rich's Wi-Fi"),
        (r"'back\slash\ttab'", r"back\slash\ttab"),
        (r#""a\"b\\c\nd""#, r#"a"b\cnd"#),
        (r"a\ b", "a b"),
        (r#"'a'"b"c"#, "abc"),
        ("'line one\nline two'", "line one\nline two"),
        ("\"line one\nline two\"", "line one\nline two"),
        ("joined\\\nhere", "joinedhere"),
        ("joined\\\r\nhere", "joinedhere"),
        (r"a\#b", "a#b"),
        ("'émoji 🚀'", "émoji 🚀"),
        ("''", ""),
    ];
    for (raw, expected) in cases {
        let arena = Arena::new();
        let text: &str = arena.alloc(format!(
            "config test 'x'\n\toption v {raw}\n\toption after 'ok'\n"
        ));
        let config = Config::parse_str(&arena, text).unwrap_or_else(|e| panic!("{raw}: {e}"));
        let lines = &config.sections[0].lines;
        let Line::Option { value, .. } = lines[1] else {
            panic!("{raw}: {:?}", lines[1]);
        };
        assert_eq!(value.as_str(), expected, "{raw}");
        let Line::Option { option, value, .. } = lines[2] else {
            panic!("{raw}: {:?}", lines[2]);
        };
        assert_eq!(
            (&*option.as_str(), &*value.as_str()),
            ("after", "ok"),
            "{raw}"
        );
        assert_eq!(config.dump_str(), text, "{raw}");
    }
}

#[test]
fn test_libuci_statements() {
    let arena = Arena::new();
    let text = "package network\n\
                c interface lan # trailing\n\
                \to proto static\n\
                \tl dns 1.1.1.1; l dns 9.9.9.9\n\
                \toption key abc#def\n\
                \toption q 'x'#c\n\
                \toption empty\n";
    let config = Config::parse_str(&arena, text).unwrap();
    assert!(matches!(
        config.prefix[0],
        Line::Package { name, .. } if name.as_str() == "network"
    ));
    let section = &config.sections[0];
    assert_eq!(section.ty(), "interface");
    assert_eq!(section.name().unwrap(), "lan");
    let values: Vec<(String, String)> = section
        .lines
        .iter()
        .filter_map(|line| match line {
            Line::Option { option, value, .. } => {
                Some((option.unquoted_string(), value.unquoted_string()))
            }
            Line::List { list, item, .. } => Some((list.unquoted_string(), item.unquoted_string())),
            _ => None,
        })
        .collect();
    let expected = [
        ("proto", "static"),
        ("dns", "1.1.1.1"),
        ("dns", "9.9.9.9"),
        ("key", "abc"),
        ("q", "x"),
        ("empty", ""),
    ]
    .map(|(k, v)| (k.to_string(), v.to_string()));
    assert_eq!(values, expected);
    assert_eq!(
        config.dump_str(),
        "package network\n\
         config interface lan # trailing\n\
         \toption proto static\n\
         \tlist dns 1.1.1.1\n\
         \tlist dns 9.9.9.9\n\
         \toption key abc #def\n\
         \toption q 'x' #c\n\
         \toption empty ''\n"
    );
}

#[test]
fn test_libuci_error_lines() {
    let arena = Arena::new();
    let text = "config a\n\toption v 'one\ntwo'\n\toption bad 'x' 'y'\n";
    match Config::parse_str(&arena, text) {
        Err(Error::BadOption {
            src: Source::UnknownLine(3),
            ..
        }) => {}
        other => panic!("{:?}", other.err()),
    }
    for text in [
        "config a\n\toption v 'open\n",
        "config a\n\toption v \"open\n",
    ] {
        assert!(
            matches!(
                Config::parse_str(&arena, text),
                Err(Error::BadOption { .. })
            ),
            "{text:?}"
        );
    }
}

#[test]
fn test_written_values_round_trip() {
    let values = [
        "Rich's Wi-Fi",
        "it's 'quoted'",
        r"back\slash",
        "two\nlines",
        "tab\there",
        r#"say "hi""#,
        "hash # semi ;",
        "",
        "émoji 🚀",
    ];
    for value in values {
        let arena = Arena::new();
        let mut config = Config::new(&arena);
        let mut section = Section::new(&arena, "test", Some("x"));
        section
            .lines
            .push(Line::option_from_display("v", value, &arena));
        config.sections.push(section);
        let text: &str = arena.alloc(config.dump_str());
        let parsed = Config::parse_str(&arena, text).unwrap_or_else(|e| panic!("{text}: {e}"));
        let Some(Line::Option { value: read, .. }) = parsed.sections[0]
            .lines
            .iter()
            .find(|line| matches!(line, Line::Option { .. }))
        else {
            panic!("{text}");
        };
        assert_eq!(read.as_str(), value, "{text}");
    }
    let arena = Arena::new();
    assert_eq!(
        Token::from_string("Rich's Wi-Fi".into(), &arena).to_string(),
        r"'Rich'\''s Wi-Fi'"
    );
    assert_eq!(Token::from_str("a b", &arena).to_string(), "'a b'");
}

#[test]
fn test_backslash_at_end_of_input() {
    let arena = Arena::new();
    let config = Config::parse_str(&arena, "config a\n\toption v abc\\").unwrap();
    let Line::Option { value, .. } = config.sections[0].lines[1] else {
        panic!("{:?}", config.sections[0].lines[1]);
    };
    assert_eq!(value.as_str(), "abc");
    assert_eq!(Token::from_str("", &arena).to_string(), "''");
}

#[test]
fn test_eof_escape_does_not_consume_an_appended_option() {
    for (raw, expected, written) in [
        (r"abc\", "abc", "'abc'"),
        (r"'abc'\", "abc", "'abc'"),
        (r"abc\\", r"abc\", r"abc\\"),
        (r"\", "", "''"),
        ("abc\\\r", "abc\r", "'abc\r'"),
    ] {
        let arena = Arena::new();
        let original: &str = arena.alloc(format!("config a\n\toption v {raw}"));
        let mut config = Config::parse_str(&arena, original).unwrap();
        config.sections[0]
            .lines
            .push(Line::option_from_display("after", "ok", &arena));
        let dumped: &str = arena.alloc(config.dump_str());
        let mut bytes = Vec::new();
        config.dump_io(&mut bytes).unwrap();
        assert_eq!(bytes, dumped.as_bytes());
        assert_eq!(
            dumped,
            format!("config a\n\toption v {written}\n\toption after 'ok'\n"),
            "{raw:?}"
        );
        let parsed = Config::parse_str(&arena, dumped).unwrap();
        let values: Vec<_> = parsed.sections[0]
            .lines
            .iter()
            .filter_map(|line| match line {
                Line::Option { option, value, .. } => {
                    Some((option.unquoted_string(), value.unquoted_string()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            values,
            vec![
                ("v".to_string(), expected.to_string()),
                ("after".to_string(), "ok".to_string()),
            ],
            "{raw:?}"
        );
    }
}

#[test]
fn test_plain_quoted_value_borrows() {
    let Line::Option { value, .. } = Line::parse("\toption v 'a \"b\" \\c'", 0).unwrap() else {
        panic!("expected an option");
    };
    assert!(matches!(
        value.as_str(),
        std::borrow::Cow::Borrowed(r#"a "b" \c"#)
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

    let Line::Option {
        value: token_sq_parsed,
        ..
    } = Line::parse("\toption a 'sq word'", 0).unwrap()
    else {
        panic!("expected an option");
    };
    assert_eq!(token_sq_parsed.as_str(), "sq word");
    assert_eq!(format!("{}", token_sq_parsed), "'sq word'");

    assert_eq!(Token::from_bool(true).as_str(), "1");
    assert_eq!(Token::from_bool(false).as_str(), "0");

    assert!(Token::from_str("true", &arena).parse_bool("b").unwrap());
    assert!(!Token::from_str("0", &arena).parse_bool("b").unwrap());
    assert!(matches!(
        Token::from_str("maybe", &arena).parse_bool("b"),
        Err(Error::InvalidValue { option, found, .. }) if option == "b" && found == "maybe"
    ));

    assert_eq!(
        Token::from_str("123", &arena)
            .parse_fromstr::<i32>("n")
            .unwrap(),
        123
    );
    assert!(matches!(
        Token::from_str("abc", &arena).parse_fromstr::<i32>("n"),
        Err(Error::InvalidValue {
            src: Source::Unknown,
            ..
        })
    ));
}

/// Typed-read failures name the file line, option, and value.
#[test]
fn test_typed_read_errors_are_located() {
    let arena = Arena::new();
    let text = "# header\n\nconfig bar 'x'\n\toption always 'zero'\n\nconfig requiredfield 'y'\n";
    let config = Config::parse_str(&arena, text).unwrap();
    match config.sections[0].get::<Bar>() {
        Err(Error::InvalidValue {
            src: Source::UnknownLine(3),
            option,
            found,
            ..
        }) => assert_eq!((option.as_str(), found.as_str()), ("always", "zero")),
        other => panic!("{:?}", other.err()),
    }
    match config.sections[1].get::<RequiredField>() {
        Err(Error::MissingOption {
            src: Source::UnknownLine(5),
            missing,
        }) => assert_eq!(missing, "must_exist"),
        other => panic!("{:?}", other.err()),
    }
    match config.sections[1].get::<Bar>() {
        Err(Error::ExpectedSectionType {
            src: Source::UnknownLine(5),
            ..
        }) => {}
        other => panic!("{:?}", other.err()),
    }
    let bytes_err = Config::parse_str(&arena, "config a\n\toption b 'c' 'd'\n")
        .err()
        .unwrap()
        .map_src(|src| match src {
            Source::UnknownLine(n) => Source::Line("wireless".into(), n),
            src => src,
        });
    assert_eq!(bytes_err.src().line(), Some(1));
    assert!(bytes_err.to_string().contains("wireless:2"), "{bytes_err}");
}

#[derive(TypedSection, PartialEq, Eq, Debug, Default)]
#[uci(ty = "host")]
struct ListedHost {
    #[uci(default, accept_list)]
    mac: String,
    #[uci(default)]
    name: Option<String>,
}

#[test]
fn test_accept_list_reads_and_rewrites_first_item() {
    let arena = Arena::new();
    let text = "config host\n\toption name 'nas'\n\tlist mac 'AA'\n\tlist mac 'BB'\n";
    let mut config = Config::parse_str(&arena, text).unwrap();
    let host: ListedHost = config.sections[0].get().unwrap();
    assert_eq!(host.mac, "AA");
    config.sections[0]
        .set(&ListedHost {
            mac: "CC".into(),
            name: Some("nas".into()),
        })
        .unwrap();
    assert_eq!(
        config.dump_str(),
        "config host\n\toption name 'nas'\n\toption mac 'CC'\n\tlist mac 'BB'\n"
    );
}

#[derive(TypedSection, PartialEq, Eq, Debug, Default)]
#[uci(ty = "rule")]
struct ListedRule {
    proto: Vec<String>,
    #[uci(accept_list)]
    ipaddr: Option<String>,
}

#[test]
fn test_list_field_reads_an_option_as_words() {
    let arena = Arena::new();
    let text = "config rule
	option proto 'tcp udp'
	option x '1'
";
    let mut config = Config::parse_str(&arena, text).unwrap();
    let rule: ListedRule = config.sections[0].get().unwrap();
    assert_eq!(rule.proto, vec!["tcp", "udp"]);
    config.sections[0]
        .set(&ListedRule {
            proto: vec!["udp".into(), "icmp".into()],
            ipaddr: None,
        })
        .unwrap();
    assert_eq!(
        config.dump_str(),
        "config rule
	list proto 'udp'
	option x '1'
	list proto 'icmp'
"
    );
    config.sections[0].set(&ListedRule::default()).unwrap();
    assert_eq!(
        config.dump_str(),
        "config rule
	option x '1'
"
    );
}

#[test]
fn test_option_after_list_replaces_it() {
    let arena = Arena::new();
    let text = "config rule
	list proto 'tcp'
	option proto 'udp'
	list proto 'icmp'
";
    let config = Config::parse_str(&arena, text).unwrap();
    let rule: ListedRule = config.sections[0].get().unwrap();
    assert_eq!(rule.proto, vec!["udp", "icmp"]);
}

#[test]
fn test_accept_list_option_wins_over_list_items() {
    let arena = Arena::new();
    let text = "config rule\n\tlist ipaddr '10.0.1.1/24'\n\tlist ipaddr '10.0.4.1/24'\n\toption ipaddr '10.0.0.1'\n\tlist ipaddr '10.0.3.1/24'\n";
    let mut config = Config::parse_str(&arena, text).unwrap();
    let rule: ListedRule = config.sections[0].get().unwrap();
    assert_eq!(rule.ipaddr.as_deref(), Some("10.0.0.1"));
    config.sections[0]
        .set(&ListedRule {
            proto: vec![],
            ipaddr: Some("10.0.2.1".into()),
        })
        .unwrap();
    assert_eq!(
        config.dump_str(),
        "config rule\n\toption ipaddr '10.0.2.1'\n\tlist ipaddr '10.0.3.1/24'\n"
    );
}

#[test]
fn test_accept_list_optional_keeps_later_items() {
    let arena = Arena::new();
    let text = "config rule
	list ipaddr '10.0.0.1/24'
	list ipaddr '10.0.1.1/24'
";
    let mut config = Config::parse_str(&arena, text).unwrap();
    let rule: ListedRule = config.sections[0].get().unwrap();
    assert_eq!(rule.ipaddr.as_deref(), Some("10.0.0.1/24"));
    config.sections[0]
        .set(&ListedRule {
            proto: vec![],
            ipaddr: Some("10.0.2.1/24".into()),
        })
        .unwrap();
    assert_eq!(
        config.dump_str(),
        "config rule
	option ipaddr '10.0.2.1/24'
	list ipaddr '10.0.1.1/24'
"
    );
    config.sections[0].set(&ListedRule::default()).unwrap();
    assert_eq!(
        config.dump_str(),
        "config rule
	list ipaddr '10.0.1.1/24'
"
    );
}

#[test]
fn test_ipv4_address_forms() {
    use crate::openwrt::Ipv4Address;
    let bare: Ipv4Address = "192.168.1.1".parse().unwrap();
    assert_eq!(bare.prefix, None);
    assert_eq!(bare.to_string(), "192.168.1.1");
    let cidr: Ipv4Address = "192.168.1.1/24".parse().unwrap();
    assert_eq!(cidr.prefix, Some(24));
    assert_eq!(cidr.to_string(), "192.168.1.1/24");
    assert!("192.168.1.1/33".parse::<Ipv4Address>().is_err());
    assert!("192.168.1/24".parse::<Ipv4Address>().is_err());
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
    option with_quotes ''\''custom'\'''

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

#[test]
fn test_unknown_interface_proto_roundtrip() {
    use crate::openwrt::{InterfaceProto, NetworkInterface};

    // A proto startwrt doesn't manage (e.g. a hand-configured 6in4 tunnel)
    // must parse instead of erroring, and write back verbatim.
    let original = r"config interface 'wan6'
    option device '@wan'
    option proto '6in4'
";

    let arena = Arena::new();
    let mut config = Config::parse_str(&arena, original).unwrap();
    let iface: NetworkInterface = config.sections[0].get().unwrap();
    assert_eq!(iface.proto, InterfaceProto::UNKNOWN("6in4".to_string()));

    config.sections[0].set(&iface).unwrap();
    assert!(config.dump_str().contains("option proto '6in4'"));
}
