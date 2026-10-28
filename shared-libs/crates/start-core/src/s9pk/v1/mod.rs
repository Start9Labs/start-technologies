use std::path::PathBuf;

use clap::Parser;
use serde::{Deserialize, Serialize};

pub mod builder;
pub mod docker;
pub mod header;
pub mod manifest;
pub mod reader;

pub const SIG_CONTEXT: &[u8] = b"s9pk";

#[derive(Deserialize, Serialize, Parser, visit_rs::VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct VerifyParams {
    #[arg(help = "help.arg.s9pk-file-path")]
    pub path: PathBuf,
}

rpc_toolkit::reflect_ts!(VerifyParams);
