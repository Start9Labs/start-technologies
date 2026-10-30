//! The **superblock**: the one file read at mount before anything else, and
//! the anchor for on-disk format evolution.
//!
//! It supersedes the old `cryptinfo` (which held only the master key). It
//! carries a format version and a snapshot of every format-affecting constant
//! so a *future* build can recognize a store it predates and either migrate it
//! or read it with the original parameters — and so this build refuses, with
//! an actionable error, a store it cannot safely read.
//!
//! ## On-disk layout (`data_dir/superblock`, replicated to `superblock.bak1`)
//!
//! ```text
//!   ── plaintext envelope (raw little-endian, parsed forever) ──
//!   magic          "BKSB"   (4)
//!   envelope_ver   u8                 layout version of THIS header
//!   format_version u32                on-disk DATA format version (key-free version gate)
//!   kdf_algo       u8        (=1)     PBKDF2-HMAC-SHA256
//!   kdf_rounds     u32       (=600k)  validated == build & hard-bounded
//!   salt           [u8; 16]
//!   ── sealed body = vault::seal(superblock_config.encode(Body), derive_key(pw,salt,rounds)) ──
//! ```
//!
//! ### Why this layering is bootstrap-safe (no chicken-and-egg)
//!
//! Everything needed to *open* the superblock is either raw plaintext in the
//! envelope (magic, versions, KDF params, salt) or self-described in the vault
//! blob header (ECC params, nonce, lengths — see [`crate::vault`]). The body is
//! always encoded with a single *hardcoded* config ([`superblock_config`]),
//! never the configurable `bincode_encoding` it records. So the superblock is
//! always decodable, and the constants it carries govern only the *data* files
//! and *future writes* — never the superblock's own decode.
//!
//! ### Security of the plaintext envelope
//!
//! The envelope is unauthenticated (the vault tag covers only the sealed body).
//! To keep that from being a downgrade/DoS surface, `kdf_algo`/`kdf_rounds` are
//! **not** taken as tunable inputs: they are validated equal to this build's
//! constants and hard-bounded *before* any PBKDF2 work runs, so a flipped byte
//! can only produce a clean error, never an expensive or weakened derivation.
//! `format_version` is additionally echoed inside the sealed body and
//! cross-checked, turning envelope tampering into an authenticated mismatch.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chacha20::Key;
use log::warn;
use rand::rand_core::UnwrapErr;
use rand::{rng, Rng};
use zeroize::{Zeroize, Zeroizing};

use crate::atomic_file::AtomicFile;
use crate::blockstore::CHUNK_SIZE;
use crate::error::{BkfsError, BkfsErrorKind, BkfsResult, BkfsResultExt};
use crate::serde::{decode, encode, superblock_config, Deserialize, Serialize};
use crate::vault::{self, EccParams, PBKDF2_ROUNDS, PBKDF2_SALT_LEN};

/// Number of redundant superblock copies (`superblock`, `superblock.bak1`).
/// Each is internally ECC-protected; the extra full copy additionally survives
/// whole-file loss or truncation of one replica.
const SUPERBLOCK_REPLICAS: usize = 2;

const SB_MAGIC: [u8; 4] = *b"BKSB";
/// Layout version of the plaintext envelope. Bump if the envelope's byte shape
/// (or the body's hardcoded codec) ever changes, so old builds reject via the
/// raw parse rather than a confusing decode error.
const ENVELOPE_VER: u8 = 1;
const KDF_PBKDF2_HMAC_SHA256: u8 = 1;

/// The on-disk DATA format version this build *writes*.
///
/// History: v1 is the first versioned format — block-chunked content, packed
/// extents in a log-structured store, hash-bucketed directories, per-file
/// compression, and **bincode v2 (`standard`) encoding**. (The pre-versioning
/// `cryptinfo`-based dev format is not readable; there is no migration path
/// from it — it predates any deployment.) V2 requires authenticated vault blobs.
pub const FORMAT_VERSION: u32 = 2;
/// Highest DATA format version this build can read. A store recording a higher
/// version is refused with an actionable "upgrade the software" error.
pub const SUPPORTED_FORMAT_VERSION: u32 = FORMAT_VERSION;
const MIGRATION_PENDING: u64 = 1;

// Scheme identifiers for the VALIDATE-EQUALITY constants. Each governs reads,
// so a build that doesn't recognize the recorded value must refuse to mount.
const BINCODE_STANDARD_V2: u8 = 1;
/// `SHA256(master_key ‖ "block"|"dirbucket" ‖ id.to_le_bytes() ‖ …)`, with the
/// 16-bit-dir / 120-bit-name split (see `ctrl::block_path`). Frozen for v1.
pub const PATH_HASH_SCHEME_V1: u8 = 1;
/// `SHA256(master_key ‖ "dirent" ‖ name)[..8] % buckets` (see
/// `directory::bucket_of`). Frozen for v1.
pub const DIRENT_HASH_SCHEME_V1: u8 = 1;

/// Hard bounds on an accepted PBKDF2 work factor. Defends against an
/// unauthenticated-envelope DoS (absurdly high) or KDF weakening (too low).
const MIN_KDF_ROUNDS: u32 = 100_000;
const MAX_KDF_ROUNDS: u32 = 4_000_000;

