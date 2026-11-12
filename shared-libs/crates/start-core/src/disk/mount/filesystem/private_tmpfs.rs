use std::path::Path;

use sha2::{Digest, Sha256};

use super::FileSystem;
use crate::prelude::*;
use crate::rpc_continuations::Guid;

/// Private RAM storage excluded from swap, including after boot handoff.
pub struct PrivateTmpfs(Guid);

impl PrivateTmpfs {
    pub fn new() -> Self {
        Self(Guid::new())
    }
}

impl FileSystem for PrivateTmpfs {
    fn mount_type(&self) -> Option<impl AsRef<str>> {
        Some("tmpfs")
    }

    fn mount_options(&self) -> impl IntoIterator<Item = impl std::fmt::Display> {
        ["noswap", "nosuid", "nodev", "mode=0700"]
    }

    async fn source(&self) -> Result<Option<impl AsRef<Path>>, Error> {
        Ok(Some("tmpfs"))
    }

    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        Ok(Sha256::digest(self.0.to_string().as_bytes()))
    }
}
