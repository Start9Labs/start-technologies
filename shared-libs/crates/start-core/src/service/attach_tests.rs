use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

use serde_json::json;

use super::AttachParams;

fn decode(command: serde_json::Value) -> serde_json::Result<AttachParams> {
    serde_json::from_value(
        json!({ "id": "demo", "command": command, "tty": false, "stderrTty": false }),
    )
}

#[test]
fn attach_accepts_the_advertised_string_arguments_without_normalization() {
    let args = [
        "sh",
        "-c",
        "printf 'hello'",
        "",
        " spaced ",
        "λ/日本語",
        "line\nend",
    ];
    let params = decode(json!(args)).unwrap();
    assert_eq!(params.command, args.map(OsString::from));
    assert!(decode(json!([])).unwrap().command.is_empty());
}

#[test]
fn attach_preserves_legacy_non_utf8_arguments_and_native_emission() {
    let command = vec![
        OsString::from("sh"),
        OsString::from_vec(vec![0xff, b'x', 0xfe]),
    ];
    let native = serde_json::to_value(&command).unwrap();
    let params = decode(native.clone()).unwrap();
    assert_eq!(params.command, command);
    assert_eq!(serde_json::to_value(&params.command).unwrap(), native);
}

#[test]
fn attach_can_mix_string_and_legacy_arguments_in_one_command() {
    let params = decode(json!(["sh", { "Unix": [255, 120, 254] }, ""])).unwrap();
    assert_eq!(
        params.command,
        vec![
            OsString::from("sh"),
            OsString::from_vec(vec![255, 120, 254]),
            OsString::new()
        ]
    );
}

#[test]
fn attach_rejects_malformed_argument_shapes_and_native_bytes() {
    for command in [
        json!(null),
        json!({}),
        json!("sh"),
        json!([1]),
        json!([null]),
        json!([{}]),
        json!([{ "Unix": [256] }]),
        json!([{ "Unix": [-1] }]),
        json!([{ "Unix": "sh" }]),
        json!([["sh"]]),
    ] {
        assert!(decode(command).is_err());
    }
}
