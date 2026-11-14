use librqbit::ListenerOptions;
use rpc_toolkit::Empty;

use crate::context::RpcContext;
use crate::db::model::DatabaseModel;
use crate::hostname::ServerHostname;
use crate::net::host::ForServer;
use crate::net::host::binding::{
    self, BindInfo, BindOptions, BindingPreview, ProxyOptions, Security,
};
use crate::net::net_controller::{bind_in, export_interface_in, retire_binding_in};
use crate::net::service_interface::{
    AddressInfo, HostnameInfo, ServiceInterface, ServiceInterfaceType,
};
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

fn stored_binding(db: &DatabaseModel) -> Result<Option<BindInfo>, Error> {
    db.as_public()
        .as_server_info()
        .as_network()
        .as_host()
        .as_bindings()
        .as_idx(&INTERNAL_PORT)
        .map(|binding| binding.de())
        .transpose()
}

/// Startup preserves consented bindings while their feature owner resumes.
pub(crate) fn retained_bindings(
    db: &DatabaseModel,
    available: bool,
) -> Result<std::collections::BTreeSet<binding::BindId>, Error> {
    if !available
        || !stored_binding(db)?
            .is_some_and(|binding| binding.enabled && !binding.enabled_addresses().is_empty())
    {
        return Ok(Default::default());
    }
    let preferences: TorrentPreferences = db.as_public().as_server_info().as_bittorrent().de()?;
    let retain = preferences.downloads && preferences.seeding;
    Ok(if retain {
        [binding::BindId {
            id: HostId::admin(),
            internal_port: INTERNAL_PORT,
        }]
        .into()
    } else {
        Default::default()
    })
}

pub(super) fn preview(db: &DatabaseModel) -> Result<Option<BindingPreview>, Error> {
    if stored_binding(db)?.is_some() {
        return Ok(None);
    }
    let info = db.as_public().as_server_info();
    let hostname = ServerHostname::load(info)?;
    info.as_network()
        .as_host()
        .preview_binding(
            INTERNAL_PORT,
            binding_options(None),
            &hostname,
            &info.as_network().as_gateways().de()?,
            &db.as_private().as_available_ports().de()?,
        )
        .map(Some)
}

/// Incoming consent owns the binding's lifetime, including its ports and interfaces.
pub(super) fn clear_selections(db: &mut DatabaseModel) -> Result<(), Error> {
    if stored_binding(db)?.is_some() {
        retire_binding_in(db, &PackageId::start_os(), &HostId::admin(), INTERNAL_PORT)?;
    }
    Ok(())
}

fn eligible(db: &DatabaseModel, available: bool) -> Result<bool, Error> {
    if !available {
        return Ok(false);
    }
    let info = db.as_public().as_server_info();
    let preferences: TorrentPreferences = info.as_bittorrent().de()?;
    Ok(preferences.downloads
        && preferences.seeding
        && !info.as_status_info().as_bittorrent_watcher_failed().de()?)
}

fn configured(binding: &BindInfo) -> bool {
    matches!(binding.options.proxy, Some(ProxyOptions::Tcp))
        && binding.options.secure.is_some_and(|security| !security.ssl)
        && binding.options.default_off
        && binding.options.transport == crate::net::transport::Transport::tcp()
        && binding.options.preferred_external_port == 0
}

fn active(db: &DatabaseModel, available: bool, binding: &BindInfo) -> Result<bool, Error> {
    Ok(eligible(db, available)? && binding.enabled && !binding.enabled_addresses().is_empty())
}

pub(crate) async fn configure_incoming(ctx: &RpcContext) -> Result<Option<ListenerOptions>, Error> {
    let available = ctx.torrents.available();
    let snapshot = ctx.db.peek().await;
    let Some(binding) = stored_binding(&snapshot)? else {
        return Ok(None);
    };
    if configured(&binding) && active(&snapshot, available, &binding)? {
        return listener_for(&binding).map(Some);
    }
    drop(snapshot);
    ctx.db.mutate(|db| configure_in(db, available)).await.result
}

pub(super) fn configure_in(
    db: &mut DatabaseModel,
    available: bool,
) -> Result<Option<ListenerOptions>, Error> {
    let Some(binding) = stored_binding(db)? else {
        return Ok(None);
    };
    if !active(db, available, &binding)? {
        clear_selections(db)?;
        return Ok(None);
    }
    if !configured(&binding) {
        bind_in(
            db,
            &PackageId::start_os(),
            &HostId::admin(),
            INTERNAL_PORT,
            binding_options(Some(binding.options)),
        )?;
    }
    listener_for(&stored_binding(db)?.unwrap()).map(Some)
}

fn unavailable() -> Error {
    Error::new(
        eyre!("{}", t!("net.host.binding.address-unavailable")),
        ErrorKind::InvalidRequest,
    )
}

