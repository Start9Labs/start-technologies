#![cfg(feature = "mcp")]

#[cfg(feature = "ts")]
mod common;

use std::net::SocketAddr;

use axum::extract::Request;
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use http::StatusCode;
use imbl_value::Value;
use rpc_toolkit::mcp::rmcp::model::{Implementation, Tool, ToolAnnotations};
use rpc_toolkit::mcp::rmcp::transport::StreamableHttpServerConfig;
use rpc_toolkit::mcp::{McpServer, ToolPolicy, UnknownTool};
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{
    from_fn, from_fn_async, Context, Empty, HandlerExt, Middleware, ParentHandler, RpcRequest,
    RpcResponse, Server,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone)]
struct Ctx;
impl Context for Ctx {}

#[derive(Deserialize, Serialize)]
struct Greeting {
    name: String,
}
#[derive(Deserialize, Serialize)]
struct Repeat {
    times: usize,
}

fn tree() -> ParentHandler<Ctx> {
    ParentHandler::new()
        .subcommand(
            "greet",
            from_fn(|_: Ctx, p: Greeting| {
                Ok::<_, RpcError>(json!({ "greeting": format!("hello {}", p.name) }))
            })
            .with_metadata("readonly", Value::Bool(true))
            .no_cli()
            .no_ts(),
        )
        .subcommand(
            "echo",
            ParentHandler::<Ctx, Repeat>::new()
                .subcommand(
                    "line",
                    from_fn(|_: Ctx, p: rpc_toolkit::util::Flat<Greeting, Repeat>| {
                        Ok::<_, RpcError>(p.0.name.repeat(p.1.times))
                    })
                    .no_cli()
                    .no_ts(),
                )
                .no_cli()
                .no_ts(),
        )
        .subcommand(
            "fail",
            from_fn(|_: Ctx, _: Empty| {
                Err::<(), _>(RpcError {
                    code: 7,
                    message: "failed on purpose".into(),
                    data: None,
                })
            })
            .no_cli(),
        )
        .subcommand(
            "guarded",
            from_fn_async(|_: Ctx, _: Empty| async { Ok::<_, RpcError>(()) })
                .with_metadata("guarded", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "hidden",
            from_fn(|_: Ctx, _: Empty| Ok::<_, RpcError>(()))
                .with_metadata("mcp", Value::Bool(false))
                .no_cli(),
        )
}

fn tool(name: &str, input: serde_json::Value, output: Option<serde_json::Value>) -> Tool {
    let mut tool: Tool = serde_json::from_value(json!({
        "name": name,
        "inputSchema": input,
    }))
    .unwrap();
    tool.output_schema = output.map(|o| std::sync::Arc::new(o.as_object().unwrap().clone()));
    tool
}

fn catalog() -> Vec<Tool> {
    let object = json!({ "type": "object" });
    vec![
        tool(
            "greet",
            json!({ "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] }),
            Some(json!({ "type": "object", "properties": { "greeting": { "type": "string" } } })),
        ),
        tool("echo.line", object.clone(), None),
        tool("fail", object.clone(), None),
        tool("guarded", object.clone(), None),
        tool("hidden", object, None),
    ]
}

struct Policy;
impl ToolPolicy for Policy {
    fn expose(&self, _: &str, metadata: &imbl_value::imbl::OrdMap<&'static str, Value>) -> bool {
        metadata.get("mcp") != Some(&Value::Bool(false))
    }
    fn annotations(
        &self,
        _: &str,
        metadata: &imbl_value::imbl::OrdMap<&'static str, Value>,
    ) -> Option<ToolAnnotations> {
        (metadata.get("readonly") == Some(&Value::Bool(true)))
            .then(|| ToolAnnotations::new().read_only(true))
    }
}

#[derive(Clone, Default)]
struct Auth {
    user: Option<String>,
    rpc_finished: bool,
}
impl Middleware<Ctx> for Auth {
    type Metadata = Value;
    async fn process_http_request(
        &mut self,
        _: &Ctx,
        request: &mut Request,
    ) -> Result<(), Response> {
        match request
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
        {
            Some(token) => {
                self.user = Some(token.to_owned());
                Ok(())
            }
            None => Err(StatusCode::UNAUTHORIZED.into_response()),
        }
    }
    async fn process_rpc_request(
        &mut self,
        _: &Ctx,
        metadata: Value,
        request: &mut RpcRequest,
    ) -> Result<(), RpcResponse> {
        if metadata["guarded"] == Value::Bool(true) && self.user.as_deref() != Some("admin") {
            return Err(RpcResponse {
                id: request.id.clone(),
                result: Err(RpcError {
                    code: 34,
                    message: "forbidden".into(),
                    data: None,
                }),
            });
        }
        Ok(())
    }
    async fn process_rpc_response(&mut self, _: &Ctx, _: &mut RpcResponse) {
        self.rpc_finished = true;
    }
    async fn process_http_response(&mut self, _: &Ctx, response: &mut Response) {
        response.headers_mut().insert(
            "x-rpc-finished",
            if self.rpc_finished { "true" } else { "false" }
                .parse()
                .unwrap(),
        );
        if let Some(user) = &self.user {
            response
                .headers_mut()
                .insert("x-user", user.parse().unwrap());
        }
    }
}

async fn serve() -> SocketAddr {
    serve_with_config(
        StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true),
    )
    .await
}

async fn serve_with_config(config: StreamableHttpServerConfig) -> SocketAddr {
    let mcp = McpServer::new(
        Server::new(|| async { Ok(Ctx) }, tree()).middleware(Auth::default()),
        catalog(),
        Policy,
        Implementation::new("test", "0.0.0"),
    )
    .unwrap()
    .service(config);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/mcp", any(mcp)))
            .await
            .unwrap()
    });
    addr
}

