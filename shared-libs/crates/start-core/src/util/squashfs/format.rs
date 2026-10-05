use std::io::{self, Read, Write};

use super::blocks::invalid;

pub(super) const MAGIC: u32 = 0x73717368;
pub(super) const INVALID_TABLE: u64 = u64::MAX;
pub(super) const INVALID_INDEX: u32 = u32::MAX;
pub(super) const ALWAYS_FRAGMENTS: u16 = 1 << 5;
pub(super) const EXPORTABLE: u16 = 1 << 7;
pub(super) const DEDUPLICATED: u16 = 1 << 6;
pub(super) const NO_XATTRS: u16 = 1 << 9;

pub(super) const XATTR_PREFIXES: [&[u8]; 3] = [b"user.", b"trusted.", b"security."];

pub(super) fn xattr_kind(name: &[u8]) -> Option<(u16, &[u8])> {
    XATTR_PREFIXES
        .iter()
        .zip(0..)
        .find_map(|(prefix, kind)| name.strip_prefix(*prefix).map(|name| (kind, name)))
}

pub(super) trait Disk: Sized {
    const SIZE: usize;
    fn serialize(&self, writer: &mut impl Write) -> io::Result<()>;
    fn deserialize(reader: &mut impl Read) -> io::Result<Self>;
}

macro_rules! disk_struct {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub(super) struct $name {
            $(pub $field: $ty),*
        }
        impl Disk for $name {
            const SIZE: usize = $(std::mem::size_of::<$ty>() +)* 0;

            fn serialize(&self, writer: &mut impl Write) -> io::Result<()> {
                $(writer.write_all(&self.$field.to_le_bytes())?;)*
                Ok(())
            }

            fn deserialize(reader: &mut impl Read) -> io::Result<Self> {
                Ok(Self {
                    $($field: {
                        let mut bytes = [0; std::mem::size_of::<$ty>()];
                        reader.read_exact(&mut bytes)?;
                        <$ty>::from_le_bytes(bytes)
                    }),*
                })
            }
        }
    };
}

disk_struct!(Superblock {
    magic: u32,
    inode_count: u32,
    modification_time: u32,
    block_size: u32,
    fragment_count: u32,
    compression: u16,
    block_log: u16,
    flags: u16,
    id_count: u16,
    version_major: u16,
    version_minor: u16,
    root_inode: u64,
    bytes_used: u64,
    id_table: u64,
    xattr_id_table: u64,
    inode_table: u64,
    directory_table: u64,
    fragment_table: u64,
    export_table: u64,
});

impl Superblock {
    pub fn new(block_size: u32, modification_time: u32) -> Self {
        Self {
            magic: MAGIC,
            inode_count: 0,
            modification_time,
            block_size,
            fragment_count: 0,
            compression: 6,
            block_log: block_size.ilog2() as u16,
            flags: EXPORTABLE | ALWAYS_FRAGMENTS,
            id_count: 0,
            version_major: 4,
            version_minor: 0,
            root_inode: 0,
            bytes_used: 0,
            id_table: INVALID_TABLE,
            xattr_id_table: INVALID_TABLE,
            inode_table: INVALID_TABLE,
            directory_table: INVALID_TABLE,
            fragment_table: INVALID_TABLE,
            export_table: INVALID_TABLE,
        }
    }

    pub fn validate(&self) -> io::Result<()> {
        if self.magic != MAGIC || (self.version_major, self.version_minor) != (4, 0) {
            return Err(invalid("not a SquashFS version 4 image"));
        }
        if self.compression != 6 {
            return Err(invalid("unsupported SquashFS compression (expected Zstd)"));
        }
        validate_block_size(self.block_size)?;
        if u32::from(self.block_log) != self.block_size.ilog2()
            || self.bytes_used < Self::SIZE as u64
            || self.inode_count == 0
            || self.id_count == 0
            || self.root_inode & 0xffff >= 8192
        {
            return Err(invalid("invalid SquashFS superblock"));
        }
        for table in [self.id_table, self.inode_table, self.directory_table] {
            if table < Self::SIZE as u64 || table >= self.bytes_used {
                return Err(invalid("SquashFS table lies outside image"));
            }
        }
        for table in [self.fragment_table, self.export_table, self.xattr_id_table] {
            if table != INVALID_TABLE && (table < Self::SIZE as u64 || table >= self.bytes_used) {
                return Err(invalid("SquashFS optional table lies outside image"));
            }
        }
        if self.inode_table >= self.directory_table
            || self.directory_table > self.id_table
            || (self.fragment_count != 0 && self.fragment_table == INVALID_TABLE)
        {
            return Err(invalid("invalid SquashFS table ordering"));
        }
        Ok(())
    }
}

pub(super) fn validate_block_size(size: u32) -> io::Result<()> {
    if !size.is_power_of_two() || !(4096..=1048576).contains(&size) {
        return Err(invalid(
            "SquashFS block size must be a power of two from 4 KiB through 1 MiB",
        ));
    }
    Ok(())
}

