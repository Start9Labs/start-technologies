# backup-fs storage design

`startos-backup-fs` is an encrypted, error-corrected FUSE filesystem for StartOS
backups on physical drives and network filesystems. Its data lives in a small
number of append-only segment files, so backups to a high-latency share pay
few metadata round trips and keep many writes in flight. A backing filesystem
must support the file, rename, and durability operations used here; an object
transport alone does not supply those semantics.

## On-disk layout

```text
$data_dir/
  superblock                  password-protected master key and format constants
  superblock.bak1             redundant superblock replica
  segments/state              authenticated current index, counters, catalog reference
  segments/*.idx              packed authenticated index transactions
  segments/state-objects/*    immutable index checkpoints and snapshot catalogs
  segments/*.seg              sealed inode, directory-bucket, packed-content, and block records
```

Filesystem format 2 keeps every record in append-only segments. Named snapshots
retain historical views; they do not activate a separate storage mode. Copy the
complete backing directory while it is offline: the index, catalog, and their
referenced segments are authoritative. The legacy `index.cache` may remain after
migration but is not consulted by normal format-2 opens.

## Encryption, references, and ECC

Format 1 retains its original encryption/checksum protocol, one file per block and
directory bucket, and stable-name writes until explicit conversion. The
authenticated references below apply to format 2.

`vault::seal_bound` derives context-specific keys for superblocks, records,
transactions, and catalogs. Independent encryption and MAC subkeys protect
ChaCha20 ciphertext and its vault header with HMAC-SHA256. Fresh random nonces
protect repeated plaintext. Reed-Solomon coding covers the ciphertext and
authentication tag; each shard carries a CRC-32. Stores default to ten data and
two parity shards, with creation-time overrides persisted in the superblock.

Reads repair recoverable erasures, verify the MAC in constant time, then decrypt.
A log reference supplies the physical location, sequence, framed and payload
lengths, and expected authentication tag. The record's decoded key — inode,
packed content, file block, or directory bucket — must also match its caller.
Rewriting an unkeyed frame CRC or sequence does not authorize replacement
ciphertext.

Index transactions authenticate their parent references and the committed length
of every segment. The small `segments/state` anchor selects the current graph and
catalog; ordinary opens never recover intent from uncommitted segment tails or an
optional cache, and a writable open deletes segments no committed index knows.
Expected tags identify corrected authenticated content, not raw ECC bytes.
Missing or mistyped referenced data reports EIO; an absent block reference or
deletion denotes a hole. Corruption in an individual record remains localized,
while missing or corrupt namespace/index metadata blocks access to the affected
view.

A genuine older authenticated root can select a genuine older backup view. There
is no external freshness authority or whole-backup rollback prevention. The
pre-unlock discovery timestamp is editable plaintext; `BackupInfo.timestamp` in
encrypted `metadata.json` authenticates the date of the selected backup view.

## Segments and writes

Namespace records (inodes and directory buckets) and file data (blocks and packed
content) append to separate active segments, so a view's metadata stays dense
for lookups. A segment closes at the creation-time `segment_size` (64 MiB by
default) and each mount starts new ones.

Frames are copied into aligned 1 MiB units. A full unit is written by a pool of
threads while later frames fill the next one, with up to 16 MiB in flight
(`BACKUPFS_WRITE_IN_FLIGHT`, in MiB). A commit pads each open unit to a 4 KiB
boundary, so no byte range is ever written twice. Frames not yet readable from
their files are served from memory. A write that makes no progress within
`BACKUPFS_FLUSH_TIMEOUT_SECS` (120 s) fails the store closed. The threads start
on first use in the mounting process.

Reads keep segment files open. Records up to 64 KiB are served from cached 1 MiB
windows of committed segment bytes; larger records are read exactly.

## Durability and commits

State owns publication durability. Every record write is appended and published
by the next commit; ordering between related writes needs no earlier commit
because a commit publishes all of them atomically and storage is reclaimed only
by compaction after publication. Losing power or the process reverts the store
to its last commit.

Each commit records the whole filesystem at one moment between requests. Once
4,096 saves accumulate (`BACKUPFS_SYNC_BATCH` overrides it), the request that
finishes next commits: it locks every open file, logs their buffered content and
attributes, the dirty inode cache, and changed directory chunks, then publishes.
A file being written is recorded as it was at that moment. FUSE fsync, fsyncdir
and syncfs commit the same way; so do snapshot operations and unmount.

A commit waits for its segment writes, synchronizes every segment and index
journal written since the previous commit, synchronizes the index directories,
then atomically replaces `segments/state` and synchronizes its directory. These
per-file barriers send server flushes on network filesystems, where `syncfs`
sends none. A failed backing write or barrier, or an uncertain publication,
blocks mutations until reopen, even if a later barrier succeeds; uncommitted
tails do not override the previous root. Format 1 deletes object files
immediately, so it commits the records that stop referencing them before
reaping directory buckets or removing a deleted file's content.

Index deltas share append-only 8 MiB journal files rather than a file per
mutation. Each commit's transactions start on a fresh 4 KiB block, and segment
writes are padded to 4 KiB at every commit, so a torn write during power loss
reaches only uncommitted bytes. On a network share, directory renames and
deletions are durable when the server's filesystem commits its metadata, in
order; a lost rename reverts to the previous commit together with every later
deletion. Coarse checkpoints shard maps into chunks of at most 16,384 entries and
enforce decoder budgets before publication. Unmount writes a checkpoint only once
the journal holds more than 65,536 entries and more than a quarter of the
checkpoint's own entries; snapshot operations always checkpoint. Unchanged
checkpoints reuse their existing references without rereading newly written
chunks. Bulk reference scans sort borrowed locations and reuse enclosing buffered
windows and open segment files; index walks use the same range reader in reverse
order.

