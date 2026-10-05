use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt};

use super::blocks::{METADATA_SIZE, MetadataTable, UNCOMPRESSED_DATA, encode_block, invalid};
use super::format::*;
use super::tree::{normalize_path, *};
use crate::prelude::*;
use crate::s9pk::merkle_archive::source::FileSource;
use crate::util::io::{ParallelBlake3Writer, TrackingIO};

struct FlatNode<'a, S> {
    node: &'a Node<S>,
    path: PathBuf,
    parent: usize,
    children: Vec<(OsString, usize)>,
    links: u32,
}

fn flatten<S>(root: &Node<S>) -> std::io::Result<Vec<FlatNode<'_, S>>> {
    let mut nodes = vec![FlatNode {
        node: root,
        path: PathBuf::new(),
        parent: 0,
        children: Vec::new(),
        links: 1,
    }];
    let mut paths = BTreeMap::from([(PathBuf::new(), Ok(0usize))]);
    let mut links = Vec::new();
    let mut stack = vec![0];
    while let Some(parent) = stack.pop() {
        let directory = nodes[parent]
            .node
            .contents
            .as_directory()
            .expect("queued directories");
        for (name, entry) in directory.iter() {
            let path = nodes[parent].path.join(name);
            match entry {
                Entry::Node(node) => {
                    if node.metadata.mode & !0o7777 != 0 {
                        return Err(invalid("invalid SquashFS inode mode"));
                    }
                    let index = nodes.len();
                    nodes.push(FlatNode {
                        node,
                        path: path.clone(),
                        parent,
                        children: Vec::new(),
                        links: 1,
                    });
                    paths.insert(path, Ok(index));
                    nodes[parent].children.push((name.clone(), index));
                    if matches!(node.contents, NodeContents::Directory(_)) {
                        stack.push(index);
                    }
                }
                Entry::Hardlink(target) => {
                    let target = normalize_path(target)?;
                    paths.insert(path.clone(), Err(target.clone()));
                    links.push((parent, name.clone(), path, target));
                }
            }
        }
    }
    if root.metadata.mode & !0o7777 != 0 {
        return Err(invalid("invalid SquashFS root mode"));
    }
    u32::try_from(nodes.len() + 1).map_err(|_| invalid("too many SquashFS inodes"))?;
    for (parent, name, path, mut target) in links {
        let mut visited = BTreeSet::from([path]);
        let index = loop {
            if !visited.insert(target.clone()) {
                return Err(invalid("cyclic archive hardlink"));
            }
            match paths.get(&target) {
                Some(Ok(index)) => break *index,
                Some(Err(next)) => target = next.clone(),
                None => {
                    return Err(invalid(format!(
                        "archive hardlink target does not exist: {}",
                        target.display()
                    )));
                }
            }
        };
        if matches!(nodes[index].node.contents, NodeContents::Directory(_)) {
            return Err(invalid("archive hardlinks cannot target directories"));
        }
        nodes[index].links = nodes[index]
            .links
            .checked_add(1)
            .ok_or_else(|| invalid("too many inode links"))?;
        nodes[parent].children.push((name, index));
    }
    for node in &mut nodes {
        node.children.sort_by(|a, b| a.0.cmp(&b.0));
    }
    Ok(nodes)
}

#[derive(Clone)]
struct FileLayout {
    block: u64,
    size: u64,
    sparse: u64,
    blocks: Vec<u32>,
    fragment: u32,
    offset: u32,
}

struct DataWriter {
    block_size: usize,
    level: i32,
    fragments: Vec<Fragment>,
    tail: Vec<u8>,
}

impl DataWriter {
    fn new(options: Options) -> Self {
        Self {
            block_size: options.block_size() as usize,
            level: options.compression_level(),
            fragments: Vec::new(),
            tail: Vec::new(),
        }
    }

    async fn block<W: AsyncWrite + Unpin>(
        &self,
        bytes: &[u8],
        writer: &mut TrackingIO<W>,
    ) -> Result<u32, Error> {
        let block = encode_block(bytes, self.level)?;
        let size = u32::try_from(block.bytes.len())
            .map_err(|_| invalid("SquashFS data block exceeds format limit"))?
            | if block.compressed {
                0
            } else {
                UNCOMPRESSED_DATA
            };
        writer.write_all(&block.bytes).await?;
        Ok(size)
    }

