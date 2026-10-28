use digest::Update;
use futures::Future;
use tokio::io::AsyncWrite;

use crate::prelude::*;

pub mod blake3;
pub mod merkle_archive;
pub mod request;

/// A commitment that registry signatures cover.
pub trait SignedCommitment: Digestable {
    /// Distinguishes commitment types that encode alike.
    const TYPE_ID: u8;
    /// The `type` tag in its JSON and request-auth header. Matches the serde container name.
    const TYPE_NAME: &'static str;
}

pub trait Digestable {
    fn update<D: Update>(&self, digest: &mut D);
}

pub trait Commitment<Resource>: Sized + Digestable {
    fn create(resource: Resource) -> impl Future<Output = Result<Self, Error>> + Send;
    fn copy_to<W: AsyncWrite + Unpin + Send>(
        &self,
        resource: Resource,
        writer: W,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn check(&self, resource: Resource) -> impl Future<Output = Result<(), Error>> + Send {
        self.copy_to(resource, tokio::io::sink())
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;
    use crate::sign::commitment::blake3::Blake3Commitment;
    use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
    use crate::sign::commitment::request::RequestCommitment;
    use crate::util::serde::Base64;

    fn tag<C: SignedCommitment + serde::Serialize>(commitment: &C) -> Value {
        to_value(commitment).unwrap()["type"].clone()
    }

    #[test]
    fn json_tags_match_type_names() {
        let blake3 = Blake3Commitment {
            hash: Base64([1; 32]),
            size: 2,
        };
        let merkle = MerkleArchiveCommitment {
            root_sighash: Base64([1; 32]),
            root_maxsize: 2,
        };
        let request = RequestCommitment {
            timestamp: 1,
            nonce: 2,
            size: 3,
            blake3: Base64([1; 32]),
        };
        assert_eq!(tag(&blake3), json!(Blake3Commitment::TYPE_NAME));
        assert_eq!(tag(&merkle), json!(MerkleArchiveCommitment::TYPE_NAME));
        assert_eq!(tag(&request), json!(RequestCommitment::TYPE_NAME));
    }

    #[test]
    fn untagged_json_deserializes() {
        let blake3: Blake3Commitment = from_value(json!({
            "hash": "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=",
            "size": 2,
        }))
        .unwrap();
        assert_eq!(blake3.size, 2);
    }
}
