use std::collections::BTreeSet;
use std::net::SocketAddr;

use crate::net::host::Host;
use crate::net::host::binding::DerivedAddressInfo;
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
) -> Result<Value, Error> {
    let context = context.deref()?;
    let package_id = package_id.unwrap_or_else(|| context.seed.id.clone());

    // `start-os` is the server: its single `admin` host lives in serverInfo.
    let ptr = if package_id.is_start_os() {
        if host_id != HostId::admin() {
            return Ok(Value::Null);
        }
        "/public/serverInfo/network/host".to_owned()
    } else {
        format!("/public/packageData/{}/hosts/{}", package_id, host_id)
    }
    .parse()
    .expect("valid json pointer");
    let mut watch = context.seed.ctx.db.watch(ptr).await.typed::<Host>();

    let res = service_host_info(watch.peek_and_mark_seen()?.de().ok())?;

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

pub(in crate::service::effects) fn project_host_info(value: Value) -> Result<Value, Error> {
    service_host_info(imbl_value::from_value(value)?)
}

/// The host as service SDKs of every version read it: `Host::into_service_view`, with
/// each binding's and range's `enabled` holding socket addresses instead of keys.
fn service_host_info(host: Option<Host>) -> Result<Value, Error> {
    let Some(host) = host.map(Host::into_service_view) else {
        return Ok(Value::Null);
    };
    let mut value = imbl_value::to_value(&host)?;
    let bindings = host
        .bindings
        .iter()
        .map(|(port, binding)| ("bindings", port, &binding.addresses));
    let ranges = host
        .binding_ranges
        .iter()
        .map(|(port, range)| ("bindingRanges", port, &range.addresses));
    for (table, port, addresses) in bindings.chain(ranges) {
        value[table][&*port.to_string()]["addresses"]["enabled"] =
            imbl_value::to_value(&legacy_enabled(addresses))?;
    }
    Ok(value)
}

/// The enabled public IPs in the form SDKs predating `PublicIpKey` match.
fn legacy_enabled(addresses: &DerivedAddressInfo) -> BTreeSet<SocketAddr> {
    addresses
        .enabled()
        .into_iter()
        .filter(|a| a.public && a.metadata.is_ip())
        .filter_map(|a| a.to_socket_addr())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::forward::AvailablePorts;
    use crate::net::host::binding::{BindInfo, BindOptions, PublicIpKey, RangeBindInfo};
    use crate::net::service_interface::{HostnameInfo, HostnameMetadata};

    #[test]
    fn initial_and_callback_host_views_keep_legacy_service_readers_in_sync() {
        let mut host = Host::default();
        let mut ports = AvailablePorts::new();
        let mut bind = BindInfo::new(
            &mut ports,
            BindOptions {
                transport: Default::default(),
                preferred_external_port: 8080,
                add_ssl: None,
                secure: None,
                proxy: None,
            },
            false,
        )
        .unwrap();
        let name = |gateway: &str| HostnameInfo {
            hostname: "server.local".into(),
            port: Some(8080),
            ssl: false,
            public: false,
            metadata: HostnameMetadata::Mdns {
                gateways: BTreeSet::from([gateway.parse().unwrap()]),
            },
        };
        let insecure_name = name("eth0");
        let trusted_name = name("wlan0");
        let lan = HostnameInfo {
            hostname: "192.0.2.10".into(),
            metadata: HostnameMetadata::Ipv4 {
                gateway: "eth0".parse().unwrap(),
            },
            ..insecure_name.clone()
        };
        bind.addresses.available =
            BTreeSet::from([insecure_name.clone(), trusted_name.clone(), lan.clone()]);
        bind.addresses.insecure = BTreeSet::from([
            (
                insecure_name.hostname.clone(),
                8080,
                "eth0".parse().unwrap(),
            ),
            (lan.hostname.clone(), 8080, "eth0".parse().unwrap()),
        ]);
        bind.addresses
            .disabled
            .insert((trusted_name.hostname.clone(), 8080));
        host.bindings.insert(80, bind);
        let snapshot = imbl_value::to_value(&host).unwrap();
        let mut legacy: Host = imbl_value::from_value(snapshot.clone()).unwrap();
        legacy
            .bindings
            .get_mut(&80)
            .unwrap()
            .addresses
            .disabled
            .clear();
        assert_eq!(
            legacy.into_service_view().bindings[&80].addresses.effective,
            BTreeSet::from([trusted_name.clone()])
        );
        let hidden = host.into_service_view();
        assert_eq!(
            hidden.bindings[&80].addresses.available,
            BTreeSet::from([trusted_name])
        );
        assert_eq!(
            project_host_info(snapshot).unwrap(),
            imbl_value::to_value(&hidden).unwrap()
        );

        let mut host = hidden;
        let addresses = &mut host.bindings.get_mut(&80).unwrap().addresses;
        addresses
            .available
            .extend([insecure_name.clone(), lan.clone()]);
        addresses.insecure_enabled = addresses.insecure.clone();
        addresses.lan_enabled.insert((lan.hostname.clone(), 8080));
        addresses.effective = addresses.enabled().into_iter().cloned().collect();
        let snapshot = imbl_value::to_value(&host).unwrap();
        let enabled = host.into_service_view();
        let addresses = &enabled.bindings[&80].addresses;
        assert_eq!(addresses.available, BTreeSet::from([insecure_name, lan]));
        assert!(addresses.disabled.is_empty());
        assert_eq!(
            project_host_info(snapshot).unwrap(),
            imbl_value::to_value(&enabled).unwrap()
        );
        assert_eq!(project_host_info(Value::Null).unwrap(), Value::Null);
    }

    #[test]
    fn service_readers_see_enabled_public_ips_as_socket_addresses() {
        let public = |port: u16, hostname: &str, metadata: HostnameMetadata| HostnameInfo {
            hostname: hostname.into(),
            port: Some(port),
            ssl: false,
            public: true,
            metadata,
        };
        let eth0 = || "eth0".parse().unwrap();
        let addresses = |port: u16| {
            let wan = public(
                port,
                "203.0.113.7",
                HostnameMetadata::Ipv4 { gateway: eth0() },
            );
            let gua = public(
                port,
                "2001:db8::7",
                HostnameMetadata::Ipv6 {
                    gateway: eth0(),
                    scope_id: 2,
                },
            );
            DerivedAddressInfo {
                enabled: BTreeSet::from([PublicIpKey::of(&wan).unwrap()]),
                available: BTreeSet::from([wan, gua]),
                ..Default::default()
            }
        };
        let mut host = Host::default();
        let mut ports = AvailablePorts::new();
        let mut bind = BindInfo::new(
            &mut ports,
            BindOptions {
                transport: Default::default(),
                preferred_external_port: 8080,
                add_ssl: None,
                secure: None,
                proxy: None,
            },
            false,
        )
        .unwrap();
        bind.addresses = addresses(8080);
        host.bindings.insert(80, bind);
        host.binding_ranges.insert(
            40000,
            RangeBindInfo {
                enabled: true,
                transport: Default::default(),
                external_start_port: 40000,
                number_of_ports: 10,
                addresses: addresses(40000),
                interface: None,
            },
        );

        let view = project_host_info(imbl_value::to_value(&host).unwrap()).unwrap();
        assert_eq!(
            view["bindings"]["80"]["addresses"]["enabled"],
            imbl_value::json!(["203.0.113.7:8080"])
        );
        assert_eq!(
            view["bindingRanges"]["40000"]["addresses"]["enabled"],
            imbl_value::json!(["203.0.113.7:40000"])
        );
    }
}
