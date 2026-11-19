use std::collections::VecDeque;
use std::ffi::OsString;

use super::native_tool::{finish, path, runtime};
use crate::os_overlay::public_source::SourceCache;

/// The parent owns current-signature admission and the inherited image-store lease.
pub fn assemble_source(args: VecDeque<OsString>) {
    finish(path(args).and_then(|directory| {
        use std::io::{Read, Write};

        let mut input = Vec::new();
        std::io::stdin().read_to_end(&mut input)?;
        let bytes =
            runtime()?.block_on(crate::fde::public_image::assemble_source(&directory, input))?;
        rpc_toolkit::output::stdout().write_all(&bytes)?;
        Ok(())
    }));
}

/// The caller retains the validated image-store lease until this internal tool exits.
pub fn prune_sources(args: VecDeque<OsString>) {
    finish(
        path(args)
            .and_then(|volume| runtime()?.block_on(SourceCache::new(volume).prune_boot_sources())),
    );
}
