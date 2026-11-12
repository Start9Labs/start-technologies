use std::sync::Arc;

use crate::net::http_proxy::{
    HttpProxyAttachment, HttpProxyProfile, HttpProxyRegistration, attachments_for,
    profiles_are_valid,
};
use crate::service::Service;
use crate::service::effects::callbacks::CallbackHandler;
use crate::service::effects::plugin::PluginId;
use crate::service::effects::prelude::*;
use crate::service::rpc::CallbackId;
use crate::{HostId, PackageId};

fn require_http_proxy_plugin(service: &Arc<Service>) -> Result<(), Error> {
    if !service
        .seed
        .persistent_container
        .s9pk
        .as_manifest()
        .metadata
        .plugins
        .contains(&PluginId::HttpProxyV0)
    {
        return Err(Error::new(
            eyre!(
                "{}",
                t!(
                    "net.plugin.manifest-missing-plugin",
                    plugin = "http-proxy-v0"
                )
            ),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SetHttpProxyProfilesParams {
    pub profiles: Vec<HttpProxyProfile>,
}

rpc_toolkit::reflect_ts!(SetHttpProxyProfilesParams);
rpc_toolkit::ts_export!(SetHttpProxyProfilesParams, namespaces = [""]);

pub async fn set_profiles(
    context: EffectContext,
    SetHttpProxyProfilesParams { profiles }: SetHttpProxyProfilesParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    require_http_proxy_plugin(&context)?;
    if !profiles_are_valid(&profiles) {
        return Err(Error::new(
            eyre!("{}", t!("net.http-proxy.invalid-profiles")),
            ErrorKind::InvalidRequest,
        ));
    }
    let package_id = context.seed.id.clone();
    context
        .seed
        .ctx
        .db
        .mutate(|db| {
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&package_id)
                .or_not_found(&package_id)?
                .as_plugin_mut()
                .as_http_proxy_mut()
                .ser(&Some(HttpProxyRegistration { profiles }))
        })
        .await
        .result
}

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct GetHttpProxyAttachmentsParams {
    pub callback: Option<CallbackId>,
}

rpc_toolkit::reflect_ts!(GetHttpProxyAttachmentsParams);
rpc_toolkit::ts_export!(GetHttpProxyAttachmentsParams, namespaces = [""]);

pub async fn get_attachments(
    context: EffectContext,
    GetHttpProxyAttachmentsParams { callback }: GetHttpProxyAttachmentsParams,
) -> Result<Vec<HttpProxyAttachment>, Error> {
    let context = context.deref()?;
    require_http_proxy_plugin(&context)?;
    let mut watch: patch_db::TypedDbWatch<crate::db::model::public::Public> = context
        .seed
        .ctx
        .db
        .watch("/public".parse().unwrap())
        .await
        .typed();
    let snapshot = watch.peek_and_mark_seen()?;
    let packages = snapshot.as_package_data().de()?;
    let os = snapshot.as_server_info().as_network().as_host().de()?;
    let mut attachments = attachments_for(&packages, &context.seed.id);
    attachments.extend(crate::net::http_proxy::host_attachments(
        &packages,
        &context.seed.id,
        &PackageId::start_os(),
        &HostId::admin(),
        &os,
    ));
    if let Some(callback) = callback {
        let callback = callback.register(&context.seed.persistent_container);
        context.seed.ctx.callbacks.add_get_http_proxy_attachments(
            context.seed.id.clone(),
            watch,
            CallbackHandler::new(&context, callback),
        );
    }
    Ok(attachments)
}
