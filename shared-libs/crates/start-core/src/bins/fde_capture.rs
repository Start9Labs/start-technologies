use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::PathBuf;

use super::native_tool::{finish, path as request_path, runtime};
use crate::prelude::*;

fn stop_arguments(mut args: VecDeque<OsString>) -> Result<(PathBuf, String), Error> {
    args.pop_front();
    match (args.pop_front(), args.pop_front(), args.pop_front()) {
        (Some(path), Some(unit), None) => Ok((
            path.into(),
            unit.into_string().map_err(|_| {
                Error::new(
                    eyre!(t!("error.invalid-request")),
                    ErrorKind::InvalidRequest,
                )
            })?,
        )),
        _ => Err(Error::new(
            eyre!(t!("error.invalid-request")),
            ErrorKind::InvalidRequest,
        )),
    }
}

pub fn capture(args: VecDeque<OsString>) {
    finish(
        request_path(args).and_then(|path| runtime()?.block_on(crate::fde::capture::run(&path))),
    );
}

pub fn stop(args: VecDeque<OsString>) {
    finish(
        stop_arguments(args)
            .and_then(|(path, unit)| runtime()?.block_on(crate::fde::capture::stop(&path, &unit))),
    );
}

pub fn release(args: VecDeque<OsString>) {
    finish(
        request_path(args)
            .and_then(|path| runtime()?.block_on(crate::fde::capture::release(&path))),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_applet_requires_the_native_request_and_unit() {
        for args in [
            vec!["start-fde-stop"],
            vec!["start-fde-stop", "/request"],
            vec!["start-fde-stop", "/request", "unit", "extra"],
        ] {
            assert!(stop_arguments(args.into_iter().map(OsString::from).collect()).is_err());
        }
        assert_eq!(
            stop_arguments(
                ["start-fde-stop", "/request", "unit"]
                    .into_iter()
                    .map(OsString::from)
                    .collect()
            )
            .unwrap(),
            (PathBuf::from("/request"), "unit".to_owned())
        );
    }

    #[test]
    fn capture_applets_accept_only_one_native_request_path() {
        for args in [
            vec![],
            vec!["start-fde-capture"],
            vec!["start-fde-capture", "/request", "--extra"],
        ] {
            assert!(request_path(args.into_iter().map(OsString::from).collect()).is_err());
        }
        assert_eq!(
            request_path(
                ["start-fde-capture", "/request"]
                    .into_iter()
                    .map(OsString::from)
                    .collect()
            )
            .unwrap(),
            PathBuf::from("/request")
        );
    }
}
