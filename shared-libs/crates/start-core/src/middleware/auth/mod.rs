use axum::extract::Request;
use axum::response::Response;
use rpc_toolkit::{Context, DynMiddleware, Middleware, RpcRequest, RpcResponse};
use serde::Deserialize;

use crate::context::RpcContext;
use crate::db::model::Database;
use crate::middleware::auth::local::{LocalAuth, LocalAuthContext};
use crate::middleware::auth::signature::{SignatureAuth, SignatureAuthContext};
use crate::prelude::*;
use crate::util::serde::const_true;

pub mod local;
pub mod signature;

pub trait DbContext: Context {
    type Database: HasModel<Model = Model<Self::Database>> + Send + Sync;
    fn db(&self) -> &TypedPatchDb<Self::Database>;
}
impl DbContext for RpcContext {
    type Database = Database;
    fn db(&self) -> &TypedPatchDb<Self::Database> {
        &self.db
    }
}

#[derive(Deserialize)]
pub struct Metadata {
    #[serde(default = "const_true")]
    authenticated: bool,
}

/// Prefix of the params the auth middlewares inject on a verified request.
const AUTH_PARAM_PREFIX: &str = "__Auth_";

/// Drops caller-supplied auth params, so a handler never reads one an auth middleware did not set.
fn strip_auth_params(request: &mut RpcRequest) {
    if let Some(params) = request.params.as_object_mut() {
        params.retain(|k, _| !k.starts_with(AUTH_PARAM_PREFIX));
    }
}

/// Accepts a request once any middleware does, in the order added. A rejection reports the first middleware's error.
pub struct Auth<C: Context>(Vec<DynMiddleware<C>>);
impl<C: Context> Clone for Auth<C> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<C: Context> Auth<C> {
    pub fn new() -> Self {
        Self(Vec::new())
    }
}
impl<C: LocalAuthContext> Auth<C> {
    pub fn with_local_auth(mut self) -> Self {
        self.0.push(DynMiddleware::new(LocalAuth::new()));
        self
    }
}
impl<C: SignatureAuthContext> Auth<C> {
    pub fn with_signature_auth(mut self) -> Self {
        self.0.push(DynMiddleware::new(SignatureAuth::new()));
        self
    }
}
impl<C: Context> Middleware<C> for Auth<C> {
    type Metadata = Value;
    async fn process_http_request(
        &mut self,
        context: &C,
        request: &mut Request,
    ) -> Result<(), Response> {
        for middleware in self.0.iter_mut() {
            middleware.process_http_request(context, request).await?;
        }
        Ok(())
    }
    async fn process_rpc_request(
        &mut self,
        context: &C,
        metadata: Self::Metadata,
        request: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        let m: Metadata =
            from_value(metadata.clone()).map_err(|e| RpcResponse::from_result(Err(e)))?;
        strip_auth_params(request);
        let mut err = None;
        for middleware in self.0.iter_mut() {
            if let Err(e) = middleware
                .process_rpc_request(context, metadata.clone(), request)
                .await
            {
                if m.authenticated && err.is_none() {
                    err = Some(e);
                }
            } else {
                return Ok(());
            }
        }
        if let Some(e) = err {
            return Err(e);
        }

        Ok(())
    }
    async fn process_rpc_response(&mut self, context: &C, response: &mut RpcResponse) {
        for middleware in self.0.iter_mut() {
            middleware.process_rpc_response(context, response).await;
        }
    }
    async fn process_http_response(&mut self, context: &C, response: &mut Response) {
        for middleware in self.0.iter_mut() {
            middleware.process_http_response(context, response).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use rpc_toolkit::yajrc::GenericRpcMethod;

    use super::*;

    #[derive(Clone)]
    struct Ctx;
    impl Context for Ctx {}

    #[derive(Clone)]
    struct Reject(&'static str);
    impl Middleware<Ctx> for Reject {
        type Metadata = Value;
        async fn process_rpc_request(
            &mut self,
            _: &Ctx,
            _: Self::Metadata,
            _: &mut RpcRequest,
        ) -> Result<(), RpcResponse> {
            Err(RpcResponse::from_result(Err(Error::new(
                eyre!("{}", self.0),
                ErrorKind::Authorization,
            ))))
        }
    }

    #[tokio::test]
    async fn rejection_reports_first_middleware_error() {
        let mut auth = Auth::<Ctx>(vec![
            DynMiddleware::new(Reject("first")),
            DynMiddleware::new(Reject("second")),
        ]);
        let mut request = RpcRequest {
            id: None,
            method: GenericRpcMethod::new("package.get".into()),
            params: imbl_value::json!({}),
        };
        let res = auth
            .process_rpc_request(&Ctx, imbl_value::json!({}), &mut request)
            .await
            .unwrap_err();
        let message = serde_json::to_string(&res.result.unwrap_err()).unwrap();
        assert!(message.contains("first"), "{message}");
    }

    #[test]
    fn caller_supplied_auth_params_are_dropped() {
        let mut request = RpcRequest {
            id: None,
            method: GenericRpcMethod::new("package.get".into()),
            params: imbl_value::json!({
                "id": "hello-world",
                "__Auth_signer": "forged",
                "__Auth_userAgent": "forged",
                "__DeviceInfo_device_info": "kept",
            }),
        };
        strip_auth_params(&mut request);
        assert_eq!(
            request.params,
            imbl_value::json!({ "id": "hello-world", "__DeviceInfo_device_info": "kept" })
        );
    }
}
