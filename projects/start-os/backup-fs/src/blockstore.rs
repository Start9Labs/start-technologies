//! Fixed-size, independently sealed content objects.
//!
//! COW stores append immutable versions; snapshot roots retain prior versions.
//! Partial writes replace one logical chunk.

use std::io::{self, Read, Write};

use rand::rand_core::UnwrapErr;
use rand::{rng, Rng};

use crate::aligned_io::BufferedDirectFile;
use crate::ctrl::Controller;
use crate::error::{BkfsError, BkfsResult};
use crate::inode::ContentId;
use crate::{open_direct, vault};

const FRAMED_PAYLOAD: u8 = 2;

/// Logical chunk size. Each chunk maps to one sealed block file. 1 MiB
/// balances per-file overhead (inode/dir-entry cost on the backing store)
/// against incremental-write granularity (the unit re-sent on change).
pub const CHUNK_SIZE: u64 = 1 << 20;

/// Number of blocks a file of `size` bytes occupies.
pub fn block_count(size: u64) -> u64 {
    size.div_ceil(CHUNK_SIZE)
}

/// The block index and intra-block offset for a logical file offset.
pub fn locate(offset: u64) -> (u64, usize) {
    ((offset / CHUNK_SIZE), (offset % CHUNK_SIZE) as usize)
}

/// Read one block's logical (decrypted, error-corrected) bytes. `Ok(None)`
/// means the block file is absent — a hole — which the caller reads as
/// zeros. The returned buffer may be shorter than `CHUNK_SIZE` (the final
/// block) or, with size padding enabled, longer; callers must slice it to
/// the logically-valid length.
pub fn read_block(ctrl: &Controller, content: ContentId, idx: u64) -> BkfsResult<Option<Vec<u8>>> {
    let Some((path, binding)) = ctrl.resolve_object(ctrl.block_id(content, idx))? else {
        return Ok(None);
    };
    let mut file = open_direct(&path, false).map_err(BkfsError::referenced_io)?;
    let mut blob = Vec::new();
    file.read_to_end(&mut blob)
        .map_err(BkfsError::referenced_io)?;
    let stored = vault::open_bound(&blob, ctrl.key(), binding)?.plaintext;
    let compressed = if stored.first() == Some(&FRAMED_PAYLOAD) {
        let len = stored
            .get(1..5)
            .ok_or_else(|| io::Error::other("truncated block payload"))?;
        let len = u32::from_le_bytes(len.try_into().unwrap()) as usize;
        stored
            .get(5..)
            .and_then(|bytes| bytes.get(..len))
            .ok_or_else(|| io::Error::other("invalid block payload length"))?
    } else {
        stored.as_slice()
    };
    Ok(Some(crate::compress::decompress(
        compressed,
        max_block_len(ctrl),
    )?))
}

/// Upper bound on a decompressed block: one chunk, grown by the configured
/// size-padding factor (the final block may be padded up to `len * (1+pad)`).
fn max_block_len(ctrl: &Controller) -> usize {
    let pad = ctrl.config().file_size_padding.unwrap_or(0.0).max(0.0);
    ((CHUNK_SIZE as f64) * (1.0 + pad)).ceil() as usize + 16
}

/// Compress (per `codec`), seal, and write one block whole. `durable`
/// selects `sync_all` (the crash-safe path used on explicit syncs) versus
/// the batched fast path (rename only; durability deferred to a later
/// `syncfs`).
pub fn write_block(
    ctrl: &Controller,
    content: ContentId,
    idx: u64,
    plaintext: &[u8],
    codec: crate::compress::Codec,
    durable: bool,
) -> BkfsResult<()> {
    store_block(
        ctrl,
        content,
        idx,
        &encode_block(ctrl, plaintext, codec),
        durable,
    )
}

pub fn write_final_block(
    ctrl: &Controller,
    content: ContentId,
    idx: u64,
    plaintext: &[u8],
    codec: crate::compress::Codec,
    durable: bool,
) -> BkfsResult<()> {
    if ctrl.is_legacy() {
        let mut padded = plaintext.to_vec();
        let len = padded.len();
        padded.resize(ctrl.file_pad(len as u64) as usize, 0);
        UnwrapErr(rng()).fill_bytes(&mut padded[len..]);
        return write_block(ctrl, content, idx, &padded, codec, durable);
    }
    let mut stored = encode_block(ctrl, plaintext, codec);
    let len = stored.len();
    stored.resize(ctrl.file_pad(len as u64) as usize, 0);
    UnwrapErr(rng()).fill_bytes(&mut stored[len..]);
    store_block(ctrl, content, idx, &stored, durable)
}

fn encode_block(ctrl: &Controller, plaintext: &[u8], codec: crate::compress::Codec) -> Vec<u8> {
    let compressed = crate::compress::compress(plaintext, codec);
    if ctrl.is_legacy() {
        return compressed;
    }
    let mut stored = Vec::with_capacity(5 + compressed.len());
    stored.push(FRAMED_PAYLOAD);
    stored.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    stored.extend_from_slice(&compressed);
    stored
}

fn store_block(
    ctrl: &Controller,
    content: ContentId,
    idx: u64,
    stored: &[u8],
    durable: bool,
) -> BkfsResult<()> {
    ctrl.check_rw()?;
    ctrl.write_object(ctrl.block_id(content, idx), durable, |path, binding| {
        let blob = vault::seal_bound(stored, ctrl.key(), ctrl.ecc(), binding);
        let mut file = BufferedDirectFile::create(path)?;
        file.write_all(&blob.bytes)?;
        file.save_fast()
    })
}

/// Remove one block file, tolerating an already-absent file.
pub fn remove_block(ctrl: &Controller, content: ContentId, idx: u64) -> BkfsResult<()> {
    ctrl.remove_object(ctrl.block_id(content, idx))
}

/// Remove every block of a file whose logical size is `size`. Used by the
/// inode GC path when a file's last link goes away.
pub fn remove_all_blocks(ctrl: &Controller, content: ContentId, size: u64) -> BkfsResult<()> {
    for idx in 0..block_count(size) {
        remove_block(ctrl, content, idx)?;
    }
    Ok(())
}
