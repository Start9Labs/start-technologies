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
use crate::util::io::{create_file_mod, maybe_read_file_to_string, read_file_to_string};
use crate::util::serde::BASE64;

const LOCAL_AUTH_TOKEN_BYTES: usize = 32;

pub trait LocalAuthContext: Context {
    const LOCAL_AUTH_COOKIE_PATH: &str;
    const LOCAL_AUTH_COOKIE_OWNERSHIP: &str;
    const LOCAL_SERVICE: Option<&str> = None;
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

#[derive(Debug)]
pub(crate) enum LocalAuthMissing {
    PermissionDenied,
    NotFound,
    Invalid(Error),
}

fn is_valid_cookie(token: &str) -> bool {
    BASE64
        .decode(token)
        .is_ok_and(|bytes| bytes.len() == LOCAL_AUTH_TOKEN_BYTES)
}

pub(crate) async fn read_local_auth_header(path: &Path) -> Result<HeaderValue, LocalAuthMissing> {
    match maybe_read_file_to_string(path).await {
        Ok(Some(token)) => {
            let invalid = || {
                LocalAuthMissing::Invalid(Error::new(
                    eyre!(
                        "{}",
                        t!("middleware.auth.local-auth-invalid", path = path.display())
                    ),
                    crate::ErrorKind::Filesystem,
                ))
            };
            if !is_valid_cookie(&token) {
                return Err(invalid());
            }
            let mut header =
                HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| invalid())?;
            header.set_sensitive(true);
            Ok(header)
        }
        Ok(None) => Err(LocalAuthMissing::NotFound),
        Err(err)
            if err.source.chain().any(|e| {
                e.downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
            }) =>
        {
            Err(LocalAuthMissing::PermissionDenied)
        }
        Err(err) => Err(LocalAuthMissing::Invalid(err)),
    }
}

pub(crate) async fn local_auth_header_status<C: LocalAuthContext>()
-> Result<HeaderValue, LocalAuthMissing> {
    read_local_auth_header(Path::new(C::LOCAL_AUTH_COOKIE_PATH)).await
}

/// `Authorization: Bearer` header carrying the server's local auth token, if
/// this process can read it (i.e. it runs on the server itself). Only attach
/// this to requests bound for the local machine — a loopback URL
/// ([`is_loopback`]), or one derived from the daemon's own listen address.
pub async fn local_auth_header<C: LocalAuthContext>() -> Option<HeaderValue> {
    local_auth_header_status::<C>().await.ok()
}

#[cfg(target_os = "linux")]
const SYSTEMD_UNIT_DIRS: &[&str] = &[
    "/etc/systemd/system",
    "/run/systemd/system",
    "/usr/lib/systemd/system",
    "/lib/systemd/system",
];

#[cfg(any(target_os = "linux", test))]
pub(crate) fn unit_installed_in<P: AsRef<Path>>(unit: &str, dirs: &[P]) -> bool {
    dirs.iter().any(|dir| dir.as_ref().join(unit).exists())
}

pub(crate) fn installed_local_service<C: LocalAuthContext>() -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        C::LOCAL_SERVICE
            .filter(|service| unit_installed_in(&format!("{service}.service"), SYSTEMD_UNIT_DIRS))
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

