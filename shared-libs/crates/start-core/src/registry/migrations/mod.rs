use std::path::Path;

use patch_db::ModelExt;

use crate::prelude::*;
use crate::registry::RegistryDatabase;

mod m_00_package_signer_scope;
mod m_01_registry_asset_array;
mod m_02_ed25519ph_signature_label;
mod m_03_commitment_type_tags;
mod m_04_icon_store;
mod m_05_os_release_note_translations;

pub trait RegistryMigration {
    fn name(&self) -> &'static str {
        let val = std::any::type_name_of_val(self);
        val.rsplit_once("::").map_or(val, |v| v.1)
    }
    fn action(&self, db: &mut Value, datadir: &Path) -> Result<(), Error>;
}

pub const MIGRATIONS: &[&dyn RegistryMigration] = &[
    &m_00_package_signer_scope::PackageSignerScopeMigration,
    &m_01_registry_asset_array::RegistryAssetArray,
    &m_02_ed25519ph_signature_label::Ed25519phSignatureLabel,
    &m_03_commitment_type_tags::CommitmentTypeTags,
    &m_04_icon_store::IconStore,
    &m_05_os_release_note_translations::OsReleaseNoteTranslations,
];

#[instrument(skip_all)]
pub fn run_migrations(db: &mut Model<RegistryDatabase>, datadir: &Path) -> Result<(), Error> {
    let mut migrations = db.as_migrations().de().unwrap_or_default();
    for migration in MIGRATIONS {
        let name = migration.name();
        if !migrations.contains(name) {
            migration.action(ModelExt::as_value_mut(db), datadir)?;
            migrations.insert(name.into());
        }
    }
    let mut db_deser = db.de()?;
    db_deser.migrations = migrations;
    db.ser(&db_deser)?;
    Ok(())
}
