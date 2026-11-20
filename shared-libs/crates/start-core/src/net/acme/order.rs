use std::collections::{BTreeSet, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use hickory_server::resolver::Resolver;
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::dns::Name;
use hyper_util::client::legacy::connect::{Connect, HttpConnector};
use hyper_util::rt::TokioExecutor;
use instant_acme::{
    Account, AuthorizationStatus, BodyWrapper, BytesResponse, ChallengeStatus, ChallengeType,
    CryptoProvider as AcmeCryptoProvider, Error as AcmeError, HttpClient, Identifier, Key,
    NewOrder, Order, OrderStatus, RetryPolicy,
};
use openssl::pkey::PKey;
use openssl::x509::X509;
use rcgen::{CertificateParams, CustomExtension, DistinguishedName, KeyPair};
use tokio_rustls::rustls::crypto::CryptoProvider;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::sign::CertifiedKey;

use super::{AcmeCert, AcmeCertCache, AcmeCertStore, certified_key};
use crate::GatewayId;
use crate::db::DbAccessMut;
use crate::error::ErrorData;
use crate::net::dns::gateway_resolver;
use crate::net::dns::upstream::GatewayRuntime;
use crate::net::gateway::gateway_connector;
use crate::prelude::*;
use crate::util::serde::{Der64, Pem};

type AcmeConnector<R> = hyper_tls::HttpsConnector<HttpConnector<R>>;

pub(super) struct AcmeHttpClient<R = GatewayDns>(Client<AcmeConnector<R>, Full<Bytes>>);

impl AcmeHttpClient {
    /// Every lookup and request leaves through the gateway, or fails.
    pub(super) fn via_gateway(
        gateway: &GatewayId,
        index: u32,
        dns_servers: &VecDeque<SocketAddr>,
    ) -> Result<Self, AcmeError> {
        let resolver = gateway_resolver(gateway.clone(), index, dns_servers)
            .map_err(|e| other_error(ErrorData::from(e)))?;
        Self::new(
            gateway_connector(gateway.as_str(), GatewayDns(Arc::new(resolver)))
                .map_err(|e| other_error(ErrorData::from(e)))?,
            native_tls::TlsConnector::new().map_err(other_error)?,
        )
    }
}

impl<R> AcmeHttpClient<R>
where
    AcmeConnector<R>: Connect + Clone + Send + Sync + 'static,
{
    fn new(connector: HttpConnector<R>, tls: native_tls::TlsConnector) -> Result<Self, AcmeError> {
        Ok(Self(Client::builder(TokioExecutor::new()).build(
            hyper_tls::HttpsConnector::from((connector, tls.into())),
        )))
    }
}

/// Resolves through one gateway's DNS servers, over that gateway alone.
#[derive(Clone)]
pub(super) struct GatewayDns(Arc<Resolver<GatewayRuntime>>);

impl tower_service::Service<Name> for GatewayDns {
    type Response = std::vec::IntoIter<SocketAddr>;
    type Error = std::io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, name: Name) -> Self::Future {
        let resolver = self.0.clone();
        Box::pin(async move {
            let lookup = resolver
                .lookup_ip(name.as_str())
                .await
                .map_err(std::io::Error::other)?;
            Ok(lookup
                .iter()
                .map(|ip| SocketAddr::new(ip, 0))
                .collect::<Vec<_>>()
                .into_iter())
        })
    }
}

impl<R> HttpClient for AcmeHttpClient<R>
where
    AcmeConnector<R>: Connect + Clone + Send + Sync + 'static,
{
    fn request(
        &self,
        request: http::Request<BodyWrapper<Bytes>>,
    ) -> Pin<Box<dyn Future<Output = Result<BytesResponse, AcmeError>> + Send>> {
        let client = self.0.clone();
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let body = body
                .collect()
                .await
                .expect("infallible ACME request body")
                .to_bytes();
            let (parts, body) = client
                .request(http::Request::from_parts(parts, Full::new(body)))
                .await
                .map_err(other_error)?
                .into_parts();
            let body = body.collect().await.map_err(other_error)?.to_bytes();
            Ok(http::Response::from_parts(parts, Full::new(body)).into())
        })
    }
}

pub(super) async fn order<M, F>(
    set_auth_key: F,
    directory_url: &str,
    sans: &BTreeSet<InternedString>,
    cache: &AcmeCertCache<'_, M>,
    contact: &[String],
    crypto_provider: &CryptoProvider,
    http: Box<dyn HttpClient>,
) -> Result<CertifiedKey, AcmeError>
where
    M: HasModel<Model = Model<M>> + DbAccessMut<AcmeCertStore> + Send + Sync,
    F: Fn(&Identifier, CertifiedKey) -> Result<(), AcmeError>,
{
    let account = load_account(cache, directory_url, contact, http).await?;
    let identifiers = sans
        .iter()
        .map(|name| match name.parse::<IpAddr>() {
            Ok(ip) => Identifier::Ip(ip),
            Err(_) => Identifier::Dns(name.to_string()),
        })
        .collect::<Vec<_>>();
    let mut order = account.new_order(&NewOrder::new(&identifiers)).await?;
    authorize(&mut order, set_auth_key, crypto_provider).await?;

    let key = KeyPair::generate().map_err(other_error)?;
    let params = certificate_params(sans.iter().map(ToString::to_string).collect())?;
    let csr = params.serialize_request(&key).map_err(other_error)?;
    order.finalize_csr(csr.der()).await?;
    let chain = order.poll_certificate(&retry_policy()).await?;
    let cert = AcmeCert {
        key: Der64(PKey::private_key_from_pkcs8(&key.serialize_der()).map_err(other_error)?),
        fullchain: X509::stack_from_pem(chain.as_bytes())
            .map_err(other_error)?
            .into_iter()
            .map(Pem)
            .collect(),
    };
    let certified = certified_key(&cert, crypto_provider).map_err(other_error)?;
    cache
        .write_certificate(sans, directory_url, &cert)
        .await
        .map_err(other_error)?;
    Ok(certified)
}

