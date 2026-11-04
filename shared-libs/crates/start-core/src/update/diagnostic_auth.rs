use axum::extract::Request;
use axum::response::Response;
use rpc_toolkit::{Context, DynMiddleware, Middleware, RpcRequest, RpcResponse};
use serde::Deserialize;

use crate::middleware::auth::local::{LocalAuth, LocalAuthContext, unauthorized};
use crate::net::request_origin::RequestOrigin;
use crate::prelude::*;

#[derive(Clone)]
pub(crate) struct DiagnosticUpdateAuth<C: Context> {
    local: DynMiddleware<C>,
    loopback: bool,
}

impl<C: LocalAuthContext> DiagnosticUpdateAuth<C> {
    pub fn new() -> Self {
        Self {
            local: DynMiddleware::new(LocalAuth::new()),
            loopback: false,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct Metadata {
    #[serde(default)]
    diagnostic_update: bool,
}

impl<C: Context + Clone> Middleware<C> for DiagnosticUpdateAuth<C> {
    type Metadata = Metadata;

    async fn process_http_request(
        &mut self,
        context: &C,
        request: &mut Request,
    ) -> Result<(), Response> {
        self.loopback = RequestOrigin::from_extensions(request.extensions())
            .peer_ip
            .is_some_and(|ip| ip.is_loopback());
        self.local.process_http_request(context, request).await
    }

    async fn process_rpc_request(
        &mut self,
        context: &C,
        metadata: Metadata,
        request: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        if metadata.diagnostic_update
            && request.params.get("trust").and_then(Value::as_bool) == Some(true)
        {
            if !self.loopback {
                return Err(RpcResponse::from_result(Err(unauthorized())));
            }
            self.local
                .process_rpc_request(context, imbl_value::json!({}), request)
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;
    use rpc_toolkit::{Empty, HandlerFor};

    use super::*;
    use crate::context::DiagnosticContext;
    use crate::net::web_server::TcpMetadata;

    #[derive(Clone)]
    struct TestContext;
    impl Context for TestContext {}

    #[derive(Clone)]
    struct Credential(bool);
    impl Middleware<TestContext> for Credential {
        type Metadata = Empty;
        async fn process_rpc_request(
            &mut self,
            _: &TestContext,
            _: Empty,
            _: &mut RpcRequest,
        ) -> Result<(), RpcResponse> {
            if self.0 {
                Ok(())
            } else {
                Err(RpcResponse::from_result(Err(unauthorized())))
            }
        }
    }

    #[test]
    fn diagnostic_update_handler_selects_the_override_check() {
        let metadata = crate::diagnostic::diagnostic::<DiagnosticContext>()
            .metadata(["update"].into_iter().collect());
        assert_eq!(metadata.get("diagnostic_update"), Some(&Value::Bool(true)));
    }

    #[tokio::test]
    async fn diagnostic_override_requires_both_loopback_and_local_credentials() {
        for loopback in [false, true] {
            for credential in [false, true] {
                for trust in [false, true] {
                    let mut auth = DiagnosticUpdateAuth {
                        local: DynMiddleware::new(Credential(credential)),
                        loopback,
                    };
                    let mut request = RpcRequest {
                        id: None,
                        method: rpc_toolkit::yajrc::GenericRpcMethod::new(
                            "diagnostic.update".into(),
                        ),
                        params: json!({"trust": trust}),
                    };
                    let result = auth
                        .process_rpc_request(
                            &TestContext,
                            Metadata {
                                diagnostic_update: true,
                            },
                            &mut request,
                        )
                        .await;
                    assert_eq!(result.is_ok(), !trust || (loopback && credential));
                }
            }
        }
    }

    #[tokio::test]
    async fn local_proxy_retains_the_remote_origin_for_the_override_gate() {
        let mut auth = DiagnosticUpdateAuth {
            local: DynMiddleware::new(Credential(true)),
            loopback: false,
        };
        let mut http_request = Request::new(axum::body::Body::empty());
        http_request.extensions_mut().insert(TcpMetadata {
            peer_addr: "127.0.0.1:1234".parse().unwrap(),
            local_addr: "127.0.0.1:80".parse().unwrap(),
        });
        http_request
            .extensions_mut()
            .insert(RequestOrigin::from_connection(
                Some(&TcpMetadata {
                    peer_addr: "203.0.113.8:4567".parse().unwrap(),
                    local_addr: "192.168.1.2:443".parse().unwrap(),
                }),
                None,
            ));
        auth.process_http_request(&TestContext, &mut http_request)
            .await
            .unwrap();
        let mut rpc_request = RpcRequest {
            id: None,
            method: rpc_toolkit::yajrc::GenericRpcMethod::new("diagnostic.update".into()),
            params: json!({"trust": true}),
        };
        assert!(
            auth.process_rpc_request(
                &TestContext,
                Metadata {
                    diagnostic_update: true
                },
                &mut rpc_request
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn loopback_comes_from_the_accepted_connection_not_host_headers() {
        let mut auth = DiagnosticUpdateAuth {
            local: DynMiddleware::new(Credential(true)),
            loopback: false,
        };
        for (peer, expected) in [
            ("203.0.113.8:1234", false),
            ("127.0.0.1:1234", true),
            ("[::1]:1234", true),
            ("[::ffff:127.0.0.1]:1234", true),
            ("[::ffff:203.0.113.8]:1234", false),
        ] {
            let mut request = Request::builder()
                .header("Host", "localhost")
                .header("X-Forwarded-For", "127.0.0.1")
                .body(axum::body::Body::empty())
                .unwrap();
            request.extensions_mut().insert(TcpMetadata {
                peer_addr: peer.parse().unwrap(),
                local_addr: "127.0.0.1:80".parse().unwrap(),
            });
            auth.process_http_request(&TestContext, &mut request)
                .await
                .unwrap();
            assert_eq!(auth.loopback, expected);
        }
        let mut request = Request::new(axum::body::Body::empty());
        auth.process_http_request(&TestContext, &mut request)
            .await
            .unwrap();
        assert!(!auth.loopback);
    }
}
