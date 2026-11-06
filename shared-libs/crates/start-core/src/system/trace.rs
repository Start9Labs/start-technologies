use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::context::RpcContext;
use crate::prelude::*;
use crate::util::logger::LOGGER;

/// Controls the running daemon's process-wide trace override.
#[derive(Debug, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct SetTraceParams {
    /// Disabling restores the filter selected at daemon startup.
    #[arg(action = clap::ArgAction::Set, help = "help.arg.trace-enabled")]
    pub enabled: bool,
}

rpc_toolkit::reflect_ts!(SetTraceParams);

pub fn set_trace(_: RpcContext, SetTraceParams { enabled }: SetTraceParams) -> Result<(), Error> {
    LOGGER.set_trace(enabled).with_kind(ErrorKind::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_requires_a_boolean_trace_setting() {
        assert!(
            serde_json::from_str::<SetTraceParams>(r#"{"enabled":true}"#)
                .unwrap()
                .enabled
        );
        assert!(
            !serde_json::from_str::<SetTraceParams>(r#"{"enabled":false}"#)
                .unwrap()
                .enabled
        );
        for invalid in ["{}", r#"{"enabled":"true"}"#, r#"{"enabled":null}"#] {
            assert!(serde_json::from_str::<SetTraceParams>(invalid).is_err());
        }
    }
}