fn validate_selection(
    db: &DatabaseModel,
    available: bool,
    address: &HostnameInfo,
) -> Result<(), Error> {
    if !eligible(db, available)? {
        return Err(unavailable());
    }
    let candidates = match stored_binding(db)? {
        Some(binding) if binding.enabled => binding.addresses.available,
        Some(_) => return Err(unavailable()),
        None => preview(db)?.unwrap().addresses.available,
    };
    if !candidates.contains(address) {
        return Err(unavailable());
    }
    Ok(())
}

fn allocate_address(
    db: &mut DatabaseModel,
    mut address: HostnameInfo,
) -> Result<HostnameInfo, Error> {
    if stored_binding(db)?.is_none() {
        bind_in(
            db,
            &PackageId::start_os(),
            &HostId::admin(),
            INTERNAL_PORT,
            binding_options(None),
        )?;
        export_interface_in(db, &PackageId::start_os(), interface())?;
        address.port = stored_binding(db)?.unwrap().net.assigned_port;
    }
    Ok(address)
}

fn release_if_unselected(db: &mut DatabaseModel) -> Result<(), Error> {
    if stored_binding(db)?.is_some_and(|binding| binding.enabled_addresses().is_empty()) {
        clear_selections(db)?;
    }
    Ok(())
}

pub(super) fn select_address_in(
    db: &mut DatabaseModel,
    available: bool,
    address: HostnameInfo,
    enabled: bool,
) -> Result<HostnameInfo, Error> {
    validate_selection(db, available, &address)?;
    if !enabled && stored_binding(db)?.is_none() {
        return Ok(address);
    }
    let address = allocate_address(db, address)?;
    binding::set_address_enabled_in::<ForServer>(
        db,
        binding::BindingSetAddressEnabledParams {
            internal_port: INTERNAL_PORT,
            address: address.clone(),
            enabled: Some(enabled),
        },
        &Empty {},
    )?;
    release_if_unselected(db)?;
    Ok(address)
}

pub(super) fn select_gua_in(
    db: &mut DatabaseModel,
    available: bool,
    address: HostnameInfo,
    wan: bool,
) -> Result<(), Error> {
    validate_selection(db, available, &address)?;
    if address.gua_ip().is_none() {
        return Err(Error::new(
            eyre!("{}", t!("net.host.binding.not-global-unicast")),
            ErrorKind::InvalidRequest,
        ));
    }
    if !wan && stored_binding(db)?.is_none() {
        return Ok(());
    }
    let address = allocate_address(db, address)?;
    binding::set_gua_wan_in::<ForServer>(
        db,
        binding::BindingSetGuaWanParams {
            internal_port: INTERNAL_PORT,
            address,
            wan,
        },
        &Empty {},
    )?;
    release_if_unselected(db)
}

fn binding_options(existing: Option<BindOptions>) -> BindOptions {
    BindOptions {
        transport: crate::net::transport::Transport::tcp(),
        preferred_external_port: 0,
        add_ssl: existing.and_then(|options| options.add_ssl),
        secure: Some(Security { ssl: false }),
        proxy: Some(ProxyOptions::Tcp),
        default_off: true,
    }
}

fn listener_for(binding: &BindInfo) -> Result<ListenerOptions, Error> {
    Ok(listener(
        binding
            .net
            .assigned_port
            .or_not_found("BitTorrent external port")?,
    ))
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
    fn broader_transport_requires_reconfiguration() {
        let mut ports = crate::net::forward::AvailablePorts::new();
        let mut binding = BindInfo::new(&mut ports, binding_options(None), true).unwrap();
        assert!(configured(&binding));
        binding.options.transport = Default::default();
        assert!(!configured(&binding));
    }

    #[test]
    fn existing_binding_keeps_its_port_and_switches_to_default_off_tcp() {
        let mut previous = binding_options(None);
        previous.preferred_external_port = 45678;
        previous.proxy = None;
        previous.secure = None;
        previous.default_off = false;
        previous.transport = Default::default();
        let mut ports = crate::net::forward::AvailablePorts::new();
        let binding = BindInfo::new(&mut ports, previous.clone(), true).unwrap();
        let options = binding_options(Some(previous));
        assert_eq!(options.preferred_external_port, 0);
        assert_eq!(options.transport, crate::net::transport::Transport::tcp());
        assert_eq!(options.proxy, Some(ProxyOptions::Tcp));
        assert!(options.secure.is_some_and(|security| !security.ssl));
        assert!(options.default_off);
        let updated = binding.update(&mut ports, options, true).unwrap();
        assert_eq!(updated.net.assigned_port, Some(45678));
    }

    #[test]
    fn fresh_incoming_binding_leaves_service_port_6881_free() {
        let mut ports = crate::net::forward::AvailablePorts::new();
        let binding = BindInfo::new(&mut ports, binding_options(None), true).unwrap();
        let external = binding.net.assigned_port.unwrap();
        assert_ne!(external, 0);
        assert_ne!(external, INTERNAL_PORT);
        assert_eq!(
            ports.try_alloc(INTERNAL_PORT, false, false),
            Some(INTERNAL_PORT)
        );
        assert_eq!(listener(external).announce_port, Some(external));
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
