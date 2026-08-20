use crate::net::host::binding::{BindId, BindOptions, NetInfo};
use crate::net::host::host_for_existing;
use crate::service::effects::prelude::*;
use crate::{HostId, PackageId};

/// Hard cap on how many ports a single `bindPortRange` call can claim.
/// Matched on the SDK side as `MAX_BIND_PORT_RANGE_SIZE`.
pub const MAX_BIND_PORT_RANGE_SIZE: u16 = 500;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BindParams {
    id: HostId,
    internal_port: u16,
    #[serde(flatten)]
    options: BindOptions,
}
pub async fn bind(
    context: EffectContext,
    BindParams {
        id,
        internal_port,
        options,
    }: BindParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    context
        .seed
        .persistent_container
        .net_service
        .bind(id, internal_port, options)
        .await
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BindRangeParams {
    pub id: HostId,
    pub internal_start_port: u16,
    pub external_start_port: u16,
    pub number_of_ports: u16,
}

pub async fn bind_range(
    context: EffectContext,
    BindRangeParams {
        id,
        internal_start_port,
        external_start_port,
        number_of_ports,
    }: BindRangeParams,
) -> Result<(), Error> {
    if number_of_ports < 2 {
        return Err(Error::new(
            eyre!("numberOfPorts must be at least 2; use bind for a single port"),
            ErrorKind::InvalidRequest,
        ));
    }
    if number_of_ports > MAX_BIND_PORT_RANGE_SIZE {
        return Err(Error::new(
            eyre!("numberOfPorts ({number_of_ports}) exceeds maximum ({MAX_BIND_PORT_RANGE_SIZE})"),
            ErrorKind::InvalidRequest,
        ));
    }
    let context = context.deref()?;
    context
        .seed
        .persistent_container
        .net_service
        .bind_range(
            id,
            internal_start_port,
            external_start_port,
            number_of_ports,
        )
        .await
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Parser)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ClearBindingsParams {
    #[serde(default)]
    pub except: Vec<BindId>,
}

pub async fn clear_bindings(
    context: EffectContext,
    ClearBindingsParams { except }: ClearBindingsParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    context
        .seed
        .persistent_container
        .net_service
        .clear_bindings(except.into_iter().collect())
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RetireHostParams {
    pub id: HostId,
}

/// No `packageId`: a service may only retire its own hosts.
pub async fn retire_host(
    context: EffectContext,
    RetireHostParams { id }: RetireHostParams,
) -> Result<bool, Error> {
    let context = context.deref()?;
    context
        .seed
        .persistent_container
        .net_service
        .retire_host(id)
        .await
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RetireBindingParams {
    pub id: HostId,
    pub internal_port: u16,
}

pub async fn retire_binding(
    context: EffectContext,
    RetireBindingParams { id, internal_port }: RetireBindingParams,
) -> Result<bool, Error> {
    let context = context.deref()?;
    context
        .seed
        .persistent_container
        .net_service
        .retire_binding(id, internal_port)
        .await
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct GetServicePortForwardParams {
    #[ts(optional)]
    package_id: Option<PackageId>,
    host_id: HostId,
    internal_port: u16,
}
pub async fn get_service_port_forward(
    context: EffectContext,
    GetServicePortForwardParams {
        package_id,
        host_id,
        internal_port,
    }: GetServicePortForwardParams,
) -> Result<Option<NetInfo>, Error> {
    let context = context.deref()?;

    let package_id = package_id.unwrap_or_else(|| context.seed.id.clone());

    let mut db = context.seed.ctx.db.peek().await;
    Ok(host_for_existing(&mut db, &package_id, &host_id)?
        .map(|host| host.as_bindings().de())
        .transpose()?
        .and_then(|bindings| bindings.get(&internal_port).map(|info| info.net.clone())))
}
