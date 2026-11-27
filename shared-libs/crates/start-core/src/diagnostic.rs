use std::sync::Arc;

use clap::Parser;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{
    CallRemoteHandler, Context, Empty, HandlerExt, ParentHandler, from_fn, from_fn_async,
};
use serde::{Deserialize, Serialize};

use crate::context::{CliContext, DiagnosticContext, RpcContext};
use crate::disk::repair;
use crate::mcp::{EFFECT, Effect};
use crate::prelude::*;
use crate::shutdown::Shutdown;
use crate::update::diagnostic as update;
use crate::util::io::delete_file;

/// Credential for a diagnostic command that changes the server.
#[derive(Default, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticPasswordParams {
    /// The server password. Unnecessary on the server itself, or before a password is set.
    #[arg(skip)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}
rpc_toolkit::reflect_ts!(DiagnosticPasswordParams);

/// Retries once with a password from the prompt after the server rejects the call for lacking one.
pub(crate) async fn call_with_password<T, Fut>(
    params: Value,
    prompt: impl FnOnce() -> Result<String, Error>,
    mut call: impl FnMut(Value) -> Fut,
) -> Result<T, Error>
where
    Fut: Future<Output = Result<T, Error>>,
{
    match call(params.clone()).await {
        Err(e) if e.kind == ErrorKind::IncorrectPassword => {
            let mut params = params;
            params[crate::middleware::auth::password::PASSWORD_PARAM] =
                Value::String(prompt()?.into());
            call(params).await
        }
        res => res,
    }
}

/// Reads the server password from `PASSWORD` or a hidden terminal prompt.
pub(crate) fn prompt_server_password() -> Result<String, Error> {
    crate::util::tui::read_password("PASSWORD", &t!("diagnostic.prompt-server-password"))
}

pub fn diagnostic<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "error",
            from_fn(error)
                .with_about("about.display-diagnostic-error")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "logs",
            crate::system::logs::<DiagnosticContext>(
                crate::rpc_continuations::WebSocketPolicy::Public,
            )
            .with_about("about.display-os-logs"),
        )
        .subcommand(
            "logs",
            from_fn_async(crate::logs::cli_logs::<DiagnosticContext, Empty>)
                .no_ts()
                .no_display()
                .with_about("about.display-os-logs"),
        )
        .subcommand(
            "kernel-logs",
            crate::system::kernel_logs::<DiagnosticContext>(
                crate::rpc_continuations::WebSocketPolicy::Public,
            )
            .with_about("about.display-kernel-logs"),
        )
        .subcommand(
            "kernel-logs",
            from_fn_async(crate::logs::cli_logs::<DiagnosticContext, Empty>)
                .no_ts()
                .no_display()
                .with_about("about.display-kernel-logs"),
        )
        .subcommand(
            "restart",
            from_fn(restart)
                .no_display()
                .with_about("about.restart-server")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "disk",
            disk::<C>().with_about("about.command-remove-disk-filesystem"),
        )
        .subcommand(
            "update",
            from_fn_async(update::update_system)
                .with_metadata("authenticated", Value::Bool(true))
                .with_metadata("get_local_auth", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "update",
            from_fn_async(update::cli_update_system)
                .no_display()
                .with_about("about.check-update-startos"),
        )
}

pub fn error(ctx: DiagnosticContext) -> Result<Arc<RpcError>, Error> {
    Ok(ctx.error.clone())
}

pub fn restart(ctx: DiagnosticContext) -> Result<(), Error> {
    ctx.shutdown
        .send(Shutdown {
            disk_guid: ctx.disk_guid.clone(),
            restart: true,
        })
        .map_err(|_| eyre!("receiver dropped"))
        .log_err();
    Ok(())
}

pub fn disk<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "forget",
            from_fn_async(|ctx: DiagnosticContext, _: DiagnosticPasswordParams| forget_disk(ctx))
                .with_metadata("authenticated", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "forget",
            from_fn_async(forget_disk::<RpcContext>)
                .with_metadata(EFFECT, Effect::Destructive.into())
                .no_cli(),
        )
        .subcommand(
            "forget",
            CallRemoteHandler::<CliContext, _, _>::new(
                from_fn_async(forget_disk::<RpcContext>).no_display(),
            )
            .no_display()
            .with_about("about.remove-disk-filesystem"),
        )
        .subcommand(
            "repair",
            from_fn_async(|_: DiagnosticContext, _: DiagnosticPasswordParams| repair())
                .with_metadata("authenticated", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "repair",
            from_fn_async(|_: RpcContext| repair())
                .with_metadata(EFFECT, Effect::Destructive.into())
                .no_cli(),
        )
        .subcommand(
            "repair",
            CallRemoteHandler::<CliContext, _, _>::new(
                from_fn_async(|_: RpcContext| repair()).no_display(),
            )
            .no_display()
            .with_about("about.repair-disk-corruption"),
        )
}

