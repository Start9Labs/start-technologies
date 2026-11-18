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
    pub signer: DerBytes<AnyVerifyingKey>,
    pub signature: DerBytes<AnySignature>,
}

rpc_toolkit::reflect_ts!(BinaryRequestProof);
rpc_toolkit::ts_export!(BinaryRequestProof, namespaces = ["session"]);

impl From<&RequestProof> for BinaryRequestProof {
    fn from(value: &RequestProof) -> Self {
        Self {
            commitment: (&value.commitment).into(),
            signer: DerBytes(value.signer.clone()),
            signature: DerBytes(value.signature.clone()),
        }
    }
}

impl From<BinaryRequestProof> for RequestProof {
    fn from(value: BinaryRequestProof) -> Self {
        Self {
            commitment: value.commitment.into(),
            signer: value.signer.0,
            signature: value.signature.0,
        }
    }
}
