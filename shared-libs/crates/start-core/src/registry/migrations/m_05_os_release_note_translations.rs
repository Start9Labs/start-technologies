use std::path::Path;

use super::RegistryMigration;
use crate::prelude::*;

pub struct OsReleaseNoteTranslations;
impl RegistryMigration for OsReleaseNoteTranslations {
    fn action(&self, _: &mut Value, _: &Path) -> Result<(), Error> {
        // English-only records remain valid; the translations field defaults empty.
        Ok(())
    }
}
