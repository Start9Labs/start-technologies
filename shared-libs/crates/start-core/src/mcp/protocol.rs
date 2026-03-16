use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// JSON-RPC 2.0 error codes
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

pub const PROTOCOL_VERSION: &str = "2025-03-26";

// === JSON-RPC 2.0 envelope ===

#[derive(Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<JsonValue>,
    pub method: String,
    #[serde(default)]
    pub params: Option<JsonValue>,
}

#[derive(Serialize)]
pub struct McpResponse {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

impl McpResponse {
    pub fn ok(id: Option<JsonValue>, result: JsonValue) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<JsonValue>, code: i32, message: String, data: Option<JsonValue>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(McpError {
                code,
                message,
                data,
            }),
        }
    }
}

#[derive(Serialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
}

// === initialize ===

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    #[serde(default)]
    pub capabilities: JsonValue,
    #[serde(default)]
    pub client_info: Option<ClientInfo>,
}

#[derive(Deserialize)]
pub struct ClientInfo {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: &'static str,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
}

#[derive(Serialize)]
pub struct ServerCapabilities {
    pub tools: ToolsCapability,
    pub resources: ResourcesCapability,
}

#[derive(Serialize)]
pub struct ToolsCapability {}

#[derive(Serialize)]
pub struct ResourcesCapability {
    pub subscribe: bool,
}

#[derive(Serialize)]
pub struct ServerInfo {
    pub name: &'static str,
    pub version: String,
}

// === tools/list ===

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: JsonValue,
}

#[derive(Serialize)]
pub struct ToolsListResult {
    pub tools: Vec<ToolDefinition>,
}

// === tools/call ===

#[derive(Deserialize)]
pub struct ToolsCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: JsonValue,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCallResult {
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
}

// === resources/list ===

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDefinition {
    pub uri: String,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Serialize)]
pub struct ResourcesListResult {
    pub resources: Vec<ResourceDefinition>,
}

// === resources/read ===

#[derive(Deserialize)]
pub struct ResourcesReadParams {
    pub uri: String,
}

#[derive(Serialize)]
pub struct ResourcesReadResult {
    pub contents: Vec<ResourceContent>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceContent {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

// === resources/subscribe + unsubscribe ===

#[derive(Deserialize)]
pub struct ResourcesSubscribeParams {
    pub uri: String,
}

#[derive(Deserialize)]
pub struct ResourcesUnsubscribeParams {
    pub uri: String,
}

// === Server→client notification ===

#[derive(Serialize)]
pub struct McpNotification {
    pub jsonrpc: &'static str,
    pub method: &'static str,
    pub params: serde_json::Value,
}
