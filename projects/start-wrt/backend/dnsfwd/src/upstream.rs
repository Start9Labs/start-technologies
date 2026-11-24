use std::sync::Arc;

use hickory_net::h2::HttpsClientStream;
use hickory_net::proto::op::{DnsRequest, DnsRequestOptions, Message};
use hickory_net::tcp::TcpClientStream;
use hickory_net::tls::{client_config, tls_exchange};
use hickory_net::udp::UdpClientStream;
use hickory_net::xfer::{DnsExchange, FirstAnswer};
use hickory_net::{DnsHandle, NetError};
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use tokio::sync::Mutex;

use crate::bind::BoundProvider;
use crate::config::{Server, Transport};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// One configured server, with the connection that it reuses between queries.
pub struct Upstream {
    pub server: Server,
    provider: BoundProvider,
    tls: Arc<ClientConfig>,
    udp: Option<DnsExchange<BoundProvider>>,
    stream: Mutex<Option<DnsExchange<BoundProvider>>>,
}

impl Upstream {
    pub fn new(server: Server, provider: BoundProvider, tls: Arc<ClientConfig>) -> Self {
        let udp = (server.transport == Transport::Plain).then(|| {
            UdpClientStream::builder(server.socket_addr(), provider.clone())
                .with_timeout(Some(server.timeout()))
                .with_max_retries(0)
                .exchange()
        });
        Self {
            server,
            provider,
            tls,
            udp,
            stream: Mutex::new(None),
        }
    }

    /// Sends `query` and returns the answer's wire bytes, carrying the query's ID.
    pub async fn exchange(&self, query: &Message) -> Result<Vec<u8>, Error> {
        let mut answer = match &self.udp {
            Some(udp) => {
                let answer = send(udp, query).await?;
                if answer.get(2).is_some_and(|flags| flags & 0x02 != 0) {
                    self.send_stream(query).await?
                } else {
                    answer
                }
            }
            None => self.send_stream(query).await?,
        };
        let id = query.metadata.id.to_be_bytes();
        match answer.get_mut(..2) {
            Some(header) => header.copy_from_slice(&id),
            None => return Err("answer shorter than an ID".into()),
        }
        Ok(answer)
    }

    async fn send_stream(&self, query: &Message) -> Result<Vec<u8>, Error> {
        let reused = self.stream.lock().await.clone();
        if let Some(exchange) = reused {
            match send(&exchange, query).await {
                Ok(answer) => return Ok(answer),
                // The server may have closed the idle connection.
                Err(_) => {
                    self.stream.lock().await.take();
                }
            }
        }
        let exchange = self.connect().await?;
        let result = send(&exchange, query).await;
        if result.is_ok() {
            *self.stream.lock().await = Some(exchange);
        }
        result
    }

    async fn connect(&self) -> Result<DnsExchange<BoundProvider>, Error> {
        let addr = self.server.socket_addr();
        let timeout = self.server.timeout();
        let name = self.server.name.clone().unwrap_or_default();
        let provider = self.provider.clone();
        Ok(match self.server.transport {
            Transport::Plain => {
                TcpClientStream::exchange(addr, None, timeout, None, provider).await?
            }
            Transport::Tls => {
                let name = ServerName::try_from(name)?;
                tls_exchange(addr, name, (*self.tls).clone(), timeout, None, provider).await?
            }
            Transport::Https => {
                let mut builder = HttpsClientStream::builder(self.tls.clone(), provider);
                builder.request_timeout(timeout);
                builder
                    .exchange(addr, name.into(), self.server.path().into())
                    .await?
            }
        })
    }
}

pub fn tls_config() -> Result<Arc<ClientConfig>, rustls::Error> {
    Ok(Arc::new(client_config()?))
}

async fn send(exchange: &DnsExchange<BoundProvider>, query: &Message) -> Result<Vec<u8>, Error> {
    let request = DnsRequest::new(query.clone(), DnsRequestOptions::default());
    let answer = exchange
        .send(request)
        .first_answer()
        .await
        .map_err(|e: NetError| Box::new(e) as Error)?;
    Ok(answer.into_buffer())
}
