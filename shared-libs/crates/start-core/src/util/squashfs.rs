//! Native SquashFS v4 reading and writing with Zstd compression.
//!
//! [`Squashfs`] holds an editable inode tree whose regular files implement
//! [`crate::s9pk::merkle_archive::source::FileSource`]. Serialization reads file
//! data in bounded blocks; deserialization returns lazy archive-backed file
//! sources. Inode, block-index, and extended-attribute metadata remain resident
//! in memory.
//!
//! Directory imports retain inode attributes and read file contents lazily.
//! Imports skip xattr namespaces other than `user.`, `trusted.`, and `security.`.
//! Tar imports spool regular-file payloads into a flat temporary file rather
//! than extracting an archive tree. They accept GNU and PAX headers, hardlinks,
//! and GNU sparse formats, with a 16 MiB extension-metadata limit and a
//! one-million-extent sparse-map limit. Archive paths must be safe and relative.
//!
//! [`Options`] defaults to 128 KiB data blocks and Zstd level 15. Compression
//! formats other than Zstd are unsupported. Implicit directories and the
//! default root use root ownership, mode 0755, and zero timestamps. Permission
//! and timestamp changes belong to callers; imports do not modify source inodes.

mod blocks;
mod format;
mod read;
mod source;
mod tree;
mod write;

#[cfg(test)]
pub(crate) mod test_support;

pub use blocks::{MetadataBlocksReader, MetadataBlocksWriter};
pub use read::{SquashfsFileReader, SquashfsFileSource};
pub(crate) use source::LayerFilesystem;
pub(crate) use tree::resolve_hardlink;
pub use tree::{Device, DirectoryContents, Entry, Metadata, Node, NodeContents, Options, Squashfs};
