use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::LazyLock;

use http::Extensions;

use crate::GatewayId;
use crate::net::gateway::GatewayInfo;
use crate::net::utils::{is_internal_ip, is_private_client};
use crate::net::web_server::TcpMetadata;
use crate::util::GeneralBoxedGuard;
use crate::util::sync::SyncMutex;

#[derive(Clone, Debug, Default)]
pub(crate) struct RequestOrigin {
    pub peer_ip: Option<IpAddr>,
    pub local_ip: Option<IpAddr>,
    pub gateway: Option<GatewayId>,
    pub public: bool,
}

static PROXY_ORIGINS: LazyLock<SyncMutex<BTreeMap<(SocketAddr, SocketAddr), RequestOrigin>>> =
    LazyLock::new(|| SyncMutex::new(BTreeMap::new()));

fn proxy_connection_key(
    mut local: SocketAddr,
    mut backend: SocketAddr,
) -> (SocketAddr, SocketAddr) {
    local.set_ip(local.ip().to_canonical());
    backend.set_ip(backend.ip().to_canonical());
    (local, backend)
}

impl RequestOrigin {
    pub fn from_connection(tcp: Option<&TcpMetadata>, gateway: Option<&GatewayInfo>) -> Self {
        let peer_ip = tcp.map(|tcp| tcp.peer_addr.ip().to_canonical());
        let private = peer_ip.is_some_and(|ip| {
            is_private_client(
                ip,
                gateway
                    .and_then(|g| g.info.ip_info.as_ref())
                    .map(|i| &i.subnets),
            )
        });
        Self {
            peer_ip,
            local_ip: tcp.map(|tcp| tcp.local_addr.ip().to_canonical()),
            gateway: gateway
                .filter(|g| !g.id.as_str().is_empty())
                .map(|g| g.id.clone()),
            public: !private,
        }
    }

    pub fn is_internal(&self) -> bool {
        !self.public && self.local_ip.is_some_and(is_internal_ip)
    }

    pub fn from_extensions(extensions: &Extensions) -> Self {
        if let Some(origin) = extensions.get::<Self>() {
            return origin.clone();
        }
        let tcp = extensions.get::<TcpMetadata>();
        tcp.and_then(|tcp| {
            PROXY_ORIGINS.peek(|origins| {
                origins
                    .get(&proxy_connection_key(tcp.peer_addr, tcp.local_addr))
                    .cloned()
            })
        })
        .unwrap_or_else(|| Self::from_connection(tcp, extensions.get::<GatewayInfo>()))
    }

    /// Registers before relaying bytes; the backend looks up on HTTP requests, not accept.
    pub fn register_proxy(self, local: SocketAddr, backend: SocketAddr) -> GeneralBoxedGuard {
        let key = proxy_connection_key(local, backend);
        PROXY_ORIGINS.mutate(|origins| origins.insert(key, self));
        GeneralBoxedGuard::new(move || {
            PROXY_ORIGINS.mutate(|origins| origins.remove(&key));
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_clients_include_on_link_global_addresses_and_mapped_ipv4() {
        let gateway = GatewayInfo {
            id: "eth0".parse().unwrap(),
            info: crate::db::model::public::NetworkInterfaceInfo {
                ip_info: Some(std::sync::Arc::new(crate::db::model::public::IpInfo {
                    subnets: ["2001:db8:1::1/64".parse::<ipnet::IpNet>().unwrap()]
                        .into_iter()
                        .collect(),
                    ..Default::default()
                })),
                ..Default::default()
            },
        };
        for (peer, public) in [
            ("203.0.113.7:1234", true),
            ("192.168.1.7:1234", false),
            ("[::ffff:192.168.1.7]:1234", false),
            ("[2001:db8:1::7]:1234", false),
            ("[2001:db8:2::7]:1234", true),
        ] {
            let tcp = TcpMetadata {
                peer_addr: peer.parse().unwrap(),
                local_addr: "[2001:db8:1::1]:80".parse().unwrap(),
            };
            let origin = RequestOrigin::from_connection(Some(&tcp), Some(&gateway));
            assert_eq!(origin.public, public, "{peer}");
            assert_eq!(origin.gateway, Some("eth0".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn proxy_origin_lives_until_the_relay_is_dropped() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (backend, _) = listener.accept().await.unwrap();
        let mut extensions = Extensions::new();
        extensions.insert(TcpMetadata {
            peer_addr: backend.peer_addr().unwrap(),
            local_addr: backend.local_addr().unwrap(),
        });
        let origin = RequestOrigin {
            peer_ip: Some("203.0.113.7".parse().unwrap()),
            local_ip: Some("192.168.1.2".parse().unwrap()),
            gateway: Some("eth0".parse().unwrap()),
            public: true,
        };
        let registration =
            origin.register_proxy(proxy.local_addr().unwrap(), proxy.peer_addr().unwrap());
        let restored = RequestOrigin::from_extensions(&extensions);
        assert_eq!(restored.peer_ip, Some("203.0.113.7".parse().unwrap()));
        assert_eq!(restored.gateway, Some("eth0".parse().unwrap()));
        assert_eq!(restored.local_ip, Some("192.168.1.2".parse().unwrap()));
        assert!(!restored.is_internal());
        assert!(restored.public);
        extensions.insert(TcpMetadata {
            peer_addr: format!("[::ffff:127.0.0.1]:{}", backend.peer_addr().unwrap().port())
                .parse()
                .unwrap(),
            local_addr: format!(
                "[::ffff:127.0.0.1]:{}",
                backend.local_addr().unwrap().port()
            )
            .parse()
            .unwrap(),
        });
        let mapped = RequestOrigin::from_extensions(&extensions);
        assert_eq!(mapped.peer_ip, Some("203.0.113.7".parse().unwrap()));
        assert_eq!(mapped.gateway, Some("eth0".parse().unwrap()));
        assert!(mapped.public);
        assert_eq!(mapped.local_ip, Some("192.168.1.2".parse().unwrap()));
        let mut captured = extensions.clone();
        captured.insert(restored);
        drop(registration);
        let retained = RequestOrigin::from_extensions(&captured);
        assert_eq!(retained.peer_ip, Some("203.0.113.7".parse().unwrap()));
        assert_eq!(retained.local_ip, Some("192.168.1.2".parse().unwrap()));
        assert!(retained.public);
        let direct = RequestOrigin::from_extensions(&extensions);
        assert_eq!(direct.peer_ip, Some("127.0.0.1".parse().unwrap()));
        assert_eq!(direct.gateway, None);
        assert!(!direct.public);
    }
}
