use std::collections::VecDeque;
use std::ffi::OsString;

use crate::{fdl_button, init_logging};

pub fn main(_args: VecDeque<OsString>) {
    init_logging("startwrt-fdl-reset");
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(fdl_button::watch());
    if let Err(e) = result {
        tracing::error!("FDL button: {e}");
        std::process::exit(1);
    }
}