    async fn flush_fragment<W: AsyncWrite + Unpin>(
        &mut self,
        writer: &mut TrackingIO<W>,
    ) -> Result<(), Error> {
        if !self.tail.is_empty() {
            let block = writer.position();
            let size = self.block(&self.tail, writer).await?;
            self.fragments.push(Fragment {
                block,
                size,
                unused: 0,
            });
            self.tail.clear();
        }
        Ok(())
    }

    async fn file<S: FileSource, W: AsyncWrite + Unpin>(
        &mut self,
        source: &S,
        size: u64,
        hash: Option<blake3::Hash>,
        writer: &mut TrackingIO<W>,
    ) -> Result<FileLayout, Error> {
        let mut reader = source.reader().await?;
        let mut hasher = blake3::Hasher::new();
        let mut layout = FileLayout {
            block: writer.position(),
            size,
            sparse: 0,
            blocks: Vec::new(),
            fragment: INVALID_INDEX,
            offset: 0,
        };
        let mut buffer = vec![0; self.block_size];
        let mut remaining = size;
        while remaining >= self.block_size as u64 {
            reader.read_exact(&mut buffer).await?;
            if hash.is_some() {
                hasher.update(&buffer);
            }
            if buffer.iter().all(|b| *b == 0) {
                layout.blocks.push(0);
                layout.sparse += self.block_size as u64;
            } else {
                layout.blocks.push(self.block(&buffer, writer).await?);
            }
            remaining -= self.block_size as u64;
        }
        if remaining != 0 {
            let bytes = &mut buffer[..remaining as usize];
            reader.read_exact(bytes).await?;
            if hash.is_some() {
                hasher.update(bytes);
            }
            if self.tail.len() + bytes.len() > self.block_size {
                self.flush_fragment(writer).await?;
            }
            layout.fragment = u32::try_from(self.fragments.len())
                .map_err(|_| invalid("too many SquashFS fragments"))?;
            layout.offset = self.tail.len() as u32;
            self.tail.extend_from_slice(bytes);
        }
        if reader.read(&mut [0; 1]).await? != 0
            || hash.is_some_and(|expected| hasher.finalize() != expected)
        {
            return Err(invalid("file source changed while creating SquashFS image").into());
        }
        Ok(layout)
    }
}

async fn fingerprint(source: &impl FileSource, size: u64) -> Result<blake3::Hash, Error> {
    let mut writer = TrackingIO::new(0, ParallelBlake3Writer::new(crate::CAP_1_MiB));
    source.copy(&mut writer).await?;
    if writer.position() != size {
        return Err(invalid("file source size changed while hashing").into());
    }
    writer.into_inner().finalize().await
}

struct InodeTables {
    inodes: MetadataTable,
    directories: MetadataTable,
    xattrs: MetadataTable,
    xattr_ids: Vec<XattrId>,
    ids: Vec<u32>,
    references: Vec<u64>,
}

