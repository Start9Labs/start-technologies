//! Serves a handler tree as MCP tools over streamable HTTP.
//!
//! A tool is a method named in a catalog of [`Tool`] descriptions, normally
//! generated at build time from the tree's TypeScript bindings with
//! `z.toJSONSchema`. Each MCP request passes through the [`HttpServer`]'s
//! middleware exactly as an RPC request does: `process_http_request` once per
//! HTTP request, then `process_rpc_request`, the handler and
//! `process_rpc_response` per tool call, and `process_http_response` on the
//! reply.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::handler::Handler;
use axum::response::Response;
use futures::future::BoxFuture;
use futures::FutureExt;
use imbl_value::imbl::{OrdMap, Vector};
use imbl_value::{InternedString, Value};
pub use rmcp;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, NumberOrString, PaginatedRequestParams, ServerCapabilities, ServerConfig,
    Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use tokio::sync::Mutex;
use tower_service::Service;

use crate::server::{DynMiddleware, GenericRpcMethod, HttpServer, RpcRequest};
use crate::HandleAny;

/// Chooses which cataloged methods are offered as tools and how each is described.
pub trait ToolPolicy: Send + Sync + 'static {
    /// Whether the method is offered as a tool.
    #[allow(unused_variables)]
    fn expose(&self, method: &str, metadata: &OrdMap<&'static str, Value>) -> bool {
        true
    }
    /// Hints MCP clients read about the method's effects.
    #[allow(unused_variables)]
    fn annotations(
        &self,
        method: &str,
        metadata: &OrdMap<&'static str, Value>,
    ) -> Option<ToolAnnotations> {
        None
    }
}

/// Offers every cataloged method, without annotations.
pub struct ExposeAll;
impl ToolPolicy for ExposeAll {}

/// A catalog entry naming no method of the handler tree.
#[derive(Debug, thiserror::Error)]
#[error("cataloged tool {0} names no method of the handler tree")]
pub struct UnknownTool(pub String);

/// The tools of a handler tree, served by [`McpServer::service`].
pub struct McpServer<Context: crate::Context> {
    http: HttpServer<Context>,
    tools: Arc<BTreeMap<String, Tool>>,
    info: Arc<ServerConfig>,
}
impl<Context: crate::Context> Clone for McpServer<Context> {
    fn clone(&self) -> Self {
        Self {
            http: self.http.clone(),
            tools: self.tools.clone(),
            info: self.info.clone(),
        }
    }
}

impl<Context: crate::Context> McpServer<Context> {
    /// Fails when the catalog names a method the tree lacks.
    pub fn new(
        http: HttpServer<Context>,
        catalog: impl IntoIterator<Item = Tool>,
        policy: impl ToolPolicy,
        server_info: Implementation,
    ) -> Result<Self, UnknownTool> {
        let root = &http.inner.root_handler;
        let mut tools = BTreeMap::new();
        for mut tool in catalog {
            let method = root
                .method_from_dots(&tool.name)
                .ok_or_else(|| UnknownTool(tool.name.to_string()))?;
            let metadata = root.metadata(method);
            if !policy.expose(&tool.name, &metadata) {
                continue;
            }
            tool.annotations = policy.annotations(&tool.name, &metadata);
            tools.insert(tool.name.to_string(), tool);
        }
        Ok(Self {
            http,
            tools: Arc::new(tools),
            info: Arc::new(
                ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
                    .with_server_info(server_info),
            ),
        })
    }

    /// The stateless MCP endpoint, as an axum handler with JSON responses.
    ///
    /// Overrides `config.legacy_session_mode` and `config.json_response`.
    /// `config.allowed_hosts` defaults to loopback names; a server reached by
    /// other names must list them.
    pub fn service(self, config: StreamableHttpServerConfig) -> McpService<Context> {
        let http = self.http.clone();
        McpService {
            http,
            inner: StreamableHttpService::new(
                move || Ok(self.clone()),
                Arc::new(NeverSessionManager::default()),
                config
                    .with_legacy_session_mode(false)
                    .with_json_response(true),
            ),
        }
    }

    fn tool_result(tool: &Tool, result: Result<Value, yajrc::RpcError>) -> CallToolResult {
        match result {
            Ok(value) => {
                let json = imbl_value::from_value::<serde_json::Value>(value)
                    .unwrap_or(serde_json::Value::Null);
                let text = ContentBlock::text(json.to_string());
                let mut result = CallToolResult::success(vec![text]);
                if tool.output_schema.is_some() {
                    result.structured_content = Some(json);
                }
                result
            }
            Err(error) => CallToolResult::error(vec![ContentBlock::text(
                serde_json::to_string(&error).unwrap_or_else(|_| error.message.to_string()),
            )]),
        }
    }
}

/// The context and middleware of the HTTP request carrying an MCP message.
struct McpRequest<Context> {
    context: Context,
    middleware: Mutex<Vector<DynMiddleware<Context>>>,
}

impl<Context: crate::Context> ServerHandler for McpServer<Context> {
    fn get_info(&self) -> ServerConfig {
        (*self.info).clone()
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.get(name).cloned()
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(
            self.tools.values().cloned().collect(),
        ))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let tool = self.tools.get(&*request.name).ok_or_else(|| {
            McpError::invalid_params(format!("unknown tool {}", request.name), None)
        })?;
        let http_request = context
            .extensions
            .get::<http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<Arc<McpRequest<Context>>>())
            .cloned()
            .ok_or_else(|| McpError::internal_error("tool call outside McpService", None))?;
        let params = imbl_value::to_value(&request.arguments.unwrap_or_default())
            .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
        let rpc_request = RpcRequest {
            id: Some(match context.id {
                NumberOrString::Number(n) => yajrc::Id::Number(n.into()),
                NumberOrString::String(s) => yajrc::Id::String(s.to_string()),
            }),
            method: GenericRpcMethod::new(InternedString::intern(&*tool.name)),
            params,
        };
        let mut middleware = http_request.middleware.lock().await;
        let response = self
            .http
            .process_rpc_request(&http_request.context, &mut middleware, rpc_request)
            .await;
        Ok(Self::tool_result(tool, response.result).into())
    }
}

/// The MCP endpoint of an [`McpServer`].
pub struct McpService<Context: crate::Context> {
    http: HttpServer<Context>,
    inner: StreamableHttpService<McpServer<Context>, NeverSessionManager>,
}
impl<Context: crate::Context> Clone for McpService<Context> {
    fn clone(&self) -> Self {
        Self {
            http: self.http.clone(),
            inner: self.inner.clone(),
        }
    }
}

impl<Context: crate::Context> McpService<Context> {
    pub fn handle(&self, mut req: Request) -> BoxFuture<'static, Response> {
        let this = self.clone();
        async move {
            let (context, middleware) = match this.http.begin_http_request(&mut req).await {
                Ok(a) => a,
                Err(res) => return res,
            };
            let state = Arc::new(McpRequest {
                context,
                middleware: Mutex::new(middleware),
            });
            req.extensions_mut().insert(state.clone());
            let Ok::<_, Infallible>(res) = this.inner.clone().call(req).await;
            let mut res = res.map(Body::new);
            let mut middleware = state.middleware.lock().await;
            HttpServer::finish_http_response(&state.context, &mut middleware, &mut res).await;
            res
        }
        .boxed()
    }
}

impl<Context: crate::Context> Handler<(), ()> for McpService<Context> {
    type Future = BoxFuture<'static, Response>;
    fn call(self, req: Request, _: ()) -> Self::Future {
        self.handle(req)
    }
}
