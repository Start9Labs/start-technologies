use rpc_toolkit::ts::handler_bindings;
use rpc_toolkit::{from_fn_async, impl_ts_shape, Context, Empty, HandlerExt, ParentHandler};
use serde::{Deserialize, Serialize};
use visit_rs::SerdeShape;
use yajrc::RpcError;

#[derive(Clone)]
struct Ctx;
impl Context for Ctx {}

#[derive(Serialize, Deserialize, SerdeShape)]
#[serde(rename_all = "camelCase")]
struct CreateUser {
    user_name: String,
    age: u32,
    email: Option<String>,
}
impl_ts_shape!(CreateUser);

#[derive(Serialize, Deserialize, SerdeShape)]
#[serde(tag = "status", content = "data")]
enum JobResult {
    Pending,
    Done { id: u64 },
    Failed(String),
}
impl_ts_shape!(JobResult);

async fn create_user(_: Ctx, p: CreateUser) -> Result<u64, RpcError> {
    Ok(p.age as u64)
}
async fn job(_: Ctx, _: Empty) -> Result<JobResult, RpcError> {
    Ok(JobResult::Pending)
}
async fn jobs_root(_: Ctx, _: Empty) -> Result<JobResult, RpcError> {
    Ok(JobResult::Pending)
}

fn main() {
    let root = ParentHandler::<Ctx, Empty, Empty>::new()
        .subcommand("createUser", from_fn_async(create_user).no_cli())
        .subcommand(
            "jobs",
            ParentHandler::<Ctx, Empty, Empty>::new()
                .root_handler(from_fn_async(jobs_root).no_cli())
                .subcommand("run", from_fn_async(job).no_cli()),
        );

    match handler_bindings(&root, "Api").unwrap() {
        Some(module) => print!("{module}"),
        None => eprintln!("root handler opted out of TS"),
    }
}
