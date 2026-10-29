# backup-fs storage design

`startos-backup-fs` is a FUSE filesystem whose backing store is an
encrypted, error-corrected, block-chunked directory tree. It is built to be
written through to physically-attached USB drives of any filesystem, to
CIFS shares, and to `rclone mount` backends (S3, SFTP, SSHFS, …), and to be
copied incrementally with `rsync`/`rclone`.

## On-disk layout

```
$data_dir/
  superblock           versioned envelope and sealed master key + format constants
  superblock.bak1      redundant superblock replica
  segments/*.seg       append-only sealed inode and packed-content records
  segments/index.cache sealed, rebuildable index checkpoint
  segments/roots       authoritative replay floor for snapshot-enabled stores
  snapshots            sealed catalog of named recovery roots
  contents/<bucket>/<name>[.<version>]   sealed content blocks
  dirents/<bucket>/<name>[.<version>]    sealed directory buckets
```

Every object on disk is a **sealed blob** (see `vault.rs`). A file's data is
split into fixed-size **blocks** (see `blockstore.rs`); each block is its own
sealed file.

## Sealed blobs: encryption + integrity + ECC (`vault.rs`, `ecc.rs`)

`seal(plaintext)` →

1. Derive independent encryption and authentication subkeys with domain-separated HMAC-SHA256.
2. Encrypt plaintext with ChaCha20 under a fresh random nonce.
3. Authenticate the vault header and ciphertext with HMAC-SHA256.
4. Reed-Solomon-encode the ciphertext and authentication tag with `data` and `parity` shards; store a CRC-32 with each shard.

`open(blob)` treats CRC failures and missing shards as erasures, reconstructs up to `parity` lost shards, verifies the MAC in constant time, and only then decrypts. A wrong key, unauthenticated modification, or corruption beyond recovery returns `BadChecksum`. New stores default to 10 data + 2 parity shards; creation-time overrides are persisted in the superblock.

Vault version 2 rejects version-1 blobs during normal reads. A writable open upgrades an existing store once: it durably marks every superblock replica as migration-pending, atomically reseals each object and segment, then clears the marker after synchronizing the replacements. An interrupted upgrade resumes from that marker; read-only opens refuse an unfinished upgrade. Migration needs temporary space for one replacement segment or object and rewrites the existing encrypted data. Older binaries cannot read an upgraded store.

Unreadable objects and frames stay in place and fail normal authentication; they do not block migration of healthy data. Filesystem I/O failures keep the pending marker for retry.

Migration cannot establish whether legacy data was altered before the upgrade. Tags authenticate each blob, not its file path or log position; substitution or replay of valid same-store blobs remains possible. Previously copied weak key-wrapping metadata remains outside its protection.

Authenticated storage uses filesystem format 2. Creating the first snapshot explicitly enables format 3; ordinary writable opens retain format 2 until then. The COW capability remains enabled after snapshots are deleted.

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
- **Padding is separate from logical data.** A block payload records its compressed length before optional random padding. Growth cannot expose padding bytes; legacy final-block padding is clipped before extending the file.
- **A decoded block is kept for the reads that follow it.** A handle from
  `open` is served `FOPEN_DIRECT_IO` and the backing store is read
  `O_DIRECT`, so a read of that handle reaches us at no more than the
  caller's own size, with no page cache and no readahead behind it. (The
  transport caps one request at just under a chunk, so a 1 MiB read arrives
  as two.) Reading 8 KiB at a time would otherwise fetch and decode the whole
  enclosing block once per read — 154× the file's own bytes off the backing
  store, since each fetch carries the block's parity shards too. One block is
  held per open inode, shared by
  every handle on it, and freed when the last one closes; a file being read
  therefore costs one block of memory. A write moves the block into the dirty
  map and takes it out of the cache, and a flush drops a cached block its
  trailing-block prune is about to unlink, so no cached copy outlives the
  version on disk.

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

## Read-only snapshots

`BackupFSOptions::enable_snapshots` enables COW storage during an exclusive writable open. The helper's `snapshot create` command sets this option automatically. `BackupFS::create_snapshot` flushes dirty inodes and open contents, synchronizes the backing filesystem, saves authoritative log roots, and durably publishes a named recovery point. Unchanged file content is shared: creation copies root indices, not file data. Later writes create immutable versions of affected blocks or directory buckets and append pointer records to the existing log.

The authenticated catalog pins inode and packed-content log segments and retains the block/bucket versions each point uses. Existing stable-name objects are pinned without copying. Compaction skips pinned segments and saves an authoritative replay floor before deleting old segments; tombstoned records in retained history cannot resurrect current objects. Deletion removes the catalog entry durably before reclaiming storage unused by the live tree or retained snapshots. Object overrides survive while their stable-name legacy files remain on disk, including between replay-root publication and unlink. A later synchronized collection can discard those masks once removal is durable. A failed catalog publication blocks mutations until reopen.

Snapshot creation, listing, deletion, and recovery mounts use the existing exclusive store lock. These are offline operations; a writer and a recovery mount do not share the store concurrently. Recovery opens force read-only behavior throughout the filesystem and avoid atime writes. Names contain 1–128 ASCII letters, digits, `-`, `_`, or `.`, excluding `.` and `..`.

The catalog stores flat indices and uses the existing 64 MiB metadata decode bound. Creation is proportional to the indexed namespace, not constant-time; very large or numerous snapshots can reach that bound. Marking scans logical block slots for legacy/sparse files. Pinned segments retain dead neighbors until their final pin is released. Retention is explicit and manual.

Copies of an enabled store must include `snapshots` and `segments/roots`: both are authoritative metadata, unlike `segments/index.cache`.

## Durability and dispatch

The dirty-inode cache, clean-load LRU, per-inode worker ordering, and `syncfs` group commits remain shared with ordinary backups. Fast object saves defer durability to the existing batch checkpoint; explicit durable saves synchronize the object before its pointer record. Snapshot publication and reclamation synchronize the live state before committing authoritative roots.

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
