use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use hickory_net::runtime::iocompat::AsyncIoTokioAsStd;
use hickory_net::runtime::{RuntimeProvider, TokioHandle, TokioRuntimeProvider, TokioTime};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::{TcpSocket, TcpStream, UdpSocket};

/// Opens every upstream socket bound to one network device, or unbound without one.
#[derive(Clone, Default)]
pub struct BoundProvider {
    runtime: TokioRuntimeProvider,
    device: Option<Arc<str>>,
}

impl BoundProvider {
    pub fn new(device: Option<&str>) -> Self {
        Self {
            runtime: TokioRuntimeProvider::new(),
            device: device.map(Arc::from),
        }
    }
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

impl RuntimeProvider for BoundProvider {
    type Handle = TokioHandle;
    type Timer = TokioTime;
    type Udp = UdpSocket;
    type Tcp = AsyncIoTokioAsStd<TcpStream>;

    fn create_handle(&self) -> Self::Handle {
        self.runtime.create_handle()
    }

    fn connect_tcp(
        &self,
        server_addr: SocketAddr,
        bind_addr: Option<SocketAddr>,
        timeout: Option<Duration>,
    ) -> Pin<Box<dyn Send + Future<Output = io::Result<Self::Tcp>>>> {
        let device = self.device.clone();
        Box::pin(async move {
            let socket = if server_addr.is_ipv4() {
                TcpSocket::new_v4()?
            } else {
                TcpSocket::new_v6()?
            };
            if let Some(device) = &device {
                socket.bind_device(Some(device.as_bytes()))?;
            }
            if let Some(bind_addr) = bind_addr {
                socket.bind(bind_addr)?;
            }
            socket.set_nodelay(true)?;
            match tokio::time::timeout(
                timeout.unwrap_or(CONNECT_TIMEOUT),
                socket.connect(server_addr),
            )
            .await
            {
                Ok(stream) => Ok(AsyncIoTokioAsStd(stream?)),
                Err(_) => Err(io::ErrorKind::TimedOut.into()),
            }
        })
    }

    fn bind_udp(
        &self,
        local_addr: SocketAddr,
        _server_addr: SocketAddr,
    ) -> Pin<Box<dyn Send + Future<Output = io::Result<Self::Udp>>>> {
        let device = self.device.clone();
        Box::pin(async move {
            let socket = Socket::new(
                Domain::for_address(local_addr),
                Type::DGRAM,
                Some(Protocol::UDP),
            )?;
            if let Some(device) = &device {
                socket.bind_device(Some(device.as_bytes()))?;
            }
            socket.set_nonblocking(true)?;
            socket.bind(&local_addr.into())?;
            UdpSocket::from_std(socket.into())
        })
    }
}
