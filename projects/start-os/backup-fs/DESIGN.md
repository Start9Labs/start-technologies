# backup-fs storage design

`startos-backup-fs` is a FUSE filesystem whose backing store is an
encrypted, error-corrected, block-chunked directory tree. It is built to be
written through to physically-attached USB drives of any filesystem, to
CIFS shares, and to `rclone mount` backends (S3, SFTP, SSHFS, …), and to be
copied incrementally with `rsync`/`rclone`.

## On-disk layout

```
$data_dir/
  cryptinfo            sealed (PBKDF2) master key + IVs
  inode_pool           sealed free-inode allocator
  inodes/<bucket>/<id> one sealed file per inode (metadata + dir contents)
  contents/<bucket>/<name>   sealed content blocks, ≤ 1 MiB of plaintext each
```

Every object on disk is a **sealed blob** (see `vault.rs`). A file's data is
split into fixed-size **blocks** (see `blockstore.rs`); each block is its own
sealed file.

## Sealed blobs: encryption + integrity + ECC (`vault.rs`, `ecc.rs`)

`seal(plaintext)` →

1. `secret = plaintext || SHA-256(plaintext)`
2. encrypt `secret` with ChaCha20 under a fresh random nonce
3. split the ciphertext into `data` Reed-Solomon shards and compute
   `parity` extra shards (default 10 + 2, tunable via `BACKUPFS_ECC_DATA` /
   `BACKUPFS_ECC_PARITY`); store a CRC-32 with each shard

`open(blob)` verifies each shard's CRC, treats any failing shard as an
erasure, Reed-Solomon-reconstructs the ciphertext if **≤ parity** shards are
bad, decrypts, and checks the SHA-256 tag. The tag both detects residual
corruption and rejects a wrong password (`BadChecksum`).

ECC is computed over **ciphertext**, so on-medium bit rot is repaired before
decryption. Each block carries its own parity, so corruption in one block
never spreads to the rest of a file.

## Block-chunked content (`blockstore.rs`, `contents.rs`)

A regular file is `ceil(size / 1 MiB)` blocks. Writes do read-modify-write at
block granularity and a block is always written **whole** — buffered, then
atomically renamed into place. This is the core of the redesign:

- **No random writes into existing objects.** Backends that can only replace
  an object wholesale (S3/`rclone`) never see a sub-object overwrite; a small
  edit rewrites one ≤ 1 MiB block, not the whole file.
- **Cheap incremental backup.** A block's filename is a stable keyed
  SHA-256 of `(content_id, block_index)`. Editing one region rewrites exactly
  one block file and leaves every other block byte-for-byte identical, so
  `rsync`/`rclone` of `$data_dir` transfer only the changed blocks.
- **Small and large files both behave.** A tiny file is one small block file;
  a huge file is many independent blocks written/verified in parallel.
- **Sparse files** cost nothing for holes — unwritten blocks have no file on
  disk and read back as zeros.

## Cache-deadlock avoidance

Content blocks are read and written with `O_DIRECT` (via the aligned-I/O
buffer in `aligned_io.rs`, with an io_uring fast path, network-FS-aware flush
chunking, and a flush timeout). This keeps file data out of the kernel page
cache, avoiding the CIFS writeback deadlock where freeing memory needs a
flush and the flush needs memory. Small metadata goes through the page cache
and is made durable by the batched `syncfs` group-commit.

## Preserved Linux inode attributes

Inode metadata stores mode, uid/gid, atime/mtime/ctime/crtime, and the full
xattr map — so **POSIX ACLs** (`system.posix_acl_access` /
`system.posix_acl_default`) round-trip like any other xattr. `mknod` supports
regular files, directories, symlinks, FIFOs, sockets, and character/block
devices (with `rdev` preserved).

## What carried over unchanged

The inode/handle/controller layer — directory bookkeeping, the dirty-inode
coalescing cache, the clean-load LRU, crash-consistent rename/unlink/GC
ordering, the sharded worker pool, and the `syncfs` group-commit durability
path — is retained from the previous design; only the content-storage and
serialization layers were replaced.

## Small-file packing (log-structured metadata)

Layered on top of the block design to cut the per-file backing-store object
count, which dominates many-small-files backups to CIFS/rclone:

- **Hash-bucketed directories (Stage 1, shipped).** A directory's entries no
  longer live inline in its inode once it grows large; they spill into
  hash-bucketed sealed files, so a create touches one bucket (O(1)) instead
  of rewriting the whole listing (O(n) → O(n²) to build a directory).

- **Inodes in a log (Stage 3, shipped).** Inodes are append-only records in
  shared segment files (`segments/`, see `seglog.rs`), not one file per
  inode. A monotonic allocator + replay-rebuilt index; tombstone-on-delete
  durable before content removal.

- **Inline tiny content (Stage 5, shipped).** A file ≤ 4 KiB stores its bytes
  directly in its inode record — zero extra content objects. Larger files
  migrate to per-file 1 MiB block files.

- **Shared content packing + compaction (shipped).** A file in
  `(4 KiB, pack_max=1 MiB]` is stored as a single extent in the shared
  content log (`Record::Content` in `seglog`, a separate content index;
  the inode holds `FileData::Packed(content_id)`) rather than its own block
  file. Tiers: ≤ 4 KiB inline → ≤ 1 MiB packed → > 1 MiB blocks; growth
  migrates inline→packed→blocks one-way. `BACKUPFS_PACK_MAX` tunes/disables
  packing. **Compaction** (`seglog::compact`, on unmount, gated by
  `BACKUPFS_COMPACT_RATIO`, default 0.6) reclaims dead extents: live frames
  are relocated **verbatim** (no re-seal — keeps bytes stable for rsync,
  preserves `seq` for replay) into the active segment, durable before the
  old segment is unlinked. Local A/B (500 × 64 KiB files): ~27% faster than
  one-block-per-file; larger on CIFS.

### Deferred items (space/robustness, not correctness)

- Compaction of the inode/content log **mid-mount** (currently only on
  unmount) and streamed multi-generation checkpoints for fast remount on
  huge stores (replay currently decrypts every frame).
- A `find_orphans` sweep to reclaim directory bucket / content files orphaned
  by a crash mid-reshard or mid-GC.
- `fsync` of parent directories in `AtomicFile` (durability currently rides
  the trailing `syncfs`).
