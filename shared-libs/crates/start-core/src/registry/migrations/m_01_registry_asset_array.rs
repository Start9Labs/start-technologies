use std::path::Path;

use imbl::vector;

use super::RegistryMigration;
use crate::prelude::*;

pub struct RegistryAssetArray;
impl RegistryMigration for RegistryAssetArray {
    fn action(&self, db: &mut Value, _: &Path) -> Result<(), Error> {
        for (_, info) in db["index"]["package"]["packages"]
            .as_object_mut()
            .unwrap()
            .iter_mut()
        {
            for (_, info) in info["versions"].as_object_mut().unwrap().iter_mut() {
                let hw_req = info["hardwareRequirements"].take();
                let mut s9pk = info["s9pk"].take();
                s9pk["urls"] = Value::Array(vector![s9pk["url"].take()]);
                info["s9pks"] = Value::Array(vector![Value::Array(vector![hw_req, s9pk])]);
            }
        }
        for (_, info) in db["index"]["os"]["versions"]
            .as_object_mut()
            .unwrap()
            .iter_mut()
        {
            for asset_ty in ["iso", "squashfs", "img"] {
                for (_, info) in info[asset_ty].as_object_mut().unwrap().iter_mut() {
                    info["urls"] = Value::Array(vector![info["url"].take()]);
                }
            }
        }

        Ok(())
    }
}
