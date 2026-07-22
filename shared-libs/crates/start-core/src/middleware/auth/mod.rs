use axum::extract::Request;
use axum::response::Response;
use http::HeaderValue;
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

/// Every value for cookie `name` in a `Cookie` request header, parsed
/// leniently per-cookie: a malformed or non-UTF8 sibling cookie (e.g. one
/// planted by a co-hosted service on another port) is skipped rather than
/// failing the whole header.
pub fn cookie_values<'a>(
    header: &'a HeaderValue,
    name: &'static str,
) -> impl Iterator<Item = &'a str> + 'a {
    header
        .as_bytes()
        .split(|&b| b == b';')
        .filter_map(move |pair| {
            let eq = pair.iter().position(|&b| b == b'=')?;
            if pair[..eq].trim_ascii() != name.as_bytes() {
                return None;
            }
            std::str::from_utf8(pair[eq + 1..].trim_ascii()).ok()
        })
}

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

#[cfg(test)]
mod tests {
    use http::HeaderValue;

    use super::cookie_values;

    fn parse(header: &[u8], name: &'static str) -> Vec<String> {
        // from_bytes (not from_str) so a hostile sibling can carry obs-text /
        // non-UTF8 bytes, exactly as a browser would resend them.
        cookie_values(&HeaderValue::from_bytes(header).unwrap(), name)
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn hostile_sibling_cannot_hide_our_token() {
        // A co-hosted service planted cookies with a non-UTF8 byte and a space
        // in the value — both fatal to a whole-header parse. Ours still resolves.
        let header = b"nc_session=\xff\xff; other=a b; session=good";
        assert_eq!(parse(header, "session"), ["good"]);
    }

    #[test]
    fn returns_every_session_candidate() {
        let header = b"session=first; foo=bar; session=second";
        assert_eq!(parse(header, "session"), ["first", "second"]);
    }

    #[test]
    fn skips_non_utf8_same_named_cookie() {
        let header = b"session=\xff; session=ok";
        assert_eq!(parse(header, "session"), ["ok"]);
    }

    #[test]
    fn no_match_yields_nothing() {
        assert!(parse(b"foo=bar; baz=qux", "session").is_empty());
        assert!(parse(b"", "session").is_empty());
    }
}
