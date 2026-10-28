use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use std::task::{Context, Poll};

use tokio::net::TcpListener;

use super::ipv4_listener_addr;
use crate::net::web_server::{Accept, AcceptStream, TcpMetadata};
use crate::prelude::*;
use crate::tunnel::forward::sni::{FallbackSource, SniDemux};

pub(crate) struct UiListener {
    listeners: Vec<TcpListener>,
    fallback: Option<(Arc<SniDemux>, SocketAddrV4, SocketAddrV4)>,
}

impl UiListener {
    pub(crate) fn bind(addr: SocketAddr, sni: Arc<SniDemux>) -> Result<Self, Error> {
        let mut listeners = Vec::new();
        if let Some(addr) = direct_ipv6_addr(addr) {
            listeners.push(crate::net::utils::bind_tokio_listener_v6_only(addr)?);
        }
        let fallback = if let Some(external) = ipv4_listener_addr(addr) {
            let listener = crate::net::utils::bind_tokio_listener(
                SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0).into(),
            )?;
            let SocketAddr::V4(target) = listener.local_addr()? else {
                unreachable!()
            };
            sni.register_local_fallback(
                *external.ip(),
                external.port(),
                target,
                FallbackSource::Any,
            )
            .map_err(|code| {
                Error::new(
                    eyre!(t!(
                        "tunnel.web.ui-fallback-failed",
                        address = external,
                        code = code
                    )),
                    ErrorKind::Network,
                )
            })?;
            listeners.push(listener);
            Some((sni, external, target))
        } else {
            None
        };
        Ok(Self {
            listeners,
            fallback,
        })
    }

    pub(crate) fn check_available(addr: SocketAddr) -> Result<(), Error> {
        if let Some(addr) = direct_ipv6_addr(addr) {
            crate::net::utils::bind_tokio_listener_v6_only(addr)?;
        }
        if let Some(external) = ipv4_listener_addr(addr) {
            crate::net::utils::bind_tokio_listener_reuse_port(external.into())?;
        }
        Ok(())
    }
}

fn direct_ipv6_addr(addr: SocketAddr) -> Option<SocketAddrV6> {
    match addr {
        SocketAddr::V6(addr) if addr.ip().to_ipv4_mapped().is_none() => Some(addr),
        _ => None,
    }
}

impl Accept for UiListener {
    type Metadata = TcpMetadata;

    fn poll_accept(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::Metadata, AcceptStream), Error>> {
        self.listeners.poll_accept(cx)
    }
}

impl Drop for UiListener {
    fn drop(&mut self) {
        if let Some((sni, external, target)) = &self.fallback {
            sni.unregister_fallback(*external.ip(), external.port(), *target);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::time::timeout;

    use super::*;

    #[tokio::test]
    async fn wildcard_ui_address_is_available_beside_sni_listener() {
        let sni =
            crate::net::utils::bind_tokio_listener_reuse_port((Ipv4Addr::LOCALHOST, 0).into())
                .unwrap();
        let port = sni.local_addr().unwrap().port();
        assert!(
            tokio::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))
                .await
                .is_err()
        );
        UiListener::check_available((Ipv4Addr::UNSPECIFIED, port).into()).unwrap();
        UiListener::check_available((Ipv6Addr::UNSPECIFIED, port).into()).unwrap();
    }

    #[tokio::test]
    async fn ui_ipv6_and_dual_stack_listeners_remain_reachable() {
        for ip in [Ipv6Addr::LOCALHOST, Ipv6Addr::UNSPECIFIED] {
            let probe = std::net::TcpListener::bind((ip, 0)).unwrap();
            let port = probe.local_addr().unwrap().port();
            drop(probe);
            let demux = SniDemux::new();
            let mut ui = UiListener::bind((ip, port).into(), demux).unwrap();
            let mut destinations = vec![SocketAddr::from((Ipv6Addr::LOCALHOST, port))];
            if ip.is_unspecified() {
                destinations.push((Ipv4Addr::LOCALHOST, port).into());
            }
            for destination in destinations {
                let mut client = TcpStream::connect(destination).await.unwrap();
                let payload = b"\x16\x03\x01\x00\x00";
                client.write_all(payload).await.unwrap();
                let (_, mut stream) = timeout(
                    Duration::from_secs(2),
                    futures::future::poll_fn(|cx| ui.poll_accept(cx)),
                )
                .await
                .unwrap()
                .unwrap();
                let mut received = [0; 5];
                timeout(Duration::from_secs(2), stream.read_exact(&mut received))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(&received, payload);
            }
        }
    }
}