impl InodeTables {
    fn new<S>(nodes: &[FlatNode<'_, S>], level: i32) -> std::io::Result<Self> {
        let ids: Vec<_> = nodes
            .iter()
            .flat_map(|node| [node.node.metadata.uid, node.node.metadata.gid])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        u16::try_from(ids.len()).map_err(|_| invalid("too many SquashFS ownership IDs"))?;
        Ok(Self {
            inodes: MetadataTable::new(level),
            directories: MetadataTable::new(level),
            xattrs: MetadataTable::new(level),
            xattr_ids: Vec::new(),
            ids,
            references: vec![0; nodes.len()],
        })
    }

    fn id(&self, id: u32) -> u16 {
        self.ids
            .binary_search(&id)
            .expect("IDs collected from every inode") as u16
    }

    fn xattrs(&mut self, metadata: &Metadata) -> std::io::Result<u32> {
        if metadata.xattrs.is_empty() {
            return Ok(INVALID_INDEX);
        }
        let reference = self.xattrs.reference()?;
        let mut size = 0u32;
        for (name, value) in &metadata.xattrs {
            let bytes = name.as_bytes();
            let (kind, name) = [
                (0, &b"user."[..]),
                (1, &b"trusted."[..]),
                (2, &b"security."[..]),
            ]
            .into_iter()
            .find_map(|(kind, prefix)| bytes.strip_prefix(prefix).map(|name| (kind, name)))
            .ok_or_else(|| {
                invalid("SquashFS xattr namespace must be user, trusted, or security")
            })?;
            if name.is_empty() || name.contains(&0) {
                return Err(invalid("invalid SquashFS xattr name"));
            }
            let name_size = u16::try_from(name.len())
                .map_err(|_| invalid("SquashFS xattr name exceeds format limit"))?;
            XattrEntry { kind, name_size }.serialize(&mut self.xattrs)?;
            self.xattrs.write_all(name)?;
            let value_size = u32::try_from(value.len())
                .map_err(|_| invalid("SquashFS xattr value exceeds format limit"))?;
            self.xattrs.write_all(&value_size.to_le_bytes())?;
            self.xattrs.write_all(value)?;
            size = size
                .checked_add(
                    u32::try_from(bytes.len() + 1)
                        .map_err(|_| invalid("SquashFS xattr name exceeds format limit"))?,
                )
                .and_then(|size| size.checked_add(value_size))
                .ok_or_else(|| invalid("SquashFS xattrs exceed format limit"))?;
        }
        let index = u32::try_from(self.xattr_ids.len())
            .map_err(|_| invalid("too many SquashFS xattr sets"))?;
        self.xattr_ids.push(XattrId {
            reference,
            count: metadata
                .xattrs
                .len()
                .try_into()
                .map_err(|_| invalid("too many inode xattrs"))?,
            size,
        });
        Ok(index)
    }

    fn inode<S>(
        &mut self,
        index: usize,
        node: &FlatNode<'_, S>,
        layouts: &[Option<Arc<FileLayout>>],
        nodes: &[FlatNode<'_, S>],
    ) -> std::io::Result<()> {
        self.references[index] = self.inodes.reference()?;
        let header = InodeHeader {
            kind: node.node.contents.kind().extended(),
            mode: node.node.metadata.mode,
            uid: self.id(node.node.metadata.uid),
            gid: self.id(node.node.metadata.gid),
            modification_time: node.node.metadata.modification_time,
            number: (index + 1) as u32,
        };
        header.serialize(&mut self.inodes)?;
        let xattr = self.xattrs(&node.node.metadata)?;
        match &node.node.contents {
            NodeContents::File(_) => {
                let layout = layouts[index]
                    .as_ref()
                    .expect("file layouts precede inodes");
                FileInode {
                    block: layout.block,
                    size: layout.size,
                    sparse: layout.sparse,
                    links: node.links,
                    fragment: layout.fragment,
                    offset: layout.offset,
                    xattr,
                }
                .serialize(&mut self.inodes)?;
                for size in &layout.blocks {
                    self.inodes.write_all(&size.to_le_bytes())?;
                }
            }
            NodeContents::Directory(_) => {
                let start = self.directories.reference()?;
                let mut size = 3u32;
                let mut indexes = Vec::new();
                let mut indexed_block = start >> 16;
                let mut children = node.children.iter().peekable();
                while let Some((name, child)) = children.next() {
                    let block = self.references[*child] >> 16;
                    let number = (*child + 1) as u32;
                    let mut group = vec![(name, *child)];
                    while let Some((name, child)) = children.peek() {
                        let delta = (*child as i64 + 1) - i64::from(number);
                        if group.len() == 256
                            || self.references[*child] >> 16 != block
                            || i16::try_from(delta).is_err()
                        {
                            break;
                        }
                        group.push((name, *child));
                        children.next();
                    }
                    let position = self.directories.reference()?;
                    if position >> 16 != indexed_block && indexes.len() < u16::MAX as usize {
                        let first_name = name.as_bytes();
                        indexes.push((
                            DirectoryIndex {
                                index: size - 3,
                                block: (position >> 16) as u32,
                                name_size: first_name.len() as u32 - 1,
                            },
                            first_name,
                        ));
                        indexed_block = position >> 16;
                    }
                    DirectoryHeader {
                        count: group.len() as u32 - 1,
                        block: block as u32,
                        inode: number,
                    }
                    .serialize(&mut self.directories)?;
                    size = size
                        .checked_add(DirectoryHeader::SIZE as u32)
                        .ok_or_else(|| invalid("SquashFS directory exceeds format limit"))?;
                    for (name, child) in group {
                        let name = name.as_bytes();
                        DirectoryEntry {
                            offset: self.references[child] as u16,
                            inode_delta: ((child as i64 + 1) - i64::from(number)) as i16,
                            kind: nodes[child].node.contents.kind().basic(),
                            name_size: name.len() as u16 - 1,
                        }
                        .serialize(&mut self.directories)?;
                        self.directories.write_all(name)?;
                        size = size
                            .checked_add((DirectoryEntry::SIZE + name.len()) as u32)
                            .ok_or_else(|| invalid("SquashFS directory exceeds format limit"))?;
                    }
                }
                let subdirs = node
                    .children
                    .iter()
                    .filter(|(_, child)| {
                        matches!(nodes[*child].node.contents, NodeContents::Directory(_))
                    })
                    .count();
                DirectoryInode {
                    links: u32::try_from(subdirs + 2)
                        .map_err(|_| invalid("too many directory links"))?,
                    size,
                    block: (start >> 16) as u32,
                    parent: if index == 0 {
                        (nodes.len() + 1) as u32
                    } else {
                        (node.parent + 1) as u32
                    },
                    index_count: indexes.len() as u16,
                    offset: start as u16,
                    xattr,
                }
                .serialize(&mut self.inodes)?;
                for (index, name) in indexes {
                    index.serialize(&mut self.inodes)?;
                    self.inodes.write_all(name)?;
                }
            }
            NodeContents::Symlink(target) => {
                let bytes = target.as_os_str().as_bytes();
                if bytes.contains(&0) {
                    return Err(invalid("SquashFS symlink target contains NUL"));
                }
                SymlinkInode {
                    links: node.links,
                    size: bytes
                        .len()
                        .try_into()
                        .map_err(|_| invalid("SquashFS symlink exceeds format limit"))?,
                }
                .serialize(&mut self.inodes)?;
                self.inodes.write_all(bytes)?;
                self.inodes.write_all(&xattr.to_le_bytes())?;
            }
            NodeContents::BlockDevice(device) | NodeContents::CharacterDevice(device) => {
                DeviceInode {
                    links: node.links,
                    device: device_number(device.major, device.minor)?,
                    xattr,
                }
                .serialize(&mut self.inodes)?;
            }
            NodeContents::Fifo | NodeContents::Socket => {
                IpcInode {
                    links: node.links,
                    xattr,
                }
                .serialize(&mut self.inodes)?;
            }
        }
        Ok(())
    }
}

async fn table_blocks<W: AsyncWrite + Unpin>(
    bytes: &[u8],
    level: i32,
    writer: &mut TrackingIO<W>,
) -> Result<Vec<u64>, Error> {
    let mut pointers = Vec::new();
    for chunk in bytes.chunks(METADATA_SIZE) {
        pointers.push(writer.position());
        let mut block = MetadataTable::new(level);
        block.write_all(chunk)?;
        writer.write_all(&block.finish()?).await?;
    }
    Ok(pointers)
}

async fn table_index<W: AsyncWrite + Unpin>(
    pointers: &[u64],
    writer: &mut TrackingIO<W>,
) -> Result<u64, Error> {
    let index = writer.position();
    for pointer in pointers {
        writer.write_all(&pointer.to_le_bytes()).await?;
    }
    Ok(index)
}

async fn indexed_table<W: AsyncWrite + Unpin>(
    bytes: &[u8],
    level: i32,
    writer: &mut TrackingIO<W>,
) -> Result<u64, Error> {
    if bytes.is_empty() {
        return Ok(INVALID_TABLE);
    }
    let pointers = table_blocks(bytes, level, writer).await?;
    table_index(&pointers, writer).await
}

impl<S: FileSource> Squashfs<S> {
    /// Writes an image at the current position and leaves the output after its 4 KiB padding.
    pub async fn serialize<W: AsyncWrite + AsyncSeek + Unpin + Send>(
        &self,
        writer: &mut W,
    ) -> Result<(), Error> {
        let nodes = flatten(self.root())?;
        let base = writer.stream_position().await?;
        let mut output = TrackingIO::new(0, &mut *writer);
        output.write_all(&[0; Superblock::SIZE]).await?;
        let mut data = DataWriter::new(self.options);
        let mut layouts = vec![None; nodes.len()];
        let mut duplicates = BTreeMap::new();
        for (index, node) in nodes.iter().enumerate() {
            if let NodeContents::File(source) = &node.node.contents {
                let size = source.size().await?;
                let hash = if self.options.deduplicate {
                    Some(fingerprint(source, size).await?)
                } else {
                    None
                };
                let key = hash.map(|hash| (*hash.as_bytes(), size));
                let layout = if let Some(layout) = key.and_then(|key| duplicates.get(&key)) {
                    Arc::clone(layout)
                } else {
                    let layout = Arc::new(data.file(source, size, hash, &mut output).await?);
                    if let Some(key) = key {
                        duplicates.insert(key, layout.clone());
                    }
                    layout
                };
                layouts[index] = Some(layout);
            }
        }
        data.flush_fragment(&mut output).await?;
        let level = self.options.compression_level();
        let mut tables = InodeTables::new(&nodes, level)?;
        for (index, node) in nodes.iter().enumerate() {
            if !matches!(node.node.contents, NodeContents::Directory(_)) {
                tables.inode(index, node, &layouts, &nodes)?;
            }
        }
        for (index, node) in nodes.iter().enumerate().rev() {
            if matches!(node.node.contents, NodeContents::Directory(_)) {
                tables.inode(index, node, &layouts, &nodes)?;
            }
        }
        let mut header = Superblock::new(self.options.block_size(), self.options.modification_time);
        header.inode_count = nodes.len() as u32;
        header.root_inode = tables.references[0];
        header.inode_table = output.position();
        output.write_all(&tables.inodes.finish()?).await?;
        header.directory_table = output.position();
        output.write_all(&tables.directories.finish()?).await?;
        let mut fragment_bytes = Vec::new();
        for fragment in &data.fragments {
            fragment.serialize(&mut fragment_bytes)?;
        }
        header.fragment_count = data
            .fragments
            .len()
            .try_into()
            .map_err(|_| invalid("too many SquashFS fragments"))?;
        header.fragment_table = indexed_table(&fragment_bytes, level, &mut output).await?;
        let mut export_bytes = Vec::new();
        for reference in &tables.references {
            export_bytes.extend_from_slice(&reference.to_le_bytes());
        }
        header.export_table = indexed_table(&export_bytes, level, &mut output).await?;
        let mut id_bytes = Vec::new();
        for id in &tables.ids {
            id_bytes.extend_from_slice(&id.to_le_bytes());
        }
        header.id_count = tables
            .ids
            .len()
            .try_into()
            .map_err(|_| invalid("too many SquashFS ownership IDs"))?;
        header.id_table = indexed_table(&id_bytes, level, &mut output).await?;
        if tables.xattr_ids.is_empty() {
            header.flags |= NO_XATTRS;
        } else {
            let block = output.position();
            output.write_all(&tables.xattrs.finish()?).await?;
            let mut bytes = Vec::new();
            for id in &tables.xattr_ids {
                id.serialize(&mut bytes)?;
            }
            let pointers = table_blocks(&bytes, level, &mut output).await?;
            header.xattr_id_table = output.position();
            let mut table_bytes = Vec::new();
            XattrTable {
                block,
                count: tables
                    .xattr_ids
                    .len()
                    .try_into()
                    .map_err(|_| invalid("too many SquashFS xattr sets"))?,
                unused: 0,
            }
            .serialize(&mut table_bytes)?;
            output.write_all(&table_bytes).await?;
            table_index(&pointers, &mut output).await?;
        }
        if self.options.deduplicate {
            header.flags |= DEDUPLICATED;
        }
        header.bytes_used = output.position();
        let padding = (4096 - header.bytes_used % 4096) % 4096;
        output.write_all(&vec![0; padding as usize]).await?;
        let end = output.position();
        let inner = output.into_inner();
        inner.seek(std::io::SeekFrom::Start(base)).await?;
        let mut bytes = Vec::new();
        header.serialize(&mut bytes)?;
        inner.write_all(&bytes).await?;
        inner.seek(std::io::SeekFrom::Start(base + end)).await?;
        inner.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "write/tests.rs"]
mod tests;
