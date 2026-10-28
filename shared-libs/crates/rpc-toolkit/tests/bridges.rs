#![cfg(all(
    feature = "chrono",
    feature = "ipnet",
    feature = "josekit",
    feature = "url",
    feature = "yajrc"
))]

mod common;

use common::module;
use rpc_toolkit::ts::TS;
use serde::de::DeserializeOwned;
use serde::Serialize;

fn assert_bridge<T: DeserializeOwned + Serialize + TS>(
    fixture: &str,
    invalid: &str,
    expression: &str,
    schema_type: &str,
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
    let bindings = module::<T>();
    assert!(
        bindings.schemas.contains(&format!(
            "export const TestType: {schema_type} = /* @__PURE__ */ {expression}.meta({{\nid: \"TestType\" }});\n"
        )),
        "{}",
        bindings.schemas
    );
}

#[test]
fn scalar_bridges_match_their_serde_representations() {
    assert_bridge::<url::Url>(
        r#""https://example.test/path""#,
        "42",
        "z.string()",
        "z.ZodString",
    );
    assert_bridge::<ipnet::IpNet>(
        r#""192.0.2.0/24""#,
        r#""not-a-network""#,
        "z.string()",
        "z.ZodString",
    );
    assert_bridge::<chrono::DateTime<chrono::Utc>>(
        r#""2026-01-02T03:04:05Z""#,
        "42",
        "z.string()",
        "z.ZodString",
    );
    assert_bridge::<chrono::DateTime<chrono::FixedOffset>>(
        r#""2026-01-02T03:04:05+01:00""#,
        r#""2026-02-30T03:04:05Z""#,
        "z.string()",
        "z.ZodString",
    );
    assert_bridge::<ipnet::Ipv4Net>(
        r#""192.0.2.0/24""#,
        r#""2001:db8::/32""#,
        "z.string()",
        "z.ZodString",
    );
    assert_bridge::<ipnet::Ipv6Net>(
        r#""2001:db8::/32""#,
        r#""192.0.2.0/24""#,
        "z.string()",
        "z.ZodString",
    );
}

#[test]
fn object_bridges_preserve_required_and_opaque_members() {
    let jwk = "z.record(z.string(), z.unknown())";
    let jwk_type = "z.ZodRecord<z.ZodString, z.ZodUnknown>";
    assert_bridge::<josekit::jwk::Jwk>(
        r#"{"kty":"oct","k":"c2VjcmV0","kid":"example"}"#,
        "42",
        jwk,
        jwk_type,
    );
    assert_bridge::<josekit::jwk::Jwk>("{}", "false", jwk, jwk_type);
    assert_bridge::<josekit::jwk::Jwk>(r#"{"kty":42}"#, "null", jwk, jwk_type);
    assert_bridge::<yajrc::RpcError>(
        r#"{"code":12,"message":"failure","data":{"details":"example"}}"#,
        r#"{"code":"12","message":"failure"}"#,
        "object({ code: z.number(), message: z.string(), data: z.unknown().optional() })",
        "z.ZodObject<{ code: z.ZodNumber; message: z.ZodString; data: z.ZodOptional<z.ZodUnknown> }>",
    );
}

#[cfg(feature = "exver")]
#[test]
fn version_bridge_matches_the_existing_scalar_serializer() {
    assert_bridge::<exver::Version>(r#""0.4.0.1""#, "42", "z.string()", "z.ZodString");
}
#[cfg(feature = "patch-db")]
#[test]
fn patch_dump_uses_generic_reflection_with_rpc_owned_value_bridge() {
    let fixture = serde_json::json!({"id":17,"value":{"nested":[true,null,3]}});
    let value: patch_db::Dump = serde_json::from_value(fixture.clone()).unwrap();
    assert_eq!(serde_json::to_value(value).unwrap(), fixture);
    let bindings = common::declarations::<patch_db::Dump>();
    assert!(bindings.schemas.contains("\"id\": z.int()"));
    assert!(bindings.schemas.contains("\"value\": z.unknown()"));
}