disk_struct!(InodeHeader {
    kind: u16,
    mode: u16,
    uid: u16,
    gid: u16,
    modification_time: u32,
    number: u32,
});

disk_struct!(BasicDirectoryInode {
    block: u32,
    links: u32,
    size: u16,
    offset: u16,
    parent: u32,
});
disk_struct!(BasicFileInode {
    block: u32,
    fragment: u32,
    offset: u32,
    size: u32,
});
disk_struct!(BasicDeviceInode {
    links: u32,
    device: u32,
});
disk_struct!(BasicIpcInode { links: u32 });
disk_struct!(XattrValue { size: u32 });

disk_struct!(DirectoryInode {
    links: u32,
    size: u32,
    block: u32,
    parent: u32,
    index_count: u16,
    offset: u16,
    xattr: u32,
});

disk_struct!(FileInode {
    block: u64,
    size: u64,
    sparse: u64,
    links: u32,
    fragment: u32,
    offset: u32,
    xattr: u32,
});

disk_struct!(SymlinkInode {
    links: u32,
    size: u32
});
disk_struct!(DeviceInode {
    links: u32,
    device: u32,
    xattr: u32
});
disk_struct!(IpcInode {
    links: u32,
    xattr: u32
});
disk_struct!(DirectoryHeader {
    count: u32,
    block: u32,
    inode: u32
});
disk_struct!(DirectoryEntry {
    offset: u16,
    inode_delta: i16,
    kind: u16,
    name_size: u16
});
disk_struct!(DirectoryIndex {
    index: u32,
    block: u32,
    name_size: u32
});
disk_struct!(Fragment {
    block: u64,
    size: u32,
    unused: u32
});
disk_struct!(XattrEntry {
    kind: u16,
    name_size: u16
});
disk_struct!(XattrId {
    reference: u64,
    count: u32,
    size: u32
});
disk_struct!(XattrTable {
    block: u64,
    count: u32,
    unused: u32
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InodeKind {
    Directory,
    File,
    Symlink,
    BlockDevice,
    CharacterDevice,
    Fifo,
    Socket,
}

impl InodeKind {
    pub fn basic(self) -> u16 {
        match self {
            Self::Directory => 1,
            Self::File => 2,
            Self::Symlink => 3,
            Self::BlockDevice => 4,
            Self::CharacterDevice => 5,
            Self::Fifo => 6,
            Self::Socket => 7,
        }
    }

    pub fn extended(self) -> u16 {
        self.basic() + 7
    }

    pub fn from_disk(kind: u16) -> io::Result<Self> {
        match kind {
            1 | 8 => Ok(Self::Directory),
            2 | 9 => Ok(Self::File),
            3 | 10 => Ok(Self::Symlink),
            4 | 11 => Ok(Self::BlockDevice),
            5 | 12 => Ok(Self::CharacterDevice),
            6 | 13 => Ok(Self::Fifo),
            7 | 14 => Ok(Self::Socket),
            _ => Err(invalid("invalid SquashFS inode type")),
        }
    }
}

pub(super) fn device_number(major: u32, minor: u32) -> io::Result<u32> {
    if major >= 4096 || minor >= 1048576 {
        return Err(invalid("device number does not fit SquashFS encoding"));
    }
    Ok(((minor & 0xfff00) << 12) | (major << 8) | (minor & 0xff))
}

pub(super) fn device_parts(device: u32) -> (u32, u32) {
    (
        (device >> 8) & 0xfff,
        (device & 0xff) | ((device >> 12) & 0xfff00),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn superblock_wire_layout() {
        let mut bytes = Vec::new();
        let block = Superblock::new(131072, 1234);
        block.serialize(&mut bytes).unwrap();
        assert_eq!(bytes.len(), 96);
        assert_eq!(&bytes[..4], b"hsqs");
        assert_eq!(&bytes[8..12], &1234u32.to_le_bytes());
        assert_eq!(&bytes[20..24], &[6, 0, 17, 0]);
        assert_eq!(&bytes[24..26], &[160, 0]);
        assert_eq!(&bytes[64..72], &[255; 8]);
        assert_eq!(Superblock::deserialize(&mut &bytes[..]).unwrap(), block);
    }

    #[test]
    fn signed_directory_delta_is_little_endian() {
        let entry = DirectoryEntry {
            offset: 33,
            inode_delta: -12,
            kind: 2,
            name_size: 255,
        };
        let mut bytes = Vec::new();
        entry.serialize(&mut bytes).unwrap();
        assert_eq!(bytes, [33, 0, 244, 255, 2, 0, 255, 0]);
    }

    #[test]
    fn device_encoding_boundaries() {
        assert_eq!(device_number(1, 3).unwrap(), 0x103);
        assert_eq!(device_number(4095, 1048575).unwrap(), u32::MAX);
        assert!(device_number(4096, 0).is_err());
        assert!(device_number(0, 1048576).is_err());
        for (major, minor) in [(0, 0), (1, 3), (4095, 1048575), (314, 65537)] {
            assert_eq!(
                device_parts(device_number(major, minor).unwrap()),
                (major, minor)
            );
        }
    }
}
