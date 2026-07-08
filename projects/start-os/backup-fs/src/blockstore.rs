//! Content storage as fixed-size, independently-sealed blocks.
//!
//! A regular file's data is split into [`CHUNK_SIZE`] logical chunks. Each
//! chunk is stored as its own file on the backing store, sealed by
//! [`crate::vault`] (encryption + integrity + Reed-Solomon ECC). This block
//! layout is the core of the redesign and buys several properties at once:
//!
//! * **No random writes into existing objects.** A block is always written
//!   whole — buffered, then atomically renamed into place. Backends that
//!   can only replace an object wholesale (S3/`rclone mount`, and the like)
//!   never see a sub-object overwrite. A partial write read-modify-writes a
//!   single ≤1 MiB block, not the whole file.
//!
//! * **Cheap incremental backup.** A block's filename is a stable keyed
//!   hash of `(content_id, block_index)`, so editing one region of a large
//!   file rewrites exactly one block file and leaves every other block
//!   byte-for-byte identical. `rsync`/`rclone` copying the data directory
//!   transfer only the changed blocks.
//!
//! * **Small + large files both behave.** A tiny file is a single small
//!   block file; a huge file is many independent block files that can be
//!   written and verified in parallel.
//!
//! * **Per-block error correction.** Bit rot in one block is recovered from
//!   that block's parity shards and never spreads to the rest of the file.

use std::io::{self, Read, Write};

use crate::aligned_io::BufferedDirectFile;
use crate::atomic_file::AtomicFile;
use crate::ctrl::Controller;
use crate::error::BkfsResult;
use crate::inode::ContentId;
use crate::{open_direct, vault};

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
    let path = ctrl.resolve_block_path(content, idx);
    let mut file = match open_direct(&path, false) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut blob = Vec::new();
    file.read_to_end(&mut blob)?;
    // open (decrypt + ECC) → decompress back to the logical block bytes. A
    // block is at most one chunk, plus any size-padding applied to the final
    // block; cap the decompressed size accordingly so a malformed frame can't
    // allocate without bound.
    let stored = vault::open(&blob, ctrl.key())?;
    Ok(Some(crate::compress::decompress(
        &stored,
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
    ctrl.check_rw()?;
    // Compress BEFORE sealing — ciphertext is incompressible. Each block is
    // compressed independently, so a one-block edit recompresses only it.
    let stored = crate::compress::compress(plaintext, codec);
    let blob = vault::seal(&stored, ctrl.key(), ctrl.ecc());
    let mut file = BufferedDirectFile::new(AtomicFile::create(ctrl.block_path(content, idx))?)?;
    file.write_all(&blob)?;
    if durable {
        file.save()
    } else {
        file.save_fast()
    }
}

/// Remove one block file, tolerating an already-absent file.
pub fn remove_block(ctrl: &Controller, content: ContentId, idx: u64) -> BkfsResult<()> {
    match std::fs::remove_file(ctrl.resolve_block_path(content, idx)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Remove every block of a file whose logical size is `size`. Used by the
/// inode GC path when a file's last link goes away.
pub fn remove_all_blocks(ctrl: &Controller, content: ContentId, size: u64) -> BkfsResult<()> {
    for idx in 0..block_count(size) {
        remove_block(ctrl, content, idx)?;
    }
    Ok(())
}
