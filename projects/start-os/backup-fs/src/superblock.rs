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
/// from it — it predates any deployment.)
pub const FORMAT_VERSION: u32 = 1;
/// Highest DATA format version this build can read. A store recording a higher
/// version is refused with an actionable "upgrade the software" error.
pub const SUPPORTED_FORMAT_VERSION: u32 = 1;

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
const ENVELOPE_LEN: usize = OFF_SALT + PBKDF2_SALT_LEN;

// ── creation-time defaults / env snapshot ─────────────────────────────
// Defaults for the policy constants, read from the environment ONCE at
// filesystem creation and then frozen into the superblock. Existing stores
// source these from the superblock, never the environment.
const DEFAULT_INLINE_THRESHOLD: u64 = 4096;
const DEFAULT_SEGMENT_SIZE: u64 = 8 * 1024 * 1024;
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
    /// Reserved feature bitflags for forward-compat (none defined yet).
    features: u64,
}

/// The opened superblock: the master key and the format constants the running
/// filesystem must use.
pub struct Superblock {
    pub key: Key,
    pub constants: Constants,
    pub generation: u64,
    pub created_unix: u64,
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
            Self::load(primary, password, readonly)
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
        };
        sb.persist(primary, password)?;
        Ok(sb)
    }

    /// Load, picking the replica with the highest generation as canonical and
    /// healing the rest toward it. Distinguishes a wrong password (every
    /// readable replica fails the integrity tag) from genuine loss.
    fn load(primary: &Path, password: &str, readonly: bool) -> BkfsResult<Superblock> {
        let paths = replica_paths(primary);
        let mut best: Option<Superblock> = None;
        let mut bad_checksum = 0usize;
        let mut healthy = 0usize;
        let mut last_err: Option<BkfsError> = None;

        for path in &paths {
            let raw = match std::fs::read(path) {
                Ok(b) => b,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    last_err = Some(e.into());
                    continue;
                }
            };
            match parse_one(&raw, password) {
                Ok(sb) => {
                    healthy += 1;
                    best = Some(match best {
                        Some(prev) if prev.generation >= sb.generation => prev,
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
                if healthy < paths.len() && !readonly {
                    // Heal damaged/missing replicas from the canonical copy.
                    // Skipped on a read-only mount (which must never write); a
                    // near-full store likewise keeps mounting off the good
                    // replica with reduced redundancy.
                    if let Err(e) = sb.persist(primary, password) {
                        warn!("superblock self-heal failed (reduced redundancy): {e}");
                    }
                } else if healthy < paths.len() {
                    warn!("superblock has reduced redundancy; self-heal skipped (read-only mount)");
                }
                Ok(sb)
            }
            // No replica decoded, but at least one failed the integrity tag:
            // almost certainly the wrong password rather than independent loss.
            // (`> 0`, not `== readable`, so a second replica that is also
            // independently corrupt at the envelope level can't mask the
            // wrong-password signal — `best` is None here, so `healthy == 0`.)
            None if bad_checksum > 0 => Err(BkfsError {
                kind: BkfsErrorKind::BadChecksum,
                backtrace: None,
            }),
            None => Err(last_err
                .unwrap_or_else(|| BkfsError::unsupported("no readable superblock replica found"))),
        }
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
            format_version: FORMAT_VERSION,
            generation: self.generation,
            created_unix: self.created_unix,
            master_key: master,
            constants: self.constants,
            features: 0,
        };
        let mut body_bytes = encode(&body, superblock_config())?;
        let sealed = vault::seal(&body_bytes, <&Key>::from(&*key), self.constants.ecc());
        body_bytes.zeroize();

        let mut file = Vec::with_capacity(ENVELOPE_LEN + sealed.len());
        file.extend_from_slice(&SB_MAGIC);
        file.push(ENVELOPE_VER);
        file.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
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

/// Parse and authenticate one superblock replica. The version gate runs on the
/// plaintext envelope *before* any decode, so a store written by a newer build
/// produces the actionable "upgrade" message rather than a confusing decode
/// error.
fn parse_one(raw: &[u8], password: &str) -> BkfsResult<Superblock> {
    if raw.len() < ENVELOPE_LEN || raw[..4] != SB_MAGIC {
        return Err(BkfsError::unsupported(
            "not a backup-fs superblock (an older unversioned store, or a corrupt/truncated header)",
        ));
    }
    let envelope_ver = raw[OFF_ENVELOPE_VER];
    if envelope_ver != ENVELOPE_VER {
        return Err(BkfsError::unsupported(format!(
            "superblock envelope version {envelope_ver} unsupported (this build understands {ENVELOPE_VER})"
        )));
    }
    let format_version =
        u32::from_le_bytes(raw[OFF_FORMAT_VERSION..OFF_KDF_ALGO].try_into().unwrap());
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
    let mut plain = vault::open(sealed, <&Key>::from(&*key))?;
    let body: Body = decode(&plain, superblock_config())?;
    plain.zeroize();

    if body.format_version != format_version {
        return Err(BkfsError::unsupported(
            "superblock envelope/body version mismatch (corrupt or tampered header)",
        ));
    }
    body.constants.validate()?;

    Ok(Superblock {
        key: Key::from(*body.master_key),
        constants: body.constants,
        generation: body.generation,
        created_unix: body.created_unix,
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
}
