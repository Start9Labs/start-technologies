use blake3::Hash;
use digest::Update;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWrite;

use crate::CAP_10_MiB;
use crate::prelude::*;
use crate::s9pk::merkle_archive::hash::VerifyingWriter;
use crate::s9pk::merkle_archive::source::ArchiveSource;
use crate::sign::commitment::{Commitment, Digestable};
use crate::util::io::{ParallelBlake3Writer, TrackingIO};
use crate::util::serde::Base64;

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, PartialEq, Eq, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct Blake3Commitment {
    pub hash: Base64<[u8; 32]>,
    #[visit(ts(type = "number"), wire = "rpc_toolkit::ts::Unknown")]
    #[visit(opaque, type_attributes(visit::wire))]
    pub size: u64,
}

rpc_toolkit::reflect_ts!(Blake3Commitment);
rpc_toolkit::ts_export!(Blake3Commitment, namespaces = [""]);
impl Digestable for Blake3Commitment {
    fn update<D: Update>(&self, digest: &mut D) {
        digest.update(&*self.hash);
        digest.update(&u64::to_be_bytes(self.size));
    }
}
impl<'a, Resource: ArchiveSource> Commitment<&'a Resource> for Blake3Commitment {
    async fn create(resource: &'a Resource) -> Result<Self, Error> {
        let mut hasher = TrackingIO::new(0, ParallelBlake3Writer::new(CAP_10_MiB));
        resource.copy_all_to(&mut hasher).await?;
        Ok(Self {
            size: hasher.position(),
            hash: Base64(*hasher.into_inner().finalize().await?.as_bytes()),
        })
    }
    async fn copy_to<W: AsyncWrite + Unpin + Send>(
        &self,
        resource: &'a Resource,
        writer: W,
    ) -> Result<(), Error> {
        let mut hasher =
            VerifyingWriter::new(writer, Some((Hash::from_bytes(*self.hash), self.size)));
        resource.copy_to(0, self.size, &mut hasher).await?;
        hasher.verify().await?;
        Ok(())
    }
}
