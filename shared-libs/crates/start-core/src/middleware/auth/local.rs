use base64::Engine;
use http::HeaderValue;
use http::header::COOKIE;
use rand::random;
use rpc_toolkit::yajrc::{RpcError, RpcResponse};
use rpc_toolkit::{Context, Empty, Middleware};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::context::RpcContext;
use crate::middleware::auth::cookie_values;
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

fn unauthorized() -> Error {
    Error::new(
        eyre!("{}", t!("middleware.auth.unauthorized")),
        crate::ErrorKind::Authorization,
    )
}

async fn check_from_header<C: LocalAuthContext>(header: Option<&HeaderValue>) -> Result<(), Error> {
    if let Some(cookie_header) = header {
        if let Ok(token) = read_file_to_string(C::LOCAL_AUTH_COOKIE_PATH).await {
            if cookie_values(cookie_header, "local").any(|v| v == &*token) {
                return Ok(());
            }
        }
    }
    Err(unauthorized())
}

#[derive(Clone)]
pub struct LocalAuth {
    cookie: Option<HeaderValue>,
}
impl LocalAuth {
    pub fn new() -> Self {
        Self { cookie: None }
    }
}

impl<C: LocalAuthContext> Middleware<C> for LocalAuth {
    type Metadata = Empty;
    async fn process_http_request(
        &mut self,
        _: &C,
        request: &mut axum::extract::Request,
    ) -> Result<(), axum::response::Response> {
        self.cookie = request.headers().get(COOKIE).cloned();
        Ok(())
    }
    async fn process_rpc_request(
        &mut self,
        _: &C,
        _: Self::Metadata,
        _: &mut rpc_toolkit::RpcRequest,
    ) -> Result<(), rpc_toolkit::RpcResponse> {
        check_from_header::<C>(self.cookie.as_ref())
            .await
            .map_err(|e| RpcResponse::from(RpcError::from(e)))
    }
}
