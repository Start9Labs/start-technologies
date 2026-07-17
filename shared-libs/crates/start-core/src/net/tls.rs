use std::sync::Arc;
use std::task::{Poll, ready};
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt};
use imbl_value::InternedString;
use openssl::x509::{X509, X509Ref};
use tokio::io::AsyncWriteExt;
use tokio_rustls::LazyConfigAcceptor;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::crypto::{
    CryptoProvider, verify_tls12_signature, verify_tls13_signature,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::server::{Acceptor, ClientHello, ResolvesServerCert};
use tokio_rustls::rustls::sign::CertifiedKey;
use tokio_rustls::rustls::{
    ClientConfig, DigitallySignedStruct, RootCertStore, ServerConfig, SignatureScheme,
};
use visit_rs::{Visit, VisitFields};

/// Result of a TLS handler's decision about how to handle a connection.
pub enum TlsHandlerAction {
    /// Complete the TLS handshake with this ServerConfig.
    Tls(ServerConfig),
    /// Don't complete TLS — rewind the BackTrackingIO and return the raw stream.
    Passthrough,
}

use crate::net::http::handle_http_on_https;
use crate::net::web_server::{Accept, AcceptStream, MetadataVisitor};
use crate::prelude::*;
use crate::util::io::BackTrackingIO;
use crate::util::serde::MaybeUtf8String;
use crate::util::sync::SyncMutex;

#[derive(Debug, Clone, VisitFields)]
pub struct TlsMetadata<M> {
    pub inner: M,
    pub tls_info: TlsHandshakeInfo,
}
impl<V: MetadataVisitor<Result = ()>, M: Visit<V>> Visit<V> for TlsMetadata<M> {
    fn visit(&self, visitor: &mut V) -> <V as visit_rs::Visitor>::Result {
        self.visit_fields(visitor).collect()
    }
}

#[derive(Debug, Clone)]
pub struct TlsHandshakeInfo {
    pub sni: Option<InternedString>,
    pub alpn: Option<MaybeUtf8String>,
}
impl<V: MetadataVisitor> Visit<V> for TlsHandshakeInfo {
    fn visit(&self, visitor: &mut V) -> <V as visit_rs::Visitor>::Result {
        visitor.visit(self)
    }
}

pub trait TlsHandler<'a, A: Accept> {
    fn get_config(
        &'a mut self,
        hello: &'a ClientHello<'a>,
        metadata: &'a A::Metadata,
    ) -> impl Future<Output = Option<TlsHandlerAction>> + Send + 'a;
}

#[derive(Clone)]
pub struct ChainedHandler<H0, H1>(pub H0, pub H1);
impl<'a, A, H0, H1> TlsHandler<'a, A> for ChainedHandler<H0, H1>
where
    A: Accept + 'a,
    <A as Accept>::Metadata: Send + Sync,
    H0: TlsHandler<'a, A> + Send,
    H1: TlsHandler<'a, A> + Send,
{
    async fn get_config(
        &'a mut self,
        hello: &'a ClientHello<'a>,
        metadata: &'a <A as Accept>::Metadata,
    ) -> Option<TlsHandlerAction> {
        if let Some(config) = self.0.get_config(hello, metadata).await {
            return Some(config);
        }
        self.1.get_config(hello, metadata).await
    }
}

#[derive(Debug)]
pub struct SingleCertResolver(pub Arc<CertifiedKey>);
impl ResolvesServerCert for SingleCertResolver {
    fn resolve(&self, _: ClientHello) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }
}

