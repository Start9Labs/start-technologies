use std::path::Path;

use super::RegistryMigration;
use crate::prelude::*;

pub struct Ed25519phSignatureLabel;
impl RegistryMigration for Ed25519phSignatureLabel {
    fn action(&self, db: &mut Value, _: &Path) -> Result<(), Error> {
        crate::sign::compat::relabel_legacy_signatures(&mut db["index"]);
        Ok(())
    }
}
