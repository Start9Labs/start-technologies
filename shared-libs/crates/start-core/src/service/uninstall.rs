use std::collections::BTreeSet;
use std::path::Path;

use crate::context::RpcContext;
use crate::db::model::package::{InstalledState, InstallingInfo, InstallingState, PackageState};
use crate::net::host::all_hosts;
use crate::net::service_interface::{HostnameInfo, HostnameMetadata};
use crate::prelude::*;
use crate::volume::PKG_VOLUME_DIR;
use crate::{DATA_DIR, PACKAGE_DATA, PackageId};

pub async fn cleanup(ctx: &RpcContext, id: &PackageId, soft: bool) -> Result<(), Error> {
    Ok(
        if let Some(pde) = ctx
            .db
            .mutate(|d| {
                if let Some(pde) = d
                    .as_public_mut()
                    .as_package_data_mut()
                    .remove(&id)?
                    .map(|d| d.de())
                    .transpose()?
                {
                    d.as_private_mut().as_available_ports_mut().mutate(|p| {
                        // Free single-port binding allocations...
                        p.free(
                            pde.hosts
                                .0
                                .values()
                                .flat_map(|h| h.bindings.values())
                                .flat_map(|b| {
                                    b.net
                                        .assigned_port
                                        .into_iter()
                                        .chain(b.net.assigned_ssl_port)
                                }),
                        );
                        // ...and the contiguous port-range allocations, which
                        // are tracked separately in `binding_ranges` and would
                        // otherwise leak (up to MAX_BIND_PORT_RANGE_SIZE ports
                        // per range) on every uninstall. Subtract before adding
                        // so a range ending at 65535 doesn't overflow u16.
                        p.free(pde.hosts.0.values().flat_map(|h| {
                            h.binding_ranges.values().flat_map(|r| {
                                r.external_start_port
                                    ..=r.external_start_port + (r.number_of_ports - 1)
                            })
                        }));
                        Ok(())
                    })?;
                    d.as_private_mut().as_package_stores_mut().remove(&id)?;
                    // Remove plugin URLs exported by this package from all hosts
                    for host in all_hosts(d) {
                        let host = host?;
                        for (_, bind) in host.as_bindings_mut().as_entries_mut()? {
                            bind.as_addresses_mut().as_available_mut().mutate(
                                |available: &mut BTreeSet<HostnameInfo>| {
                                    available.retain(|h| {
                                        !matches!(
                                            &h.metadata,
                                            HostnameMetadata::Plugin { package_id, .. }
                                            if package_id == id
                                        )
                                    });
                                    Ok(())
                                },
                            )?;
                        }
                    }
                    Ok(Some(pde))
                } else {
                    Ok(None)
                }
            })
            .await
            .result?
        {
            let manifest = match pde.state_info {
                PackageState::Installing(InstallingState {
                    installing_info:
                        InstallingInfo {
                            new_manifest: manifest,
                            ..
                        },
                })
                | PackageState::Restoring(InstallingState {
                    installing_info:
                        InstallingInfo {
                            new_manifest: manifest,
                            ..
                        },
                })
                | PackageState::Removing(InstalledState { manifest }) => manifest,
                s => {
                    return Err(Error::new(
                        eyre!(
                            "{}",
                            t!(
                                "service.uninstall.invalid-package-state-for-cleanup",
                                state = format!("{s:?}")
                            )
                        ),
                        ErrorKind::InvalidRequest,
                    ));
                }
            };
            if !soft {
                let path = Path::new(DATA_DIR).join(PKG_VOLUME_DIR).join(&manifest.id);
                crate::util::btrfs::delete_tree(&path).await?;
                #[cfg(not(feature = "dev"))]
                {
                    let logs_dir = Path::new(PACKAGE_DATA).join("logs").join(&manifest.id);
                    crate::util::io::delete_dir(&logs_dir).await?;
                }
            }
        },
    )
}
