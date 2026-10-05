use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};

use futures::FutureExt;
use futures::future::BoxFuture;
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

use super::blocks::{
    METADATA_SIZE, MetadataBlocksReader, UNCOMPRESSED_DATA, decode_block, invalid,
};
use super::format::*;
use super::tree::*;
use crate::prelude::Error;
use crate::s9pk::merkle_archive::source::{ArchiveSource, FileSource};
use crate::util::io::TrackingIO;

#[derive(Debug, Clone)]
struct DataBlock {
    position: u64,
    encoded: u32,
    offset: usize,
    size: usize,
    fragment: bool,
}

/// Archive-backed file contents; readers decode one data block at a time.
pub struct SquashfsFileSource<S> {
    source: Arc<S>,
    blocks: Arc<Vec<DataBlock>>,
    size: u64,
    block_size: usize,
}

impl<S> Clone for SquashfsFileSource<S> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            blocks: self.blocks.clone(),
            size: self.size,
            block_size: self.block_size,
        }
    }
}

/// A bounded-memory reader over a file or a file slice.
pub struct SquashfsFileReader<S> {
    file: SquashfsFileSource<S>,
    position: u64,
    end: u64,
    buffer: Vec<u8>,
    buffer_position: usize,
    pending: Option<BoxFuture<'static, io::Result<Vec<u8>>>>,
    failed: bool,
}

impl<S: ArchiveSource> FileSource for SquashfsFileSource<S> {
    type Reader = SquashfsFileReader<S>;
    type SliceReader = SquashfsFileReader<S>;

    async fn size(&self) -> Result<u64, Error> {
        Ok(self.size)
    }
    async fn reader(&self) -> Result<Self::Reader, Error> {
        self.slice(0, self.size).await
    }
    async fn slice(&self, position: u64, size: u64) -> Result<Self::SliceReader, Error> {
        let position = position.min(self.size);
        Ok(SquashfsFileReader {
            file: self.clone(),
            position,
            end: position + size.min(self.size - position),
            buffer: Vec::new(),
            buffer_position: 0,
            pending: None,
            failed: false,
        })
    }
}

impl<S: ArchiveSource> AsyncRead for SquashfsFileReader<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if output.remaining() == 0 || this.position == this.end {
            return Poll::Ready(Ok(()));
        }
        if this.failed {
            return Poll::Ready(Err(invalid("SquashFS file reader failed")));
        }
        if this.buffer_position == this.buffer.len() {
            this.buffer.clear();
            if this.pending.is_none() {
                let index = (this.position / this.file.block_size as u64) as usize;
                let block = this.file.blocks[index].clone();
                let source = this.file.source.clone();
                let limit = this.file.block_size;
                this.pending = Some(
                    async move {
                        if block.encoded == 0 {
                            return Ok(vec![0; block.size]);
                        }
                        let size = block.encoded & !UNCOMPRESSED_DATA;
                        let mut bytes = vec![0; size as usize];
                        source
                            .fetch(block.position, u64::from(size))
                            .await
                            .map_err(|e| io::Error::other(e.to_string()))?
                            .read_exact(&mut bytes)
                            .await?;
                        let decoded =
                            decode_block(&bytes, block.encoded & UNCOMPRESSED_DATA == 0, limit)?;
                        if block.offset + block.size > decoded.len()
                            || (!block.fragment && decoded.len() != block.size)
                        {
                            return Err(invalid("invalid SquashFS data block length"));
                        }
                        Ok(decoded)
                    }
                    .boxed(),
                );
            }
            let result = ready!(this.pending.as_mut().unwrap().as_mut().poll(cx));
            this.pending = None;
            match result {
                Ok(buffer) => this.buffer = buffer,
                Err(error) => {
                    this.failed = true;
                    return Poll::Ready(Err(error));
                }
            }
            let index = (this.position / this.file.block_size as u64) as usize;
            this.buffer_position = this.file.blocks[index].offset
                + (this.position % this.file.block_size as u64) as usize;
        }
        let index = (this.position / this.file.block_size as u64) as usize;
        let block = &this.file.blocks[index];
        let count = output
            .remaining()
            .min((this.end - this.position) as usize)
            .min(block.offset + block.size - this.buffer_position);
        output.put_slice(&this.buffer[this.buffer_position..this.buffer_position + count]);
        this.buffer_position += count;
        this.position += count as u64;
        if this.buffer_position == block.offset + block.size {
            this.buffer.clear();
            this.buffer_position = 0;
        }
        Poll::Ready(Ok(()))
    }
}

