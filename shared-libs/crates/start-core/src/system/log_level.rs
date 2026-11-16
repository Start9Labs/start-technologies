use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::context::RpcContext;
use crate::prelude::*;
use crate::util::logger::{LOGGER, LogLevel};

/// Sets verbosity for StartOS backend code in the running daemon.
#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetLogLevelParams {
    /// Applies to start-core/start-os until the daemon restarts.
    #[arg(value_enum, help = "help.arg.log-level")]
    pub level: LogLevel,
}

rpc_toolkit::reflect_ts!(SetLogLevelParams);

pub fn set_log_level(
    _: RpcContext,
    SetLogLevelParams { level }: SetLogLevelParams,
) -> Result<(), Error> {
    LOGGER.set_log_level(level).with_kind(ErrorKind::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_accepts_only_the_named_log_levels() {
        for (value, expected) in [
            ("error", LogLevel::Error),
            ("warn", LogLevel::Warn),
            ("info", LogLevel::Info),
            ("debug", LogLevel::Debug),
            ("trace", LogLevel::Trace),
        ] {
            let params: SetLogLevelParams =
                serde_json::from_value(serde_json::json!({"level":value})).unwrap();
            assert_eq!(params.level, expected);
        }
        for invalid in [
            serde_json::json!({}),
            serde_json::json!({"level":"off"}),
            serde_json::json!({"level":"TRACE"}),
            serde_json::json!({"level":"5"}),
            serde_json::json!({"level":true}),
            serde_json::json!({"level":null}),
        ] {
            assert!(serde_json::from_value::<SetLogLevelParams>(invalid).is_err());
        }
    }
}
