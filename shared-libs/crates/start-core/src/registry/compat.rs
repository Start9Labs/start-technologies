use crate::prelude::*;
use crate::sign::{AnySignature, AnyVerifyingKey};

pub fn pem_wire(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (name, value) in object.iter_mut() {
                match (&**name, value) {
                    ("signatures", Value::Object(signatures)) => {
                        *signatures = signatures
                            .iter()
                            .map(|(key, signature)| {
                                match (
                                    key.parse::<AnyVerifyingKey>(),
                                    from_value::<AnySignature>(signature.clone()),
                                ) {
                                    (Ok(key), Ok(signature)) => (
                                        InternedString::from_display(&key),
                                        Value::from(signature.to_string()),
                                    ),
                                    _ => (key.clone(), signature.clone()),
                                }
                            })
                            .collect();
                    }
                    ("keys" | "addKey" | "removeKey", Value::Array(keys)) => {
                        for key in keys.iter_mut() {
                            pem_string::<AnyVerifyingKey>(key);
                        }
                    }
                    ("signer" | "uploader" | "key", value @ Value::String(_)) => {
                        pem_string::<AnyVerifyingKey>(value);
                    }
                    ("signature", value @ Value::String(_)) => {
                        pem_string::<AnySignature>(value);
                    }
                    (_, value) => pem_wire(value),
                }
            }
        }
        Value::Array(array) => {
            for value in array.iter_mut() {
                pem_wire(value);
            }
        }
        _ => (),
    }
}

fn pem_string<T: serde::de::DeserializeOwned + std::fmt::Display>(value: &mut Value) {
    if let Ok(decoded) = from_value::<T>(value.clone()) {
        *value = Value::from(decoded.to_string());
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;
    use crate::registry::asset::RegistryAsset;
    use crate::sign::commitment::blake3::Blake3Commitment;
    use crate::util::serde::Base64;

    #[test]
    fn registry_requests_keep_legacy_pem_parameters() {
        use ed25519_dalek::Signer;
        let signing = ed25519_dalek::SigningKey::from_bytes(&[13; 32]);
        let key = AnyVerifyingKey::Ed25519(signing.verifying_key());
        let signature = AnySignature::Ed25519ph(signing.sign(b"message"));
        let mut params = json!({
            "signer": key.clone(),
            "uploader": key.clone(),
            "signature": signature.clone(),
            "addKey": [key.clone()],
            "removeKey": [key.clone()],
            "metadata": { "keys": ["unrelated metadata"] },
        });
        pem_wire(&mut params);
        for field in ["signer", "uploader"] {
            assert_eq!(params[field].as_str(), Some(key.to_string().as_str()));
        }
        for field in ["addKey", "removeKey"] {
            assert_eq!(params[field][0].as_str(), Some(key.to_string().as_str()));
        }
        assert_eq!(
            params["signature"].as_str(),
            Some(signature.to_string().as_str())
        );
        assert_eq!(
            params["metadata"]["keys"][0].as_str(),
            Some("unrelated metadata")
        );
    }

    #[test]
    fn legacy_index_keeps_pem_keys_and_signature_labels() {
        use ed25519_dalek::Signer;
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[31; 32]);
        let key = AnyVerifyingKey::Ed25519(signing_key.verifying_key());
        let signature = AnySignature::Ed25519(signing_key.sign(b"message"));
        let asset = RegistryAsset {
            commitment: Blake3Commitment {
                hash: Base64([1; 32]),
                size: 7,
            },
            signatures: [(key.clone(), signature.clone())].into_iter().collect(),
            published_at: chrono::Utc::now(),
            urls: vec!["https://example.com/asset".parse().unwrap()],
        };
        let mut response = json!({
            "signers": { "a": { "keys": [key.clone()] } },
            "os": { "versions": { "0.4.0.5": { "squashfs": [asset] } } },
            "description": "not a key",
        });
        assert!(!response.to_string().contains("BEGIN"));
        pem_wire(&mut response);
        assert_eq!(
            response["signers"]["a"]["keys"][0].as_str(),
            Some(key.to_string().as_str())
        );
        let signature_map = &response["os"]["versions"]["0.4.0.5"]["squashfs"][0]["signatures"];
        assert_eq!(
            signature_map[&key.to_string()].as_str(),
            Some(signature.to_string().as_str())
        );
        let once = response.clone();
        pem_wire(&mut response);
        assert_eq!(response, once);
        assert_eq!(response["description"].as_str(), Some("not a key"));
    }
}