struct Image<S> {
    source: Arc<S>,
    superblock: Superblock,
    inode_blocks: BTreeMap<u64, u64>,
    directory_blocks: BTreeMap<u64, u64>,
    directory_end: u64,
    ids: Vec<u32>,
    fragments: Vec<Fragment>,
    exports: Option<Vec<u64>>,
    xattrs: Vec<XattrId>,
    xattr_blocks: BTreeMap<u64, u64>,
    xattr_start: u64,
    xattr_end: u64,
}

type MetadataReader<S> = MetadataBlocksReader<<S as ArchiveSource>::FetchReader>;

async fn disk<T: Disk>(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<T> {
    let mut bytes = vec![0; T::SIZE];
    reader.read_exact(&mut bytes).await?;
    T::deserialize(&mut bytes.as_slice())
}
async fn u32_value(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<u32> {
    Ok(disk::<XattrValue>(reader).await?.size)
}
async fn u64_value(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<u64> {
    let mut bytes = [0; 8];
    reader.read_exact(&mut bytes).await?;
    Ok(u64::from_le_bytes(bytes))
}

impl<S: ArchiveSource> Image<S> {
    fn range(&self, start: u64, size: u64, end: u64) -> io::Result<()> {
        if start < Superblock::SIZE as u64 || start > end || size > end - start {
            return Err(invalid("SquashFS reference lies outside its table"));
        }
        Ok(())
    }
    async fn fetch(&self, start: u64, size: u64) -> Result<S::FetchReader, Error> {
        self.range(start, size, self.superblock.bytes_used)?;
        self.source.fetch(start, size).await
    }
    async fn boundaries(&self, start: u64, end: u64) -> Result<BTreeMap<u64, u64>, Error> {
        self.range(start, 0, end)?;
        let mut blocks = BTreeMap::new();
        let mut logical = 0;
        let mut position = start;
        while position < end {
            self.range(position, 2, end)?;
            let mut bytes = [0; 2];
            self.fetch(position, 2)
                .await?
                .read_exact(&mut bytes)
                .await?;
            let size = u64::from(u16::from_le_bytes(bytes) & 0x7fff);
            if size == 0 || size > METADATA_SIZE as u64 {
                return Err(invalid("invalid metadata block size").into());
            }
            self.range(position + 2, size, end)?;
            let encoded = u16::from_le_bytes(bytes);
            let mut payload = vec![0; size as usize];
            self.fetch(position + 2, size)
                .await?
                .read_exact(&mut payload)
                .await?;
            let decoded = decode_block(&payload, encoded & 0x8000 == 0, METADATA_SIZE)?;
            if decoded.is_empty() {
                return Err(invalid("invalid decoded metadata block length").into());
            }
            blocks.insert(position, logical);
            logical += decoded.len() as u64;
            position += size + 2;
        }
        Ok(blocks)
    }
    async fn metadata(
        &self,
        start: u64,
        end: u64,
        blocks: &BTreeMap<u64, u64>,
        reference: u64,
    ) -> Result<MetadataReader<S>, Error> {
        let position = start
            .checked_add(reference >> 16)
            .ok_or_else(|| invalid("metadata reference overflow"))?;
        if reference & 0xffff >= METADATA_SIZE as u64 || !blocks.contains_key(&position) {
            return Err(invalid("invalid SquashFS metadata reference").into());
        }
        let mut reader = MetadataBlocksReader::new(self.fetch(position, end - position).await?);
        let offset = reference & 0xffff;
        if offset != 0 {
            // Offsets belong to the referenced block, not the concatenated metadata stream.
            let mut header = [0; 2];
            self.fetch(position, 2)
                .await?
                .read_exact(&mut header)
                .await?;
            let encoded = u16::from_le_bytes(header);
            let mut bytes = vec![0; (encoded & 0x7fff) as usize];
            self.fetch(position + 2, bytes.len() as u64)
                .await?
                .read_exact(&mut bytes)
                .await?;
            if offset as usize >= decode_block(&bytes, encoded & 0x8000 == 0, METADATA_SIZE)?.len()
            {
                return Err(invalid("metadata offset exceeds decoded block").into());
            }
            let mut skip = [0; METADATA_SIZE];
            reader.read_exact(&mut skip[..offset as usize]).await?;
        }
        Ok(reader)
    }
    async fn indexed(
        &self,
        position: u64,
        count: u64,
        width: usize,
    ) -> Result<(Vec<u8>, u64), Error> {
        let size = count
            .checked_mul(width as u64)
            .ok_or_else(|| invalid("table size overflow"))?;
        let blocks = size.div_ceil(METADATA_SIZE as u64);
        self.range(
            position,
            blocks
                .checked_mul(8)
                .ok_or_else(|| invalid("table index overflow"))?,
            self.superblock.bytes_used,
        )?;
        let mut pointers = self.fetch(position, blocks * 8).await?;
        let mut result = Vec::new();
        let mut first = position;
        let mut previous = None;
        for index in 0..blocks {
            let pointer = u64_value(&mut pointers).await?;
            if previous.is_some_and(|p| pointer <= p) {
                return Err(invalid("unordered metadata table index").into());
            }
            if pointer < self.superblock.directory_table {
                return Err(invalid("indexed metadata overlaps inode or data table").into());
            }
            self.range(pointer, 2, position)?;
            let mut header = [0; 2];
            self.fetch(pointer, 2)
                .await?
                .read_exact(&mut header)
                .await?;
            let encoded = u16::from_le_bytes(header);
            let length = (encoded & 0x7fff) as usize;
            if length == 0 || length > METADATA_SIZE {
                return Err(invalid("invalid indexed metadata block").into());
            }
            self.range(pointer + 2, length as u64, position)?;
            let mut bytes = vec![0; length];
            self.fetch(pointer + 2, length as u64)
                .await?
                .read_exact(&mut bytes)
                .await?;
            let decoded = decode_block(&bytes, encoded & 0x8000 == 0, METADATA_SIZE)?;
            let expected = (size - index * METADATA_SIZE as u64).min(METADATA_SIZE as u64) as usize;
            if decoded.len() != expected {
                return Err(invalid("incorrect indexed table length").into());
            }
            result.extend_from_slice(&decoded);
            first = first.min(pointer);
            previous = Some(pointer + 2 + length as u64 - 1);
        }
        Ok((result, first))
    }
    async fn load(source: S) -> Result<Self, Error> {
        let source = Arc::new(source);
        let superblock =
            disk::<Superblock>(&mut source.fetch(0, Superblock::SIZE as u64).await?).await?;
        superblock.validate()?;
        source
            .fetch(superblock.bytes_used - 1, 1)
            .await?
            .read_exact(&mut [0; 1])
            .await?;
        if source
            .size()
            .await
            .is_some_and(|size| size < superblock.bytes_used)
        {
            return Err(invalid("truncated SquashFS image").into());
        }
        let mut image = Self {
            source,
            superblock,
            inode_blocks: BTreeMap::new(),
            directory_blocks: BTreeMap::new(),
            directory_end: 0,
            ids: Vec::new(),
            fragments: Vec::new(),
            exports: None,
            xattrs: Vec::new(),
            xattr_blocks: BTreeMap::new(),
            xattr_start: 0,
            xattr_end: 0,
        };
        let sb = &image.superblock;
        let (bytes, mut directory_end) = image
            .indexed(sb.id_table, u64::from(sb.id_count), 4)
            .await?;
        for chunk in bytes.chunks_exact(4) {
            image
                .ids
                .push(u32::from_le_bytes(chunk.try_into().unwrap()));
        }
        if sb.fragment_count != 0 {
            let (bytes, start) = image
                .indexed(
                    sb.fragment_table,
                    u64::from(sb.fragment_count),
                    Fragment::SIZE,
                )
                .await?;
            directory_end = directory_end.min(start);
            let mut bytes = bytes.as_slice();
            while !bytes.is_empty() {
                let fragment = Fragment::deserialize(&mut bytes)?;
                image.data_bounds(fragment.block, fragment.size)?;
                if fragment.size == 0 || fragment.unused != 0 {
                    return Err(invalid("invalid fragment entry").into());
                }
                image.fragments.push(fragment);
            }
        }
        if sb.export_table != INVALID_TABLE {
            let (bytes, start) = image
                .indexed(sb.export_table, u64::from(sb.inode_count), 8)
                .await?;
            directory_end = directory_end.min(start);
            image.exports = Some(
                bytes
                    .chunks_exact(8)
                    .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
                    .collect(),
            );
        } else if sb.flags & EXPORTABLE != 0 {
            return Err(invalid("missing export table").into());
        }
        if sb.xattr_id_table != INVALID_TABLE {
            let table = disk::<XattrTable>(
                &mut image
                    .fetch(sb.xattr_id_table, XattrTable::SIZE as u64)
                    .await?,
            )
            .await?;
            if table.count == 0 || table.unused != 0 || table.block <= directory_end {
                return Err(invalid("invalid xattr table header").into());
            }
            let (bytes, end) = image
                .indexed(
                    sb.xattr_id_table + XattrTable::SIZE as u64,
                    u64::from(table.count),
                    XattrId::SIZE,
                )
                .await?;
            image.xattr_start = table.block;
            image.xattr_end = end;
            image.xattr_blocks = image.boundaries(table.block, end).await?;
            let mut bytes = bytes.as_slice();
            while !bytes.is_empty() {
                image.xattrs.push(XattrId::deserialize(&mut bytes)?);
            }
        }
        image.directory_end = directory_end;
        image.inode_blocks = image
            .boundaries(
                image.superblock.inode_table,
                image.superblock.directory_table,
            )
            .await?;
        image.directory_blocks = image
            .boundaries(image.superblock.directory_table, directory_end)
            .await?;
        Ok(image)
    }
    fn data_bounds(&self, position: u64, encoded: u32) -> io::Result<()> {
        if encoded & !(UNCOMPRESSED_DATA | (UNCOMPRESSED_DATA - 1)) != 0
            || encoded & !UNCOMPRESSED_DATA > self.superblock.block_size
        {
            return Err(invalid("invalid data block size"));
        }
        if encoded != 0 && encoded & !UNCOMPRESSED_DATA == 0 {
            return Err(invalid("empty data block"));
        }
        self.range(
            position,
            u64::from(encoded & !UNCOMPRESSED_DATA),
            self.superblock.inode_table,
        )
    }
    async fn attributes(&self, index: u32) -> Result<BTreeMap<OsString, Vec<u8>>, Error> {
        let mut result = BTreeMap::new();
        if index == INVALID_INDEX {
            return Ok(result);
        }
        let id = self
            .xattrs
            .get(index as usize)
            .ok_or_else(|| invalid("invalid xattr ID"))?;
        let mut reader = self
            .metadata(
                self.xattr_start,
                self.xattr_end,
                &self.xattr_blocks,
                id.reference,
            )
            .await?;
        let mut total = 0u64;
        for _ in 0..id.count {
            let entry = disk::<XattrEntry>(&mut reader).await?;
            let prefix = *XATTR_PREFIXES
                .get((entry.kind & !0x100) as usize)
                .ok_or_else(|| invalid("invalid xattr namespace"))?;
            if entry.name_size == 0 {
                return Err(invalid("invalid xattr name size").into());
            }
            let mut name = prefix.to_vec();
            name.resize(prefix.len() + entry.name_size as usize, 0);
            reader.read_exact(&mut name[prefix.len()..]).await?;
            if name.contains(&0) {
                return Err(invalid("NUL in xattr name").into());
            }
            let mut value_size = u32_value(&mut reader).await?;
            let mut value_reader;
            let input = if entry.kind & 0x100 != 0 {
                if value_size != 8 {
                    return Err(invalid("invalid out-of-line xattr reference size").into());
                }
                let reference = u64_value(&mut reader).await?;
                value_reader = self
                    .metadata(
                        self.xattr_start,
                        self.xattr_end,
                        &self.xattr_blocks,
                        reference,
                    )
                    .await?;
                value_size = u32_value(&mut value_reader).await?;
                &mut value_reader
            } else {
                &mut reader
            };
            total = total
                .checked_add(name.len() as u64 + 1 + u64::from(value_size))
                .ok_or_else(|| invalid("xattr size overflow"))?;
            if total > u64::from(id.size) {
                return Err(invalid("xattr set exceeds declared size").into());
            }
            let mut value = Vec::new();
            let mut remaining = value_size as usize;
            let mut buffer = [0; METADATA_SIZE];
            while remaining != 0 {
                let size = remaining.min(buffer.len());
                input.read_exact(&mut buffer[..size]).await?;
                value.extend_from_slice(&buffer[..size]);
                remaining -= size;
            }
            if result.insert(OsString::from_vec(name), value).is_some() {
                return Err(invalid("duplicate xattr name").into());
            }
        }
        if total != u64::from(id.size) {
            return Err(invalid("incorrect xattr set size").into());
        }
        Ok(result)
    }
}

struct Child {
    name: OsString,
    reference: u64,
    number: u32,
    kind: InodeKind,
}
struct Parsed<S> {
    header: InodeHeader,
    node: Node<SquashfsFileSource<S>>,
    children: Vec<Child>,
    parent: u32,
    links: u32,
    span: (u64, u64),
}

impl<S: ArchiveSource> Image<S> {
    async fn inode(&self, reference: u64) -> Result<Parsed<S>, Error> {
        let sb = &self.superblock;
        let reader = self
            .metadata(
                sb.inode_table,
                sb.directory_table,
                &self.inode_blocks,
                reference,
            )
            .await?;
        let start = self.inode_blocks[&(sb.inode_table + (reference >> 16))] + (reference & 0xffff);
        let mut reader = TrackingIO::new(start, reader);
        let header = disk::<InodeHeader>(&mut reader).await?;
        let kind = InodeKind::from_disk(header.kind)?;
        if header.mode & !0o7777 != 0 || header.number == 0 || header.number > sb.inode_count {
            return Err(invalid("invalid inode header").into());
        }
        if let Some(exports) = &self.exports
            && exports[header.number as usize - 1] != reference
        {
            return Err(invalid("export table disagrees with inode reference").into());
        }
        let uid = *self
            .ids
            .get(header.uid as usize)
            .ok_or_else(|| invalid("invalid inode UID"))?;
        let gid = *self
            .ids
            .get(header.gid as usize)
            .ok_or_else(|| invalid("invalid inode GID"))?;
        let extended = header.kind > 7;
        let mut xattr = INVALID_INDEX;
        let mut children = Vec::new();
        let mut parent = 0;
        let links;
        let contents = match kind {
            InodeKind::File => {
                let file = if extended {
                    disk::<FileInode>(&mut reader).await?
                } else {
                    let file = disk::<BasicFileInode>(&mut reader).await?;
                    FileInode {
                        block: u64::from(file.block),
                        size: u64::from(file.size),
                        sparse: 0,
                        links: 1,
                        fragment: file.fragment,
                        offset: file.offset,
                        xattr: INVALID_INDEX,
                    }
                };
                links = file.links;
                xattr = file.xattr;
                if file.size > i64::MAX as u64 || file.sparse > file.size {
                    return Err(invalid("invalid file size").into());
                }
                let block_size = u64::from(sb.block_size);
                let tail = file.size % block_size;
                let count =
                    file.size / block_size + u64::from(tail != 0 && file.fragment == INVALID_INDEX);
                let mut blocks = Vec::new();
                let mut position = file.block;
                for index in 0..count {
                    let encoded = u32_value(&mut reader).await?;
                    self.data_bounds(position, encoded)?;
                    let size = (file.size - index * block_size).min(block_size) as usize;
                    if encoded & UNCOMPRESSED_DATA != 0
                        && (encoded & !UNCOMPRESSED_DATA) as usize != size
                    {
                        return Err(invalid("incorrect uncompressed file block size").into());
                    }
                    blocks.push(DataBlock {
                        position,
                        encoded,
                        offset: 0,
                        size,
                        fragment: false,
                    });
                    position += u64::from(encoded & !UNCOMPRESSED_DATA);
                }
                if file.fragment != INVALID_INDEX {
                    if tail == 0 {
                        return Err(invalid("fragment without file tail").into());
                    }
                    let fragment = self
                        .fragments
                        .get(file.fragment as usize)
                        .ok_or_else(|| invalid("invalid file fragment index"))?;
                    if u64::from(file.offset) + tail > block_size {
                        return Err(invalid("file tail exceeds fragment block").into());
                    }
                    blocks.push(DataBlock {
                        position: fragment.block,
                        encoded: fragment.size,
                        offset: file.offset as usize,
                        size: tail as usize,
                        fragment: true,
                    });
                } else if file.offset != 0 {
                    return Err(invalid("fragment offset without fragment").into());
                }
                NodeContents::File(SquashfsFileSource {
                    source: self.source.clone(),
                    blocks: Arc::new(blocks),
                    size: file.size,
                    block_size: sb.block_size as usize,
                })
            }
            InodeKind::Directory => {
                let directory = if extended {
                    disk::<DirectoryInode>(&mut reader).await?
                } else {
                    let d = disk::<BasicDirectoryInode>(&mut reader).await?;
                    DirectoryInode {
                        links: d.links,
                        size: u32::from(d.size),
                        block: d.block,
                        parent: d.parent,
                        index_count: 0,
                        offset: d.offset,
                        xattr: INVALID_INDEX,
                    }
                };
                links = directory.links;
                parent = directory.parent;
                xattr = directory.xattr;
                if directory.size < 3 || directory.offset as usize >= METADATA_SIZE {
                    return Err(invalid("invalid directory size or offset").into());
                }
                let mut indexes = Vec::new();
                let mut previous = None;
                for _ in 0..directory.index_count {
                    let index = disk::<DirectoryIndex>(&mut reader).await?;
                    if index.index >= directory.size - 3
                        || index.name_size >= 256
                        || previous.is_some_and(|p| index.index <= p)
                        || !self
                            .directory_blocks
                            .contains_key(&(sb.directory_table + u64::from(index.block)))
                    {
                        return Err(invalid("invalid directory index").into());
                    }
                    let mut name = vec![0; index.name_size as usize + 1];
                    reader.read_exact(&mut name).await?;
                    let name = OsString::from_vec(name);
                    validate_name(&name)?;
                    previous = Some(index.index);
                    indexes.push((index, name));
                }
                if directory.size > 3 {
                    let reference =
                        (u64::from(directory.block) << 16) | u64::from(directory.offset);
                    let mut directory_reader = self
                        .metadata(
                            sb.directory_table,
                            self.directory_end,
                            &self.directory_blocks,
                            reference,
                        )
                        .await?
                        .take(u64::from(directory.size - 3));
                    let initial = self.directory_blocks
                        [&(sb.directory_table + u64::from(directory.block))]
                        + u64::from(directory.offset);
                    let mut indexes = indexes.into_iter().peekable();
                    let mut remaining = directory.size - 3;
                    let mut last_name: Option<OsString> = None;
                    while remaining != 0 {
                        if remaining < DirectoryHeader::SIZE as u32 {
                            return Err(invalid("truncated directory header").into());
                        }
                        let group_offset = directory.size - 3 - remaining;
                        let group = disk::<DirectoryHeader>(&mut directory_reader).await?;
                        remaining -= DirectoryHeader::SIZE as u32;
                        if group.count >= 256 {
                            return Err(invalid("invalid directory group size").into());
                        }
                        for entry_index in 0..=group.count {
                            if remaining < DirectoryEntry::SIZE as u32 {
                                return Err(invalid("truncated directory entry").into());
                            }
                            let entry = disk::<DirectoryEntry>(&mut directory_reader).await?;
                            let length = u32::from(entry.name_size) + 1;
                            if length > 256
                                || remaining - (DirectoryEntry::SIZE as u32) < length
                                || !(1..=7).contains(&entry.kind)
                            {
                                return Err(invalid("invalid directory entry").into());
                            }
                            let mut name = vec![0; length as usize];
                            directory_reader.read_exact(&mut name).await?;
                            let name = OsString::from_vec(name);
                            validate_name(&name)?;
                            if last_name.as_ref().is_some_and(|last| last >= &name) {
                                return Err(invalid("unordered or duplicate directory name").into());
                            }
                            if entry_index == 0 {
                                if indexes
                                    .peek()
                                    .is_some_and(|(index, _)| index.index < group_offset)
                                {
                                    return Err(invalid(
                                        "directory index does not point to a group",
                                    )
                                    .into());
                                }
                                if indexes
                                    .peek()
                                    .is_some_and(|(index, _)| index.index == group_offset)
                                {
                                    let (index, indexed_name) = indexes.next().unwrap();
                                    let physical = sb.directory_table + u64::from(index.block);
                                    let indexed_block = self.directory_blocks[&physical];
                                    let group_position = initial + u64::from(group_offset);
                                    let beyond_block = self
                                        .directory_blocks
                                        .range((physical + 1)..)
                                        .next()
                                        .is_some_and(|(_, logical)| group_position >= *logical);
                                    if indexed_name != name
                                        || group_position < indexed_block
                                        || beyond_block
                                    {
                                        return Err(invalid(
                                            "directory index disagrees with directory contents",
                                        )
                                        .into());
                                    }
                                }
                            }
                            last_name = Some(name.clone());
                            let number = i64::from(group.inode) + i64::from(entry.inode_delta);
                            if number <= 0 || number > i64::from(sb.inode_count) {
                                return Err(invalid("invalid directory inode number").into());
                            }
                            children.push(Child {
                                name,
                                reference: (u64::from(group.block) << 16) | u64::from(entry.offset),
                                number: number as u32,
                                kind: InodeKind::from_disk(entry.kind)?,
                            });
                            remaining -= DirectoryEntry::SIZE as u32 + length;
                        }
                    }
                    if indexes.next().is_some() {
                        return Err(invalid("unused directory index").into());
                    }
                } else if !indexes.is_empty() {
                    return Err(invalid("empty directory has indexes").into());
                }
                NodeContents::Directory(DirectoryContents::new())
            }
            InodeKind::Symlink => {
                let link = disk::<SymlinkInode>(&mut reader).await?;
                links = link.links;
                let mut target = Vec::new();
                let mut remaining = u64::from(link.size);
                let mut buffer = [0; METADATA_SIZE];
                while remaining != 0 {
                    let size = remaining.min(METADATA_SIZE as u64) as usize;
                    reader.read_exact(&mut buffer[..size]).await?;
                    if buffer[..size].contains(&0) {
                        return Err(invalid("NUL in symlink target").into());
                    }
                    target.extend_from_slice(&buffer[..size]);
                    remaining -= size as u64;
                }
                if extended {
                    xattr = u32_value(&mut reader).await?;
                }
                NodeContents::Symlink(PathBuf::from(OsString::from_vec(target)))
            }
            InodeKind::BlockDevice | InodeKind::CharacterDevice => {
                let device = if extended {
                    disk::<DeviceInode>(&mut reader).await?
                } else {
                    let d = disk::<BasicDeviceInode>(&mut reader).await?;
                    DeviceInode {
                        links: d.links,
                        device: d.device,
                        xattr: INVALID_INDEX,
                    }
                };
                links = device.links;
                xattr = device.xattr;
                let (major, minor) = device_parts(device.device);
                if kind == InodeKind::BlockDevice {
                    NodeContents::BlockDevice(Device { major, minor })
                } else {
                    NodeContents::CharacterDevice(Device { major, minor })
                }
            }
            InodeKind::Fifo | InodeKind::Socket => {
                let ipc = if extended {
                    disk::<IpcInode>(&mut reader).await?
                } else {
                    let ipc = disk::<BasicIpcInode>(&mut reader).await?;
                    IpcInode {
                        links: ipc.links,
                        xattr: INVALID_INDEX,
                    }
                };
                links = ipc.links;
                xattr = ipc.xattr;
                if kind == InodeKind::Fifo {
                    NodeContents::Fifo
                } else {
                    NodeContents::Socket
                }
            }
        };
        if links == 0 {
            return Err(invalid("inode has zero links").into());
        }
        let metadata = Metadata {
            mode: header.mode,
            uid,
            gid,
            modification_time: header.modification_time,
            xattrs: self.attributes(xattr).await?,
        };
        Ok(Parsed {
            header,
            node: Node { metadata, contents },
            children,
            parent,
            links,
            span: (start, reader.position()),
        })
    }
}

impl<S: ArchiveSource> Squashfs<SquashfsFileSource<S>> {
    /// Reads the inode tree while retaining archive-backed, lazy file contents.
    pub async fn deserialize(source: S) -> Result<Self, Error> {
        let image = Image::load(source).await?;
        let root = image.inode(image.superblock.root_inode).await?;
        if root.node.contents.kind() != InodeKind::Directory {
            return Err(invalid("SquashFS root is not a directory").into());
        }
        if u64::from(root.parent) != u64::from(image.superblock.inode_count) + 1 {
            return Err(invalid("invalid root directory parent").into());
        }
        let mut spans = BTreeMap::from([root.span]);
        let mut numbers = BTreeMap::from([(root.header.number, image.superblock.root_inode)]);
        let mut references = BTreeMap::from([(image.superblock.root_inode, 0usize)]);
        let mut nodes = vec![Some(root)];
        let mut paths = vec![PathBuf::new()];
        let mut entries: Vec<Vec<(OsString, Result<usize, PathBuf>)>> = vec![Vec::new()];
        let mut occurrences = vec![1u32];
        let mut queue = VecDeque::from([0usize]);
        while let Some(parent) = queue.pop_front() {
            let parent_number = nodes[parent].as_ref().unwrap().header.number;
            let children = std::mem::take(&mut nodes[parent].as_mut().unwrap().children);
            let mut subdirs = 0u32;
            for child in children {
                if let Some(&index) = references.get(&child.reference) {
                    let inode = nodes[index].as_ref().unwrap();
                    if inode.node.contents.kind() == InodeKind::Directory
                        || inode.header.number != child.number
                        || inode.node.contents.kind() != child.kind
                    {
                        return Err(invalid("cyclic directory or inconsistent hardlink").into());
                    }
                    occurrences[index] = occurrences[index]
                        .checked_add(1)
                        .ok_or_else(|| invalid("link count overflow"))?;
                    entries[parent].push((child.name, Err(paths[index].clone())));
                    continue;
                }
                if nodes.len() >= image.superblock.inode_count as usize {
                    return Err(invalid("more inodes than superblock declares").into());
                }
                let inode = image.inode(child.reference).await?;
                if inode.header.number != child.number
                    || inode.node.contents.kind() != child.kind
                    || numbers
                        .insert(inode.header.number, child.reference)
                        .is_some()
                {
                    return Err(invalid("inconsistent directory inode identity").into());
                }
                if spans
                    .range(..=inode.span.0)
                    .next_back()
                    .is_some_and(|(_, end)| *end > inode.span.0)
                    || spans
                        .range(inode.span.0..)
                        .next()
                        .is_some_and(|(start, _)| *start < inode.span.1)
                {
                    return Err(invalid("overlapping SquashFS inodes").into());
                }
                spans.insert(inode.span.0, inode.span.1);
                let index = nodes.len();
                if child.kind == InodeKind::Directory {
                    if inode.parent != parent_number {
                        return Err(invalid("incorrect directory parent").into());
                    }
                    subdirs += 1;
                    queue.push_back(index);
                }
                paths.push(paths[parent].join(&child.name));
                entries[parent].push((child.name, Ok(index)));
                references.insert(child.reference, index);
                nodes.push(Some(inode));
                entries.push(Vec::new());
                occurrences.push(1);
            }
            if nodes[parent].as_ref().unwrap().links != subdirs + 2 {
                return Err(invalid("incorrect directory link count").into());
            }
        }
        if nodes.len() != image.superblock.inode_count as usize {
            return Err(invalid("unreachable SquashFS inodes").into());
        }
        for (index, inode) in nodes.iter().enumerate() {
            let inode = inode.as_ref().unwrap();
            if inode.node.contents.kind() != InodeKind::Directory
                && (inode.header.kind > 7 || inode.header.kind != InodeKind::File.basic())
                && inode.links != occurrences[index]
            {
                return Err(invalid("incorrect inode link count").into());
            }
        }
        for index in (0..nodes.len()).rev() {
            let mut inode = nodes[index].take().unwrap();
            if let NodeContents::Directory(directory) = &mut inode.node.contents {
                for (name, entry) in std::mem::take(&mut entries[index]) {
                    let entry = match entry {
                        Ok(child) => Entry::Node(nodes[child].take().unwrap().node),
                        Err(path) => Entry::Hardlink(path),
                    };
                    directory.insert(name, entry)?;
                }
            }
            nodes[index] = Some(inode);
        }
        let root = nodes[0].take().unwrap().node;
        let NodeContents::Directory(contents) = root.contents else {
            unreachable!()
        };
        let mut result = Squashfs::new(root.metadata, contents);
        result.options = Options::new(image.superblock.block_size, 3)?;
        result.options.modification_time = image.superblock.modification_time;
        result.options.deduplicate = image.superblock.flags & DEDUPLICATED != 0;
        Ok(result)
    }
}

#[cfg(test)]
#[path = "read/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "read/index_tests.rs"]
mod index_tests;

#[cfg(test)]
#[path = "read/corruption_tests.rs"]
mod corruption_tests;

#[cfg(test)]
#[path = "read/xattr_tests.rs"]
mod xattr_tests;