/// Upper bound on `dir_max_bucket` (the per-bucket reshard trigger). At ~100k
/// entries a bucket's bincode encoding stays well under `serde`'s data decode
/// limit (64 MiB), so a misconfigured value can't produce an undecodable
/// bucket. Reshard normally keeps buckets near `TARGET_PER_BUCKET` (64).
const MAX_DIR_MAX_BUCKET: u32 = 100_000;

// Envelope byte offsets.
const OFF_ENVELOPE_VER: usize = 4;
const OFF_FORMAT_VERSION: usize = 5;
const OFF_KDF_ALGO: usize = 9;
const OFF_KDF_ROUNDS: usize = 10;
const OFF_SALT: usize = 14;
pub(crate) const ENVELOPE_LEN: usize = OFF_SALT + PBKDF2_SALT_LEN;

// ── creation-time defaults / env snapshot ─────────────────────────────
// Defaults for the policy constants, read from the environment ONCE at
// filesystem creation and then frozen into the superblock. Existing stores
// source these from the superblock, never the environment.
const DEFAULT_INLINE_THRESHOLD: u64 = 4096;
const DEFAULT_SEGMENT_SIZE: u64 = 64 * 1024 * 1024;
const DEFAULT_DIR_SPILL: u32 = 1024;
const DEFAULT_DIR_MAX_BUCKET: u32 = 4096;

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok().and_then(|s| s.parse().ok())
}
fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok().and_then(|s| s.parse().ok())
}

/// The format constants for a filesystem. Recorded in the superblock so a
/// future build can detect and migrate a store it predates, and so this build
/// refuses anything it cannot honor (rather than silently mis-reading).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constants {
    // ── VALIDATE-EQUALITY: changing these changes how *reads* resolve bytes,
    //    so a mismatch against this build is refused (needs a real migration).
    /// Logical block size for `>1 MiB` files. Not self-describing per file
    /// (block files are named only by `(content_id, index)`), so a different
    /// value would mis-map every offset.
    pub chunk_size: u64,
    /// bincode wire format of the data structures. `1` = bincode v2 standard.
    pub bincode_encoding: u8,
    /// Keyed-hash scheme that names block / dir-bucket files.
    pub path_hash_scheme: u8,
    /// Keyed-hash scheme that maps a dir entry name to its bucket.
    pub dirent_hash_scheme: u8,

    // ── ADOPT (validate-on-adopt): self-describing per blob/inode/frame for
    //    READS, so the recorded value governs only new writes. The superblock
    //    value wins over the environment, giving mount-to-mount consistency.
    pub ecc_data: u8,
    pub ecc_parity: u8,
    pub inline_threshold: u64,
    pub pack_max: u64,
    pub segment_size: u64,
    pub dir_spill: u32,
    pub dir_max_bucket: u32,
}

impl Constants {
    /// Snapshot the constants for a *new* filesystem: build invariants for the
    /// validate-equality set, environment-or-default for the adopt set. `ecc`
    /// is chosen once by the caller so the superblock's own seal and its
    /// recorded params agree.
    fn create(ecc: EccParams) -> Constants {
        let inline_threshold = env_u64("BACKUPFS_INLINE_MAX")
            .unwrap_or(DEFAULT_INLINE_THRESHOLD)
            .min(CHUNK_SIZE);
        let pack_max = env_u64("BACKUPFS_PACK_MAX")
            .map(|n| n.min(CHUNK_SIZE))
            .unwrap_or(CHUNK_SIZE)
            .max(inline_threshold);
        let segment_size = env_u64("BACKUPFS_SEGMENT_SIZE")
            .filter(|&n| n >= 4096)
            .unwrap_or(DEFAULT_SEGMENT_SIZE);
        let dir_spill = env_u32("BACKUPFS_DIR_SPILL")
            .filter(|&n| n > 0)
            .unwrap_or(DEFAULT_DIR_SPILL);
        let dir_max_bucket = env_u32("BACKUPFS_DIR_MAX_BUCKET")
            .filter(|&n| n > 0)
            .unwrap_or(DEFAULT_DIR_MAX_BUCKET);
        Constants {
            chunk_size: CHUNK_SIZE,
            bincode_encoding: BINCODE_STANDARD_V2,
            path_hash_scheme: PATH_HASH_SCHEME_V1,
            dirent_hash_scheme: DIRENT_HASH_SCHEME_V1,
            ecc_data: ecc.data,
            ecc_parity: ecc.parity,
            inline_threshold,
            pack_max,
            segment_size,
            dir_spill,
            dir_max_bucket,
        }
    }

