use std::path::Path;

use crate::DATA_DIR;
use crate::service::effects::prelude::*;
use crate::util::io::{delete_file, write_file_atomic};
use crate::volume::PKG_VOLUME_DIR;

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetDataVersionParams {
    version: Option<String>,
}

rpc_toolkit::reflect_ts!(SetDataVersionParams);
rpc_toolkit::ts_export!(SetDataVersionParams, namespaces = [""]);
#[instrument(skip(context))]
pub async fn set_data_version(
    context: EffectContext,
    SetDataVersionParams { version }: SetDataVersionParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    let package_id = &context.seed.id;
    let path = Path::new(DATA_DIR)
        .join(PKG_VOLUME_DIR)
        .join(package_id)
        .join("data")
        .join(".version");
    if let Some(version) = version {
        write_file_atomic(path, version.as_bytes()).await?;
    } else {
        delete_file(path).await?;
    }

    Ok(())
}

#[instrument(skip_all)]
pub async fn get_data_version(context: EffectContext) -> Result<Option<String>, Error> {
    let context = context.deref()?;
    crate::service::get_data_version(&context.seed.id).await
}