async fn body(response: reqwest::Response) -> serde_json::Value {
    serde_json::from_slice(&response.bytes().await.unwrap()).unwrap()
}

struct Client {
    addr: SocketAddr,
    token: Option<&'static str>,
    http: reqwest::Client,
}
impl Client {
    async fn send(&self, method: &str, mut params: serde_json::Value) -> reqwest::Response {
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {},
            "io.modelcontextprotocol/clientInfo": { "name": "test", "version": "0.0.0" },
        });
        let mut request = self
            .http
            .post(format!("http://{}/mcp", self.addr))
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2026-07-28")
            .header("mcp-method", method)
            .header("content-type", "application/json")
            .body(
                serde_json::to_vec(
                    &json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }),
                )
                .unwrap(),
            );
        if let Some(name) = params["name"].as_str() {
            request = request.header("mcp-name", name);
        }
        if let Some(token) = self.token {
            request = request.bearer_auth(token);
        }
        request.send().await.unwrap()
    }
    async fn call(&self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let response = self.send(method, params).await;
        assert_eq!(response.status(), StatusCode::OK);
        body(response).await
    }
}

#[tokio::test]
async fn tools_follow_the_catalog_and_policy() {
    let client = Client {
        addr: serve().await,
        token: Some("user"),
        http: reqwest::Client::new(),
    };
    let listed = client.call("tools/list", json!({})).await;
    let tools = listed["result"]["tools"].as_array().unwrap();
    let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["echo.line", "fail", "greet", "guarded"]);
    let greet = tools.iter().find(|t| t["name"] == "greet").unwrap();
    assert_eq!(greet["annotations"]["readOnlyHint"], true);
    assert!(tools.iter().find(|t| t["name"] == "fail").unwrap()["annotations"].is_null());
}

