use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use imbl_value::json;
use rpc_toolkit::Empty;

use super::*;

struct TestContext;
impl Context for TestContext {}

#[derive(Clone)]
struct Credential {
    accepted: bool,
    calls: Arc<AtomicUsize>,
}

impl Middleware<TestContext> for Credential {
    type Metadata = Empty;

    async fn process_rpc_request(
        &mut self,
        _: &TestContext,
        _: Empty,
        _: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.accepted {
            Ok(())
        } else {
            Err(RpcResponse::from_result(Err(Error::new(
                eyre!("credential rejected"),
                ErrorKind::Authorization,
            ))))
        }
    }
}

fn auth(
    local: Option<bool>,
    signature: Option<bool>,
    calls: &Arc<AtomicUsize>,
) -> Auth<TestContext> {
    let credential = |accepted| {
        DynMiddleware::new(Credential {
            accepted,
            calls: calls.clone(),
        })
    };
    Auth {
        local: local.map(credential),
        bearer: None,
        signature: signature.map(credential),
    }
}

fn request(method: &str) -> RpcRequest {
    RpcRequest {
        id: None,
        method: rpc_toolkit::yajrc::GenericRpcMethod::new(method.into()),
        params: json!({}),
    }
}

#[tokio::test]
async fn local_login_rejects_enrollment_even_with_a_valid_signature() {
    for signature in [None, Some(false), Some(true)] {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut auth = auth(Some(true), signature, &calls);
        let error = auth
            .process_rpc_request(
                &TestContext,
                json!({ "login": true }),
                &mut request("auth.login"),
            )
            .await
            .unwrap_err()
            .result
            .unwrap_err();
        assert_eq!(error.code, ErrorKind::InvalidRequest as i32);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn login_accepts_a_signature_without_local_authorization() {
    for local in [None, Some(false)] {
        let calls = Arc::new(AtomicUsize::new(0));
        auth(local, Some(true), &calls)
            .process_rpc_request(
                &TestContext,
                json!({ "login": true }),
                &mut request("auth.login"),
            )
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn ordinary_local_requests_still_process_signatures() {
    for signature in [false, true] {
        let calls = Arc::new(AtomicUsize::new(0));
        auth(Some(true), Some(signature), &calls)
            .process_rpc_request(&TestContext, json!({}), &mut request("auth.session.list"))
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn rejected_credentials_still_require_authorization() {
    let calls = Arc::new(AtomicUsize::new(0));
    let error = auth(Some(false), Some(false), &calls)
        .process_rpc_request(&TestContext, json!({}), &mut request("auth.session.list"))
        .await
        .unwrap_err()
        .result
        .unwrap_err();
    assert_eq!(error.code, ErrorKind::Authorization as i32);
}

#[tokio::test]
async fn public_requests_do_not_require_credentials() {
    let calls = Arc::new(AtomicUsize::new(0));
    auth(Some(false), Some(false), &calls)
        .process_rpc_request(
            &TestContext,
            json!({ "authenticated": false }),
            &mut request("system.info"),
        )
        .await
        .unwrap();
}