pub struct TlsListener<A: Accept, H: for<'a> TlsHandler<'a, A>> {
    pub accept: A,
    pub tls_handler: H,
    in_progress: SyncMutex<
        FuturesUnordered<
            BoxFuture<
                'static,
                (
                    H,
                    Result<Option<(TlsMetadata<A::Metadata>, AcceptStream)>, Error>,
                ),
            >,
        >,
    >,
}
impl<A: Accept, H: for<'a> TlsHandler<'a, A>> TlsListener<A, H> {
    pub fn new(accept: A, cert_handler: H) -> Self {
        Self {
            accept,
            tls_handler: cert_handler,
            in_progress: SyncMutex::new(FuturesUnordered::new()),
        }
    }
}
impl<A, H> Accept for TlsListener<A, H>
where
    A: Accept + 'static,
    A::Metadata: Send + 'static,
    for<'a> H: TlsHandler<'a, A> + Clone + Send + 'static,
{
    type Metadata = TlsMetadata<A::Metadata>;
    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(Self::Metadata, AcceptStream), Error>> {
        self.in_progress.mutate(|in_progress| {
            // First, check if any in-progress handshakes have completed
            if !in_progress.is_empty() {
                if let Poll::Ready(Some((handler, res))) = in_progress.poll_next_unpin(cx) {
                    if let Some(res) = res.transpose() {
                        self.tls_handler = handler;
                        return Poll::Ready(res);
                    }
                    // Connection was rejected (preprocess returned None).
                    // Yield to the runtime to avoid busy-looping, but wake
                    // immediately to continue processing.
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
            }

            // Try to accept a new connection
            let (metadata, stream) = ready!(self.accept.poll_accept(cx)?);
            let mut tls_handler = self.tls_handler.clone();
            let mut fut = async move {
                let res = async {
                    // Phase 1: Accept ClientHello
                    let mut acceptor =
                        LazyConfigAcceptor::new(Acceptor::default(), BackTrackingIO::new(stream));
                    let mut mid: tokio_rustls::StartHandshake<BackTrackingIO<AcceptStream>> =
                        match tokio::time::timeout(Duration::from_secs(5), &mut acceptor).await {
                            Ok(Ok(a)) => a,
                            Ok(Err(e)) => {
                                let mut stream = acceptor.take_io().or_not_found("acceptor io")?;
                                let (_, buf) = stream.rewind();
                                if std::str::from_utf8(buf)
                                    .ok()
                                    .and_then(|buf| {
                                        buf.lines()
                                            .map(|l| l.trim())
                                            .filter(|l| !l.is_empty())
                                            .next()
                                    })
                                    .map_or(false, |buf| {
                                        regex::Regex::new("[A-Z]+ (.+) HTTP/1")
                                            .unwrap()
                                            .is_match(buf)
                                    })
                                {
                                    handle_http_on_https(stream).await.log_err();

                                    return Ok(None);
                                } else {
                                    return Err(e).with_kind(ErrorKind::Network);
                                }
                            }
                            Err(_) => {
                                crate::dev_log!(debug, "TLS ClientHello timed out");
                                return Ok(None);
                            }
                        };

                    // Phase 2: Resolve TLS config (scoped timeout so mid
                    // remains accessible for sending a TLS alert on timeout)
                    let hello = mid.client_hello();
                    let sni = hello.server_name().map(InternedString::intern);
                    let action = tls_handler.get_config(&hello, &metadata).await;
                    drop(hello);

                    match action {
                        Some(TlsHandlerAction::Tls(cfg)) => {
                            let buffered = mid.io.stop_buffering();
                            mid.io
                                .write_all(&buffered)
                                .await
                                .with_kind(ErrorKind::Network)?;
                            return Ok(
                                match tokio::time::timeout(
                                    Duration::from_secs(15),
                                    mid.into_stream(Arc::new(cfg)),
                                )
                                .await
                                .with_kind(ErrorKind::Timeout)
                                .and_then(|e| e.map_err(Error::from))
                                {
                                    Ok(stream) => {
                                        let s = stream.get_ref().1;
                                        Some((
                                            TlsMetadata {
                                                inner: metadata,
                                                tls_info: TlsHandshakeInfo {
                                                    sni: s
                                                        .server_name()
                                                        .map(InternedString::intern),
                                                    alpn: s
                                                        .alpn_protocol()
                                                        .map(|a| MaybeUtf8String(a.to_vec())),
                                                },
                                            },
                                            Box::pin(stream) as AcceptStream,
                                        ))
                                    }
                                    Err(e) => {
                                        crate::dev_log!(
                                            trace,
                                            "Error completing TLS handshake: {e}"
                                        );
                                        crate::dev_log!(trace, "{e:?}");
                                        None
                                    }
                                },
                            );
                        }
                        Some(TlsHandlerAction::Passthrough) => {
                            let (dummy, _drop) = tokio::io::duplex(1);
                            let mut bt = std::mem::replace(
                                &mut mid.io,
                                BackTrackingIO::new(Box::pin(dummy) as AcceptStream),
                            );
                            drop(mid);
                            bt.rewind();
                            return Ok(Some((
                                TlsMetadata {
                                    inner: metadata,
                                    tls_info: TlsHandshakeInfo { sni, alpn: None },
                                },
                                Box::pin(bt) as AcceptStream,
                            )));
                        }
                        None => {
                            crate::dev_log!(debug, "no certificate for SNI {:?}", sni);
                            let _ = mid
                                .io
                                .write_all(&[
                                    0x15, // ContentType: Alert
                                    0x03,
                                    0x03, // ProtocolVersion: TLS 1.2 (also used by TLS 1.3)
                                    0x00, 0x02, // Length: 2
                                    0x02, // AlertLevel: Fatal
                                    0x70, // AlertDescription: unrecognized_name (112)
                                ])
                                .await;
                            let _ = mid.io.flush().await;
                        }
                    }

                    Ok(None)
                }
                .await;
                (tls_handler, res)
            }
            .boxed();
            match fut.poll_unpin(cx) {
                Poll::Pending => {
                    in_progress.push(fut);
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Poll::Ready((handler, res)) => {
                    if let Some(res) = res.transpose() {
                        self.tls_handler = handler;
                        return Poll::Ready(res);
                    }
                    // Connection was rejected (preprocess returned None).
                    // Yield to the runtime to avoid busy-looping, but wake
                    // immediately to continue processing.
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
        })
    }
}

pub fn client_config<'a, I: IntoIterator<Item = &'a X509Ref>>(
    crypto_provider: Arc<CryptoProvider>,
    root_certs: I,
) -> Result<ClientConfig, Error> {
    let mut certs = RootCertStore::empty();
    for cert in root_certs {
        certs
            .add(CertificateDer::from_slice(&cert.to_der()?))
            .with_kind(ErrorKind::OpenSsl)?;
    }
    Ok(ClientConfig::builder_with_provider(crypto_provider.clone())
        .with_safe_default_protocol_versions()
        .with_kind(ErrorKind::OpenSsl)?
        .with_root_certificates(certs)
        .with_no_client_auth())
}

/// Like [`client_config`], but validates the upstream certificate against the
/// PEM-encoded certificate(s) in `pem` instead of the StartOS root CA.
pub fn client_config_with_cert(
    crypto_provider: Arc<CryptoProvider>,
    pem: &str,
) -> Result<ClientConfig, Error> {
    let mut certs = RootCertStore::empty();
    for cert in X509::stack_from_pem(pem.as_bytes()).with_kind(ErrorKind::OpenSsl)? {
        certs
            .add(CertificateDer::from_slice(&cert.to_der()?))
            .with_kind(ErrorKind::OpenSsl)?;
    }
    Ok(ClientConfig::builder_with_provider(crypto_provider.clone())
        .with_safe_default_protocol_versions()
        .with_kind(ErrorKind::OpenSsl)?
        .with_root_certificates(certs)
        .with_no_client_auth())
}

/// A client config that performs NO certificate validation. Only for the
/// OS→container rewrap leg over the trusted internal bridge, when the package
/// opts into `UpstreamCertValidation::Disable`.
pub fn client_config_no_verify(
    crypto_provider: Arc<CryptoProvider>,
) -> Result<ClientConfig, Error> {
    Ok(ClientConfig::builder_with_provider(crypto_provider.clone())
        .with_safe_default_protocol_versions()
        .with_kind(ErrorKind::OpenSsl)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertVerifier(crypto_provider)))
        .with_no_client_auth())
}

#[derive(Debug)]
struct NoCertVerifier(Arc<CryptoProvider>);
impl ServerCertVerifier for NoCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, tokio_rustls::rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, tokio_rustls::rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, tokio_rustls::rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeSet;
    use std::net::Ipv4Addr;

    use openssl::pkey::{PKey, Private};
    use openssl::x509::X509;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio_rustls::rustls::ServerConfig;
    use tokio_rustls::rustls::pki_types::{PrivatePkcs8KeyDer, ServerName};
    use tokio_rustls::{TlsAcceptor, TlsConnector};

    use super::*;
    use crate::net::ssl::{CertBranding, SANInfo, gen_nistp256, make_self_signed};

    fn provider() -> Arc<CryptoProvider> {
        Arc::new(tokio_rustls::rustls::crypto::ring::default_provider())
    }

    fn self_signed_for_loopback() -> (PKey<Private>, X509) {
        let key = gen_nistp256().unwrap();
        let san = SANInfo::new(&BTreeSet::from([InternedString::intern("127.0.0.1")]));
        let cert = make_self_signed((&key, &san), &CertBranding::start_os("test")).unwrap();
        (key, cert)
    }

    fn server_config(key: &PKey<Private>, cert: &X509) -> ServerConfig {
        let cert_der = CertificateDer::from(cert.to_der().unwrap());
        let key_der = PrivatePkcs8KeyDer::from(key.private_key_to_pkcs8().unwrap());
        ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der.into())
            .unwrap()
    }

    /// Stand up a one-shot TLS server presenting `(key, cert)`, then attempt a
    /// single client handshake to it with `client`. Returns whether the client
    /// completed the handshake (i.e. accepted the server cert).
    async fn handshake_succeeds(key: &PKey<Private>, cert: &X509, client: ClientConfig) -> bool {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(server_config(key, cert)));

        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            if let Ok(mut tls) = acceptor.accept(tcp).await {
                let mut buf = [0u8; 4];
                let _ = tls.read(&mut buf).await;
                let _ = tls.write_all(b"ok").await;
                let _ = tls.shutdown().await;
            }
        });

        let connector = TlsConnector::from(Arc::new(client));
        let tcp = TcpStream::connect(addr).await.unwrap();
        let name = ServerName::IpAddress(Ipv4Addr::LOCALHOST.into());
        let result = connector.connect(name, tcp).await;
        let ok = if let Ok(mut tls) = result {
            tls.write_all(b"ping").await.is_ok()
        } else {
            false
        };
        server.abort();
        ok
    }

    #[tokio::test]
    async fn disable_accepts_self_signed() {
        let (key, cert) = self_signed_for_loopback();
        let client = client_config_no_verify(provider()).unwrap();
        assert!(handshake_succeeds(&key, &cert, client).await);
    }

    #[tokio::test]
    async fn pinned_cert_accepts_matching_and_rejects_other() {
        let (key, cert) = self_signed_for_loopback();
        let pem = String::from_utf8(cert.to_pem().unwrap()).unwrap();

        let matching = client_config_with_cert(provider(), &pem).unwrap();
        assert!(
            handshake_succeeds(&key, &cert, matching).await,
            "pinned cert should accept the matching server cert"
        );

        let (other_key, other_cert) = self_signed_for_loopback();
        let other_pem = String::from_utf8(other_cert.to_pem().unwrap()).unwrap();
        let mismatched = client_config_with_cert(provider(), &other_pem).unwrap();
        assert!(
            !handshake_succeeds(&key, &cert, mismatched).await,
            "pinning an unrelated cert must reject the server cert"
        );
        // sanity: the other cert is itself valid against its own pin
        let other_match = client_config_with_cert(provider(), &other_pem).unwrap();
        assert!(handshake_succeeds(&other_key, &other_cert, other_match).await);
    }

    #[tokio::test]
    async fn empty_root_store_rejects_self_signed() {
        let (key, cert) = self_signed_for_loopback();
        let client = client_config(provider(), std::iter::empty()).unwrap();
        assert!(
            !handshake_succeeds(&key, &cert, client).await,
            "default root-CA validation must reject an untrusted self-signed cert"
        );
    }
}
