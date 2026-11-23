//! Name lookups sent only through one tunnel interface.

use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::time::Duration;

use hickory_server::net::runtime::{RuntimeProvider, TokioRuntimeProvider};
use hickory_server::resolver::config::{
    ConnectionConfig, LookupIpStrategy, NameServerConfig, ResolveHosts, ResolverConfig,
    ResolverOpts,
};
use hickory_server::resolver::Resolver;

use crate::prelude::*;

/// Opens every socket bound to one network device.
#[derive(Clone)]
struct DeviceRuntime {
    runtime: TokioRuntimeProvider,
    device: String,
}

impl DeviceRuntime {
    fn socket(&self, addr: SocketAddr, kind: socket2::Type) -> io::Result<socket2::Socket> {
        let socket = socket2::Socket::new(socket2::Domain::for_address(addr), kind, None)?;
        socket.bind_device(Some(self.device.as_bytes()))?;
        socket.set_nonblocking(true)?;
        Ok(socket)
    }
}

impl RuntimeProvider for DeviceRuntime {
    type Handle = <TokioRuntimeProvider as RuntimeProvider>::Handle;
    type Timer = <TokioRuntimeProvider as RuntimeProvider>::Timer;
    type Udp = <TokioRuntimeProvider as RuntimeProvider>::Udp;
    type Tcp = <TokioRuntimeProvider as RuntimeProvider>::Tcp;

    fn create_handle(&self) -> Self::Handle {
        self.runtime.create_handle()
    }

    fn connect_tcp(
        &self,
        server_addr: SocketAddr,
        bind_addr: Option<SocketAddr>,
        timeout: Option<Duration>,
    ) -> Pin<Box<dyn Send + Future<Output = io::Result<Self::Tcp>>>> {
        let runtime = self.clone();
        Box::pin(async move {
            use std::os::fd::{FromRawFd, IntoRawFd};

            use hickory_server::net::runtime::iocompat::AsyncIoTokioAsStd;

            let socket = runtime.socket(server_addr, socket2::Type::STREAM)?;
            if let Some(addr) = bind_addr {
                socket.bind(&addr.into())?;
            }
            socket.set_tcp_nodelay(true)?;
            let socket = unsafe { tokio::net::TcpSocket::from_raw_fd(socket.into_raw_fd()) };
            tokio::time::timeout(
                timeout.unwrap_or(Duration::from_secs(5)),
                socket.connect(server_addr),
            )
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "DNS TCP connect timed out"))?
            .map(AsyncIoTokioAsStd)
        })
    }

    fn bind_udp(
        &self,
        local_addr: SocketAddr,
        _: SocketAddr,
    ) -> Pin<Box<dyn Send + Future<Output = io::Result<Self::Udp>>>> {
        let runtime = self.clone();
        Box::pin(async move {
            let socket = runtime.socket(local_addr, socket2::Type::DGRAM)?;
            socket.bind(&local_addr.into())?;
            tokio::net::UdpSocket::from_std(socket.into())
        })
    }
}

/// The addresses of `host`, asked of `servers` through `device` alone.
pub async fn lookup(host: &str, device: &str, servers: &[IpAddr]) -> Result<Vec<IpAddr>, Error> {
    let mut config = ResolverConfig::from_parts(None, Vec::new(), Vec::new());
    for ip in servers {
        config.add_name_server(NameServerConfig::new(
            *ip,
            true,
            vec![ConnectionConfig::udp(), ConnectionConfig::tcp()],
        ));
    }
    let mut opts = ResolverOpts::default();
    opts.timeout = Duration::from_secs(5);
    opts.attempts = 2;
    opts.cache_size = 0;
    opts.ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
    opts.use_hosts_file = ResolveHosts::Never;

    let resolver = Resolver::builder_with_config(
        config,
        DeviceRuntime {
            runtime: TokioRuntimeProvider::default(),
            device: device.to_string(),
        },
    )
    .with_options(opts)
    .build()
    .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::Network))?;

    let found = resolver
        .lookup_ip(format!("{}.", host.trim_end_matches('.')))
        .await
        .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::Network))?;
    Ok(found.iter().collect())
}
