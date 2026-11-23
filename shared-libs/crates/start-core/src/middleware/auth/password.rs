use std::path::Path;

use axum::extract::Request;
use rpc_toolkit::{Context, Middleware, RpcRequest, RpcResponse};
use serde::Deserialize;

use crate::auth::{check_shadow_password, start9_password_hash};
use crate::middleware::auth::session::SessionContext;
use crate::net::request_origin::RequestOrigin;
use crate::prelude::*;
use crate::util::serde::const_true;

/// The RPC parameter carrying the password.
pub const PASSWORD_PARAM: &str = "password";

#[derive(Deserialize)]
pub struct Metadata {
    #[serde(default = "const_true")]
    authenticated: bool,
}

/// Admits a request carrying the start9 user's password from `/etc/shadow`.
/// While that user has no usable password, admits every request.
#[derive(Clone)]
pub struct ShadowPasswordAuth {
    origin: RequestOrigin,
}
impl ShadowPasswordAuth {
    pub fn new() -> Self {
        Self {
            origin: RequestOrigin::default(),
        }
    }
}

fn password_required() -> Error {
    Error::new(
        eyre!("{}", t!("middleware.auth.password-required")),
        ErrorKind::IncorrectPassword,
    )
}

/// Rate-limited like login. A password reaching the server outside an encrypted session is refused.
async fn authorize(
    root: &Path,
    password: Option<&str>,
    session_address: Option<InternedString>,
    origin: &RequestOrigin,
) -> Result<(), Error> {
    let Some(hash) = start9_password_hash(root).await? else {
        return Ok(());
    };
    let password = password.ok_or_else(password_required)?;
    let address = session_address.ok_or_else(super::local::unauthorized)?;
    super::login_rate_limit::check(address, origin)?;
    check_shadow_password(&hash, password)
}

impl<C: SessionContext + Context> Middleware<C> for ShadowPasswordAuth {
    type Metadata = Metadata;
    async fn process_http_request(
        &mut self,
        _: &C,
        request: &mut Request,
    ) -> Result<(), axum::response::Response> {
        self.origin = RequestOrigin::from_extensions(request.extensions());
        Ok(())
    }
    async fn process_rpc_request(
        &mut self,
        context: &C,
        metadata: Self::Metadata,
        request: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        if !metadata.authenticated {
            return Ok(());
        }
        let password = request.params.get(PASSWORD_PARAM).and_then(|p| p.as_str());
        authorize(
            Path::new("/"),
            password,
            context.secure_session().map(|s| s.address().clone()),
            &self.origin,
        )
        .await
        .map_err(|e| RpcResponse::from_result(Err(e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::io::{TmpDir, write_file_atomic};

    const PASSWORD: &str = "correct horse";

    async fn root_with_start9(field: &str) -> TmpDir {
        let root = TmpDir::new().await.unwrap();
        write_file_atomic(
            root.join("etc/shadow"),
            format!("root:*:19000:0:99999:7:::\nstart9:{field}:19000:0:99999:7:::\n"),
        )
        .await
        .unwrap();
        root
    }

    fn sha512(password: &str) -> String {
        sha_crypt::sha512_simple(password, &sha_crypt::Sha512Params::default()).unwrap()
    }

    #[tokio::test]
    async fn unset_or_locked_start9_password_admits_without_password() {
        for field in ["!", "*", "!$6$salt$hash", ""] {
            let root = root_with_start9(field).await;
            authorize(&root, None, None, &RequestOrigin::default())
                .await
                .unwrap_or_else(|e| panic!("{field:?}: {e}"));
        }
    }

    #[tokio::test]
    async fn set_start9_password_is_required_and_checked() {
        let root = root_with_start9(&sha512(PASSWORD)).await;
        let origin = RequestOrigin::default();
        let session = || Some(InternedString::intern("password-checked.local"));

        let missing = authorize(&root, None, session(), &origin)
            .await
            .unwrap_err();
        assert_eq!(missing.kind, ErrorKind::IncorrectPassword);

        let unencrypted = authorize(&root, Some(PASSWORD), None, &origin)
            .await
            .unwrap_err();
        assert_eq!(unencrypted.kind, ErrorKind::Authorization);

        let wrong = authorize(&root, Some("wrong"), session(), &origin)
            .await
            .unwrap_err();
        assert_eq!(wrong.kind, ErrorKind::IncorrectPassword);

        authorize(&root, Some(PASSWORD), session(), &origin)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn password_attempts_share_the_login_rate_limit() {
        let root = root_with_start9(&sha512(PASSWORD)).await;
        let origin = RequestOrigin::default();
        let session = || Some(InternedString::intern("password-rate-limited.local"));
        for _ in 0..3 {
            authorize(&root, Some("wrong"), session(), &origin)
                .await
                .unwrap_err();
        }
        let limited = authorize(&root, Some(PASSWORD), session(), &origin)
            .await
            .unwrap_err();
        assert_eq!(limited.kind, ErrorKind::RateLimited);
    }
}