    /// Validate the constants on mount. The equality set must match this build
    /// exactly (a difference means the store needs a migration this build does
    /// not have); the adopt set must be self-consistent and honorable (we
    /// refuse rather than clamp, so new writes never diverge from the contract,
    /// and never reach the `ecc::encode` "validated params" panic).
    pub fn validate(&self) -> BkfsResult<()> {
        let bad = |m: String| Err(BkfsError::unsupported(m));
        if self.chunk_size != CHUNK_SIZE {
            return bad(format!(
                "chunk_size {} differs from this build's {CHUNK_SIZE}; the data was written with a \
                 different block size and needs an explicit migration, not a remount",
                self.chunk_size
            ));
        }
        if self.bincode_encoding != BINCODE_STANDARD_V2 {
            return bad(format!(
                "bincode encoding {} unsupported (this build writes {BINCODE_STANDARD_V2})",
                self.bincode_encoding
            ));
        }
        if self.path_hash_scheme != PATH_HASH_SCHEME_V1 {
            return bad(format!(
                "path hash scheme {} unsupported",
                self.path_hash_scheme
            ));
        }
        if self.dirent_hash_scheme != DIRENT_HASH_SCHEME_V1 {
            return bad(format!(
                "dirent hash scheme {} unsupported",
                self.dirent_hash_scheme
            ));
        }
        // validate-on-adopt
        self.ecc().validate()?;
        if self.inline_threshold > self.chunk_size {
            return bad(format!(
                "inline_threshold {} exceeds chunk_size {}",
                self.inline_threshold, self.chunk_size
            ));
        }
        if self.pack_max > self.chunk_size || self.pack_max < self.inline_threshold {
            return bad(format!(
                "pack_max {} outside [inline_threshold {}, chunk_size {}]",
                self.pack_max, self.inline_threshold, self.chunk_size
            ));
        }
        if self.segment_size < 4096 {
            return bad(format!(
                "segment_size {} below minimum 4096",
                self.segment_size
            ));
        }
        if self.dir_spill == 0 || self.dir_max_bucket == 0 {
            return bad("dir_spill / dir_max_bucket must be non-zero".to_owned());
        }
        // Keep a single bucket's encoded size comfortably under the data decode
        // limit even at the reshard trigger, so a degenerate dir_max_bucket
        // (set via env at creation) can't produce a bucket that fails to decode.
        if self.dir_max_bucket > MAX_DIR_MAX_BUCKET {
            return bad(format!(
                "dir_max_bucket {} exceeds the supported maximum {MAX_DIR_MAX_BUCKET}",
                self.dir_max_bucket
            ));
        }
        Ok(())
    }

    pub fn ecc(&self) -> EccParams {
        EccParams {
            data: self.ecc_data,
            parity: self.ecc_parity,
        }
    }
}

/// The sealed body of the superblock.
#[derive(Serialize, Deserialize)]
struct Body {
    /// Echoed copy of the envelope's `format_version`, cross-checked on open so
    /// envelope tampering surfaces as an authenticated mismatch.
    format_version: u32,
    /// Monotonic write counter. On load the replica with the highest generation
    /// wins, so a crash mid-rewrite (e.g. a half-applied password change)
    /// converges to the newest copy rather than a stale one.
    generation: u64,
    created_unix: u64,
    master_key: Zeroizing<[u8; 32]>,
    constants: Constants,
    /// Bit 0 permits mixed vault versions during the authenticated upgrade.
    features: u64,
}

/// The opened superblock: the master key and the format constants the running
/// filesystem must use.
pub struct Superblock {
    pub key: Key,
    pub constants: Constants,
    pub generation: u64,
    pub created_unix: u64,
    pub(crate) format_version: u32,
    pub(crate) migration_pending: bool,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The replica paths for a superblock at `primary`: `superblock`,
/// `superblock.bak1`, …
pub fn replica_paths(primary: &Path) -> Vec<PathBuf> {
    let mut paths = vec![primary.to_owned()];
    for i in 1..SUPERBLOCK_REPLICAS {
        paths.push(primary.with_extension(format!("bak{i}")));
    }
    paths
}

pub fn any_exists(primary: &Path) -> bool {
    replica_paths(primary).iter().any(|p| p.exists())
}

/// Whether the data store siblings of the superblock (`segments`, `contents`,
/// `dirents`) hold any data. Used to refuse creating a fresh superblock over a
/// store whose superblock replicas were lost but whose data survived. A truly
/// fresh store has none of these yet (they are created lazily after the
/// superblock), so this never false-positives at first creation.
fn data_store_nonempty(primary: &Path) -> bool {
    let Some(dir) = primary.parent() else {
        return false;
    };
    ["segments", "contents", "dirents"].iter().any(|sub| {
        std::fs::read_dir(dir.join(sub))
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
    })
}

impl Superblock {
    /// Open the existing superblock at `primary`, or create a fresh one if no
    /// replica exists (unless `readonly`).
    pub fn open_or_create(
        primary: &Path,
        password: &str,
        readonly: bool,
    ) -> BkfsResult<Superblock> {
        if any_exists(primary) {
            let sb = Self::load(primary, password, readonly)?;
            if sb.migration_pending {
                return Err(BkfsError::unsupported(
                    "backup conversion was interrupted; resume conversion before mounting",
                ));
            }
            if sb.format_version == 1
                && primary
                    .parent()
                    .unwrap()
                    .join("segments/state")
                    .try_exists()?
            {
                return Err(BkfsError::unsupported(
                    "authenticated state exists with a legacy superblock",
                ));
            }
            Ok(sb)
        } else if primary.with_file_name("cryptinfo").exists() {
            // A pre-versioning store (master key in `cryptinfo`, bincode-v1
            // data). There is no migration path; refuse with a clear error
            // rather than silently creating a fresh superblock over it (which
            // would leave the old data unreadable and the store apparently
            // empty).
            Err(BkfsError::unsupported(
                "found a legacy unversioned `cryptinfo` store with no superblock; this build cannot \
                 read the pre-versioning on-disk format",
            ))
        } else if data_store_nonempty(primary) {
            // Both superblock replicas are gone but a populated data store
            // remains (e.g. an unreliable backing store dropped the two small
            // superblock files while segments/content survived). Minting a
            // fresh superblock here would generate a NEW master key, present an
            // empty filesystem, and orphan/overwrite the surviving data on the
            // next backup pass. Refuse instead, so the operator can restore the
            // superblock (or its `.bak1`) rather than silently lose data.
            Err(BkfsError::unsupported(
                "superblock replicas are missing but the data store is non-empty; refusing to \
                 create a fresh superblock over existing data — restore `superblock` or \
                 `superblock.bak1` from backup",
            ))
        } else if readonly {
            BkfsResult::errno_notrace(libc::EROFS)
        } else {
            Self::create(primary, password)
        }
    }