pub(crate) fn missing_local_auth_error(
    missing: LocalAuthMissing,
    cookie_path: &str,
    stopped_service: impl FnOnce() -> Option<&'static str>,
    remote_required: Error,
) -> Error {
    match missing {
        LocalAuthMissing::PermissionDenied => Error::new(
            eyre!(
                "{}",
                t!(
                    "middleware.auth.local-auth-permission-denied",
                    path = cookie_path
                )
            ),
            crate::ErrorKind::Filesystem,
        ),
        LocalAuthMissing::NotFound => match stopped_service() {
            Some(service) => Error::new(
                eyre!(
                    "{}",
                    t!(
                        "middleware.auth.local-service-not-running",
                        service = service
                    )
                ),
                crate::ErrorKind::Systemd,
            ),
            None => remote_required,
        },
        LocalAuthMissing::Invalid(err) => err,
    }
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
    is_valid_cookie(token) && bearer == token
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

    #[tokio::test]
    async fn read_local_auth_header_distinguishes_statuses() {
        let temp = TmpDir::new().await.unwrap();
        let token = BASE64.encode([7; LOCAL_AUTH_TOKEN_BYTES]);
        let cookie = temp.join("authcookie");
        write_file_atomic(&cookie, &token).await.unwrap();

        let header = read_local_auth_header(&cookie).await.unwrap();
        assert!(header.is_sensitive());
        assert_eq!(header.to_str().unwrap(), format!("Bearer {token}"));

        let missing = read_local_auth_header(&temp.join("missing")).await;
        assert!(matches!(missing, Err(LocalAuthMissing::NotFound)));

        let dir_err = read_local_auth_header(&temp).await;
        match dir_err {
            Err(LocalAuthMissing::Invalid(err)) => {
                assert_eq!(err.kind, crate::ErrorKind::Filesystem);
            }
            other => panic!("expected Invalid for directory, got {other:?}"),
        }

        let bad_cookie = temp.join("bad.authcookie");
        let short_token = BASE64.encode([7; LOCAL_AUTH_TOKEN_BYTES - 1]);
        for bad in [
            "",
            "not base64",
            &token[..8],
            short_token.as_str(),
            "bad\ntoken",
        ] {
            write_file_atomic(&bad_cookie, bad).await.unwrap();
            match read_local_auth_header(&bad_cookie).await {
                Err(LocalAuthMissing::Invalid(err)) => {
                    assert_eq!(err.kind, crate::ErrorKind::Filesystem);
                    let msg = err.source.to_string();
                    assert!(msg.contains("Invalid local auth token"), "{msg}");
                    assert!(msg.contains(&bad_cookie.display().to_string()), "{msg}");
                }
                Ok(header) => panic!(
                    "expected Invalid for {bad:?}, got Ok({:?})",
                    String::from_utf8_lossy(header.as_bytes())
                ),
                other => panic!("expected Invalid for {bad:?}, got {other:?}"),
            }
        }

        #[cfg(unix)]
        if !nix::unistd::geteuid().is_root() {
            use std::os::unix::fs::PermissionsExt;

            let denied = temp.join("denied.authcookie");
            write_file_atomic(&denied, &token).await.unwrap();
            std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o000)).unwrap();
            let status = read_local_auth_header(&denied).await;
            std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert!(matches!(status, Err(LocalAuthMissing::PermissionDenied)));
        }
    }

    #[tokio::test]
    async fn unit_installed_in_detects_matching_unit_file() {
        let dir_a = TmpDir::new().await.unwrap();
        let dir_b = TmpDir::new().await.unwrap();
        let empty: [&Path; 0] = [];
        let dirs: [&Path; 2] = [dir_a.as_ref(), dir_b.as_ref()];

        assert!(!unit_installed_in("start-tunneld.service", &empty));
        assert!(!unit_installed_in("start-tunneld.service", &dirs));

        write_file_atomic(dir_b.join("start-tunneld.service"), "[Unit]\n")
            .await
            .unwrap();
        assert!(unit_installed_in("start-tunneld.service", &dirs));
        assert!(!unit_installed_in("start-registryd.service", &dirs));
    }

    #[test]
    fn missing_local_auth_error_maps_all_arms() {
        let remote = || {
            Error::new(
                eyre!("`--tunnel` required"),
                crate::ErrorKind::InvalidRequest,
            )
        };
        let no_service_check = || panic!("stopped_service should not be queried");

        let err = missing_local_auth_error(
            LocalAuthMissing::PermissionDenied,
            "/run/startos/tunnel.authcookie",
            no_service_check,
            remote(),
        );
        assert_eq!(err.kind, crate::ErrorKind::Filesystem);
        let msg = err.source.to_string();
        assert!(msg.contains("/run/startos/tunnel.authcookie"), "{msg}");
        assert!(msg.contains("sudo"), "{msg}");
        assert!(!msg.contains("--tunnel"), "{msg}");
        assert!(!msg.contains("--registry"), "{msg}");

        let stopped = missing_local_auth_error(
            LocalAuthMissing::NotFound,
            "/run/startos/tunnel.authcookie",
            || Some("start-tunneld"),
            remote(),
        );
        assert_eq!(stopped.kind, crate::ErrorKind::Systemd);
        let msg = stopped.source.to_string();
        assert!(msg.contains("start-tunneld"), "{msg}");
        assert!(!msg.contains("start-tunneld.service"), "{msg}");
        assert!(!msg.contains("--tunnel"), "{msg}");
        assert!(!msg.contains("--registry"), "{msg}");

        let not_installed = missing_local_auth_error(
            LocalAuthMissing::NotFound,
            "/run/startos/tunnel.authcookie",
            || None,
            remote(),
        );
        assert_eq!(not_installed.kind, crate::ErrorKind::InvalidRequest);
        assert!(not_installed.source.to_string().contains("--tunnel"));

        let invalid = missing_local_auth_error(
            LocalAuthMissing::Invalid(Error::new(
                eyre!("corrupt cookie"),
                crate::ErrorKind::Filesystem,
            )),
            "/run/startos/tunnel.authcookie",
            no_service_check,
            remote(),
        );
        assert_eq!(invalid.kind, crate::ErrorKind::Filesystem);
        assert!(invalid.source.to_string().contains("corrupt cookie"));
    }
}