## Legacy migration

Ordinary opens keep pre-existing filesystem format 1 readable and writable without
conversion. `startos-backup-fs convert` explicitly converts a closed store to format 2
under the same exclusive lock used by mounts. Every superblock replica first receives
an authenticated pending marker. Ordinary opens refuse pending stores until conversion
is explicitly resumed. Migration reseals log frames in place,
preserving their locations, and publishes an index of them. Older binaries
reject the upgraded format. Format 1 has one file per block and directory bucket
and no named snapshots.

In-place segment replacements sync their temporary data before rename, then sync
the containing directory.

Format-1 block and bucket files referenced by a live inode are then read in
parallel, resealed as records, and appended to segments. Each committed batch
of at most 64 MiB deletes its source files; a retry resumes from the committed
index, deleting already-moved sources without rereading them. Space exhaustion
retries with smaller batches, so staging needs room for one batch and metadata,
not a second complete copy. Objects no live inode refers to are not adopted;
the format-1 object directories are removed after the last batch. Migration
reads and rewrites each referenced object once and may be slow on a large or
remote target.

Unreadable blobs remain unreadable: their bytes move into segments as failing
references, and known references to unreadable frames remain failing references.
Healthy data can still migrate. Filesystem I/O failures retain the pending
marker. Migration cannot prove the provenance of legacy data or revoke
previously captured weak key wraps and the unchanged credentials they protect.

## Conversion discovery and progress

`startos-backup-fs inspect` reads bounded superblock prefixes and the plaintext
`conversion.pending` hint without a password or mutation. These observations are
not authenticated. Explicit conversion creates the hint after validating the
credential, then removes it after publishing the completed superblock. A stale
hint does not block a valid store or authorize conversion; only the authenticated
pending marker governs whether an ordinary mount can open it.

The `backup-fs-protocol` crate owns discovery and newline-delimited JSON progress
messages for the helper and core. Phases cover inspection, record conversion, a
scan of live inodes for referenced objects, object conversion and final
publication. Record progress counts bytes; scan and object progress count steps. A
resumed attempt counts its remaining objects rather than reusing an old
percentage. `Complete` follows durable publication and hint cleanup. CLI progress
output is limited to four updates per second plus phase/completion boundaries;
core rejects progress lines over 4 KiB. Corruption warnings are aggregated once
per attempt instead of once per object.

## Blocks, packing, and inode metadata

Regular content uses three tiers: up to 4 KiB inline in its inode record, up to
`BACKUPFS_PACK_MAX` (default 1 MiB) packed in shared content records, then 1 MiB
block records. Growth moves through these tiers one way. A partial block write
preserves existing edges and appends one replacement record for that block;
unaffected blocks keep their locations and bytes. Writes beyond EOF and
new-output alignment tails avoid backing reads. Sparse holes have no record.

Block payloads record compressed length separately from optional random padding.
Growth cannot expose padding, truncated tails, or block references committed
before their extending inode size. Newly visible whole-block slots discard any
existing reference before use; absent slots generate no index change. Each open
inode retains one decoded clean block, shared by its handles and released on last
close; writes and trailing-block pruning invalidate the affected cache entry.

A directory with more than 1,024 entries leaves its inode. In format 2 its
entries are appended to chunks — directory-bucket records of about 56 KiB — so
adding names rewrites only the last chunk; removals rewrite the chunk holding the
name, and a directory whose chunks fall below a quarter full is repacked. Each
mounted directory's names are indexed in memory on first use, up to 262,144
entries across directories. Format 1 hashes large directories into buckets;
format 2 moves such a directory into chunks on its first change. Inodes preserve
mode, uid/gid,
all four timestamps, hardlinks, symlinks, device numbers, and xattrs, including
POSIX ACLs.

## Read-only snapshots and reclamation

`BackupFS::create_snapshot` flushes dirty inode/content state, freezes an immutable
checkpoint view, and publishes a named descriptor in the authenticated catalog.
Unchanged records and index chunks are shared. Later writes append new records
and index deltas. Recovery mounts enforce read-only behavior, including atime
handling. Names contain 1–128 ASCII letters, digits, `-`, `_`, or `.`, excluding
`.` and `..`.

Creation, listing, deletion, and recovery mounts hold the existing exclusive store
lock. A writer and recovery mount cannot share the store concurrently. Retention
is manual. Index sharding removes the full-namespace-per-snapshot catalog entry;
the descriptor catalog still has the metadata decoder's 64 MiB budget. Creation
can checkpoint an indexed namespace, and marking visits its logical block slots.

Compaction relocates the live frames of sealed, unpinned segments whose dead
fraction exceeds `BACKUPFS_COMPACT_RATIO` (default 0.6), most-dead first, verbatim
and with authenticated relocated references, commits, then deletes the segments.
One pass relocates at most the bytes appended during the mount or
`BACKUPFS_COMPACT_BUDGET` (default 256 MiB), whichever is larger. It runs at
unmount and after snapshot deletion. Snapshot views pin their segments and index
graphs; deletion publishes the reduced catalog before compaction. Shared metadata
graphs are traversed once during collection. Pinned segments can retain dead
neighbors until their last snapshot is deleted, and a segment below the dead
fraction keeps its dead records until more of it dies. Snapshots share the
target's hardware and do not replace backups on independent devices.