#[tokio::test]
async fn tool_calls_dispatch_through_the_middleware() {
    let client = Client {
        addr: serve().await,
        token: Some("user"),
        http: reqwest::Client::new(),
    };
    let response = client
        .send(
            "tools/call",
            json!({ "name": "greet", "arguments": { "name": "mcp" } }),
        )
        .await;
    assert_eq!(response.headers()["x-user"], "user");
    let greet = body(response).await;
    assert_eq!(
        greet["result"]["structuredContent"],
        json!({ "greeting": "hello mcp" })
    );
    assert_eq!(
        greet["result"]["content"][0]["text"],
        r#"{"greeting":"hello mcp"}"#
    );
    assert_ne!(greet["result"]["isError"], true);

    let echo = client
        .call(
            "tools/call",
            json!({ "name": "echo.line", "arguments": { "name": "ab", "times": 3 } }),
        )
        .await;
    assert_eq!(echo["result"]["content"][0]["text"], r#""ababab""#);
    assert!(echo["result"]["structuredContent"].is_null());

    let fail = client
        .call("tools/call", json!({ "name": "fail", "arguments": {} }))
        .await;
    assert_eq!(fail["result"]["isError"], true);
    let error: serde_json::Value =
        serde_json::from_str(fail["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(error["code"], 7);

    let guarded = client
        .call("tools/call", json!({ "name": "guarded", "arguments": {} }))
        .await;
    assert_eq!(guarded["result"]["isError"], true);
    let admin = Client {
        token: Some("admin"),
        ..client
    };
    let allowed = admin
        .call("tools/call", json!({ "name": "guarded", "arguments": {} }))
        .await;
    assert_ne!(allowed["result"]["isError"], true, "{}", allowed);

    for name in ["hidden", "missing"] {
        let unknown = admin
            .call("tools/call", json!({ "name": name, "arguments": {} }))
            .await;
        assert!(unknown["error"].is_object(), "{}", unknown);
    }
}

#[tokio::test]
async fn http_middleware_rejects_before_mcp() {
    let client = Client {
        addr: serve().await,
        token: None,
        http: reqwest::Client::new(),
    };
    let response = client.send("tools/list", json!({})).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn catalog_drift_is_an_error() {
    let mut tools = catalog();
    tools.push(tool("nowhere.method", json!({ "type": "object" }), None));
    let result = McpServer::new(
        Server::new(|| async { Ok(Ctx) }, tree()).for_http(),
        tools,
        Policy,
        Implementation::new("test", "0.0.0"),
    );
    assert!(matches!(result, Err(UnknownTool(name)) if name == "nowhere.method"));
}

#[tokio::test]
async fn legacy_tool_calls_finish_before_http_response_middleware() {
    let addr =
        serve_with_config(StreamableHttpServerConfig::default().with_legacy_session_mode(false))
            .await;
    let response = reqwest::Client::new()
        .post(format!("http://{}/mcp", addr))
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "2025-06-18")
        .bearer_auth("user")
        .body(
            serde_json::to_vec(&json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": "greet", "arguments": { "name": "legacy" } },
            }))
            .unwrap(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["x-rpc-finished"], "true");
}

#[tokio::test]
async fn legacy_clients_initialize_then_call() {
    let addr = serve_with_config(StreamableHttpServerConfig::default()).await;
    let http = reqwest::Client::new();
    let post = |version: Option<&'static str>, body: serde_json::Value| {
        let mut request = http
            .post(format!("http://{}/mcp", addr))
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .bearer_auth("user")
            .body(serde_json::to_vec(&body).unwrap());
        if let Some(version) = version {
            request = request.header("mcp-protocol-version", version);
        }
        request.send()
    };
    let initialized = post(
        None,
        json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "legacy", "version": "0.0.0" },
        } }),
    )
    .await
    .unwrap();
    assert_eq!(initialized.status(), StatusCode::OK);
    let initialized = body(initialized).await;
    assert_eq!(
        initialized["result"]["protocolVersion"], "2025-06-18",
        "{}",
        initialized
    );
    assert!(initialized["result"]["capabilities"]["tools"].is_object());
    let called = body(
        post(
            Some("2025-06-18"),
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
                "name": "greet",
                "arguments": { "name": "legacy" },
            } }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(
        called["result"]["structuredContent"]["greeting"], "hello legacy",
        "{}",
        called
    );
}

#[cfg(feature = "ts")]
mod catalog {
    use rpc_toolkit::mcp::ExposeAll;
    use rpc_toolkit::reflect_ts;
    use rpc_toolkit::ts::handler_bindings;

    use super::*;

    /// Two numbers to add.
    #[derive(Deserialize, Serialize, visit_rs::VisitFields)]
    struct Sum {
        a: u32,
        b: u32,
    }
    reflect_ts!(Sum);
    #[derive(Deserialize, Serialize, visit_rs::VisitFields)]
    struct Total {
        total: u32,
    }
    reflect_ts!(Total);
    #[derive(Deserialize, Serialize, visit_rs::VisitFields)]
    struct Scope {
        scale: u32,
    }
    reflect_ts!(Scope);

