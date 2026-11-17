use std::collections::VecDeque;
use std::ffi::OsString;

use super::native_tool::{finish, path, runtime};
use crate::os_overlay::public_source::SourceCache;

/// The caller retains the validated image-store lease until this internal tool exits.
pub fn prune_sources(args: VecDeque<OsString>) {
    finish(
        path(args)
            .and_then(|volume| runtime()?.block_on(SourceCache::new(volume).prune_boot_sources())),
    );
}