    fn create(primary: &Path, password: &str) -> BkfsResult<Superblock> {
        // Choose ECC once so the body's own seal and its recorded params agree.
        let ecc = EccParams::from_env_or_default();
        let constants = Constants::create(ecc);
        constants.validate()?;
        let mut master = Zeroizing::new([0u8; 32]);
        UnwrapErr(rng()).fill_bytes(&mut *master);
        let sb = Superblock {
            key: Key::from(*master),
            constants,
            generation: 1,
            created_unix: now_unix(),
            format_version: FORMAT_VERSION,
            migration_pending: true,
        };
        let mut sb = sb;
        sb.upgrade_with_progress(primary, password, &mut |_| Ok(()))?;
        Ok(sb)
    }

    /// Load, picking the replica with the highest generation as canonical and
    /// healing the rest toward it. Distinguishes a wrong password (every
    /// readable replica fails the integrity tag) from genuine loss.
    pub(crate) fn load(primary: &Path, password: &str, readonly: bool) -> BkfsResult<Superblock> {
        let paths = replica_paths(primary);
        let mut best: Option<Superblock> = None;
        let mut bad_checksum = 0usize;
        let mut generations = Vec::new();
        let mut last_err: Option<BkfsError> = None;

        let mut candidates = Vec::new();
        for path in &paths {
            let raw = match std::fs::read(path) {
                Ok(b) => b,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    last_err = Some(e.into());
                    continue;
                }
            };
            candidates.push(raw);
        }
        let has_current = candidates.iter().any(|raw| {
            envelope_format(raw).is_ok_and(|version| {
                version == FORMAT_VERSION && !vault::is_legacy(&raw[ENVELOPE_LEN..])
            })
        });
        for raw in &candidates {
            if has_current && envelope_format(raw).is_ok_and(|version| version == 1) {
                continue;
            }
            match parse_one(raw, password) {
                Ok(sb) => {
                    generations.push((sb.format_version, sb.generation));
                    best = Some(match best {
                        Some(prev)
                            if (prev.format_version, prev.generation)
                                >= (sb.format_version, sb.generation) =>
                        {
                            prev
                        }
                        _ => sb,
                    });
                }
                Err(e) => {
                    if matches!(e.kind, BkfsErrorKind::BadChecksum) {
                        bad_checksum += 1;
                    }
                    last_err = Some(e);
                }
            }
        }

        match best {
            Some(sb) => {
                let needs_heal = generations.len() != paths.len()
                    || generations
                        .iter()
                        .any(|&pair| pair != (sb.format_version, sb.generation));
                if needs_heal && !readonly && !sb.migration_pending {
                    if let Err(e) = sb.persist(primary, password) {
                        warn!("superblock self-heal failed (reduced redundancy): {e}");
                    }
                }
                Ok(sb)
            }
            // No replica decoded, but at least one failed the integrity tag:
            // almost certainly the wrong password rather than independent loss.
            // A corrupt second envelope must not mask the wrong-password signal.
            None if bad_checksum > 0 => Err(BkfsError {
                kind: BkfsErrorKind::BadChecksum,
                backtrace: None,
            }),
            None => Err(last_err
                .unwrap_or_else(|| BkfsError::unsupported("no readable superblock replica found"))),
        }
    }

    #[cfg(test)]
    pub(crate) fn convert(primary: &Path, password: &str) -> BkfsResult<Self> {
        Self::convert_with_progress(primary, password, &mut |_| Ok(()))
    }

    pub(crate) fn convert_with_progress(
        primary: &Path,
        password: &str,
        report: &mut dyn FnMut(crate::ConversionProgress) -> BkfsResult<()>,
    ) -> BkfsResult<Self> {
        use std::io::Write;

        use crate::{ConversionPhase, ConversionProgress};
        report(ConversionProgress {
            phase: ConversionPhase::Inspecting,
            done: 0,
            total: None,
        })?;
        let mut sb = Self::load(primary, password, false)?;
        let data = primary.parent().unwrap();
        let hint = data.join(crate::conversion::PENDING_HINT);
        if sb.format_version < FORMAT_VERSION || sb.migration_pending {
            let mut file = AtomicFile::create_buffered(hint.clone())?;
            file.write_all(b"conversion requested\n")?;
            file.save_durable()?;
            sb.upgrade_with_progress(primary, password, report)?;
        }
        match std::fs::remove_file(hint) {
            Ok(()) => File::open(data)?.sync_all()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        report(ConversionProgress {
            phase: ConversionPhase::Complete,
            done: 0,
            total: None,
        })?;
        Ok(sb)
    }

    fn upgrade_with_progress(
        &mut self,
        primary: &Path,
        password: &str,
        report: &mut dyn FnMut(crate::ConversionProgress) -> BkfsResult<()>,
    ) -> BkfsResult<()> {
        if self.format_version >= FORMAT_VERSION && !self.migration_pending {
            return Ok(());
        }
        if self.format_version < FORMAT_VERSION
            && primary
                .parent()
                .unwrap()
                .join("segments/state")
                .try_exists()?
        {
            return Err(BkfsError::unsupported(
                "authenticated current-version state exists with an older superblock",
            ));
        }
        if self.format_version < FORMAT_VERSION {
            self.format_version = FORMAT_VERSION;
            self.migration_pending = true;
            self.constants.segment_size = self.constants.segment_size.max(DEFAULT_SEGMENT_SIZE);
            self.generation = self
                .generation
                .checked_add(1)
                .ok_or_else(|| BkfsError::unsupported("superblock generation exhausted"))?;
        }
        // All replicas must durably reject old binaries before any data changes.
        self.persist(primary, password)?;
        crate::migration::reseal_store(
            primary.parent().unwrap(),
            &self.key,
            self.constants,
            report,
        )?;
        self.migration_pending = false;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| BkfsError::unsupported("superblock generation exhausted"))?;
        self.persist(primary, password)
    }