    fn tree() -> ParentHandler<Ctx> {
        ParentHandler::new()
            .subcommand(
                "math",
                ParentHandler::<Ctx, Scope>::new()
                    .subcommand(
                        "sum",
                        from_fn(|_: Ctx, p: Sum| Ok::<_, RpcError>(Total { total: p.a + p.b }))
                            .no_cli(),
                    )
                    .subcommand(
                        "label",
                        from_fn(|_: Ctx, _: Empty| Ok::<_, RpcError>("sum".to_owned())).no_cli(),
                    )
                    .no_cli(),
            )
            .subcommand(
                "opaque",
                from_fn(|_: Ctx, _: Empty| Ok::<_, RpcError>(()))
                    .no_cli()
                    .no_ts(),
            )
            .subcommand(
                "mixed",
                ParentHandler::<Ctx>::new()
                    .root_handler(
                        from_fn(|_: Ctx, _: Empty| Ok::<_, RpcError>(()))
                            .no_cli()
                            .no_ts(),
                    )
                    .subcommand(
                        "bound",
                        from_fn(|_: Ctx, _: Empty| Ok::<_, RpcError>("bound".to_owned())).no_cli(),
                    )
                    .no_cli(),
            )
    }

    fn generated_catalog() -> Vec<Tool> {
        let bindings = handler_bindings(&tree(), "Api", common::SCHEMAS)
            .unwrap()
            .unwrap();
        let json = common::evaluate(
            &bindings,
            "process.stdout.write(JSON.stringify(schemas.toolCatalog(schemas.Api)));",
        );
        serde_json::from_str(&json).unwrap()
    }

    fn required(schema: &serde_json::Value, root: &serde_json::Value) -> Vec<String> {
        if let Some(name) = schema["$ref"].as_str() {
            return required(&root["$defs"][name.trim_start_matches("#/$defs/")], root);
        }
        let mut keys: Vec<String> = schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|k| k.as_str().unwrap().to_owned())
            .collect();
        for part in schema["allOf"].as_array().into_iter().flatten() {
            keys.extend(required(part, root));
        }
        keys.sort();
        keys
    }

    #[test]
    fn generated_catalog_describes_callable_methods_with_bindings() {
        let tools = generated_catalog();
        let names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
        assert_eq!(names, ["math.label", "math.sum", "mixed.bound"]);
        let sum = serde_json::to_value(&tools[1]).unwrap();
        assert_eq!(sum["inputSchema"]["type"], "object");
        assert_eq!(
            required(&sum["inputSchema"], &sum["inputSchema"]),
            ["a", "b", "scale"],
            "{}",
            sum
        );
        assert_eq!(sum["outputSchema"]["type"], "object");
        assert_eq!(sum["outputSchema"]["required"], json!(["total"]), "{}", sum);
        assert!(tools[0].output_schema.is_none());
    }

    #[test]
    fn opted_out_callable_parent_preserves_type_projection() {
        let bindings = handler_bindings(&tree(), "Api", common::SCHEMAS)
            .unwrap()
            .unwrap();
        common::typecheck(
            &bindings,
            r#"
const result: unknown extends RpcReturnType<Api, 'mixed'> ? true : false = true;
const marker: '_OPAQUE' extends keyof Api['_CHILDREN']['mixed'] ? true : false = false;
"#,
        );
    }

    #[tokio::test]
    async fn generated_catalog_serves_tool_calls() {
        let mcp = McpServer::new(
            Server::new(|| async { Ok(Ctx) }, tree()).middleware(Auth::default()),
            generated_catalog(),
            ExposeAll,
            Implementation::new("test", "0.0.0"),
        )
        .unwrap()
        .service(StreamableHttpServerConfig::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/mcp", any(mcp)))
                .await
                .unwrap()
        });
        let client = Client {
            addr,
            token: Some("user"),
            http: reqwest::Client::new(),
        };
        let sum = client
            .call(
                "tools/call",
                json!({ "name": "math.sum", "arguments": { "a": 1, "b": 2, "scale": 3 } }),
            )
            .await;
        assert_eq!(
            sum["result"]["structuredContent"],
            json!({ "total": 3 }),
            "{}",
            sum
        );
        let missing = client
            .call(
                "tools/call",
                json!({ "name": "math.sum", "arguments": { "a": 1, "b": 2 } }),
            )
            .await;
        assert_eq!(missing["result"]["isError"], true, "{}", missing);
    }
}
