use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use tokio_tar::{EntryType, Header};

pub(crate) fn append(tar: &mut Vec<u8>, path: &[u8], ty: u8, payload: &[u8], link: Option<&[u8]>) {
    let mut header = Header::new_gnu();
    header
        .set_path(Path::new(std::ffi::OsStr::from_bytes(path)))
        .unwrap();
    header.set_mode(0o640);
    header.set_uid(17);
    header.set_gid(23);
    header.set_mtime(42);
    header.set_size(payload.len() as u64);
    header.set_entry_type(EntryType::new(ty));
    header.set_device_major(0).unwrap();
    header.set_device_minor(0).unwrap();
    if let Some(link) = link {
        header
            .set_link_name(Path::new(std::ffi::OsStr::from_bytes(link)))
            .unwrap();
    }
    header.set_cksum();
    tar.extend_from_slice(header.as_bytes());
    tar.extend_from_slice(payload);
    tar.resize(tar.len().next_multiple_of(512), 0);
}
