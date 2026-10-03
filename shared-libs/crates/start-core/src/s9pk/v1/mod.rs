use std::path::PathBuf;

use clap::Parser;
use serde::{Deserialize, Serialize};
use visit_rs::ts::TS;

pub mod builder;
pub mod docker;
pub mod header;
pub mod manifest;
pub mod reader;

pub const SIG_CONTEXT: &[u8] = b"s9pk";

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct VerifyParams {
    #[arg(help = "help.arg.s9pk-file-path")]
    pub path: PathBuf,
}
