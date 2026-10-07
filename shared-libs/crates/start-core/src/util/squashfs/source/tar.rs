use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::SeekFrom;
use std::os::unix::ffi::OsStringExt;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_tar::{GnuExtSparseHeader, GnuSparseHeader, Header};

use super::*;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::s9pk::merkle_archive::source::{ArchiveSource, Section, TmpSource};
use crate::util::io::{TmpDir, create_file};

type Payload = Section<TmpSource<MultiCursorFile>>;
type Pax = BTreeMap<Vec<u8>, Vec<u8>>;
const EXTENSION_LIMIT: u64 = 16 * 1024 * 1024;
const EXTENT_LIMIT: usize = 1_000_000;

fn decimal(bytes: &[u8]) -> std::io::Result<u64> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(invalid("invalid unsigned tar decimal"));
    }
    bytes.iter().try_fold(0u64, |n, b| {
        n.checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as u64))
            .ok_or_else(|| invalid("tar decimal overflow"))
    })
}

fn apply(pax: &mut Pax, key: Vec<u8>, value: Vec<u8>) {
    if value.is_empty() {
        pax.remove(&key);
    } else {
        pax.insert(key, value);
    }
}

fn pax_records(mut bytes: &[u8]) -> std::io::Result<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut records = Vec::new();
    while !bytes.is_empty() {
        let space = bytes
            .iter()
            .position(|b| *b == b' ')
            .ok_or_else(|| invalid("invalid PAX length"))?;
        let len = usize::try_from(decimal(&bytes[..space])?)
            .map_err(|_| invalid("PAX length overflow"))?;
        if len <= space + 2 || len > bytes.len() || bytes[len - 1] != b'\n' {
            return Err(invalid("truncated PAX record"));
        }
        let record = &bytes[space + 1..len - 1];
        let eq = record
            .iter()
            .position(|b| *b == b'=')
            .filter(|i| *i > 0)
            .ok_or_else(|| invalid("invalid PAX record"))?;
        records.push((record[..eq].to_vec(), record[eq + 1..].to_vec()));
        bytes = &bytes[len..];
    }
    Ok(records)
}

async fn exact_copy<R: AsyncRead + Unpin, W: tokio::io::AsyncWrite + Unpin>(
    reader: &mut R,
    writer: &mut W,
    count: u64,
) -> std::io::Result<()> {
    if tokio::io::copy(&mut reader.take(count), writer).await? != count {
        return Err(std::io::ErrorKind::UnexpectedEof.into());
    }
    Ok(())
}

async fn padding<R: AsyncRead + Unpin>(reader: &mut R, size: u64) -> std::io::Result<()> {
    exact_copy(reader, &mut tokio::io::sink(), (512 - size % 512) % 512).await
}

async fn extension<R: AsyncRead + Unpin>(reader: &mut R, size: u64) -> std::io::Result<Vec<u8>> {
    if size > EXTENSION_LIMIT {
        return Err(invalid("tar extension exceeds bounded metadata limit"));
    }
    let mut bytes = vec![0; size as usize];
    reader.read_exact(&mut bytes).await?;
    padding(reader, size).await?;
    Ok(bytes)
}

fn number(
    pax: &Pax,
    key: &[u8],
    fallback: impl FnOnce() -> std::io::Result<u64>,
) -> std::io::Result<u64> {
    match pax.get(key) {
        Some(value) => decimal(value),
        None => fallback(),
    }
}

fn percent_decode(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut decoded = Vec::new();
    let mut bytes = bytes.iter().copied();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes.next().and_then(|b| (b as char).to_digit(16));
            let low = bytes.next().and_then(|b| (b as char).to_digit(16));
            decoded.push(
                ((high.ok_or_else(|| invalid("invalid xattr name escape"))? << 4)
                    | low.ok_or_else(|| invalid("invalid xattr name escape"))?)
                    as u8,
            );
        } else {
            decoded.push(byte);
        }
    }
    Ok(decoded)
}

