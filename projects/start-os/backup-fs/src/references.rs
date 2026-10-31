use crate::inode::{Attributes, FileData};
use crate::seglog::RecordKey;

/// The indexed storage an inode's current attributes refer to.
#[derive(Clone, Copy)]
pub(crate) enum StorageReferences {
    None,
    Blocks {
        content: u64,
        count: u64,
    },
    Packed(u64),
    Buckets {
        inode: u64,
        generation: u64,
        count: u32,
    },
}

impl StorageReferences {
    pub fn for_inode(inode: u64, attrs: &Attributes) -> Self {
        match &attrs.contents {
            FileData::File(content) => Self::Blocks {
                content: content.0,
                count: crate::blockstore::block_count(attrs.size),
            },
            FileData::Packed(content) => Self::Packed(content.0),
            FileData::Directory(directory) => {
                let (generation, count) = directory.bucket_geometry();
                Self::Buckets {
                    inode,
                    generation,
                    count,
                }
            }
            _ => Self::None,
        }
    }

    pub fn packed(self) -> Option<u64> {
        match self {
            Self::Packed(id) => Some(id),
            _ => None,
        }
    }

    /// Visits every block and directory-bucket key, present or not.
    pub fn visit(self, mut visit: impl FnMut(RecordKey)) {
        match self {
            Self::Blocks { content, count } => {
                for index in 0..count {
                    visit(RecordKey::Block(content, index));
                }
            }
            Self::Buckets {
                inode,
                generation,
                count,
            } => {
                for index in 0..count {
                    visit(RecordKey::Bucket(inode, generation, index));
                }
            }
            _ => {}
        }
    }
}
