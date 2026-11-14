use librqbit::ListenerOptions;

use crate::context::RpcContext;
use crate::db::model::DatabaseModel;
use crate::net::host::binding::{BindInfo, BindOptions, ProxyOptions, Security};
use crate::net::net_controller::{bind_in, export_interface_in};
use crate::net::service_interface::{AddressInfo, ServiceInterface, ServiceInterfaceType};
use crate::prelude::*;
use crate::registry::bittorrent::TorrentPreferences;
use crate::{HostId, Id, PackageId, ServiceInterfaceId};

const INTERNAL_PORT: u16 = 6881;

pub(super) fn interface() -> ServiceInterface {
    ServiceInterface {
        id: ServiceInterfaceId::from(Id::try_from("bittorrent".to_owned()).expect("valid id")),
        name: t!("system.bittorrent.interface-name").into(),
        description: t!("system.bittorrent.interface-description").into(),
        masked: false,
        address_info: AddressInfo {
            username: None,
            host_id: HostId::admin(),
            internal_port: INTERNAL_PORT,
            scheme: Some("tcp".into()),
            ssl_scheme: None,
            suffix: String::new(),
        },
        interface_type: ServiceInterfaceType::P2p,
        preferred_launcher_address: None,
    }
}

/// Address rows own ingress opt-in; disabling either parent clears their selections.
pub(super) fn clear_selections(db: &mut DatabaseModel) -> Result<(), Error> {
    if let Some(binding) = db
        .as_public_mut()
        .as_server_info_mut()
        .as_network_mut()
        .as_host_mut()
        .as_bindings_mut()
        .as_idx_mut(&INTERNAL_PORT)
    {
        binding.mutate(|binding| {
            binding.addresses.disable_all();
            binding.disable();
            Ok(())
        })?;
    }
    Ok(())
}

pub(crate) async fn configure_incoming(ctx: &RpcContext) -> Result<Option<ListenerOptions>, Error> {
    let available = ctx.torrents.available();
    ctx.db.mutate(|db| configure_in(db, available)).await.result
}

pub(super) fn configure_in(
    db: &mut DatabaseModel,
    available: bool,
) -> Result<Option<ListenerOptions>, Error> {
    let preferences: TorrentPreferences = db.as_public().as_server_info().as_bittorrent().de()?;
    let binding: Option<BindInfo> = db
        .as_public()
        .as_server_info()
        .as_network()
        .as_host()
        .as_bindings()
        .as_idx(&INTERNAL_PORT)
        .map(|binding| binding.de())
        .transpose()?;
    let configured = binding.as_ref().is_some_and(|binding| {
        matches!(binding.options.proxy, Some(ProxyOptions::Tcp))
            && binding.options.secure.is_some_and(|security| !security.ssl)
            && binding.options.default_off
    });
    if !configured {
        clear_selections(db)?;
        bind_in(
            db,
            &PackageId::start_os(),
            &HostId::admin(),
            INTERNAL_PORT,
            binding_options(binding.as_ref().map(|binding| binding.options.clone())),
        )?;
    }
    let interface = interface();
    if binding
        .as_ref()
        .is_none_or(|binding| !binding.interfaces.contains_key(&interface.id))
    {
        export_interface_in(db, &PackageId::start_os(), interface)?;
    }
    db.as_public_mut()
        .as_server_info_mut()
        .as_network_mut()
        .as_host_mut()
        .as_bindings_mut()
        .as_idx_mut(&INTERNAL_PORT)
        .or_not_found(INTERNAL_PORT)?
        .mutate(|binding| {
            binding.enabled = available && preferences.seeding;
            if !binding.enabled {
                binding.addresses.disable_all();
            }
            if !binding.enabled || binding.enabled_addresses().is_empty() {
                return Ok(None);
            }
            Ok(Some(listener(
                binding
                    .net
                    .assigned_port
                    .or_not_found("BitTorrent external port")?,
            )))
        })
}

fn binding_options(existing: Option<BindOptions>) -> BindOptions {
    let mut options = existing.unwrap_or(BindOptions {
        preferred_external_port: INTERNAL_PORT,
        add_ssl: None,
        secure: None,
        proxy: None,
        default_off: true,
    });
    options.proxy = Some(ProxyOptions::Tcp);
    options.secure = Some(Security { ssl: false });
    options.default_off = true;
    options
}

fn listener(assigned_port: u16) -> ListenerOptions {
    ListenerOptions {
        listen_addr: ([127, 0, 0, 1], INTERNAL_PORT).into(),
        announce_port: Some(assigned_port),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_binding_keeps_its_port_and_switches_to_default_off_tcp() {
        let mut previous = binding_options(None);
        previous.preferred_external_port = 45678;
        previous.proxy = None;
        previous.secure = None;
        previous.default_off = false;
        let options = binding_options(Some(previous));
        assert_eq!(options.preferred_external_port, 45678);
        assert_eq!(options.proxy, Some(ProxyOptions::Tcp));
        assert!(options.secure.is_some_and(|security| !security.ssl));
        assert!(options.default_off);
    }

    #[test]
    fn listener_uses_native_port_allocation_without_independent_upnp() {
        let interface = interface();
        let options = binding_options(None);
        assert_eq!(options.proxy, Some(ProxyOptions::Tcp));
        let listener = listener(45678);
        assert!(listener.listen_addr.ip().is_loopback());
        assert_eq!(
            listener.listen_addr.port(),
            interface.address_info.internal_port
        );
        assert_eq!(listener.announce_port, Some(45678));
        assert!(!listener.enable_upnp_port_forwarding);
        assert!(matches!(
            interface.interface_type,
            ServiceInterfaceType::P2p
        ));
        assert_eq!(interface.address_info.host_id, HostId::admin());
    }
}
