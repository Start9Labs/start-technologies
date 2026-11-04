use axum::routing::post;
use axum::Router;
use rpc_toolkit::{from_fn_async, Context, HandlerArgs, HandlerExt, ParentHandler, Server};
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

struct ExampleContext;
impl Context for ExampleContext {}

#[derive(Serialize, Deserialize)]
struct Dump {
    revision: u64,
    value: u64,
}

#[derive(Serialize)]
struct Revision {
    revision: u64,
    increment: u64,
}

async fn watch(args: HandlerArgs<ExampleContext>) -> Result<Dump, RpcError> {
    if let Some(stream) = args.rpc_stream {
        stream.attach(futures::stream::iter([
            Ok::<_, RpcError>(Revision {
                revision: 1,
                increment: 1,
            }),
            Ok(Revision {
                revision: 2,
                increment: 1,
            }),
        ]))?;
    }
    Ok(Dump {
        revision: 0,
        value: 0,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tree = ParentHandler::new().subcommand("watch", from_fn_async(watch).no_cli().no_ts());
    let server = Server::new(|| async { Ok(ExampleContext) }, tree).for_http();
    let app = Router::new().route("/rpc", post(server.clone()).get(server.for_websocket()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
