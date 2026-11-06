use std::collections::BTreeMap;

use imbl_value::Value;
use serde_cbor::value::{CanonicalValue, Value as CborValue};

use super::{
    SignatureAuthContext, SignatureHeader, accept_nonce, signing_identities, verify_auth_message,
};
use crate::net::request_origin::RequestOrigin;
use crate::prelude::*;
use crate::sign::{AnyVerifyingKey, Signer};

/// Reserved root field for JSON-RPC request proofs.
pub const RPC_SIGNATURE_FIELD: &str = "signature";
const RPC_AUTH_TAG: &[u8] = b"Start-RPC-Auth-Sig v1\0";

impl SignatureHeader {
    /// Signs the complete RPC message except its root proof field.
    pub async fn sign_rpc(signer: &Signer, request: &Value, context: &str) -> Result<Self, Error> {
        Self::sign_tagged(signer, &rpc_signing_body(request)?, RPC_AUTH_TAG, context).await
    }

    /// Checks the signature and server identity, not the body, clock, or replay cache.
    pub fn verify_rpc_commitment(&self, context: &str) -> Result<(), Error> {
        verify_auth_message(
            &self.signer,
            &self.signature,
            &crate::sign::signing_message(RPC_AUTH_TAG, &self.commitment, context)?,
        )
    }
}

pub(super) async fn verify_rpc_signature<C: SignatureAuthContext>(
    context: &C,
    message: &Value,
    origin: &RequestOrigin,
    loopback: Option<InternedString>,
) -> Result<(AnyVerifyingKey, InternedString), Error> {
    let proof: String =
        from_value(message[RPC_SIGNATURE_FIELD].clone()).with_kind(ErrorKind::InvalidSignature)?;
    let header = SignatureHeader::from_header(
        &http::HeaderValue::from_str(&proof).with_kind(ErrorKind::InvalidSignature)?,
    )?;
    let mut ingress = http::Extensions::new();
    ingress.insert(origin.clone());
    let address = signing_identities(context, &ingress, loopback)
        .await?
        .into_iter()
        .find(|address| header.verify_rpc_commitment(address).is_ok())
        .ok_or_else(|| {
            Error::new(
                eyre!("{}", t!("middleware.auth.invalid-request-signature")),
                ErrorKind::Authorization,
            )
        })?;
    accept_nonce(context, &header.commitment).await?;
    header
        .commitment
        .verify_bytes(&rpc_signing_body(message)?)
        .await?;
    Ok((header.signer, address))
}

/// Length-first deterministic CBOR without the root `signature` field.
/// Floats become eight big-endian IEEE-754 bytes; JSON has no byte-string values.
pub fn rpc_signing_body(request: &Value) -> Result<Vec<u8>, Error> {
    let mut request: BTreeMap<String, Value> = from_value(request.clone())?;
    request.remove(RPC_SIGNATURE_FIELD);
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

    #[tokio::test]
    async fn verifies_and_reproduces_the_typescript_rpc_signature_fixture() {
        use crate::sign::AnySigningKey;
        use crate::util::serde::DerEncoding;
        #[derive(serde::Deserialize)]
        struct ProofFixture {
            request: String,
            context: String,
            proof: String,
        }
        let fixture: ProofFixture = serde_json::from_str(include_str!(
            "../../../../../../ts-modules/start-core/lib/auth/fixtures/rpc-signature.json"
        ))
        .unwrap();
        let message: Value = serde_json::from_str(&fixture.request).unwrap();
        let header =
            SignatureHeader::from_header(&http::HeaderValue::from_str(&fixture.proof).unwrap())
                .unwrap();
        header.verify_rpc_commitment(&fixture.context).unwrap();
        header
            .commitment
            .verify_bytes(&rpc_signing_body(&message).unwrap())
            .await
            .unwrap();
        let key = Signer::Key(AnySigningKey::Ed25519(
            ed25519_dalek::SigningKey::from_bytes(&[2; 32]),
        ));
        assert_eq!(header.signer, key.verifying_key().await.unwrap());
        let signature = key
            .sign_message(
                &crate::sign::signing_message(RPC_AUTH_TAG, &header.commitment, &fixture.context)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            signature.to_der::<serde_json::Error>().unwrap(),
            header.signature.to_der::<serde_json::Error>().unwrap()
        );
    }

    #[tokio::test]
    async fn rpc_proofs_are_domain_separated_from_legacy_http_signatures() {
        use super::super::verify_request;
        use crate::sign::AnySigningKey;
        let key = Signer::Key(AnySigningKey::Ed25519(
            ed25519_dalek::SigningKey::from_bytes(&[91; 32]),
        ));
        let message = imbl_value::json!({"id":u64::MAX,"method":"test","params":{"value":1.0}});
        let proof = SignatureHeader::sign_rpc(&key, &message, "example.test")
            .await
            .unwrap();
        proof.verify_rpc_commitment("example.test").unwrap();
        assert!(proof.verify_rpc_commitment("other.test").is_err());
        proof
            .commitment
            .verify_bytes(&rpc_signing_body(&message).unwrap())
            .await
            .unwrap();
        assert!(
            verify_request(
                &proof.signer,
                &proof.commitment,
                "example.test",
                &proof.signature
            )
            .is_err()
        );
        let http =
            SignatureHeader::sign(&key, &rpc_signing_body(&message).unwrap(), "example.test")
                .await
                .unwrap();
        assert!(http.verify_rpc_commitment("example.test").is_err());
    }

    #[tokio::test]
    async fn json_verification_rejects_signed_prefixes_and_oversized_declared_commitments() {
        use crate::sign::AnySigningKey;
        let temp = tempfile::tempdir().unwrap();
        let key = Signer::Key(AnySigningKey::Ed25519(
            ed25519_dalek::SigningKey::from_bytes(&[92; 32]),
        ));
        let ctx = crate::registry::context::test_context(
            temp.path().into(),
            key.verifying_key().await.unwrap(),
        )
        .await;
        let mut message = imbl_value::json!({"id":1,"method":"test","params":{}});
        let body = rpc_signing_body(&message).unwrap();
        let prefix = SignatureHeader::sign_tagged(&key, &body[..1], RPC_AUTH_TAG, "127.0.0.1")
            .await
            .unwrap();
        message[RPC_SIGNATURE_FIELD] = prefix.to_header().to_str().unwrap().into();
        let error = verify_rpc_signature(&ctx, &message, &RequestOrigin::default(), None)
            .await
            .unwrap_err();
        assert_eq!(
            rpc_toolkit::yajrc::RpcError::from(error).code,
            ErrorKind::InvalidSignature as i32
        );
        let mut oversized = SignatureHeader::sign_rpc(&key, &message, "127.0.0.1")
            .await
            .unwrap();
        oversized.commitment.size = u64::MAX;
        oversized.signature = key
            .sign_message(
                &crate::sign::signing_message(RPC_AUTH_TAG, &oversized.commitment, "127.0.0.1")
                    .unwrap(),
            )
            .await
            .unwrap();
        message[RPC_SIGNATURE_FIELD] = oversized.to_header().to_str().unwrap().into();
        let error = verify_rpc_signature(&ctx, &message, &RequestOrigin::default(), None)
            .await
            .unwrap_err();
        assert_eq!(
            rpc_toolkit::yajrc::RpcError::from(error).code,
            ErrorKind::InvalidSignature as i32
        );
    }

    #[test]
    fn signing_requires_an_object() {
        for request in [imbl_value::json!([]), Value::Null, imbl_value::json!(1)] {
            assert!(rpc_signing_body(&request).is_err());
        }
    }
}
