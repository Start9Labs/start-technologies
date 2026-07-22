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
        let mut err = None;
        for middleware in self.0.iter_mut() {
            if let Err(e) = middleware
                .process_rpc_request(context, metadata.clone(), request)
                .await
            {
                if m.authenticated {
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