async fn load_account<M>(
    cache: &AcmeCertCache<'_, M>,
    directory_url: &str,
    contact: &[String],
    http: Box<dyn HttpClient>,
) -> Result<Account, AcmeError>
where
    M: HasModel<Model = Model<M>> + DbAccessMut<AcmeCertStore> + Send + Sync,
{
    let provider = AcmeCryptoProvider::ring();
    let contacts = contact.iter().map(String::as_str).collect::<Vec<_>>();
    let (key, der) = match cache.read_account(&contacts).await.map_err(other_error)? {
        Some(der) => {
            let der = PrivatePkcs8KeyDer::from(der);
            (Key::from_pkcs8_der(der.clone_key(), provider)?, der)
        }
        None => {
            let (key, der) = Key::generate(provider)?;
            cache
                .write_account(&contacts, der.secret_pkcs8_der())
                .await
                .map_err(other_error)?;
            (key, der)
        }
    };
    let (account, _) = Account::builder(http, provider)?
        .create_from_key((key, PrivateKeyDer::Pkcs8(der)), directory_url.to_owned())
        .await?;
    account.update_contacts(&contacts).await?;
    Ok(account)
}

async fn authorize<F>(
    order: &mut Order,
    set_auth_key: F,
    crypto_provider: &CryptoProvider,
) -> Result<(), AcmeError>
where
    F: Fn(&Identifier, CertifiedKey) -> Result<(), AcmeError>,
{
    let mut authorizations = order.authorizations();
    while let Some(authorization) = authorizations.next().await {
        let mut authorization = authorization?;
        if authorization.status == AuthorizationStatus::Valid {
            continue;
        }
        let mut challenge = authorization
            .challenge(ChallengeType::TlsAlpn01)
            .ok_or(AcmeError::Unsupported("tls-alpn-01"))?;
        let cert = challenge_cert(
            challenge.identifier().identifier,
            &challenge.key_authorization()?.digest(),
            crypto_provider,
        )?;
        set_auth_key(challenge.identifier().identifier, cert)?;
        if challenge.status == ChallengeStatus::Pending {
            match challenge.set_ready().await {
                Ok(()) => {}
                Err(AcmeError::Api(problem)) if problem.status == Some(409) => {}
                Err(error) => return Err(error),
            }
        }
    }
    if order.poll_ready(&retry_policy()).await? != OrderStatus::Ready {
        let mut authorizations = order.authorizations();
        while let Some(authorization) = authorizations.next().await {
            let mut authorization = authorization?;
            let state = authorization.refresh().await?;
            if let Some(problem) = state.challenges.iter().find_map(|c| c.error.clone()) {
                return Err(AcmeError::Api(problem));
            }
        }
        return Err(other_error(std::io::Error::other(
            t!("acme.order-invalid").to_string(),
        )));
    }
    Ok(())
}

fn retry_policy() -> RetryPolicy {
    RetryPolicy::new()
        .initial_delay(Duration::from_secs(1))
        .timeout(Duration::from_secs(60))
}

fn challenge_cert(
    identifier: &Identifier,
    digest: &[u8],
    crypto_provider: &CryptoProvider,
) -> Result<CertifiedKey, AcmeError> {
    let key = KeyPair::generate().map_err(other_error)?;
    let mut params = certificate_params(vec![identifier.authorized(false).to_string()])?;
    params.custom_extensions = vec![CustomExtension::new_acme_identifier(digest)];
    let cert = params.self_signed(&key).map_err(other_error)?;
    let signing_key = crypto_provider
        .key_provider
        .load_private_key(PrivatePkcs8KeyDer::from(key.serialize_der()).into())
        .map_err(other_error)?;
    // Ordinary certificate parsers reject the critical ACME extension.
    Ok(CertifiedKey::new(
        vec![CertificateDer::from(cert.der().to_vec())],
        signing_key,
    ))
}

fn certificate_params(sans: Vec<String>) -> Result<CertificateParams, AcmeError> {
    let mut params = CertificateParams::new(sans).map_err(other_error)?;
    params.distinguished_name = DistinguishedName::new();
    Ok(params)
}

fn other_error(error: impl std::error::Error + Send + Sync + 'static) -> AcmeError {
    AcmeError::Other(Box::new(error))
}

#[cfg(test)]
mod tests;
