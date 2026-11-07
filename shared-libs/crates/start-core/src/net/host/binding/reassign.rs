use clap::Parser;
use serde::{Deserialize, Serialize};

use super::{BindInfo, HostnameMetadata};
use crate::context::RpcContext;
use crate::db::model::DatabaseModel;
use crate::hostname::ServerHostname;
use crate::net::forward::AvailablePorts;
use crate::net::host::{HostApiKind, all_hosts};
use crate::prelude::*;

#[cfg(test)]
mod tests;

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct ReassignPortParams {
    #[arg(help = "help.arg.internal-port")]
    internal_port: u16,
    #[arg(help = "help.arg.port", value_parser = clap::value_parser!(u16).range(1..))]
    external_port: u16,
    #[arg(long, help = "help.arg.binding-ssl-port")]
    #[serde(default)]
    ssl: bool,
}

rpc_toolkit::reflect_ts!(ReassignPortParams);

pub(super) async fn reassign_port<Kind: HostApiKind>(
    ctx: RpcContext,
    params: ReassignPortParams,
    inheritance: Kind::Inheritance,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| reassign::<Kind>(db, &params, &inheritance))
        .await
        .result
}

fn reassign<Kind: HostApiKind>(
    db: &mut DatabaseModel,
    params: &ReassignPortParams,
    inheritance: &Kind::Inheritance,
) -> Result<(), Error> {
    let mut ports = db.as_private().as_available_ports().de()?;
    let changed = Kind::host_for(inheritance, db)?
        .as_bindings_mut()
        .mutate(|bindings| {
            let binding = bindings
                .get_mut(&params.internal_port)
                .or_not_found(params.internal_port)?;
            move_assigned_port(
                binding,
                &mut ports,
                params.external_port,
                params.ssl,
                Kind::privileged(inheritance),
            )
        })?;
    if !changed {
        return Ok(());
    }
    let hostname = ServerHostname::load(db.as_public().as_server_info())?;
    let gateways = db
        .as_public()
        .as_server_info()
        .as_network()
        .as_gateways()
        .de()?;
    for host in all_hosts(db) {
        let host = host?;
        let previous = host.as_bindings().de()?;
        host.update_addresses(&hostname, &gateways, &ports)?;
        host.as_bindings_mut().mutate(|bindings| {
            for (port, binding) in bindings.iter_mut() {
                let Some(previous) = previous.get(port) else {
                    continue;
                };
                for old in previous.addresses.available.iter().filter(|address| {
                    matches!(address.metadata, HostnameMetadata::PrivateDomain { .. })
                }) {
                    let Some(new) = binding.addresses.available.iter().find(|address| {
                        address.hostname == old.hostname
                            && address.ssl == old.ssl
                            && address.metadata == old.metadata
                    }) else {
                        continue;
                    };
                    if let (Some(old_port), Some(new_port)) = (old.port, new.port) {
                        let old_key = (old.hostname.clone(), old_port);
                        if previous.addresses.disabled.contains(&old_key) {
                            binding.addresses.disabled.remove(&old_key);
                            binding
                                .addresses
                                .disabled
                                .insert((new.hostname.clone(), new_port));
                        }
                    }
                }
            }
            Ok(())
        })?;
    }
    db.as_private_mut().as_available_ports_mut().ser(&ports)
}

fn move_assigned_port(
    binding: &mut BindInfo,
    ports: &mut AvailablePorts,
    external_port: u16,
    ssl: bool,
    privileged: bool,
) -> Result<bool, Error> {
    let assigned = if ssl {
        &mut binding.net.assigned_ssl_port
    } else {
        &mut binding.net.assigned_port
    };
    let old = assigned.ok_or_else(|| {
        Error::new(
            eyre!("{}", t!("net.host.binding.reassign.no-assigned-port")),
            ErrorKind::InvalidRequest,
        )
    })?;
    if old == external_port {
        return Ok(false);
    }
    if external_port == 0 || ports.try_alloc(external_port, ssl, privileged).is_none() {
        return Err(Error::new(
            eyre!(
                "{}",
                t!(
                    "net.host.binding.reassign.port-unavailable",
                    port = external_port
                )
            ),
            ErrorKind::InvalidRequest,
        ));
    }
    ports.free([old]);
    *assigned = Some(external_port);
    let disabled_plugins = binding
        .addresses
        .available
        .iter()
        .filter(|address| matches!(address.metadata, HostnameMetadata::Plugin { .. }))
        .map(super::override_key)
        .filter(|key| binding.addresses.disabled.contains(key))
        .collect::<Vec<_>>();
    binding.addresses.rekey_port(old, external_port);
    binding.addresses.disabled.extend(disabled_plugins);
    binding.addresses.available = std::mem::take(&mut binding.addresses.available)
        .into_iter()
        .map(|mut address| {
            if address.port == Some(old)
                && !matches!(address.metadata, HostnameMetadata::Plugin { .. })
            {
                address.port = Some(external_port);
            }
            address
        })
        .collect();
    Ok(true)
}
