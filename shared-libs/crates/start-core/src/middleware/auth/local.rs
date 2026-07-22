use base64::Engine;
use http::HeaderValue;
use http::header::AUTHORIZATION;
use rand::random;
use rpc_toolkit::yajrc::{RpcError, RpcResponse};
use rpc_toolkit::{Context, Empty, Middleware};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use url::Url;

use crate::context::RpcContext;
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::{create_file_mod, read_file_to_string};
use crate::util::serde::BASE64;

pub trait LocalAuthContext: Context {
    const LOCAL_AUTH_COOKIE_PATH: &str;
    const LOCAL_AUTH_COOKIE_OWNERSHIP: &str;
    fn init_auth_cookie() -> impl Future<Output = Result<(), Error>> + Send {
        async {
            let mut file = create_file_mod(Self::LOCAL_AUTH_COOKIE_PATH, 0o640).await?;
            file.write_all(BASE64.encode(random::<[u8; 32]>()).as_bytes())
                .await?;
            file.sync_all().await?;
            drop(file);
            Command::new("chown")
                .arg(Self::LOCAL_AUTH_COOKIE_OWNERSHIP)
                .arg(Self::LOCAL_AUTH_COOKIE_PATH)
                .invoke(crate::ErrorKind::Filesystem)
                .await?;
            Ok(())
        }
    }
}

impl LocalAuthContext for RpcContext {
    const LOCAL_AUTH_COOKIE_PATH: &str = "/run/startos/rpc.authcookie";
    const LOCAL_AUTH_COOKIE_OWNERSHIP: &str = "root:startos";
}

/// Whether `url` names the local machine — the only hosts the local auth
/// token may be sent to.
pub fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(host)) => host == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// `Authorization: Bearer` header carrying the server's local auth token, if
/// this process can read it (i.e. it runs on the server itself). Only attach
/// this to requests bound for a loopback address ([`is_loopback`]).
pub async fn local_auth_header<C: LocalAuthContext>() -> Option<HeaderValue> {
    let token = read_file_to_string(C::LOCAL_AUTH_COOKIE_PATH).await.ok()?;
    let mut header = HeaderValue::from_str(&format!("Bearer {token}")).ok()?;
    header.set_sensitive(true);
    Some(header)
}

fn unauthorized() -> Error {
    Error::new(
        eyre!("{}", t!("middleware.auth.unauthorized")),
        crate::ErrorKind::Authorization,
    )
}

async fn check_from_header<C: LocalAuthContext>(header: Option<&HeaderValue>) -> Result<(), Error> {
    if let Some(bearer) = header
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
    {
        if let Ok(token) = read_file_to_string(C::LOCAL_AUTH_COOKIE_PATH).await {
            if bearer == &*token {
                return Ok(());
            }
        }
    }
    Err(unauthorized())
}

#[derive(Clone)]
pub struct LocalAuth {
    authorization: Option<HeaderValue>,
}
impl LocalAuth {
    pub fn new() -> Self {
        Self {
            authorization: None,
        }
    }
}

impl<C: LocalAuthContext> Middleware<C> for LocalAuth {
    type Metadata = Empty;
    async fn process_http_request(
        &mut self,
        _: &C,
        request: &mut axum::extract::Request,
    ) -> Result<(), axum::response::Response> {
        self.authorization = request.headers().get(AUTHORIZATION).cloned();
        Ok(())
    }
    async fn process_rpc_request(
        &mut self,
        _: &C,
        _: Self::Metadata,
        _: &mut rpc_toolkit::RpcRequest,
    ) -> Result<(), rpc_toolkit::RpcResponse> {
        check_from_header::<C>(self.authorization.as_ref())
            .await
            .map_err(|e| RpcResponse::from(RpcError::from(e)))
    }
}