pub async fn forget_disk<C: Context>(_: C) -> Result<(), Error> {
    delete_file("/media/startos/config/overlay/etc/hostname").await?;
    crate::context::startup::forget_identity().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use imbl_value::json;
    use rpc_toolkit::HandlerFor;

    use super::*;
    use crate::context::{InitContext, SetupContext};
    use crate::net::static_server::UiContext;

    #[test]
    fn destructive_disk_commands_are_context_scoped() {
        let init = InitContext::api();
        let setup = SetupContext::api();
        let diagnostic = DiagnosticContext::api();
        let runtime = RpcContext::api();
        let cli = crate::main_api::<CliContext>();

        for (method, diagnostic_available) in [
            ("diagnostic.disk.forget", true),
            ("diagnostic.disk.repair", true),
            ("disk.repair", false),
        ] {
            assert!(init.method_from_dots(method).is_none(), "{method} in init");
            assert!(
                setup.method_from_dots(method).is_none(),
                "{method} in setup"
            );
            assert_eq!(
                diagnostic.method_from_dots(method).is_some(),
                diagnostic_available,
                "{method} in diagnostic",
            );
            if let Some(diagnostic_method) = diagnostic.method_from_dots(method) {
                assert_eq!(
                    diagnostic.metadata(diagnostic_method).get("authenticated"),
                    Some(&Value::Bool(true)),
                    "{method} must require diagnostic authentication",
                );
            }
            let runtime_method = runtime
                .method_from_dots(method)
                .unwrap_or_else(|| panic!("{method} missing from runtime"));
            assert_ne!(
                runtime.metadata(runtime_method).get("authenticated"),
                Some(&Value::Bool(false)),
                "{method} must require runtime authentication",
            );
            assert!(
                cli.method_from_dots(method).is_some(),
                "{method} missing from CLI",
            );
        }
    }

    #[tokio::test]
    async fn cli_prompts_once_after_a_password_rejection() {
        let rejected = || Error::new(eyre!("rejected"), ErrorKind::IncorrectPassword);
        let mut sent = Vec::new();
        let res = call_with_password(
            json!({ "x": 1 }),
            || Ok("pw".into()),
            |params| {
                sent.push(params.clone());
                let first = sent.len() == 1;
                async move { if first { Err(rejected()) } else { Ok(()) } }
            },
        )
        .await;
        res.unwrap();
        assert_eq!(
            sent,
            [json!({ "x": 1 }), json!({ "x": 1, "password": "pw" })]
        );

        for error in [ErrorKind::IncorrectPassword, ErrorKind::Filesystem] {
            let mut calls = 0;
            let mut prompted = false;
            let res: Result<(), Error> = call_with_password(
                json!({}),
                || {
                    prompted = true;
                    Ok("pw".into())
                },
                |_| {
                    calls += 1;
                    async move { Err(Error::new(eyre!("failed"), error)) }
                },
            )
            .await;
            assert_eq!(res.unwrap_err().kind, error);
            let rejected_for_password = error == ErrorKind::IncorrectPassword;
            assert_eq!(prompted, rejected_for_password);
            assert_eq!(calls, if rejected_for_password { 2 } else { 1 });
        }
    }

    #[test]
    fn diagnostic_mode_authenticates_changes_and_keeps_reads_public() {
        let diagnostic = DiagnosticContext::api();
        assert!(diagnostic.method_from_dots("diagnostic.rebuild").is_none());
        for (method, authenticated) in [
            ("diagnostic.error", false),
            ("diagnostic.logs", false),
            ("diagnostic.kernel-logs", false),
            ("diagnostic.restart", false),
            ("diagnostic.update", true),
            ("diagnostic.disk.forget", true),
            ("diagnostic.disk.repair", true),
        ] {
            let method_path = diagnostic
                .method_from_dots(method)
                .unwrap_or_else(|| panic!("{method} missing from diagnostic"));
            assert_eq!(
                diagnostic.metadata(method_path).get("authenticated"),
                Some(&Value::Bool(authenticated)),
                "{method}",
            );
        }
    }
}
