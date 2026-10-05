use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

use super::blocks::invalid;
use super::tree::{normalize_path, *};
use crate::prelude::*;

mod tar;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_extra;

fn checked_u32(value: u64) -> std::io::Result<u32> {
    value
        .try_into()
        .map_err(|_| invalid("inode metadata exceeds SquashFS u32 limit"))
}

fn unix_metadata(path: &Path, metadata: &std::fs::Metadata) -> std::io::Result<Metadata> {
    let mut xattrs = BTreeMap::new();
    for name in xattr::list(path)? {
        if let Some(value) = xattr::get(path, &name)? {
            xattrs.insert(name, value);
        }
    }
    Ok(Metadata {
        mode: (metadata.mode() & 0o7777) as u16,
        uid: metadata.uid(),
        gid: metadata.gid(),
        modification_time: metadata
            .mtime()
            .try_into()
            .map_err(|_| invalid("inode timestamp exceeds SquashFS u32 limit"))?,
        xattrs,
    })
}

impl Squashfs<PathBuf> {
    /// Retains source inode attributes and reads regular file contents lazily.
    pub async fn from_directory(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let metadata = tokio::fs::symlink_metadata(path).await?;
        if !metadata.is_dir() {
            return Err(invalid("SquashFS source root is not a directory").into());
        }
        let mut image = Self::new(unix_metadata(path, &metadata)?, DirectoryContents::new());
        let mut pending = vec![(path.to_owned(), PathBuf::new())];
        let mut hardlinks: BTreeMap<(u64, u64), PathBuf> = BTreeMap::new();
        while let Some((host, archive)) = pending.pop() {
            let mut entries = tokio::fs::read_dir(&host).await?;
            while let Some(entry) = entries.next_entry().await? {
                let host = entry.path();
                let archive = archive.join(entry.file_name());
                let stat = tokio::fs::symlink_metadata(&host).await?;
                let kind = stat.file_type();
                if !kind.is_dir() && stat.nlink() > 1 {
                    if let Some(target) = hardlinks.get(&(stat.dev(), stat.ino())) {
                        image
                            .contents_mut()
                            .insert_path(&archive, Entry::Hardlink(target.clone()))?;
                        continue;
                    }
                    hardlinks.insert((stat.dev(), stat.ino()), archive.clone());
                }
                let contents = if kind.is_dir() {
                    pending.push((host.clone(), archive.clone()));
                    NodeContents::Directory(DirectoryContents::new())
                } else if kind.is_file() {
                    NodeContents::File(host.clone())
                } else if kind.is_symlink() {
                    NodeContents::Symlink(tokio::fs::read_link(&host).await?)
                } else if kind.is_block_device() || kind.is_char_device() {
                    let device = Device {
                        major: checked_u32(libc::major(stat.rdev() as _) as u64)?,
                        minor: checked_u32(libc::minor(stat.rdev() as _) as u64)?,
                    };
                    if kind.is_block_device() {
                        NodeContents::BlockDevice(device)
                    } else {
                        NodeContents::CharacterDevice(device)
                    }
                } else if kind.is_fifo() {
                    NodeContents::Fifo
                } else if kind.is_socket() {
                    NodeContents::Socket
                } else {
                    return Err(invalid("unsupported source inode type").into());
                };
                image
                    .contents_mut()
                    .insert_path(archive, Entry::new(unix_metadata(&host, &stat)?, contents))?;
            }
        }
        Ok(image)
    }
}

fn raw_path(bytes: &[u8]) -> std::io::Result<PathBuf> {
    if bytes.contains(&0) {
        return Err(invalid("NUL in archive path"));
    }
    normalize_path(Path::new(&OsString::from_vec(bytes.to_vec())))
}

fn insert<S>(image: &mut Squashfs<S>, path: PathBuf, entry: Entry<S>) -> std::io::Result<()> {
    if path.as_os_str().is_empty() {
        let node = entry
            .as_node()
            .filter(|n| matches!(n.contents, NodeContents::Directory(_)))
            .ok_or_else(|| invalid("archive root must be a directory"))?;
        *image.root_metadata_mut() = node.metadata.clone();
        return Ok(());
    }
    let old = image.contents_mut().insert_path(&path, entry)?;
    if let Some(Entry::Node(Node {
        contents: NodeContents::Directory(children),
        ..
    })) = old
    {
        let new = image.contents_mut();
        let mut directory = new;
        let mut components = path.iter().peekable();
        while let Some(name) = components.next() {
            let node = directory
                .get_mut(name)
                .and_then(Entry::as_node_mut)
                .ok_or_else(|| invalid("cannot replace archive directory with a link"))?;
            directory = node
                .contents
                .as_directory_mut()
                .ok_or_else(|| invalid("cannot replace archive directory with a non-directory"))?;
            if components.peek().is_none() {
                *directory = children;
                break;
            }
        }
    }
    Ok(())
}
