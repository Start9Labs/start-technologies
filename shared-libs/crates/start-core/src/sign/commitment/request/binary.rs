use serde::{Deserialize, Serialize};
use visit_rs::VisitFields;

use super::{RequestCommitment, RequestProof};
use crate::sign::{AnySignature, AnyVerifyingKey};
use crate::util::serde::Base64;
use crate::util::serde::binary::{ByteString, DerBytes, Integer};

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(deny_unknown_fields)]
pub struct BinaryRequestCommitment {
    pub timestamp: Integer<i64>,
    pub nonce: Integer<u64>,
    pub size: Integer<u64>,
    pub blake3: ByteString<[u8; 32]>,
}

rpc_toolkit::reflect_ts!(BinaryRequestCommitment);

impl From<&RequestCommitment> for BinaryRequestCommitment {
    fn from(value: &RequestCommitment) -> Self {
        Self {
            timestamp: Integer(value.timestamp),
            nonce: Integer(value.nonce),
            size: Integer(value.size),
            blake3: ByteString(*value.blake3),
        }
    }
}

impl From<BinaryRequestCommitment> for RequestCommitment {
    fn from(value: BinaryRequestCommitment) -> Self {
        Self {
            timestamp: value.timestamp.0,
            nonce: value.nonce.0,
            size: value.size.0,
            blake3: Base64(value.blake3.0),
        }
    }
}

/// Binary request proofs retain the generic commitment domain, not HTTP header framing.
#[derive(Deserialize, Serialize, VisitFields)]
#[serde(deny_unknown_fields)]
pub struct BinaryRequestProof {
    pub commitment: BinaryRequestCommitment,
    /// Omitted only with a WebAuthn signature, whose credential ID the receiver resolves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer: Option<DerBytes<AnyVerifyingKey>>,
    pub signature: DerBytes<AnySignature>,
}

rpc_toolkit::reflect_ts!(BinaryRequestProof);
rpc_toolkit::ts_export!(BinaryRequestProof, namespaces = ["session"]);

impl From<&RequestProof> for BinaryRequestProof {
    fn from(value: &RequestProof) -> Self {
        Self {
            commitment: (&value.commitment).into(),
            signer: Some(DerBytes(value.signer.clone())),
            signature: DerBytes(value.signature.clone()),
        }
    }
}

impl BinaryRequestProof {
    /// The credential ID a proof without a signer asks the receiver to resolve.
    pub fn unnamed_credential(&self) -> Option<Vec<u8>> {
        match (&self.signer, &self.signature.0) {
            (None, AnySignature::WebAuthn(assertion)) => {
                Some(assertion.credential_id.as_bytes().to_vec())
            }
            _ => None,
        }
    }

    /// `None` when the proof names no signer and none was resolved.
    pub fn into_proof(self, resolved: Option<AnyVerifyingKey>) -> Option<RequestProof> {
        Some(RequestProof {
            commitment: self.commitment.into(),
            signer: self.signer.map(|signer| signer.0).or(resolved)?,
            signature: self.signature.0,
        })
    }
}
