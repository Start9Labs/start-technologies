#![cfg(all(
    feature = "chrono",
    feature = "ipnet",
    feature = "josekit",
    feature = "url",
    feature = "yajrc"
))]

use rpc_toolkit::ts::{Direction, TSVisitor, TS};
use serde::de::DeserializeOwned;
use serde::Serialize;

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

#[cfg(feature = "exver")]
#[test]
fn version_bridge_matches_the_existing_scalar_serializer() {
    assert_bridge::<exver::Version>(r#""0.4.0.1""#, "42", "string");
}
#[cfg(feature = "patch-db")]
#[test]
fn patch_dump_uses_generic_reflection_with_rpc_owned_value_bridge() {
    let fixture = serde_json::json!({"id":17,"value":{"nested":[true,null,3]}});
    let value: patch_db::Dump = serde_json::from_value(fixture.clone()).unwrap();
    assert_eq!(serde_json::to_value(value).unwrap(), fixture);
    for direction in [Direction::Input, Direction::Output] {
        let mut visitor = TSVisitor::new();
        visitor.with_direction(direction, |v| v.append_type::<patch_db::Dump>());
        let module = visitor.into_declarations().unwrap();
        assert!(module.contains("\"id\":(number)"));
        assert!(module.contains("\"value\":(unknown)"));
    }
}
