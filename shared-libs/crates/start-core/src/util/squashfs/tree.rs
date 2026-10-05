use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

use super::blocks::invalid;
use super::format::{InodeKind, validate_block_size};
use crate::s9pk::merkle_archive::source::{DynFileSource, FileSource};

/// Unix inode attributes retained in the image, including permission and special bits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub mode: u16,
    pub uid: u32,
    pub gid: u32,
    pub modification_time: u32,
    pub xattrs: BTreeMap<OsString, Vec<u8>>,
}

impl Metadata {
    pub fn new(mode: u16) -> Self {
        Self {
            mode,
            uid: 0,
            gid: 0,
            modification_time: 0,
            xattrs: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Device {
    pub major: u32,
    pub minor: u32,
}

/// Hardlinks refer to another archive path and have no independent inode attributes.
#[derive(Debug, Clone)]
pub enum Entry<S> {
    Node(Node<S>),
    Hardlink(PathBuf),
}

impl<S> Entry<S> {
    pub fn new(metadata: Metadata, contents: NodeContents<S>) -> Self {
        Self::Node(Node { metadata, contents })
    }
    pub fn file(metadata: Metadata, source: S) -> Self {
        Self::new(metadata, NodeContents::File(source))
    }
    pub fn directory(metadata: Metadata, contents: DirectoryContents<S>) -> Self {
        Self::new(metadata, NodeContents::Directory(contents))
    }
    pub fn as_node(&self) -> Option<&Node<S>> {
        match self {
            Self::Node(node) => Some(node),
            Self::Hardlink(_) => None,
        }
    }
    pub fn as_node_mut(&mut self) -> Option<&mut Node<S>> {
        match self {
            Self::Node(node) => Some(node),
            Self::Hardlink(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Node<S> {
    pub metadata: Metadata,
    pub contents: NodeContents<S>,
}

#[derive(Debug, Clone)]
pub enum NodeContents<S> {
    File(S),
    Directory(DirectoryContents<S>),
    Symlink(PathBuf),
    BlockDevice(Device),
    CharacterDevice(Device),
    Fifo,
    Socket,
}

impl<S> NodeContents<S> {
    pub fn as_directory(&self) -> Option<&DirectoryContents<S>> {
        match self {
            Self::Directory(directory) => Some(directory),
            _ => None,
        }
    }
    pub fn as_directory_mut(&mut self) -> Option<&mut DirectoryContents<S>> {
        match self {
            Self::Directory(directory) => Some(directory),
            _ => None,
        }
    }
    pub(super) fn kind(&self) -> InodeKind {
        match self {
            Self::File(_) => InodeKind::File,
            Self::Directory(_) => InodeKind::Directory,
            Self::Symlink(_) => InodeKind::Symlink,
            Self::BlockDevice(_) => InodeKind::BlockDevice,
            Self::CharacterDevice(_) => InodeKind::CharacterDevice,
            Self::Fifo => InodeKind::Fifo,
            Self::Socket => InodeKind::Socket,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectoryContents<S> {
    entries: BTreeMap<OsString, Entry<S>>,
}

impl<S> Default for DirectoryContents<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> DirectoryContents<S> {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&OsString, &Entry<S>)> {
        self.entries.iter()
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&OsString, &mut Entry<S>)> {
        self.entries.iter_mut()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn insert(
        &mut self,
        name: impl Into<OsString>,
        entry: Entry<S>,
    ) -> std::io::Result<Option<Entry<S>>> {
        let name = name.into();
        validate_name(&name)?;
        Ok(self.entries.insert(name, entry))
    }

    pub fn get(&self, name: impl AsRef<OsStr>) -> Option<&Entry<S>> {
        self.entries.get(name.as_ref())
    }
    pub fn get_mut(&mut self, name: impl AsRef<OsStr>) -> Option<&mut Entry<S>> {
        self.entries.get_mut(name.as_ref())
    }

    pub fn get_path(&self, path: impl AsRef<Path>) -> Option<&Entry<S>> {
        let path = normalize_path(path.as_ref()).ok()?;
        let mut components = path.iter().peekable();
        let mut directory = self;
        loop {
            let name = components.next()?;
            let entry = directory.get(name)?;
            if components.peek().is_none() {
                return Some(entry);
            }
            directory = entry.as_node()?.contents.as_directory()?;
        }
    }

    /// Creates missing ancestors with root ownership, mode 0755, and zero timestamps.
    pub fn insert_path(
        &mut self,
        path: impl AsRef<Path>,
        entry: Entry<S>,
    ) -> std::io::Result<Option<Entry<S>>> {
        let path = normalize_path(path.as_ref())?;
        let name = path
            .file_name()
            .ok_or_else(|| invalid("cannot insert an entry at the archive root"))?;
        let parent = path.parent().expect("a named path has a parent");
        let mut directory = self;
        for component in parent.iter() {
            let entry = directory
                .entries
                .entry(component.to_os_string())
                .or_insert_with(|| {
                    Entry::directory(Metadata::new(0o755), DirectoryContents::new())
                });
            directory = entry
                .as_node_mut()
                .and_then(|n| n.contents.as_directory_mut())
                .ok_or_else(|| invalid("archive ancestor is not a directory"))?;
        }
        directory.insert(name, entry)
    }
}

pub(super) fn validate_name(name: &OsStr) -> std::io::Result<()> {
    let bytes = name.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 256
        || bytes == b"."
        || bytes == b".."
        || bytes.contains(&0)
        || bytes.contains(&b'/')
    {
        return Err(invalid("invalid SquashFS entry name"));
    }
    Ok(())
}

pub(super) fn normalize_path(path: &Path) -> std::io::Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => (),
            Component::Normal(name) => {
                validate_name(name)?;
                normalized.push(name);
            }
            _ => {
                return Err(invalid(
                    "archive paths must be relative and cannot contain parent traversal",
                ));
            }
        }
    }
    Ok(normalized)
}

/// Controls data compression and the filesystem creation timestamp, not inode timestamps.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    block_size: u32,
    compression_level: i32,
    pub modification_time: u32,
    pub deduplicate: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            block_size: 131072,
            compression_level: 15,
            modification_time: 0,
            deduplicate: true,
        }
    }
}

impl Options {
    pub fn new(block_size: u32, compression_level: i32) -> std::io::Result<Self> {
        validate_block_size(block_size)?;
        if !(1..=22).contains(&compression_level) {
            return Err(invalid("Zstd compression level must be between 1 and 22"));
        }
        Ok(Self {
            block_size,
            compression_level,
            ..Self::default()
        })
    }
    pub fn block_size(&self) -> u32 {
        self.block_size
    }
    pub fn compression_level(&self) -> i32 {
        self.compression_level
    }
}

#[derive(Debug, Clone)]
pub struct Squashfs<S> {
    root: Node<S>,
    pub options: Options,
}

impl<S> Squashfs<S> {
    pub fn new(metadata: Metadata, contents: DirectoryContents<S>) -> Self {
        Self {
            root: Node {
                metadata,
                contents: NodeContents::Directory(contents),
            },
            options: Options::default(),
        }
    }
    pub fn root_metadata(&self) -> &Metadata {
        &self.root.metadata
    }
    pub fn root_metadata_mut(&mut self) -> &mut Metadata {
        &mut self.root.metadata
    }
    pub fn contents(&self) -> &DirectoryContents<S> {
        self.root
            .contents
            .as_directory()
            .expect("root is a directory")
    }
    pub fn contents_mut(&mut self) -> &mut DirectoryContents<S> {
        self.root
            .contents
            .as_directory_mut()
            .expect("root is a directory")
    }
    pub(super) fn root(&self) -> &Node<S> {
        &self.root
    }
}

impl<S: FileSource> Squashfs<S> {
    /// Erases file-source types without reading file contents.
    pub fn into_dyn(self) -> Squashfs<DynFileSource> {
        Squashfs {
            root: self.root.into_dyn(),
            options: self.options,
        }
    }
}

impl<S: FileSource> Node<S> {
    fn into_dyn(self) -> Node<DynFileSource> {
        let contents = match self.contents {
            NodeContents::File(source) => NodeContents::File(DynFileSource::new(source)),
            NodeContents::Directory(directory) => NodeContents::Directory(DirectoryContents {
                entries: directory
                    .entries
                    .into_iter()
                    .map(|(name, entry)| {
                        (
                            name,
                            match entry {
                                Entry::Node(node) => Entry::Node(node.into_dyn()),
                                Entry::Hardlink(target) => Entry::Hardlink(target),
                            },
                        )
                    })
                    .collect(),
            }),
            NodeContents::Symlink(target) => NodeContents::Symlink(target),
            NodeContents::BlockDevice(device) => NodeContents::BlockDevice(device),
            NodeContents::CharacterDevice(device) => NodeContents::CharacterDevice(device),
            NodeContents::Fifo => NodeContents::Fifo,
            NodeContents::Socket => NodeContents::Socket,
        };
        Node {
            metadata: self.metadata,
            contents,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStringExt;

    use super::*;

    #[test]
    fn paths_are_raw_and_root_relative() {
        let mut dir = DirectoryContents::<&[u8]>::new();
        let path = PathBuf::from(OsString::from_vec(b"parent/nonutf8\xff".to_vec()));
        dir.insert_path(&path, Entry::file(Metadata::new(0o600), b"data"))
            .unwrap();
        assert!(dir.get_path(&path).is_some());
        assert!(dir.get_path(Path::new("./parent")).is_some());
        for bad in ["../escape", "a/../../escape", "/absolute", "name\0"] {
            assert!(
                dir.insert_path(bad, Entry::file(Metadata::new(0o600), b"bad"))
                    .is_err()
            );
        }
        assert!(
            dir.insert_path(
                path.join("child"),
                Entry::file(Metadata::new(0o600), b"bad")
            )
            .is_err()
        );
    }

    #[test]
    fn paths_do_not_follow_symlinks_or_hardlinks() {
        let mut dir = DirectoryContents::<()>::new();
        dir.insert(
            "link",
            Entry::new(Metadata::new(0o777), NodeContents::Symlink("target".into())),
        )
        .unwrap();
        dir.insert("hard", Entry::Hardlink("target".into()))
            .unwrap();
        for path in ["link/child", "hard/child"] {
            assert!(
                dir.insert_path(path, Entry::file(Metadata::new(0o600), ()))
                    .is_err()
            );
        }
    }

    #[test]
    fn name_and_options_limits() {
        let mut dir = DirectoryContents::<()>::new();
        for bad in ["", ".", "..", "a/b", "a\0b"] {
            assert!(
                dir.insert(bad, Entry::file(Metadata::new(0o644), ()))
                    .is_err()
            );
        }
        assert!(
            dir.insert("x".repeat(256), Entry::file(Metadata::new(0o644), ()))
                .is_ok()
        );
        assert!(
            dir.insert("x".repeat(257), Entry::file(Metadata::new(0o644), ()))
                .is_err()
        );
        for size in [0, 1024, 8193, 2097152] {
            assert!(Options::new(size, 3).is_err());
        }
        assert!(Options::new(4096, 1).is_ok());
        assert!(Options::new(1048576, 22).is_ok());
        assert!(Options::new(131072, 23).is_err());
    }
}
