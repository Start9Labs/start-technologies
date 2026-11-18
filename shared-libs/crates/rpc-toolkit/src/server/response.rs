use imbl_value::{InOMap, InternedString, Value};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use yajrc::{Id, RpcError};

use super::GenericRpcMethod;
use crate::util::{combine, internal_error};

/// A JSON-RPC reply with optional, transport-independent per-call metadata.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(from = "WireResponse", into = "WireResponse")]
pub struct RpcResponse {
    /// A missing ID serializes as `null`.
    pub id: Option<Id>,
    /// Handler output or a JSON-RPC error.
    pub result: Result<Value, RpcError>,
    /// Per-call fields omitted from the wire when empty.
    pub meta: InOMap<InternedString, Value>,
}

#[derive(Deserialize, Serialize)]
struct WireResponse {
    #[serde(flatten)]
    response: yajrc::RpcResponse<GenericRpcMethod>,
    #[serde(default, skip_serializing_if = "InOMap::is_empty")]
    meta: InOMap<InternedString, Value>,
}

impl RpcResponse {
    /// Creates a reply without metadata.
    pub fn new(id: Option<Id>, result: Result<Value, RpcError>) -> Self {
        Self {
            id,
            result,
            meta: InOMap::new(),
        }
    }

    /// Creates an uncorrelated reply without metadata.
    pub fn from_result<E: Into<RpcError>>(result: Result<Value, E>) -> Self {
        Self::new(None, result.map_err(Into::into))
    }

    /// Adds metadata fields atomically; rejects duplicate fields and non-object values.
    pub fn add_meta(&mut self, metadata: impl Serialize) -> Result<(), RpcError> {
        let metadata = imbl_value::to_value(&metadata).map_err(internal_error)?;
        let Value::Object(metadata) =
            combine(Value::Object(self.meta.clone()), metadata).map_err(internal_error)?
        else {
            unreachable!()
        };
        self.meta = metadata;
        Ok(())
    }

    /// Deserializes metadata; absent fields follow serde defaults.
    pub fn read_meta<T: DeserializeOwned>(&self) -> Result<T, RpcError> {
        imbl_value::from_value(Value::Object(self.meta.clone())).map_err(internal_error)
    }
}

impl From<RpcError> for RpcResponse {
    fn from(error: RpcError) -> Self {
        Self::from_result(Err(error))
    }
}

impl From<WireResponse> for RpcResponse {
    fn from(wire: WireResponse) -> Self {
        Self {
            id: wire.response.id,
            result: wire.response.result,
            meta: wire.meta,
        }
    }
}

impl From<RpcResponse> for WireResponse {
    fn from(response: RpcResponse) -> Self {
        Self {
            response: yajrc::RpcResponse {
                id: response.id,
                result: response.result,
            },
            meta: response.meta,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn empty_metadata_preserves_the_existing_wire_format() {
        let response = RpcResponse::new(Some(Id::Number(7.into())), Ok(42.into()));
        let value = serde_json::to_value(&response).unwrap();
        assert_eq!(value, json!({"jsonrpc":"2.0", "id":7, "result":42}));
        let parsed: RpcResponse = serde_json::from_value(value).unwrap();
        assert!(parsed.meta.is_empty());
    }

    #[test]
    fn typed_metadata_round_trips_on_success_and_error_and_is_ignored_by_legacy_readers() {
        #[derive(Debug, PartialEq, Deserialize, Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Metadata {
            patch_sequence: u64,
        }

        for result in [Ok(Value::Null), Err(yajrc::INVALID_PARAMS_ERROR)] {
            let mut response = RpcResponse::new(Some(Id::Number(9.into())), result);
            response.add_meta(Metadata { patch_sequence: 42 }).unwrap();
            response.add_meta(json!({"trace":"test"})).unwrap();
            let value = serde_json::to_value(&response).unwrap();
            assert_eq!(value["meta"], json!({"patchSequence":42, "trace":"test"}));
            let legacy: yajrc::RpcResponse<GenericRpcMethod> =
                serde_json::from_value(value.clone()).unwrap();
            assert_eq!(legacy.result.is_ok(), response.result.is_ok());
            let parsed: RpcResponse = serde_json::from_value(value).unwrap();
            assert_eq!(
                parsed.read_meta::<Metadata>().unwrap(),
                Metadata { patch_sequence: 42 }
            );
        }
    }

    #[test]
    fn metadata_preserves_full_integer_ids() {
        let mut response =
            RpcResponse::new(Some(yajrc::Id::Number(u64::MAX.into())), Ok(Value::Null));
        response.add_meta(json!({"patchSequence": 7})).unwrap();
        let encoded = serde_json::to_string(&response).unwrap();
        assert!(encoded.contains("18446744073709551615"));
        let decoded: RpcResponse = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.id, response.id);
    }

    #[test]
    fn rejected_metadata_preserves_existing_fields() {
        let mut response = RpcResponse::new(None, Ok(Value::Null));
        response.add_meta(json!({"sequence":1})).unwrap();
        for value in [json!({"sequence":2, "other":3}), json!("not an object")] {
            assert!(response.add_meta(value).is_err());
            assert_eq!(
                serde_json::to_value(&response).unwrap()["meta"],
                json!({"sequence":1})
            );
        }
    }

    #[test]
    fn metadata_does_not_weaken_jsonrpc_envelope_validation() {
        for value in [
            json!({"id":1,"result":null,"meta":{}}),
            json!({"jsonrpc":"1.0","id":1,"result":null,"meta":{}}),
            json!({"jsonrpc":"2.0","id":1,"result":null,"error":{"code":1,"message":"bad"},"meta":{}}),
            json!({"jsonrpc":"2.0","id":1,"meta":{}}),
            json!({"jsonrpc":"2.0","id":1,"result":null,"meta":[]} ),
        ] {
            assert!(serde_json::from_value::<RpcResponse>(value).is_err());
        }
    }
}