fn metadata(header: &Header, pax: &Pax) -> std::io::Result<Metadata> {
    let mode = header.mode()? & 0o7777;
    let mtime = match pax.get(b"mtime".as_slice()) {
        Some(bytes) => {
            let bytes = bytes.strip_prefix(b"+").unwrap_or(bytes);
            let mut parts = bytes.splitn(2, |b| *b == b'.');
            let whole = decimal(parts.next().unwrap())?;
            if let Some(frac) = parts.next()
                && (frac.is_empty() || !frac.iter().all(u8::is_ascii_digit))
            {
                return Err(invalid("invalid PAX fractional timestamp"));
            }
            whole
        }
        None => header.mtime()?,
    };
    let mut xattrs = BTreeMap::new();
    for (key, value) in pax {
        if let Some(name) = key.strip_prefix(b"SCHILY.xattr.") {
            xattrs.insert(OsString::from_vec(name.to_vec()), value.clone());
        }
        if let Some(name) = key.strip_prefix(b"LIBARCHIVE.xattr.") {
            use base64::Engine;
            let name = percent_decode(name)?;
            let value = crate::util::serde::BASE64
                .decode(value)
                .map_err(|_| invalid("invalid LIBARCHIVE xattr encoding"))?;
            xattrs.insert(OsString::from_vec(name), value);
        }
    }
    xattrs.retain(|name, _| xattr_kind(name.as_bytes()).is_some());
    Ok(Metadata {
        mode: mode as u16,
        uid: checked_u32(number(pax, b"uid", || header.uid())?)?,
        gid: checked_u32(number(pax, b"gid", || header.gid())?)?,
        modification_time: checked_u32(mtime)?,
        xattrs,
    })
}

fn extent(map: &mut Vec<(u64, u64)>, offset: u64, length: u64) -> std::io::Result<()> {
    if map.len() == EXTENT_LIMIT {
        return Err(invalid("tar sparse map exceeds bounded extent limit"));
    }
    map.push((offset, length));
    Ok(())
}

fn gnu_extents(map: &mut Vec<(u64, u64)>, items: &[GnuSparseHeader]) -> std::io::Result<()> {
    for item in items {
        if item.is_empty() {
            if (item.offset[0] == 0) != (item.numbytes[0] == 0) {
                return Err(invalid("incomplete GNU sparse extent"));
            }
        } else {
            extent(map, item.offset()?, item.length()?)?;
        }
    }
    Ok(())
}

async fn sparse_line<R: AsyncRead + Unpin>(
    reader: &mut R,
    remaining: &mut u64,
    consumed: &mut u64,
) -> std::io::Result<u64> {
    let mut bytes = Vec::new();
    loop {
        if *remaining == 0 {
            return Err(invalid("truncated sparse map"));
        }
        let b = reader.read_u8().await?;
        *remaining -= 1;
        *consumed += 1;
        if b == b'\n' {
            return decimal(&bytes);
        }
        if bytes.len() == 20 {
            return Err(invalid("sparse map decimal exceeds u64"));
        }
        bytes.push(b);
    }
}

async fn sparse_map<R: AsyncRead + Unpin>(
    reader: &mut R,
    header: &Header,
    pax: &Pax,
    repeated: &[(Vec<u8>, Vec<u8>)],
    physical: &mut u64,
) -> std::io::Result<Option<(u64, Vec<(u64, u64)>)>> {
    let mut map = Vec::new();
    if header.entry_type().is_gnu_sparse() {
        let gnu = header
            .as_gnu()
            .ok_or_else(|| invalid("GNU sparse entry lacks GNU header"))?;
        gnu_extents(&mut map, &gnu.sparse)?;
        let mut extended = gnu.is_extended();
        while extended {
            let mut block = GnuExtSparseHeader::new();
            reader.read_exact(block.as_mut_bytes()).await?;
            gnu_extents(&mut map, block.sparse())?;
            extended = block.is_extended();
        }
        return Ok(Some((gnu.real_size()?, map)));
    }
    if !pax.keys().any(|key| key.starts_with(b"GNU.sparse.")) {
        return Ok(None);
    }
    let logical = number(pax, b"GNU.sparse.realsize", || {
        number(pax, b"GNU.sparse.size", || {
            Err(invalid("missing sparse real size"))
        })
    })?;
    if let Some(major) = pax.get(b"GNU.sparse.major".as_slice())
        && major != b"0"
        && major != b"1"
    {
        return Err(invalid("unsupported GNU sparse version"));
    }
    if pax.get(b"GNU.sparse.major".as_slice()).map(Vec::as_slice) == Some(b"1") {
        if pax.get(b"GNU.sparse.minor".as_slice()).map(Vec::as_slice) != Some(b"0") {
            return Err(invalid("unsupported GNU sparse version"));
        }
        let mut consumed = 0;
        let count = sparse_line(reader, physical, &mut consumed).await?;
        if count > EXTENT_LIMIT as u64 {
            return Err(invalid("too many sparse extents"));
        }
        for _ in 0..count {
            let offset = sparse_line(reader, physical, &mut consumed).await?;
            let length = sparse_line(reader, physical, &mut consumed).await?;
            extent(&mut map, offset, length)?;
        }
        let pad = (512 - consumed % 512) % 512;
        if pad > *physical {
            return Err(invalid("truncated sparse map padding"));
        }
        exact_copy(reader, &mut tokio::io::sink(), pad).await?;
        *physical -= pad;
    } else if let Some(bytes) = pax.get(b"GNU.sparse.map".as_slice()) {
        let mut fields = bytes.split(|b| *b == b',');
        while let Some(offset) = fields.next() {
            let length = fields
                .next()
                .ok_or_else(|| invalid("incomplete sparse map"))?;
            extent(&mut map, decimal(offset)?, decimal(length)?)?;
        }
    } else {
        let mut offset = None;
        for (key, value) in repeated {
            match key.as_slice() {
                b"GNU.sparse.offset" => {
                    if offset.replace(decimal(value)?).is_some() {
                        return Err(invalid("incomplete sparse extent"));
                    }
                }
                b"GNU.sparse.numbytes" => extent(
                    &mut map,
                    offset
                        .take()
                        .ok_or_else(|| invalid("missing sparse offset"))?,
                    decimal(value)?,
                )?,
                _ => (),
            }
        }
        if offset.is_some() {
            return Err(invalid("incomplete sparse extent"));
        }
    }
    if let Some(count) = pax.get(b"GNU.sparse.numblocks".as_slice())
        && decimal(count)? != map.len() as u64
    {
        return Err(invalid("sparse extent count mismatch"));
    }
    Ok(Some((logical, map)))
}