    /// Re-seal and rewrite all replicas (used at creation, for self-heal, and
    /// for password change). Each write generates a fresh salt + nonce.
    pub fn persist(&self, primary: &Path, password: &str) -> BkfsResult<()> {
        use std::io::Write;
        let mut salt = [0u8; PBKDF2_SALT_LEN];
        UnwrapErr(rng()).fill_bytes(&mut salt);
        let key = vault::derive_key(password, &salt, PBKDF2_ROUNDS)?;

        let mut master = Zeroizing::new([0u8; 32]);
        master.copy_from_slice(self.key.as_slice());
        let body = Body {
            format_version: self.format_version,
            generation: self.generation,
            created_unix: self.created_unix,
            master_key: master,
            constants: self.constants,
            features: if self.migration_pending {
                MIGRATION_PENDING
            } else {
                0
            },
        };
        let mut body_bytes = encode(&body, superblock_config())?;
        let sealed = vault::seal_bound(
            &body_bytes,
            <&Key>::from(&*key),
            self.constants.ecc(),
            if self.format_version == 1 {
                vault::Binding::Legacy
            } else {
                vault::Binding::Superblock
            },
        )
        .bytes;
        body_bytes.zeroize();

        let mut file = Vec::with_capacity(ENVELOPE_LEN + sealed.len());
        file.extend_from_slice(&SB_MAGIC);
        file.push(ENVELOPE_VER);
        file.extend_from_slice(&self.format_version.to_le_bytes());
        file.push(KDF_PBKDF2_HMAC_SHA256);
        file.extend_from_slice(&PBKDF2_ROUNDS.to_le_bytes());
        file.extend_from_slice(&salt);
        file.extend_from_slice(&sealed);

        // Write backups first and the primary last, so a crash mid-save never
        // leaves the primary as the only (torn) copy; combined with
        // highest-generation-wins on load, a half-applied rewrite converges to
        // the newest copy.
        for path in replica_paths(primary).into_iter().rev() {
            let mut f = AtomicFile::create_buffered(path)?;
            f.write_all(&file)?;
            f.save()?;
        }
        // fsync the containing directory so the rename(s) that expose the new
        // superblock are themselves durable. AtomicFile::save fsyncs the file
        // data and renames, but not the parent dir entry — and the CLI
        // `change-password`/`fsck` paths return without the unmount syncfs, so
        // without this a reported password rotation could be lost on a crash.
        // (Mirrors seglog's fsync_dir for segment renames.)
        if let Some(dir) = primary.parent() {
            File::open(dir)?.sync_all()?;
        }
        Ok(())
    }
}

pub(crate) fn envelope_format(raw: &[u8]) -> BkfsResult<u32> {
    if raw.len() < ENVELOPE_LEN || raw[..4] != SB_MAGIC {
        return Err(BkfsError::unsupported(
            "not a backup-fs superblock (an older unversioned store, or a corrupt/truncated header)",
        ));
    }
    Ok(u32::from_le_bytes(
        raw[OFF_FORMAT_VERSION..OFF_KDF_ALGO].try_into().unwrap(),
    ))
}

