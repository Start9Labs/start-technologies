#![cfg(all(
    feature = "chrono",
    feature = "ipnet",
    feature = "josekit",
    feature = "url",
    feature = "yajrc"
))]

use serde::Serialize;
use serde::de::DeserializeOwned;
use visit_rs::ts::{Direction, TS, TSVisitor};

fn assert_bridge<T: DeserializeOwned + Serialize + TS>(
    fixture: &str,
    invalid: &str,
    expression: &str,
) {
    let value: T = serde_json::from_str(fixture).unwrap();
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::from_str::<serde_json::Value>(fixture).unwrap()
    );
    assert!(
        serde_json::from_str::<T>(invalid).is_err(),
        "{} accepted {invalid}",
        std::any::type_name::<T>()
    );
    for direction in [Direction::Input, Direction::Output] {
        let mut visitor = TSVisitor::new();
        visitor.with_direction(direction, |visitor| visitor.append_type::<T>());
        assert_eq!(
            visitor.into_module("Wire").unwrap(),
            format!("export type Wire = {expression};\n")
        );
    }
}

#[test]
fn scalar_bridges_match_their_serde_representations() {
    assert_bridge::<url::Url>(r#""https://example.test/path""#, "42", "string");
    assert_bridge::<ipnet::IpNet>(r#""192.0.2.0/24""#, r#""not-a-network""#, "string");
    assert_bridge::<chrono::DateTime<chrono::Utc>>(r#""2026-01-02T03:04:05Z""#, "42", "string");
    assert_bridge::<chrono::DateTime<chrono::FixedOffset>>(
        r#""2026-01-02T03:04:05+01:00""#,
        r#""2026-02-30T03:04:05Z""#,
        "string",
    );
    assert_bridge::<ipnet::Ipv4Net>(r#""192.0.2.0/24""#, r#""2001:db8::/32""#, "string");
    assert_bridge::<ipnet::Ipv6Net>(r#""2001:db8::/32""#, r#""192.0.2.0/24""#, "string");
}

#[test]
fn object_bridges_preserve_required_and_opaque_members() {
    assert_bridge::<josekit::jwk::Jwk>(
        r#"{"kty":"oct","k":"c2VjcmV0","kid":"example"}"#,
        "42",
        "{[key:string]:unknown}",
    );
    assert_bridge::<josekit::jwk::Jwk>("{}", "false", "{[key:string]:unknown}");
    assert_bridge::<josekit::jwk::Jwk>(r#"{"kty":42}"#, "null", "{[key:string]:unknown}");
    assert_bridge::<yajrc::RpcError>(
        r#"{"code":12,"message":"failure","data":{"details":"example"}}"#,
        r#"{"code":"12","message":"failure"}"#,
        "{code:number;message:string;data?:unknown}",
    );
}
