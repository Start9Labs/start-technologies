use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::PathBuf;

use crate::prelude::*;

pub(super) fn path(mut args: VecDeque<OsString>) -> Result<PathBuf, Error> {
    args.pop_front();
    match (args.pop_front(), args.pop_front()) {
        (Some(path), None) => Ok(path.into()),
        _ => Err(Error::new(
            eyre!(t!("error.invalid-request")),
            ErrorKind::InvalidRequest,
        )),
    }
}

pub(super) fn runtime() -> Result<tokio::runtime::Runtime, Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .with_kind(ErrorKind::Unknown)
}

pub(super) fn finish(result: Result<(), Error>) {
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
