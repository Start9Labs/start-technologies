use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::PathBuf;

use crate::prelude::*;

fn arguments(mut args: VecDeque<OsString>) -> Result<(PathBuf, PathBuf), Error> {
    args.pop_front();
    let (Some(images), Some(config), None) = (args.pop_front(), args.pop_front(), args.pop_front())
    else {
        return Err(Error::new(
            eyre!(t!("error.invalid-request")),
            ErrorKind::InvalidRequest,
        ));
    };
    Ok((images.into(), config.into()))
}

pub fn main(args: VecDeque<OsString>) {
    let result = arguments(args).and_then(|(images, config)| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .with_kind(ErrorKind::Unknown)?
            .block_on(crate::fde::initramfs::prepare(&images, &config))
    });
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_applet_accepts_only_the_two_mount_paths() {
        for args in [
            vec![],
            vec!["start-fde-prepare"],
            vec!["start-fde-prepare", "/images"],
            vec!["start-fde-prepare", "/images", "/config", "--fallback"],
        ] {
            assert!(arguments(args.into_iter().map(OsString::from).collect()).is_err());
        }
        assert_eq!(
            arguments(
                ["start-fde-prepare", "/images", "/config"]
                    .into_iter()
                    .map(OsString::from)
                    .collect()
            )
            .unwrap(),
            (PathBuf::from("/images"), PathBuf::from("/config"))
        );
    }
}
