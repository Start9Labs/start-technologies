use std::collections::BTreeSet;
use std::net::IpAddr;
use std::pin::Pin;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::BodyExt;
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
use crate::db::DbAccessMut;
use crate::prelude::*;
use crate::util::serde::{Der64, Pem};

pub(super) struct AcmeHttpClient(reqwest::Client);

impl AcmeHttpClient {
    pub(super) fn new() -> Result<Self, AcmeError> {
        reqwest::Client::builder()
            .use_native_tls()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map(Self)
            .map_err(other_error)
    }
}

impl HttpClient for AcmeHttpClient {
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
            let response = client
                .request(parts.method, parts.uri.to_string())
                .headers(parts.headers)
                .body(body)
                .send()
                .await
                .map_err(other_error)?;
            let mut result = http::Response::builder().status(response.status());
            *result.headers_mut().unwrap() = response.headers().clone();
            Ok(result
                .body(http_body_util::Full::new(
                    response.bytes().await.map_err(other_error)?,
                ))?
                .into())
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
