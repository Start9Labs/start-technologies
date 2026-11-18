//! `startbox unshare-userns` — internal helper for
//! [`crate::disk::mount::filesystem::syscall::userns_fd_from_idmap`].
//!
//! Unshares the user namespace, prints `ready\n` on stdout so the parent
//! knows it can write `/proc/<pid>/uid_map` / `gid_map`, then blocks on
//! stdin until the parent closes it. The userns fd opened by the parent
//! keeps the namespace alive after this process exits.

use std::collections::VecDeque;
use std::ffi::OsString;

use crate::disk::mount::filesystem::syscall::unshare_userns_main;

pub fn main(args: VecDeque<OsString>) {
    let owner = match args
        .get(1)
        .map(|owner| owner.to_string_lossy().parse::<u32>())
    {
        None => None,
        Some(Ok(owner)) => Some(owner),
        Some(Err(e)) => {
            eprintln!("unshare-userns: owner: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = unshare_userns_main(owner) {
        eprintln!("unshare-userns: {e}");
        std::process::exit(1);
    }
}
