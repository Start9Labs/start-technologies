use librqbit::ListenerOptions;

use crate::context::RpcContext;
use crate::net::host::binding::BindOptions;
use crate::net::service_interface::{AddressInfo, ServiceInterface, ServiceInterfaceType};
use crate::prelude::*;
use crate::{HostId, Id, ServiceInterfaceId};

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

pub(crate) async fn configure_incoming(
    ctx: &RpcContext,
    enabled: bool,
) -> Result<Option<ListenerOptions>, Error> {
    if !enabled {
        ctx.os_net_service
            .disable_binding(HostId::admin(), INTERNAL_PORT)
            .await?;
        return Ok(None);
    }
    let binding: Option<crate::net::host::binding::BindInfo> = ctx
        .db
        .peek()
        .await
        .as_public()
        .as_server_info()
        .as_network()
        .as_host()
        .as_bindings()
        .as_idx(&INTERNAL_PORT)
        .map(|binding| binding.de())
        .transpose()?;
    if !binding.as_ref().is_some_and(|binding| binding.enabled) {
        let options = binding
            .map(|binding| binding.options)
            .unwrap_or(BindOptions {
                preferred_external_port: INTERNAL_PORT,
                add_ssl: None,
                secure: None,
                proxy: None,
            });
        ctx.os_net_service
            .bind(HostId::admin(), INTERNAL_PORT, options)
            .await?;
    }
    ctx.os_net_service.export_interface(interface()).await?;
    let assigned_port = ctx
        .db
        .peek()
        .await
        .as_public()
        .as_server_info()
        .as_network()
        .as_host()
        .as_bindings()
        .as_idx(&INTERNAL_PORT)
        .or_not_found(INTERNAL_PORT)?
        .as_net()
        .de()?
        .assigned_port;
    Ok(Some(listener(
        assigned_port.or_not_found("BitTorrent external port")?,
    )))
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
    fn listener_uses_native_port_allocation_without_independent_upnp() {
        let interface = interface();
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
