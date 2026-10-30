# backup-fs storage design

`startos-backup-fs` is an encrypted, error-corrected FUSE filesystem for StartOS
backups on physical drives and network filesystems. Its chunked data and packed
metadata support incremental copies with `rsync` or `rclone`. A backing filesystem
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
  segments/*.seg              sealed inode, packed-content, and object-pointer records
  contents/<shard>/<id>.<version>  immutable sealed content blocks
  dirents/<shard>/<id>.<version>   immutable sealed directory buckets
```

Filesystem format 2 uses versioned objects for every store. Named snapshots retain
historical views; they do not activate a separate storage mode. Copy the complete
backing directory while it is offline: the index, catalog, and their referenced
files are authoritative. The legacy `index.cache` may remain after migration
but is not consulted by normal format-2 opens.

## Encryption, references, and ECC

Format 1 retains its original encryption/checksum protocol and stable-name writes
until explicit conversion. The authenticated references below apply to format 2.

`vault::seal_bound` derives context-specific keys for superblocks, records,
transactions, catalogs, and object identities/versions. Independent encryption
and MAC subkeys protect ChaCha20 ciphertext and its vault header with
HMAC-SHA256. Fresh random nonces protect repeated plaintext. Reed-Solomon coding
covers the ciphertext and authentication tag; each shard carries a CRC-32.
Stores default to ten data and two parity shards, with creation-time overrides
persisted in the superblock.

Reads repair recoverable erasures, verify the MAC in constant time, then decrypt.
An immutable object's caller supplies its expected identity and version from the
current authenticated index. A log reference supplies the physical location,
sequence, framed and payload lengths, and expected authentication tag. The
record's decoded key must also match its caller. Rewriting an unkeyed frame CRC
or sequence does not authorize replacement ciphertext.

Index transactions authenticate their parent references. The small `segments/state`
anchor selects the current graph and catalog; ordinary opens never recover intent
from uncommitted data-log tails or an optional cache. Expected tags identify
corrected authenticated content, not raw ECC bytes. Missing or mistyped referenced
files report EIO; an absent data reference or deletion denotes a hole. Corruption
in an individual data object remains localized, while missing or corrupt
namespace/index metadata blocks access to the affected view.

A genuine older authenticated root can select a genuine older backup view. There
is no external freshness authority or whole-backup rollback prevention. The
pre-unlock discovery timestamp is editable plaintext; `BackupInfo.timestamp` in
encrypted `metadata.json` authenticates the date of the selected backup view.

## Legacy migration

Ordinary opens keep pre-existing filesystem format 1 readable and writable without
conversion. `startos-backup-fs convert` explicitly converts a closed store to format 2
under the same exclusive lock used by mounts. Every superblock replica first receives
an authenticated pending marker. Ordinary opens refuse pending stores until conversion
is explicitly resumed. Migration reseals objects and log frames
with their required bindings, preserves cached frame locations, and publishes
the new State before clearing the marker. Older binaries reject the upgraded
format. Format 1 has stable-name objects and no named snapshots.

In-place segment replacements sync their temporary data before rename, then sync
the containing directory. Object copies to new names retain batched durability.

Stable-name objects receive deterministic secret-derived migration versions and
pointer records. Completed destinations and their pointers become durable before
the original files are removed. Retirement uses the existing group-commit batch
threshold; space exhaustion retires a completed smaller batch before retrying the
next object. Retry recognizes these pointers as completed migration output.
Objects outside current logical inode slots are not adopted into the live view.
Snapshot retention begins after upgrading.

Temporary space includes a replacement segment or object, a staged object batch,
and new index/catalog metadata. The batch normally contains at most 256 objects
(`BACKUPFS_SYNC_BATCH` overrides this). Space-pressure retries avoid requiring a
second complete object store, but enough space for the next replacement and
metadata is still necessary. Migration rewrites encrypted data and may be slow
on a large or remote target.

Unreadable blobs remain unreadable, including when moved to versioned names;
known references to unreadable frames remain failing references. Healthy data can still migrate.
Filesystem I/O failures retain the pending marker. Migration cannot prove the
provenance of legacy data or revoke previously captured weak key wraps and the
unchanged credentials they protect.

## Conversion discovery and progress

`startos-backup-fs inspect` reads bounded superblock prefixes and the plaintext
`conversion.pending` hint without a password or mutation. These observations are
not authenticated. Explicit conversion creates the hint after validating the
credential, then removes it after publishing the completed superblock. A stale
hint does not block a valid store or authorize conversion; only the authenticated
pending marker governs whether an ordinary mount can open it.

The `backup-fs-protocol` crate owns discovery and newline-delimited JSON progress
messages for the helper and core. Phases cover inspection, record conversion,
object inventory, object conversion and final publication. Record progress counts
bytes; object progress counts steps. A resumed attempt inventories its remaining
objects rather than reusing an old percentage. `Complete` follows durable
publication and hint cleanup. CLI progress output is limited to four updates per
second plus phase/completion boundaries; core rejects progress lines over 4 KiB.
Corruption warnings are aggregated once per attempt instead of once per object.

## Blocks, packing, and inode metadata

Regular content uses three tiers: up to 4 KiB inline in its inode record, up to
`BACKUPFS_PACK_MAX` (default 1 MiB) packed in shared content-log extents, then
1 MiB block objects. Growth moves through these tiers one way. A partial block
write preserves existing edges and creates one immutable replacement for that
block; unaffected block versions keep their names and bytes. Writes beyond EOF
and new-output alignment tails avoid backing reads. Sparse holes have no object.

Block payloads record compressed length separately from optional random padding.
Growth cannot expose padding, truncated tails, or block references committed
before their extending inode size. Newly visible whole-block slots discard any
existing reference before use; absent slots generate no deletion record. Each open inode retains one
decoded clean block, shared by its handles and released on last close; writes and
trailing-block pruning invalidate the affected cache entry.

Large directories spill into keyed hash buckets. Inodes preserve mode, uid/gid,
all four timestamps, hardlinks, symlinks, device numbers, and xattrs, including
POSIX ACLs. Packed contents and inode records share append-only data segments.
An active writer creates a new segment only on its first write.

## Durability and incremental indices

State owns publication durability. Objects, data records, and packed index deltas
are written first; one backing-filesystem `syncfs` precedes atomic root replacement
and directory synchronization. Explicit durable operations commit immediately;
fast saves use the existing batch threshold. A clean library close commits
pending successful fast updates. A failed backing writeback barrier or uncertain publication blocks mutations until
reopen, even if a later barrier succeeds; uncommitted tails do not override the previous root.

Index deltas share append-only 8 MiB target segments, rather than creating a file
per mutation. Coarse checkpoints shard maps into chunks of at most 16,384 entries
and enforce decoder budgets before publication. Unchanged checkpoints reuse their
existing references without rereading newly written chunks. Bulk reference scans
sort borrowed locations and reuse enclosing buffered windows and open segment
files; index walks use the same range reader in reverse order. Read-only range
buffers allocate no write-submission ring.

Data objects retain `O_DIRECT`, aligned buffering, io_uring write batching,
network-filesystem-aware write sizes, and flush timeouts. Small metadata uses the
page cache and group commit. Version pointers, authenticated index publication,
and reclamation add metadata work to ordinary backups; these changes are not a
WAN/CIFS throughput result.

## Read-only snapshots and reclamation

`BackupFS::create_snapshot` flushes dirty inode/content state, freezes an immutable
checkpoint view, and publishes a named descriptor in the authenticated catalog.
Unchanged data and index chunks are shared. Later writes produce new object
versions and index deltas. Recovery mounts enforce read-only behavior, including
atime handling. Names contain 1–128 ASCII letters, digits, `-`, `_`, or `.`,
excluding `.` and `..`.

Creation, listing, deletion, and recovery mounts hold the existing exclusive store
lock. A writer and recovery mount cannot share the store concurrently. Retention
is manual. Index sharding removes the full-namespace-per-snapshot catalog entry;
the descriptor catalog still has the metadata decoder's 64 MiB budget. Creation
can checkpoint an indexed namespace, and marking visits its logical block slots.

Compaction copies live frames verbatim, authenticates relocated references, and
commits before unlinking old segments. Snapshot views pin their data segments and
index graphs. Deletion publishes the reduced catalog before collecting files
unused by both current and retained views. Shared metadata graphs are traversed
once during collection. Pinned segments can retain dead neighbors until their
last snapshot is deleted. Snapshots share the target's hardware and do not replace
backups on independent devices.
