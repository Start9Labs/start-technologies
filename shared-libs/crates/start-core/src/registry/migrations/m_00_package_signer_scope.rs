use std::path::Path;

use imbl_value::json;

use super::RegistryMigration;
use crate::prelude::*;

pub struct PackageSignerScopeMigration;
impl RegistryMigration for PackageSignerScopeMigration {
    fn action(&self, db: &mut Value, _: &Path) -> Result<(), Error> {
        for (_, info) in db["index"]["package"]["packages"]
            .as_object_mut()
            .unwrap()
            .iter_mut()
        {
            let prev = info["authorized"].clone();
            if let Some(prev) = prev.as_array() {
                info["authorized"] = Value::Object(
                    prev.iter()
                        .filter_map(|g| g.as_str())
                        .map(|g| (g.into(), json!("*")))
                        .collect(),
                )
            }
        }

        Ok(())
    }
}
