use crate::net::host::Host;
use crate::service::effects::callbacks::CallbackHandler;
use crate::service::effects::prelude::*;
use crate::service::rpc::CallbackId;
use crate::{HostId, PackageId};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct GetHostInfoParams {
    host_id: HostId,

    package_id: Option<PackageId>,

    callback: Option<CallbackId>,
}

rpc_toolkit::reflect_ts!(GetHostInfoParams);
rpc_toolkit::ts_export!(GetHostInfoParams, namespaces = [""]);
pub async fn get_host_info(
    context: EffectContext,
    GetHostInfoParams {
        host_id,
        package_id,
        callback,
    }: GetHostInfoParams,
) -> Result<Option<Host>, Error> {
    let context = context.deref()?;
    let package_id = package_id.unwrap_or_else(|| context.seed.id.clone());

    // `start-os` is the server: its single `admin` host lives in serverInfo.
    let ptr = if package_id.is_start_os() {
        if host_id != HostId::admin() {
            return Ok(None);
        }
        "/public/serverInfo/network/host".to_owned()
    } else {
        format!("/public/packageData/{}/hosts/{}", package_id, host_id)
    }
    .parse()
    .expect("valid json pointer");
    let mut watch = context.seed.ctx.db.watch(ptr).await.typed::<Host>();

    let res = watch.peek_and_mark_seen()?.de().ok();

    if let Some(callback) = callback {
        let callback = callback.register(&context.seed.persistent_container);
        context.seed.ctx.callbacks.add_get_host_info(
            package_id.clone(),
            host_id.clone(),
            watch,
            CallbackHandler::new(&context, callback),
        );
    }

    Ok(res)
}
