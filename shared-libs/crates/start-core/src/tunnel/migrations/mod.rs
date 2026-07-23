use patch_db::ModelExt;

use crate::prelude::*;
use crate::tunnel::db::TunnelDatabase;

mod m_00_port_forward_entry;
mod m_01_port_forward_kind;
mod m_02_wg_client_kind;
mod m_03_port_forward_auto;
mod m_04_reclaim_port_80_redirect;

#[cfg(test)]
pub use m_01_port_forward_kind::PortForwardKind;

pub trait TunnelMigration {
    fn name(&self) -> &'static str {
        let val = std::any::type_name_of_val(self);
        val.rsplit_once("::").map_or(val, |v| v.1)
    }
    fn action(&self, db: &mut Value) -> Result<(), Error>;
}

pub const MIGRATIONS: &[&dyn TunnelMigration] = &[
    &m_00_port_forward_entry::PortForwardEntry,
    &m_01_port_forward_kind::PortForwardKind,
    &m_02_wg_client_kind::WgClientKind,
    &m_03_port_forward_auto::PortForwardAuto,
    &m_04_reclaim_port_80_redirect::ReclaimPort80Redirect,
];

#[instrument(skip_all)]
pub fn run_migrations(db: &mut Model<TunnelDatabase>) -> Result<(), Error> {
    let mut migrations = db.as_migrations().de().unwrap_or_default();
    for migration in MIGRATIONS {
        let name = migration.name();
        if !migrations.contains(name) {
            migration.action(ModelExt::as_value_mut(db))?;
            migrations.insert(name.into());
        }
    }
    let mut db_deser = db.de()?;
    db_deser.migrations = migrations;
    db.ser(&db_deser)?;
    Ok(())
}
