use axum::extract::Request;
use axum::response::Response;
use rpc_toolkit::{Context, DynMiddleware, Middleware, RpcRequest, RpcResponse};
use serde::Deserialize;

use crate::auth::LoginContext;
use crate::context::RpcContext;
use crate::db::model::Database;
use crate::middleware::auth::bearer::BearerAuth;
use crate::middleware::auth::local::{LocalAuth, LocalAuthContext};
use crate::middleware::auth::signature::{LoginMetadata, SignatureAuth, SignatureAuthContext};
use crate::prelude::*;
use crate::util::serde::const_true;

pub mod bearer;
pub mod local;
mod login_rate_limit;
pub mod signature;

#[cfg(test)]
mod tests;

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
    #[serde(flatten)]
    login_metadata: LoginMetadata,
}

pub struct Auth<C: Context> {
    local: Option<DynMiddleware<C>>,
    bearer: Option<DynMiddleware<C>>,
    signature: Option<DynMiddleware<C>>,
}
impl<C: Context> Clone for Auth<C> {
    fn clone(&self) -> Self {
        Self {
            local: self.local.clone(),
            bearer: self.bearer.clone(),
            signature: self.signature.clone(),
        }
    }
}
impl<C: Context> Auth<C> {
    pub fn new() -> Self {
        Self {
            local: None,
            bearer: None,
            signature: None,
        }
    }

    fn methods(&mut self) -> impl Iterator<Item = &mut DynMiddleware<C>> {
        self.local
            .iter_mut()
            .chain(self.bearer.iter_mut())
            .chain(self.signature.iter_mut())
    }
}
impl<C: LocalAuthContext> Auth<C> {
    pub fn with_local_auth(mut self) -> Self {
        self.local = Some(DynMiddleware::new(LocalAuth::new()));
        self
    }
}
impl<C: SignatureAuthContext> Auth<C> {
    pub fn with_signature_auth(mut self) -> Self {
        self.signature = Some(DynMiddleware::new(SignatureAuth::new()));
        self
    }
}
impl<C: LoginContext> Auth<C> {
    pub fn with_bearer_auth(mut self) -> Self {
        self.bearer = Some(DynMiddleware::new(BearerAuth::new()));
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
        for middleware in self.methods() {
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
        let mut authorized = false;
        let mut err = None;
        if let Some(local) = &mut self.local {
            match local
                .process_rpc_request(context, metadata.clone(), request)
                .await
            {
                Ok(()) => {
                    if m.login_metadata.login {
                        return Err(RpcResponse::from_result(Err(Error::new(
                            eyre!("{}", t!("middleware.auth.local-login-unnecessary")),
                            ErrorKind::InvalidRequest,
                        ))));
                    }
                    authorized = true;
                }
                Err(e) => err = Some(e),
            }
        }
        if let Some(bearer) = &mut self.bearer {
            match bearer
                .process_rpc_request(context, metadata.clone(), request)
                .await
            {
                Ok(()) => authorized = true,
                Err(e) => err = Some(e),
            }
        }
        if let Some(signature) = &mut self.signature {
            match signature
                .process_rpc_request(context, metadata, request)
                .await
            {
                Ok(()) => authorized = true,
                Err(e) => err = Some(e),
            }
        }
        match err {
            Some(e) if !authorized && m.authenticated => Err(e),
            _ => Ok(()),
        }
    }
    async fn process_rpc_response(&mut self, context: &C, response: &mut RpcResponse) {
        for middleware in self.methods() {
            middleware.process_rpc_response(context, response).await;
        }
    }
    async fn process_http_response(&mut self, context: &C, response: &mut Response) {
        for middleware in self.methods() {
            middleware.process_http_response(context, response).await;
        }
    }
}
