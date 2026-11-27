use super::{KeyboardOptions, SetLanguageParams};

fn language(language: &str) -> SetLanguageParams {
    SetLanguageParams {
        language: language.into(),
    }
}

fn keyboard(layout: &str) -> KeyboardOptions {
    KeyboardOptions {
        layout: layout.into(),
        keymap: None,
        model: None,
        variant: None,
        options: Vec::new(),
    }
}

#[test]
fn language_accepts_every_supported_locale() {
    for locale in ["en_US", "de_DE", "es_ES", "fr_FR", "pl_PL"] {
        language(locale).validate().unwrap();
    }
}

#[test]
fn language_rejects_unsupported_values() {
    for value in [
        "",
        "en",
        "en_GB",
        "C",
        "en_US\nX=1",
        "en_US.UTF-8",
        "../en_US",
    ] {
        assert!(language(value).validate().is_err(), "{value:?}");
    }
}

#[test]
fn keyboard_accepts_xkb_names() {
    keyboard("us").validate().unwrap();
    KeyboardOptions {
        layout: "latam".into(),
        keymap: Some("la".into()),
        model: Some("pc105".into()),
        variant: Some("altgr-intl".into()),
        options: vec!["grp:alt_shift_toggle".into(), "compose:ralt".into()],
    }
    .validate()
    .unwrap();
}

#[test]
fn keyboard_rejects_empty_layout_and_unsafe_characters() {
    assert!(keyboard("").validate().is_err());
    assert!(keyboard("us\"\nX").validate().is_err());
    for field in 0..4 {
        let mut options = keyboard("us");
        let bad = Some("us\nX".into());
        match field {
            0 => options.keymap = bad,
            1 => options.model = bad,
            2 => options.variant = bad,
            _ => options.options = vec!["grp:alt_shift_toggle".into(), "a\"b".into()],
        }
        assert!(options.validate().is_err(), "field {field}");
    }
}
