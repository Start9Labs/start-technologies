use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use visit_rs::VisitFields;
use zeroize::Zeroizing;

use crate::auth::PasswordType;
use crate::context::RpcContext;
use crate::prelude::*;

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct BeginEnableParams {
    password: PasswordType,
    acknowledged_offline: bool,
}
rpc_toolkit::reflect_ts!(BeginEnableParams);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct EnableStatus {
    pub supported_platform: bool,
    pub enable_requested: bool,
}
rpc_toolkit::reflect_ts!(EnableStatus);

pub(crate) fn supported_platform(platform: &str) -> bool {
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../projects/start-os/build/dpkg-deps/uefi.platforms"
    ))
    .lines()
    .any(|supported| supported == platform)
}

async fn status(ctx: RpcContext) -> Result<EnableStatus, Error> {
    Ok(EnableStatus {
        supported_platform: supported_platform(&crate::PLATFORM),
        enable_requested: ctx
            .db
            .peek()
            .await
            .as_private()
            .as_fde_enrollment()
            .transpose_ref()
            .is_some(),
    })
}

async fn begin_enable(
    ctx: RpcContext,
    params: BeginEnableParams,
) -> Result<super::enrollment::FdeEnrollmentProgress, Error> {
    let password = Zeroizing::new(params.password.decrypt(&ctx)?);
    super::enrollment::begin(ctx, password, params.acknowledged_offline).await
}

pub fn api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("status", from_fn_async(status).no_cli())
        .subcommand(
            "begin-enable",
            from_fn_async(begin_enable)
                .with_metadata("sync_db", Value::Bool(true))
                .with_metadata(crate::mcp::MCP, Value::Bool(false))
                .no_cli(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_admission_excludes_board_and_unknown_variants() {
        for platform in [
            "x86_64",
            "x86_64-nonfree",
            "x86_64-nvidia",
            "aarch64",
            "aarch64-nonfree",
            "aarch64-nvidia",
        ] {
            assert!(supported_platform(platform), "{platform}");
        }
        for platform in [
            "raspberrypi",
            "rockchip64",
            "riscv64",
            "riscv64-nonfree",
            "aarch64-unknown",
            "x86_64-unknown",
        ] {
            assert!(!supported_platform(platform), "{platform}");
        }
    }

    #[test]
    fn enable_request_requires_explicit_acknowledgment_field() {
        assert!(
            serde_json::from_value::<BeginEnableParams>(serde_json::json!({"password":"test"}))
                .is_err()
        );
        let request: BeginEnableParams = serde_json::from_value(
            serde_json::json!({"password":"test", "acknowledgedOffline":false}),
        )
        .unwrap();
        assert!(!request.acknowledged_offline);
    }
}
