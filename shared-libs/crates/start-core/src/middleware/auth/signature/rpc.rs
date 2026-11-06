use std::collections::BTreeMap;

use imbl_value::Value;
use serde_cbor::value::{CanonicalValue, Value as CborValue};

use crate::prelude::*;

/// Length-first deterministic CBOR without the root `signature` field.
/// Floats become eight big-endian IEEE-754 bytes; JSON has no byte-string values.
pub fn rpc_signing_body(request: &Value) -> Result<Vec<u8>, Error> {
    let mut request: BTreeMap<String, Value> = from_value(request.clone())?;
    request.remove("signature");
    let mut value = CborValue::serialized(&request).with_kind(ErrorKind::Serialization)?;
    prepare_signing_value(&mut value);
    let mut bytes = Vec::new();
    serde_cbor::ser::into_writer(&value, &mut bytes).with_kind(ErrorKind::Serialization)?;
    Ok(bytes)
}

fn prepare_signing_value(value: &mut CborValue) {
    match value {
        CborValue::Float(number) => *value = CborValue::Bytes(number.to_be_bytes().to_vec()),
        CborValue::Map(entries) => {
            entries.sort_by_cached_key(|(key, _)| CanonicalValue::from(key.clone()));
            for (_, value) in entries {
                prepare_signing_value(value);
            }
        }
        CborValue::Array(values) => {
            for value in values {
                prepare_signing_value(value);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Fixture {
        name: String,
        input: String,
        body_hex: String,
    }

    #[test]
    fn shared_canonical_request_fixtures() {
        let fixtures: Vec<Fixture> = serde_json::from_str(include_str!(
            "../../../../../../ts-modules/start-core/lib/auth/fixtures/rpc-canonical.json"
        ))
        .unwrap();
        for fixture in fixtures {
            let request: Value = serde_json::from_str(&fixture.input).unwrap();
            assert_eq!(
                hex::encode(rpc_signing_body(&request).unwrap()),
                fixture.body_hex,
                "{}",
                fixture.name
            );
        }
    }

    #[test]
    fn signing_requires_an_object() {
        for request in [imbl_value::json!([]), Value::Null, imbl_value::json!(1)] {
            assert!(rpc_signing_body(&request).is_err());
        }
    }
}
