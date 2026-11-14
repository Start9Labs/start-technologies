use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;

use base64::Engine;
use http::HeaderValue;
use http::header::AUTHORIZATION;
use rand::random;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{Context, Middleware, RpcResponse};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use url::Url;

use crate::context::{DiagnosticContext, RpcContext};
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::{create_file_mod, read_file_to_string};
use crate::util::serde::BASE64;

const LOCAL_AUTH_TOKEN_BYTES: usize = 32;

pub trait LocalAuthContext: Context {
    const LOCAL_AUTH_COOKIE_PATH: &str;
    const LOCAL_AUTH_COOKIE_OWNERSHIP: &str;
    fn init_auth_cookie() -> impl Future<Output = Result<(), Error>> + Send {
        async {
            let mut file = create_file_mod(Self::LOCAL_AUTH_COOKIE_PATH, 0o640).await?;
            file.write_all(
                BASE64
                    .encode(random::<[u8; LOCAL_AUTH_TOKEN_BYTES]>())
                    .as_bytes(),
            )
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

impl LocalAuthContext for DiagnosticContext {
    const LOCAL_AUTH_COOKIE_PATH: &str = "/run/startos/diagnostic.authcookie";
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

/// Where to reach a daemon bound to `listen`. The unspecified address means
/// "every interface on this host", which is not a destination — dial loopback.
pub fn dial_addr(listen: SocketAddr) -> SocketAddr {
    if listen.ip().is_unspecified() {
        let loopback = match listen.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        };
        SocketAddr::new(loopback, listen.port())
    } else {
        listen
    }
}

/// `Authorization: Bearer` header carrying the server's local auth token, if
/// this process can read it (i.e. it runs on the server itself). Only attach
/// this to requests bound for the local machine — a loopback URL
/// ([`is_loopback`]), or one derived from the daemon's own listen address.
pub async fn local_auth_header<C: LocalAuthContext>() -> Option<HeaderValue> {
    let token = read_file_to_string(C::LOCAL_AUTH_COOKIE_PATH).await.ok()?;
    let mut header = HeaderValue::from_str(&format!("Bearer {token}")).ok()?;
    header.set_sensitive(true);
    Some(header)
}

pub(crate) async fn local_auth_header_for_url<C: LocalAuthContext>(
    url: &Url,
) -> Option<HeaderValue> {
    if is_loopback(url) {
        local_auth_header::<C>().await
    } else {
        None
    }
}

pub(crate) fn unauthorized() -> Error {
    Error::new(
        eyre!("{}", t!("middleware.auth.unauthorized")),
        crate::ErrorKind::Authorization,
    )
}

fn matches_cookie(bearer: &str, token: &str) -> bool {
    BASE64
        .decode(token)
        .is_ok_and(|bytes| bytes.len() == LOCAL_AUTH_TOKEN_BYTES)
        && bearer == token
}

async fn check_from_header(cookie_path: &Path, header: Option<&HeaderValue>) -> Result<(), Error> {
    if let Some(bearer) = header
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
    {
        if let Ok(token) = read_file_to_string(cookie_path).await {
            if matches_cookie(bearer, &token) {
                return Ok(());
            }
        }
    }
    Err(unauthorized())
}

#[derive(Deserialize)]
pub struct Metadata {
    #[serde(default)]
    get_local_auth: bool,
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

    async fn authenticate_request(
        &self,
        cookie_path: &Path,
        metadata: Metadata,
        request: &mut rpc_toolkit::RpcRequest,
    ) -> Result<(), rpc_toolkit::RpcResponse> {
        let result = check_from_header(cookie_path, self.authorization.as_ref()).await;
        if let Some(params) = request.params.as_object_mut() {
            params.remove("__Auth_local");
            if metadata.get_local_auth {
                params.insert("__Auth_local".into(), Value::Bool(result.is_ok()));
            }
        }
        result.map_err(|error| RpcResponse::from(RpcError::from(error)))
    }
}

impl<C: LocalAuthContext> Middleware<C> for LocalAuth {
    type Metadata = Metadata;
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
        metadata: Self::Metadata,
        request: &mut rpc_toolkit::RpcRequest,
    ) -> Result<(), rpc_toolkit::RpcResponse> {
        self.authenticate_request(Path::new(C::LOCAL_AUTH_COOKIE_PATH), metadata, request)
            .await
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;
    use crate::net::web_server::TcpMetadata;
    use crate::util::io::{TmpDir, write_file_atomic};

    struct TestContext;
    impl Context for TestContext {}
    impl LocalAuthContext for TestContext {
        const LOCAL_AUTH_COOKIE_PATH: &str = "/unused";
        const LOCAL_AUTH_COOKIE_OWNERSHIP: &str = "root:startos";
    }

    fn request(claimed_local: bool) -> rpc_toolkit::RpcRequest {
        rpc_toolkit::RpcRequest {
            id: None,
            method: rpc_toolkit::yajrc::GenericRpcMethod::new("diagnostic.update".into()),
            params: json!({"trust": true, "__Auth_local": claimed_local}),
        }
    }

    #[tokio::test]
    async fn local_auth_overwrites_claims_and_preserves_specific_bind_support() {
        let temp = TmpDir::new().await.unwrap();
        let cookie = temp.join("cookie");
        let token = BASE64.encode([42; LOCAL_AUTH_TOKEN_BYTES]);
        write_file_atomic(&cookie, &token).await.unwrap();
        for peer in [
            "127.0.0.1:1234",
            "[::1]:1234",
            "[::ffff:127.0.0.1]:1234",
            "192.168.1.2:1234",
            "[::ffff:192.168.1.2]:1234",
        ] {
            for valid_cookie in [false, true] {
                let mut auth = LocalAuth::new();
                let mut http = axum::extract::Request::builder()
                    .header(
                        AUTHORIZATION,
                        format!(
                            "Bearer {}",
                            if valid_cookie {
                                token.clone()
                            } else {
                                BASE64.encode([43; LOCAL_AUTH_TOKEN_BYTES])
                            }
                        ),
                    )
                    .body(axum::body::Body::empty())
                    .unwrap();
                let peer: SocketAddr = peer.parse().unwrap();
                http.extensions_mut().insert(TcpMetadata {
                    peer_addr: peer,
                    local_addr: SocketAddr::new(peer.ip(), 5959),
                });
                auth.process_http_request(&TestContext, &mut http)
                    .await
                    .unwrap();
                for claimed in [false, true] {
                    let mut rpc = request(claimed);
                    assert_eq!(
                        auth.authenticate_request(
                            &cookie,
                            Metadata {
                                get_local_auth: true
                            },
                            &mut rpc
                        )
                        .await
                        .is_ok(),
                        valid_cookie
                    );
                    assert_eq!(rpc.params["__Auth_local"], Value::Bool(valid_cookie));
                    let mut unused = request(claimed);
                    assert_eq!(
                        auth.authenticate_request(
                            &cookie,
                            Metadata {
                                get_local_auth: false
                            },
                            &mut unused
                        )
                        .await
                        .is_ok(),
                        valid_cookie
                    );
                    assert!(unused.params.get("__Auth_local").is_none());
                }
            }
        }
        let mut unauthenticated = request(true);
        assert!(
            LocalAuth::new()
                .authenticate_request(
                    &cookie,
                    Metadata {
                        get_local_auth: true
                    },
                    &mut unauthenticated
                )
                .await
                .is_err()
        );
        assert_eq!(unauthenticated.params["__Auth_local"], Value::Bool(false));
        let auth = LocalAuth {
            authorization: Some(HeaderValue::from_str(&format!("Bearer {token}")).unwrap()),
        };
        for path in [temp.join("missing"), cookie] {
            if path.exists() {
                write_file_atomic(&path, "").await.unwrap();
            }
            let mut rpc = request(true);
            assert!(
                auth.authenticate_request(
                    &path,
                    Metadata {
                        get_local_auth: true
                    },
                    &mut rpc
                )
                .await
                .is_err()
            );
            assert_eq!(rpc.params["__Auth_local"], Value::Bool(false));
        }
    }

    #[test]
    fn cookie_authentication_rejects_incomplete_initialization() {
        let token = BASE64.encode([42; LOCAL_AUTH_TOKEN_BYTES]);
        assert!(matches_cookie(&token, &token));
        assert!(!matches_cookie(
            &BASE64.encode([43; LOCAL_AUTH_TOKEN_BYTES]),
            &token
        ));
        for incomplete in [
            "",
            "not base64",
            &token[..8],
            &BASE64.encode([42; LOCAL_AUTH_TOKEN_BYTES - 1]),
        ] {
            assert!(!matches_cookie(incomplete, incomplete));
        }
    }

    #[test]
    fn wildcard_listen_dials_loopback() {
        for (listen, expected) in [
            ("0.0.0.0:5959", "127.0.0.1:5959"),
            ("[::]:5959", "[::1]:5959"),
            ("127.0.0.1:5959", "127.0.0.1:5959"),
            ("192.168.1.5:5959", "192.168.1.5:5959"),
        ] {
            assert_eq!(
                dial_addr(listen.parse().unwrap()),
                expected.parse::<SocketAddr>().unwrap(),
                "{listen}"
            );
        }
    }

    #[test]
    fn dialed_wildcard_is_loopback() {
        let url: Url = format!("http://{}", dial_addr("0.0.0.0:5959".parse().unwrap()))
            .parse()
            .unwrap();
        assert!(is_loopback(&url));
    }
}
