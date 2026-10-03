#![recursion_limit = "512"]

use clap::Parser;
use rpc_toolkit::{from_fn, from_fn_async, Context, Empty, ParentHandler, Server};
#[cfg(feature = "ts")]
use rpc_toolkit::{impl_ts_shape, HandlerExt};
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

#[derive(Clone)]
struct TestContext;

impl Context for TestContext {}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[cfg_attr(feature = "ts", derive(visit_rs::SerdeShape))]
struct Thing1Params {
    thing: String,
}
#[cfg(feature = "ts")]
impl_ts_shape!(Thing1Params);

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
#[cfg_attr(feature = "ts", derive(visit_rs::SerdeShape))]
struct GroupParams {
    #[arg(short, long)]
    verbose: bool,
}
#[cfg(feature = "ts")]
impl_ts_shape!(GroupParams);

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
