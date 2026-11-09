//! Fixed-size, independently sealed content blocks.
//!
//! Format 2 keeps each block as an authenticated record in the shared data
//! segments; format 1 keeps one stable-name file per block. A partial write
//! replaces one logical chunk.

use std::io;

use rand::rand_core::UnwrapErr;
use rand::{rng, Rng};

use crate::ctrl::Controller;
use crate::error::BkfsResult;
use crate::inode::ContentId;

const FRAMED_PAYLOAD: u8 = 2;

/// Logical chunk size: the unit rewritten by a partial write.
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
/// means the block is absent — a hole — which the caller reads as zeros.
/// The returned buffer may be shorter than `CHUNK_SIZE` (the final block) or,
/// for a migrated padded block, longer; callers must slice it to the
/// logically-valid length.
pub fn read_block(ctrl: &Controller, content: ContentId, idx: u64) -> BkfsResult<Option<Vec<u8>>> {
    let Some(stored) = ctrl.block_load(content, idx)? else {
        return Ok(None);
    };
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

/// Compress (per `codec`) and store one block whole. Durability follows the
/// next commit.
pub fn write_block(
    ctrl: &Controller,
    content: ContentId,
    idx: u64,
    plaintext: &[u8],
    codec: crate::compress::Codec,
) -> BkfsResult<()> {
    ctrl.block_put(content, idx, encode_block(ctrl, plaintext, codec))
}

pub fn write_final_block(
    ctrl: &Controller,
    content: ContentId,
    idx: u64,
    plaintext: &[u8],
    codec: crate::compress::Codec,
) -> BkfsResult<()> {
    if ctrl.is_legacy() {
        let mut padded = plaintext.to_vec();
        let len = padded.len();
        padded.resize(ctrl.file_pad(len as u64) as usize, 0);
        UnwrapErr(rng()).fill_bytes(&mut padded[len..]);
        return write_block(ctrl, content, idx, &padded, codec);
    }
    let mut stored = encode_block(ctrl, plaintext, codec);
    let len = stored.len();
    stored.resize(ctrl.file_pad(len as u64) as usize, 0);
    UnwrapErr(rng()).fill_bytes(&mut stored[len..]);
    ctrl.block_put(content, idx, stored)
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

/// Remove one block, tolerating an already-absent one.
pub fn remove_block(ctrl: &Controller, content: ContentId, idx: u64) -> BkfsResult<()> {
    ctrl.block_remove(content, idx)
}

/// Remove every block of a file whose logical size is `size`. Used by the
/// inode GC path when a file's last link goes away.
pub fn remove_all_blocks(ctrl: &Controller, content: ContentId, size: u64) -> BkfsResult<()> {
    for idx in 0..block_count(size) {
        remove_block(ctrl, content, idx)?;
    }
    Ok(())
}
