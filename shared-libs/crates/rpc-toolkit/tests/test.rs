#![recursion_limit = "512"]

use clap::Parser;
use rpc_toolkit::{from_fn, from_fn_async, Context, Empty, ParentHandler, Server};
#[cfg(feature = "ts")]
use rpc_toolkit::{reflect_ts, HandlerExt};
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

#[derive(Clone)]
struct TestContext;

impl Context for TestContext {}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[cfg_attr(feature = "ts", derive(visit_rs::VisitFields))]
struct Thing1Params {
    thing: String,
}
#[cfg(feature = "ts")]
reflect_ts!(Thing1Params);

#[derive(Debug, Deserialize, Serialize, Parser)]
struct NoTSParams {
    foo: String,
}

async fn thing1_handler(_ctx: TestContext, params: Thing1Params) -> Result<String, RpcError> {
    Ok(format!("Thing1 is {}", params.thing))
}

fn no_ts_handler(_ctx: TestContext, params: NoTSParams) -> Result<String, RpcError> {
    Ok(format!("foo:{}", params.foo))
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[cfg_attr(feature = "ts", derive(visit_rs::VisitFields))]
struct GroupParams {
    #[arg(short, long)]
    verbose: bool,
}
#[cfg(feature = "ts")]
reflect_ts!(GroupParams);

#[tokio::test]
async fn test_basic_server() {
    let no_ts = from_fn(no_ts_handler);
    #[cfg(feature = "ts")]
    let no_ts = no_ts.no_ts();
    let root_handler = ParentHandler::new()
        .subcommand("thing1", from_fn_async(thing1_handler))
        .subcommand(
            "group",
            ParentHandler::<TestContext, Empty, Empty>::new()
                .subcommand("thing1", from_fn_async(thing1_handler))
                .subcommand(
                    "thing2",
                    from_fn_async(|_ctx: TestContext, params: GroupParams| async move {
                        Ok::<_, RpcError>(format!("verbose: {}", params.verbose))
                    }),
                )
                .subcommand("no-ts", no_ts),
        );

    let server = Server::new(|| async { Ok(TestContext) }, root_handler);

    let result = server
        .handle_command(
            "thing1",
            imbl_value::to_value(&Thing1Params {
                thing: "test".to_string(),
            })
            .unwrap(),
        )
        .await
        .unwrap();

    let response: String = imbl_value::from_value(result).unwrap();
    assert_eq!(response, "Thing1 is test");

    let result = server
        .handle_command(
            "group.thing1",
            imbl_value::to_value(&Thing1Params {
                thing: "nested".to_string(),
            })
            .unwrap(),
        )
        .await
        .unwrap();

    let response: String = imbl_value::from_value(result).unwrap();
    assert_eq!(response, "Thing1 is nested");
}

#[derive(Clone)]
struct ProxyContext;

impl Context for ProxyContext {}

impl rpc_toolkit::CallRemote<TestContext> for ProxyContext {
    async fn call_remote(
        &self,
        _: &str,
        metadata: imbl_value::imbl::OrdMap<&'static str, imbl_value::Value>,
        _: imbl_value::Value,
        _: Empty,
    ) -> Result<imbl_value::Value, RpcError> {
        Ok(metadata["authenticated"].clone())
    }
}

#[tokio::test]
async fn call_remote_metadata_overrides_reach_the_calling_server_alone() {
    use imbl_value::Value;
    use rpc_toolkit::HandlerFor;

    let proxy = rpc_toolkit::CallRemoteHandler::<ProxyContext, TestContext, _>::new(
        from_fn(|_: TestContext, _: Empty| Ok::<_, RpcError>(Value::Null))
            .with_metadata("authenticated", Value::Bool(false))
            .with_metadata("mcp", Value::Bool(false)),
    )
    .with_metadata("authenticated", Value::Bool(true));
    #[cfg(feature = "ts")]
    let proxy = proxy.no_ts();
    let root = ParentHandler::<ProxyContext>::new().subcommand("remote", proxy);

    let metadata = root.metadata(vec!["remote"].into());
    assert_eq!(metadata.get("authenticated"), Some(&Value::Bool(true)));
    assert_eq!(metadata.get("mcp"), Some(&Value::Bool(false)));

    assert_eq!(
        Server::new(|| async { Ok(ProxyContext) }, root)
            .handle_command("remote", Value::Object(Default::default()))
            .await
            .unwrap(),
        Value::Bool(false)
    );
}
