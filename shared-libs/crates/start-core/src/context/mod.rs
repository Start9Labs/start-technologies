pub mod cli;
pub mod config;
pub mod diagnostic;
pub mod init;
pub mod rpc;
pub mod setup;
pub(crate) mod startup;

pub use cli::CliContext;
pub use diagnostic::DiagnosticContext;
pub use init::InitContext;
pub use rpc::RpcContext;
pub use setup::SetupContext;
pub use startup::StartupIdentity;

use crate::prelude::*;

/// Calls the remote method a CLI-only handler fronts.
pub async fn call_fronted<Remote, T: serde::de::DeserializeOwned>(
    context: &impl rpc_toolkit::CallRemote<Remote>,
    parent_method: std::collections::VecDeque<&'static str>,
    method: std::collections::VecDeque<&'static str>,
    params: &impl serde::Serialize,
) -> Result<T, Error> {
    use itertools::Itertools;
    let method = parent_method.into_iter().chain(method).join(".");
    let result = context
        .call_remote(
            &method,
            imbl::OrdMap::new(),
            to_value(params)?,
            rpc_toolkit::Empty {},
        )
        .await
        .map_err(Error::from)
        .with_ctx(|e| (e.kind, &method))?;
    from_value(result)
}