/// Rejects newer formats before decrypting the replica.
fn parse_one(raw: &[u8], password: &str) -> BkfsResult<Superblock> {
    let format_version = envelope_format(raw)?;
    let envelope_ver = raw[OFF_ENVELOPE_VER];
    if envelope_ver != ENVELOPE_VER {
        return Err(BkfsError::unsupported(format!(
            "superblock envelope version {envelope_ver} unsupported (this build understands {ENVELOPE_VER})"
        )));
    }
    if format_version == 0 || format_version > SUPPORTED_FORMAT_VERSION {
        return Err(BkfsError::unsupported(format!(
            "unsupported filesystem format version {format_version}; this build supports \
             1..={SUPPORTED_FORMAT_VERSION} (a higher value was written by a newer backup-fs — \
             upgrade the software)"
        )));
    }
    let kdf_algo = raw[OFF_KDF_ALGO];
    if kdf_algo != KDF_PBKDF2_HMAC_SHA256 {
        return Err(BkfsError::unsupported(format!(
            "unknown KDF algorithm id {kdf_algo}"
        )));
    }
    let kdf_rounds = u32::from_le_bytes(raw[OFF_KDF_ROUNDS..OFF_SALT].try_into().unwrap());
    // Bound + pin BEFORE deriving: never run PBKDF2 over an attacker-tunable
    // work factor (a huge value is a DoS, a tiny one weakens the KDF).
    if !(MIN_KDF_ROUNDS..=MAX_KDF_ROUNDS).contains(&kdf_rounds) {
        return Err(BkfsError::unsupported(format!(
            "superblock KDF rounds {kdf_rounds} outside accepted range [{MIN_KDF_ROUNDS}, {MAX_KDF_ROUNDS}]"
        )));
    }
    if kdf_rounds != PBKDF2_ROUNDS {
        return Err(BkfsError::unsupported(format!(
            "superblock KDF rounds {kdf_rounds} differ from this build's {PBKDF2_ROUNDS}"
        )));
    }
    let salt = &raw[OFF_SALT..OFF_SALT + PBKDF2_SALT_LEN];
    let sealed = &raw[ENVELOPE_LEN..];

    let key = vault::derive_key(password, salt, kdf_rounds)?;
    // open()-then-decode: a wrong password fails the vault tag (BadChecksum)
    // before any decode runs, so it never surfaces as a decode error.
    let mut plain = if format_version == 1 {
        vault::open_legacy(sealed, <&Key>::from(&*key))?
    } else {
        vault::open_bound(sealed, <&Key>::from(&*key), vault::Binding::Superblock)?.plaintext
    };
    let body: Body = decode(&plain, superblock_config())?;
    plain.zeroize();

    if body.format_version != format_version {
        return Err(BkfsError::unsupported(
            "superblock envelope/body version mismatch (corrupt or tampered header)",
        ));
    }
    if body.features & !MIGRATION_PENDING != 0 || (format_version == 1 && body.features != 0) {
        return Err(BkfsError::unsupported("unknown superblock features"));
    }
    body.constants.validate()?;

    Ok(Superblock {
        key: Key::from(*body.master_key),
        constants: body.constants,
        generation: body.generation,
        created_unix: body.created_unix,
        format_version,
        migration_pending: body.features & MIGRATION_PENDING != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_constants() -> Constants {
        Constants::create(EccParams::default())
    }

    #[test]
    fn body_roundtrips_and_preserves_key_bytes() {
        // The master key must survive the bincode-v2 fixed-array encoding
        // verbatim — a silent off-by-encoding here would corrupt every read.
        let key_bytes = [0x5au8; 32];
        let body = Body {
            format_version: FORMAT_VERSION,
            generation: 7,
            created_unix: 123,
            master_key: Zeroizing::new(key_bytes),
            constants: default_constants(),
            features: 0,
        };
        let bytes = encode(&body, superblock_config()).unwrap();
        let back: Body = decode(&bytes, superblock_config()).unwrap();
        assert_eq!(*back.master_key, key_bytes);
        assert_eq!(back.generation, 7);
        assert_eq!(back.constants, default_constants());
    }

    #[test]
    fn constants_validate_accepts_defaults_and_rejects_bad() {
        assert!(default_constants().validate().is_ok());

        let mut c = default_constants();
        c.chunk_size += 1;
        assert!(c.validate().is_err(), "foreign chunk_size must be refused");

        let mut c = default_constants();
        c.bincode_encoding = 2;
        assert!(c.validate().is_err());

        let mut c = default_constants();
        c.path_hash_scheme = 2;
        assert!(c.validate().is_err());

        let mut c = default_constants();
        c.ecc_data = 0;
        assert!(
            c.validate().is_err(),
            "invalid ecc must be refused, not clamped"
        );

        let mut c = default_constants();
        c.pack_max = c.chunk_size + 1;
        assert!(c.validate().is_err());
    }
    fn legacy_bytes(sb: &Superblock, password: &str) -> Vec<u8> {
        let salt = [0x31; PBKDF2_SALT_LEN];
        let key = vault::derive_key(password, &salt, PBKDF2_ROUNDS).unwrap();
        let body = Body {
            format_version: 1,
            generation: sb.generation,
            created_unix: sb.created_unix,
            master_key: Zeroizing::new(sb.key.as_slice().try_into().unwrap()),
            constants: sb.constants,
            features: 0,
        };
        let sealed = vault::seal_legacy(
            &encode(&body, superblock_config()).unwrap(),
            <&Key>::from(&*key),
            sb.constants.ecc(),
        );
        let mut bytes = Vec::from(SB_MAGIC);
        bytes.push(ENVELOPE_VER);
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.push(KDF_PBKDF2_HMAC_SHA256);
        bytes.extend_from_slice(&PBKDF2_ROUNDS.to_le_bytes());
        bytes.extend_from_slice(&salt);
        bytes.extend_from_slice(&sealed);
        bytes
    }

    fn legacy_store() -> (tempfile::TempDir, Superblock) {
        let dir = tempfile::tempdir().unwrap();
        crate::legacy::fixture(dir.path());
        let sb = Superblock::load(&dir.path().join("superblock"), "password", true).unwrap();
        (dir, sb)
    }

    fn options(dir: &Path, readonly: bool) -> crate::BackupFSOptions {
        crate::BackupFSOptions {
            data_dir: dir.to_owned(),
            password: "password".into(),
            readonly,
            setuid_support: false,
            file_size_padding: None,
            idmapped: false,
        }
    }

    fn legacy_block(data: &Path, key: &Key, index: u64) -> PathBuf {
        crate::legacy::object_path(data, key, crate::seglog::RecordKey::Block(4, index)).unwrap()
    }

    fn read_range(data: &Path, inode: u64, offset: u64, len: usize) -> BkfsResult<Vec<u8>> {
        let ctrl = crate::ctrl::Controller::new(options(data, true))?;
        let mut contents = crate::contents::Contents::open(ctrl, crate::inode::Inode(inode))?;
        let mut bytes = vec![0; len];
        contents.read_exact_at(&mut bytes, offset)?;
        Ok(bytes)
    }

    const CHUNK: usize = crate::blockstore::CHUNK_SIZE as usize;

    fn assert_fixture_data(data: &Path) {
        assert_eq!(
            read_range(data, 4, 0, 4 * CHUNK).unwrap(),
            vec![0x31; 4 * CHUNK]
        );
        assert_eq!(read_range(data, 3, 0, 12000).unwrap(), vec![0x42; 12000]);
        assert_eq!(read_range(data, 2, 0, 13).unwrap(), b"frozen-inline");
    }

    #[test]
    fn legacy_store_upgrades_and_remounts_without_fuse() {
        let (dir, legacy) = legacy_store();
        let primary = dir.path().join("superblock");
        let block = legacy_block(dir.path(), &legacy.key, 0);
        let old_blob = std::fs::read(&block).unwrap();
        crate::BackupFS::convert(options(dir.path(), false)).unwrap();
        let mounted = crate::BackupFS::new(options(dir.path(), false)).unwrap();
        let sb = Superblock::load(&primary, "password", true).unwrap();
        assert_eq!(sb.format_version, FORMAT_VERSION);
        assert!(!sb.migration_pending);
        assert_eq!(sb.generation, legacy.generation + 2);
        assert_eq!(sb.key, legacy.key);
        for name in ["contents", "dirents"] {
            assert!(!dir.path().join(name).exists());
        }
        assert!(vault::open(&old_blob, &sb.key).is_err());
        drop(mounted);
        assert_fixture_data(dir.path());
        let remounted = crate::BackupFS::new(options(dir.path(), true)).unwrap();
        drop(remounted);
        let log = crate::seglog::SegmentLog::open(dir.path().join("segments"), sb.key).unwrap();
        assert!(log.contains(crate::inode::Inode(crate::FUSE_ROOT_ID)));
        assert_eq!(log.load_content(42).unwrap().unwrap(), b"packed bytes");
        drop(log);
        // Completed stores must not reseal an injected legacy blob on reopen.
        std::fs::create_dir_all(block.parent().unwrap()).unwrap();
        std::fs::write(&block, &old_blob).unwrap();
        let reopened = Superblock::open_or_create(&primary, "password", false).unwrap();
        assert!(!reopened.migration_pending);
        assert_eq!(std::fs::read(&block).unwrap(), old_blob);
        assert!(vault::open(&old_blob, &reopened.key).is_err());
        assert_fixture_data(dir.path());
    }

    #[test]
    fn interrupted_mixed_upgrade_resumes_and_ignores_checkpoint() {
        let (dir, legacy) = legacy_store();
        let primary = dir.path().join("superblock");
        let blocker = dir.path().join("segments/state.tmp");
        std::fs::create_dir_all(&blocker).unwrap();
        assert!(Superblock::convert(&primary, "password").is_err());
        let pending = Superblock::load(&primary, "password", true).unwrap();
        assert!(pending.migration_pending);
        assert_eq!(pending.format_version, FORMAT_VERSION);
        assert!(Superblock::open_or_create(&primary, "password", true).is_err());
        std::fs::remove_dir(blocker).unwrap();
        let resumed = Superblock::convert(&primary, "password").unwrap();
        assert!(!resumed.migration_pending);
        assert_eq!(resumed.generation, legacy.generation + 2);
        std::fs::write(
            dir.path()
                .join("segments")
                .join(crate::seglog::INDEX_CACHE_NAME),
            b"replaced stale cache",
        )
        .unwrap();
        let log =
            crate::seglog::SegmentLog::open(dir.path().join("segments"), resumed.key).unwrap();
        assert_eq!(log.load_content(42).unwrap().unwrap(), b"packed bytes");
        drop(log);
        assert_fixture_data(dir.path());
    }

    #[test]
    fn resumed_object_moves_retire_committed_sources_without_rereading_them() {
        let (dir, legacy) = legacy_store();
        let primary = dir.path().join("superblock");
        let sb = Superblock::convert(&primary, "password").unwrap();
        let block = legacy_block(dir.path(), &legacy.key, 2);
        std::fs::create_dir_all(block.parent().unwrap()).unwrap();
        std::fs::write(&block, b"not a blob").unwrap();
        crate::migration::reseal_store(dir.path(), &sb.key, sb.constants, &mut |_| Ok(())).unwrap();
        assert!(!dir.path().join("contents").exists());
        assert_fixture_data(dir.path());
    }

    fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files.extend(snapshot(&path));
            } else {
                files.push((path.clone(), std::fs::read(path).unwrap()));
            }
        }
        files.sort();
        files
    }

    #[test]
    fn unsupported_format_numbers_leave_the_store_untouched() {
        for version in [0u32, 3, 4] {
            let (dir, _) = legacy_store();
            let primary = dir.path().join("superblock");
            for replica in replica_paths(&primary) {
                let mut bytes = std::fs::read(&replica).unwrap();
                bytes[OFF_FORMAT_VERSION..OFF_KDF_ALGO].copy_from_slice(&version.to_le_bytes());
                std::fs::write(replica, bytes).unwrap();
            }
            let before = snapshot(dir.path());
            assert!(Superblock::open_or_create(&primary, "password", false).is_err());
            assert_eq!(snapshot(dir.path()), before);
        }
    }

    #[test]
    fn readonly_legacy_and_wrong_password_leave_store_untouched() {
        let (dir, _) = legacy_store();
        let primary = dir.path().join("superblock");
        let before = snapshot(dir.path());
        assert_eq!(
            Superblock::open_or_create(&primary, "password", true)
                .unwrap()
                .format_version,
            1
        );
        assert_eq!(snapshot(dir.path()), before);
        assert!(Superblock::open_or_create(&primary, "wrong password", false).is_err());
        assert_eq!(snapshot(dir.path()), before);
    }

    #[test]
    fn current_replica_blocks_legacy_fallback_even_with_higher_generation() {
        let (dir, mut sb) = legacy_store();
        let primary = dir.path().join("superblock");
        sb.generation = 999;
        std::fs::write(&primary, legacy_bytes(&sb, "password")).unwrap();
        sb.generation = 8;
        sb.format_version = FORMAT_VERSION;
        sb.migration_pending = true;
        // A crash after only the backup replica is upgraded.
        let saved = std::fs::read(&primary).unwrap();
        sb.persist(&primary, "password").unwrap();
        std::fs::write(&primary, saved).unwrap();
        let chosen = Superblock::load(&primary, "password", true).unwrap();
        assert_eq!(chosen.generation, 8);
        assert!(chosen.migration_pending);
        // A damaged authenticated replica must not revive its legacy sibling.
        let backup = replica_paths(&primary).remove(1);
        let mut raw = std::fs::read(&backup).unwrap();
        raw[ENVELOPE_LEN + 16] ^= 1;
        std::fs::write(backup, raw).unwrap();
        assert!(Superblock::load(&primary, "password", true).is_err());
    }

    #[test]
    fn mislabeled_legacy_replica_does_not_hide_healthy_sibling() {
        let (dir, legacy) = legacy_store();
        let primary = dir.path().join("superblock");
        let mut raw = std::fs::read(&primary).unwrap();
        raw[OFF_FORMAT_VERSION] ^= 2;
        std::fs::write(&primary, raw).unwrap();
        let chosen = Superblock::load(&primary, "password", true).unwrap();
        assert_eq!(chosen.format_version, 1);
        assert_eq!(chosen.key, legacy.key);
        assert!(
            !Superblock::open_or_create(&primary, "password", false)
                .unwrap()
                .migration_pending
        );
    }

    #[test]
    fn upgrade_adopts_the_current_segment_size() {
        let (dir, legacy) = legacy_store();
        assert!(legacy.constants.segment_size < DEFAULT_SEGMENT_SIZE);
        let primary = dir.path().join("superblock");
        let sb = Superblock::convert(&primary, "password").unwrap();
        assert_eq!(sb.constants.segment_size, DEFAULT_SEGMENT_SIZE);
        assert_eq!(
            Superblock::load(&primary, "password", true)
                .unwrap()
                .constants
                .segment_size,
            DEFAULT_SEGMENT_SIZE
        );
    }

    #[test]
    fn an_unreadable_indexed_inode_does_not_block_healthy_data_conversion() {
        let (dir, legacy) = legacy_store();
        let source = crate::legacy::Source::load(dir.path(), &legacy.key).unwrap();
        let location = source.references[&crate::seglog::RecordKey::Inode(2)];
        let path = crate::seglog::segment_path(&dir.path().join("segments"), location.segment);
        let mut bytes = std::fs::read(&path).unwrap();
        let start = location.offset as usize;
        bytes[start..start + location.len as usize].fill(0);
        std::fs::write(path, bytes).unwrap();

        let sb = Superblock::convert(&dir.path().join("superblock"), "password").unwrap();
        assert!(!sb.migration_pending);
        assert!(read_range(dir.path(), 2, 0, 13).is_err());
        assert_eq!(
            read_range(dir.path(), 3, 0, 12000).unwrap(),
            vec![0x42; 12000]
        );
        assert_eq!(
            read_range(dir.path(), 4, 0, 4 * CHUNK).unwrap(),
            vec![0x31; 4 * CHUNK]
        );
    }

    #[test]
    fn full_backing_store_retries_object_moves_in_smaller_batches() {
        let (dir, _) = legacy_store();
        let primary = dir.path().join("superblock");
        crate::migration::FULL_COMMITS.with(|full| full.set(2));
        let sb = Superblock::convert(&primary, "password").unwrap();
        assert_eq!(crate::migration::FULL_COMMITS.with(|full| full.get()), 0);
        assert!(!sb.migration_pending);
        assert!(!dir.path().join("contents").exists());
        assert_fixture_data(dir.path());
    }

    #[test]
    fn failed_migration_read_retains_pending_flag_and_can_retry() {
        let (dir, legacy) = legacy_store();
        let primary = dir.path().join("superblock");
        let block = legacy_block(dir.path(), &legacy.key, 0);
        let original = std::fs::read(&block).unwrap();
        std::fs::remove_file(&block).unwrap();
        std::fs::create_dir(&block).unwrap();
        assert!(Superblock::convert(&primary, "password").is_err());
        let sb = Superblock::load(&primary, "password", true).unwrap();
        assert!(sb.migration_pending);
        assert!(Superblock::open_or_create(&primary, "password", true).is_err());
        std::fs::remove_dir(&block).unwrap();
        std::fs::write(&block, original).unwrap();
        let sb = Superblock::convert(&primary, "password").unwrap();
        assert!(!sb.migration_pending);
        assert_fixture_data(dir.path());
    }
}
