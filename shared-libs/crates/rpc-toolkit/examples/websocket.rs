use axum::routing::post;
use axum::Router;
use rpc_toolkit::{from_fn_async, Context, HandlerArgs, HandlerExt, ParentHandler, Server};
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

struct ExampleContext;
impl Context for ExampleContext {}

#[derive(Default, Serialize, Deserialize)]
struct Dump {
    revision: u64,
    value: u64,
}

#[derive(Serialize)]
struct Revision {
    revision: u64,
    increment: u64,
}

const FINAL_RESULT: &str = "finished";

fn revisions() -> impl Iterator<Item = Revision> {
    (1..=2).map(|revision| Revision {
        revision,
        increment: 1,
    })
}

async fn watch(
    args: HandlerArgs<
        ExampleContext,
        rpc_toolkit::Empty,
        rpc_toolkit::Empty,
        Revision,
        &'static str,
    >,
) -> Result<Dump, RpcError> {
    if let Some(producer) = args.rpc_stream {
        tokio::spawn(async move {
            for revision in revisions() {
                if producer.send(revision).await.is_err() {
                    return;
                }
            }
            let _ = producer.close(Ok::<_, RpcError>(FINAL_RESULT));
        });
    }
    Ok(Dump::default())
}

async fn watch_thread(
    args: HandlerArgs<
        ExampleContext,
        rpc_toolkit::Empty,
        rpc_toolkit::Empty,
        Revision,
        &'static str,
    >,
) -> Result<Dump, RpcError> {
    if let Some(producer) = args.rpc_stream {
        std::thread::spawn(move || {
            for revision in revisions() {
                if producer.blocking_send(revision).is_err() {
                    return;
                }
            }
            let _ = producer.close(Ok::<_, RpcError>(FINAL_RESULT));
        });
    }
    Ok(Dump::default())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tree = ParentHandler::new()
        .subcommand("watch", from_fn_async(watch).no_cli().no_ts())
        .subcommand("watch-thread", from_fn_async(watch_thread).no_cli().no_ts());
    let server = Server::new(|| async { Ok(ExampleContext) }, tree).for_http();
    let app = Router::new().route("/rpc", post(server.clone()).get(server.for_websocket()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