impl Squashfs<Payload> {
    /// Spools payloads without extracting archive paths onto the host filesystem.
    /// Rejects extensions above 16 MiB and sparse maps above one million extents.
    pub async fn from_tar<R: AsyncRead + Unpin + Send>(mut reader: R) -> Result<Self, Error> {
        let tmp = Arc::new(TmpDir::new().await?);
        let mut spool = create_file(tmp.join("payload")).await?;
        let backing = TmpSource::new(tmp, MultiCursorFile::open(&spool).await?);
        let mut image = Self::new(Metadata::new(0o755), DirectoryContents::new());
        let mut position = 0u64;
        let mut global = Pax::new();
        let mut local = Vec::new();
        let mut pending_extension = false;
        let mut extension_bytes = 0u64;
        let mut longname = None;
        let mut longlink = None;
        loop {
            let mut header = Header::new_old();
            let first = reader.read(header.as_mut_bytes()).await?;
            if first == 0 {
                return Err(invalid("tar missing end marker").into());
            }
            reader
                .read_exact(&mut header.as_mut_bytes()[first..])
                .await?;
            if header.as_bytes().iter().all(|b| *b == 0) {
                if pending_extension || longname.is_some() || longlink.is_some() {
                    return Err(invalid("dangling tar extension").into());
                }
                let mut second = [0; 512];
                reader.read_exact(&mut second).await?;
                if second.iter().any(|b| *b != 0) {
                    return Err(invalid("invalid tar end marker").into());
                }
                tokio::io::copy(&mut reader, &mut tokio::io::sink()).await?;
                spool.flush().await?;
                return Ok(image);
            }
            let signed = header
                .as_bytes()
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    if (148..156).contains(&i) {
                        32
                    } else {
                        *b as i8 as i64
                    }
                })
                .sum::<i64>();
            let mut checksum_header = header.clone();
            checksum_header.set_cksum();
            if header.cksum()? != checksum_header.cksum()? && i64::from(header.cksum()?) != signed {
                return Err(invalid("invalid tar checksum").into());
            }
            let ty = header.entry_type().as_byte();
            if matches!(ty, b'x' | b'g' | b'L' | b'K') {
                let size = header.entry_size()?;
                extension_bytes = extension_bytes
                    .checked_add(size)
                    .ok_or_else(|| invalid("tar extension size overflow"))?;
                if extension_bytes > EXTENSION_LIMIT {
                    return Err(invalid("tar extensions exceed bounded metadata limit").into());
                }
                pending_extension = true;
                let bytes = extension(&mut reader, size).await?;
                match ty {
                    b'x' => local.extend(pax_records(&bytes)?),
                    b'g' => {
                        for (key, value) in pax_records(&bytes)? {
                            apply(&mut global, key, value);
                        }
                    }
                    _ => {
                        let bytes = bytes.strip_suffix(&[0]).unwrap_or(&bytes).to_vec();
                        if bytes.contains(&0) {
                            return Err(invalid("NUL inside GNU long path").into());
                        }
                        if ty == b'L' {
                            longname = Some(bytes);
                        } else {
                            longlink = Some(bytes);
                        }
                    }
                }
                continue;
            }
            let mut pax = global.clone();
            for (key, value) in &local {
                apply(&mut pax, key.clone(), value.clone());
            }
            let name = pax
                .get(b"GNU.sparse.name".as_slice())
                .or_else(|| pax.get(b"path".as_slice()))
                .cloned()
                .or_else(|| longname.take())
                .unwrap_or_else(|| header.path_bytes().into_owned());
            let path = raw_path(&name)?;
            let link = pax
                .get(b"linkpath".as_slice())
                .cloned()
                .or_else(|| longlink.take())
                .or_else(|| header.link_name_bytes().map(|b| b.into_owned()));
            longname = None;
            longlink = None;
            let metadata = metadata(&header, &pax)?;
            let physical = number(&pax, b"size", || header.entry_size())?;
            let mut remaining = physical;
            let sparse = sparse_map(&mut reader, &header, &pax, &local, &mut remaining).await?;
            if !matches!(ty, 0 | b'0' | b'7' | b'S') && (remaining != 0 || sparse.is_some()) {
                return Err(invalid("non-file tar entry has payload").into());
            }
            let entry = match ty {
                0 | b'0' | b'7' | b'S' => {
                    let (logical, map) = sparse.unwrap_or((remaining, vec![(0, remaining)]));
                    let end = position
                        .checked_add(logical)
                        .ok_or_else(|| invalid("payload spool size overflow"))?;
                    if end > i64::MAX as u64 {
                        return Err(invalid("payload spool exceeds filesystem offset limit").into());
                    }
                    let mut previous = 0u64;
                    let mut total = 0u64;
                    for (offset, length) in &map {
                        let extent_end = offset
                            .checked_add(*length)
                            .ok_or_else(|| invalid("sparse extent overflow"))?;
                        if *offset < previous || extent_end > logical {
                            return Err(
                                invalid("overlapping or out-of-bounds sparse extent").into()
                            );
                        }
                        previous = extent_end;
                        total = total
                            .checked_add(*length)
                            .ok_or_else(|| invalid("sparse payload overflow"))?;
                    }
                    if total != remaining {
                        return Err(invalid("sparse payload byte count mismatch").into());
                    }
                    for (offset, length) in map {
                        spool.seek(SeekFrom::Start(position + offset)).await?;
                        exact_copy(&mut reader, &mut spool, length).await?;
                    }
                    spool.set_len(end).await?;
                    let source = backing.clone().section(position, logical);
                    position = end;
                    Entry::file(metadata, source)
                }
                b'1' => Entry::Hardlink(raw_path(
                    &link.ok_or_else(|| invalid("missing hardlink target"))?,
                )?),
                b'2' => {
                    let link = link.ok_or_else(|| invalid("missing symlink target"))?;
                    if link.contains(&0) {
                        return Err(invalid("NUL in symlink target").into());
                    }
                    Entry::new(
                        metadata,
                        NodeContents::Symlink(OsString::from_vec(link).into()),
                    )
                }
                b'3' | b'4' => {
                    let device = Device {
                        major: checked_u32(number(&pax, b"SCHILY.devmajor", || {
                            Ok(header.device_major()?.unwrap_or(0) as u64)
                        })?)?,
                        minor: checked_u32(number(&pax, b"SCHILY.devminor", || {
                            Ok(header.device_minor()?.unwrap_or(0) as u64)
                        })?)?,
                    };
                    Entry::new(
                        metadata,
                        if ty == b'3' {
                            NodeContents::CharacterDevice(device)
                        } else {
                            NodeContents::BlockDevice(device)
                        },
                    )
                }
                b'5' => Entry::directory(metadata, DirectoryContents::new()),
                b'6' => Entry::new(metadata, NodeContents::Fifo),
                _ => return Err(invalid("unsupported tar entry type").into()),
            };
            padding(&mut reader, physical).await?;
            insert(&mut image, path, entry)?;
            local.clear();
            pending_extension = false;
            extension_bytes = global
                .iter()
                .map(|(key, value)| (key.len() + value.len()) as u64)
                .sum();
        }
    }
}
