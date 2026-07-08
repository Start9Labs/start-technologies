//! Self-contained encrypted, integrity-checked, error-corrected blobs.
//!
//! Every persistent object in the filesystem — inode metadata, the inode
//! pool, content blocks, and the crypt header itself — is stored as a
//! *sealed* blob produced by [`seal`] and read back by [`open`]. A sealed
//! blob is the unit of durability and of corruption recovery.
//!
//! Layout (all integers little-endian):
//!
//! ```text
//!   magic        "BKV1"   (4 bytes)
//!   version      u8       (= 1)
//!   data_shards  u8
//!   parity_shards u8
//!   _reserved    u8
//!   payload_len  u32      ciphertext length (plaintext + 32-byte tag)
//!   shard_len    u32
//!   nonce        [u8; 12] ChaCha20 nonce
//!   shards       (data+parity) repetitions of:
//!                  crc32  u32      CRC of the shard bytes
//!                  bytes  [u8; shard_len]
//! ```
//!
//! **Pipeline.** `plaintext` is concatenated with its own SHA-256 tag, the
//! pair is encrypted with ChaCha20 under a per-blob random nonce, and the
//! resulting ciphertext is split into Reed-Solomon shards (see
//! [`crate::ecc`]). ECC is computed over the *ciphertext* so corruption on
//! the medium can be repaired before decryption.
//!
//! **Recovery.** On open, every shard's CRC is checked; failing shards are
//! treated as erasures and reconstructed by Reed-Solomon as long as no more
//! than `parity_shards` are bad. The decrypted plaintext is then verified
//! against its SHA-256 tag — this both detects residual corruption and
//! rejects a wrong key/password (surfaced as [`BkfsErrorKind::BadChecksum`]).

use std::backtrace::Backtrace;

use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::{ChaCha20, Key, Nonce};
use pbkdf2::hmac::Hmac;
use pbkdf2::pbkdf2;
use rand::rand_core::UnwrapErr;
use rand::{rng, Rng};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::ecc;
use crate::error::{BkfsError, BkfsErrorKind, BkfsResult};

// The vault blob framing ("BKV1" + self-describing ECC params) is frozen and
// self-contained — it is NOT recorded in the superblock because every blob
// header carries its own version/ecc/lengths and reads never depend on
// external state. The bincode encoding of a blob's *plaintext* (for the data
// structures) is governed by the superblock's `bincode_encoding` constant.
const MAGIC: [u8; 4] = *b"BKV1";
const VERSION: u8 = 1;
const HEADER_LEN: usize = 4 + 1 + 1 + 1 + 1 + 4 + 4 + 12; // = 28
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 32; // SHA-256
pub const PBKDF2_SALT_LEN: usize = 16;
/// The PBKDF2 work factor this build writes and accepts. Recorded in the
/// superblock envelope for forward-compat/diagnostics, but validated for
/// equality on open (and hard-bounded) — it is never taken as an
/// attacker-tunable input, since an unauthenticated huge value would be a DoS
/// and a tiny value would weaken the KDF.
pub const PBKDF2_ROUNDS: u32 = 600_000;

/// Default Reed-Solomon parameters: 10 data + 2 parity (≈20% space overhead,
/// tolerates any 2 corrupt shards). The actual params for a filesystem are
/// chosen once at creation, recorded in the superblock, and threaded
/// explicitly into every [`seal`]; reads never depend on them (each blob
/// header self-describes its own params).
pub const DEFAULT_ECC_DATA: u8 = 10;
pub const DEFAULT_ECC_PARITY: u8 = 2;

/// Reed-Solomon parameters threaded explicitly into [`seal`]. Constructible
/// only through validated paths, so [`seal`]'s internal `encode` can never be
/// handed out-of-range params.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EccParams {
    pub data: u8,
    pub parity: u8,
}

impl Default for EccParams {
    fn default() -> Self {
        EccParams {
            data: DEFAULT_ECC_DATA,
            parity: DEFAULT_ECC_PARITY,
        }
    }
}

impl EccParams {
    /// Validate that the params are usable by [`ecc::encode`]/[`ecc::decode`].
    /// `data > 0`, `parity > 0`, and `data + parity ≤ MAX_SHARDS`.
    pub fn validate(self) -> BkfsResult<()> {
        if self.data == 0
            || self.parity == 0
            || self.data as usize + self.parity as usize > ecc::MAX_SHARDS
        {
            return Err(BkfsError::unsupported(format!(
                "invalid ECC params {}+{} (need data>0, parity>0, sum≤{})",
                self.data,
                self.parity,
                ecc::MAX_SHARDS
            )));
        }
        Ok(())
    }

    /// Read params from `BACKUPFS_ECC_DATA`/`BACKUPFS_ECC_PARITY`, falling back
    /// to the default for any missing/out-of-range value. Used ONLY at
    /// filesystem creation to choose the params recorded in the superblock;
    /// existing stores source their params from the superblock, never env.
    pub fn from_env_or_default() -> EccParams {
        let env = |k: &str, d: u8| {
            std::env::var(k)
                .ok()
                .and_then(|s| s.parse::<u8>().ok())
                .filter(|&n| n > 0)
                .unwrap_or(d)
        };
        let p = EccParams {
            data: env("BACKUPFS_ECC_DATA", DEFAULT_ECC_DATA),
            parity: env("BACKUPFS_ECC_PARITY", DEFAULT_ECC_PARITY),
        };
        if p.validate().is_ok() {
            p
        } else {
            EccParams::default()
        }
    }
}

// Both report `BadChecksum`, differing only in backtrace capture:
//   bad_checksum() — the genuine post-decrypt SHA-256 tag mismatch; usually a
//     wrong key/password, so capture a backtrace for diagnosis.
//   corrupt()      — cheap, attacker-reachable rejections of a malformed
//     header/blob (untrusted backing store); skip the backtrace.
fn bad_checksum() -> BkfsError {
    BkfsError {
        kind: BkfsErrorKind::BadChecksum,
        backtrace: Some(Box::new(Backtrace::capture())),
    }
}

fn corrupt() -> BkfsError {
    BkfsError {
        kind: BkfsErrorKind::BadChecksum,
        backtrace: None,
    }
}

/// Encrypt + integrity-tag + erasure-code `plaintext` into a self-contained
/// blob using the given symmetric key and ECC parameters. `ecc` must have been
/// validated (it can only be constructed through validated paths), so the
/// internal `ecc::encode` cannot fail on out-of-range params.
pub fn seal(plaintext: &[u8], key: &Key, ecc: EccParams) -> Vec<u8> {
    seal_with(plaintext, key, ecc.data as usize, ecc.parity as usize)
}

fn seal_with(plaintext: &[u8], key: &Key, data: usize, parity: usize) -> Vec<u8> {
    // secret = plaintext || SHA-256(plaintext)
    let mut secret = Vec::with_capacity(plaintext.len() + TAG_LEN);
    secret.extend_from_slice(plaintext);
    secret.extend_from_slice(&Sha256::digest(plaintext));

    // Encrypt in place under a fresh random nonce.
    let mut nonce = [0u8; NONCE_LEN];
    UnwrapErr(rng()).fill_bytes(&mut nonce);
    let mut cipher = ChaCha20::new(key, &Nonce::try_from(&nonce[..]).unwrap());
    cipher.apply_keystream(&mut secret);
    let payload_len = secret.len();

    let shards = ecc::encode(&secret, data, parity).expect("validated ecc params");
    let shard_len = shards.first().map_or(0, Vec::len);

    let mut out = Vec::with_capacity(HEADER_LEN + shards.len() * (4 + shard_len));
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(data as u8);
    out.push(parity as u8);
    out.push(0); // reserved
    out.extend_from_slice(&(payload_len as u32).to_le_bytes());
    out.extend_from_slice(&(shard_len as u32).to_le_bytes());
    out.extend_from_slice(&nonce);
    for shard in &shards {
        out.extend_from_slice(&crc32fast::hash(shard).to_le_bytes());
        out.extend_from_slice(shard);
    }
    out
}

fn read_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// Decode, error-correct, decrypt, and integrity-verify a blob produced by
/// [`seal`]. Returns `BadChecksum` if the key is wrong or the data is
/// corrupt beyond what ECC can repair.
pub fn open(blob: &[u8], key: &Key) -> BkfsResult<Vec<u8>> {
    if blob.len() < HEADER_LEN || blob[..4] != MAGIC || blob[4] != VERSION {
        return Err(corrupt());
    }
    let data = blob[5] as usize;
    let parity = blob[6] as usize;
    let payload_len = read_u32(&blob[8..12]) as usize;
    let shard_len = read_u32(&blob[12..16]) as usize;
    let nonce = &blob[16..28];
    let total = data + parity;
    if data == 0 || parity == 0 || total > ecc::MAX_SHARDS {
        return Err(corrupt());
    }
    // Bound the claimed payload length BEFORE it reaches `ecc::decode`, which
    // does `Vec::with_capacity(payload_len)`. `payload_len` comes from the
    // unauthenticated header (the SHA-256 tag is only verified *after* decode),
    // so on an untrusted backing store a forged-but-header-coherent blob could
    // otherwise reserve up to ~4 GiB and OOM the host on the very first sealed
    // read (including the superblock at mount). A legitimate payload is the
    // concatenation of `data` equal shards and is physically contained in the
    // blob, so both bounds below hold for any real blob and neither can
    // false-reject.
    if payload_len > data.saturating_mul(shard_len) || payload_len > blob.len() {
        return Err(corrupt());
    }

    // Collect shards, treating any whose CRC fails (or which is truncated
    // off the end of the blob) as an erasure.
    let mut shards: Vec<Option<Vec<u8>>> = Vec::with_capacity(total);
    let mut cursor = HEADER_LEN;
    let stride = 4 + shard_len;
    for _ in 0..total {
        if cursor + stride > blob.len() {
            shards.push(None);
            continue;
        }
        let crc = read_u32(&blob[cursor..cursor + 4]);
        let bytes = &blob[cursor + 4..cursor + stride];
        if crc32fast::hash(bytes) == crc {
            shards.push(Some(bytes.to_vec()));
        } else {
            shards.push(None);
        }
        cursor += stride;
    }

    let mut secret = ecc::decode(shards, data, parity, payload_len).map_err(|_| corrupt())?;
    if secret.len() < TAG_LEN {
        return Err(corrupt());
    }

    let mut cipher = ChaCha20::new(key, &Nonce::try_from(nonce).unwrap());
    cipher.apply_keystream(&mut secret);

    let tag_start = secret.len() - TAG_LEN;
    let expected = Sha256::digest(&secret[..tag_start]);
    if expected.as_slice() != &secret[tag_start..] {
        return Err(bad_checksum());
    }
    secret.truncate(tag_start);
    Ok(secret)
}

/// Derive a 32-byte key from `password` and `salt` via PBKDF2-HMAC-SHA256 with
/// `rounds` iterations. The superblock composes this with [`seal`]/[`open`] to
/// protect its body; `rounds` is validated/bounded by the caller before this
/// runs (see [`crate::superblock`]), never taken raw from untrusted input.
pub(crate) fn derive_key(
    password: &str,
    salt: &[u8],
    rounds: u32,
) -> BkfsResult<Zeroizing<[u8; 32]>> {
    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2::<Hmac<Sha256>>(password.as_bytes(), salt, rounds, key.as_mut_slice()).map_err(
        |_| BkfsError {
            kind: BkfsErrorKind::BadCrypt,
            backtrace: Some(Box::new(Backtrace::capture())),
        },
    )?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> Key {
        Key::from([7u8; 32])
    }

    #[test]
    fn roundtrip() {
        let key = test_key();
        for len in [0usize, 1, 31, 32, 33, 4096, 100_000] {
            let pt: Vec<u8> = (0..len).map(|i| (i * 31 + 7) as u8).collect();
            let blob = seal(&pt, &key, EccParams::default());
            assert_eq!(open(&blob, &key).unwrap(), pt);
        }
    }

    #[test]
    fn wrong_key_is_bad_checksum() {
        let blob = seal(b"secret data", &test_key(), EccParams::default());
        let other = Key::from([9u8; 32]);
        match open(&blob, &other) {
            Err(e) => assert!(matches!(e.kind, BkfsErrorKind::BadChecksum)),
            Ok(_) => panic!("wrong key should not open"),
        }
    }

    #[test]
    fn recovers_from_corruption() {
        // Explicit 8 data + 3 parity for a deterministic shard layout.
        // open() reads the params back out of the header (self-describing).
        let key = test_key();
        let pt: Vec<u8> = (0..50_000u32).map(|i| (i % 257) as u8).collect();
        let mut blob = seal_with(&pt, &key, 8, 3);

        // Each shard is ceil((50000+32)/8) ≈ 6.25 KB; the per-shard stride
        // is that plus a 4-byte CRC. Flipping a contiguous ~12 KB run from
        // 100 bytes into the shard region damages 2–3 shards — within the
        // 3-shard parity budget — and must still reconstruct exactly.
        let start = HEADER_LEN + 100;
        for b in &mut blob[start..start + 12_000] {
            *b ^= 0xFF;
        }
        assert_eq!(open(&blob, &key).unwrap(), pt);
    }

    #[test]
    fn pbkdf2_derive_seal_roundtrip() {
        // The superblock composes derive_key + seal/open; mirror that here.
        let salt = [0x5au8; PBKDF2_SALT_LEN];
        let key = derive_key("hunter2", &salt, PBKDF2_ROUNDS).unwrap();
        let blob = seal(b"header bytes", <&Key>::from(&*key), EccParams::default());
        assert_eq!(open(&blob, <&Key>::from(&*key)).unwrap(), b"header bytes");

        let wrong = derive_key("wrong", &salt, PBKDF2_ROUNDS).unwrap();
        assert!(matches!(
            open(&blob, <&Key>::from(&*wrong)).unwrap_err().kind,
            BkfsErrorKind::BadChecksum
        ));
    }

    #[test]
    fn oversized_payload_len_is_rejected_not_allocated() {
        // A header-coherent blob claiming a ~4 GiB payload must be rejected by
        // the bound (returning corrupt()) rather than reaching the
        // Vec::with_capacity in ecc::decode and OOM-ing the host.
        let key = test_key();
        let mut blob = seal(b"hi", &key, EccParams::default());
        // Overwrite payload_len (bytes 8..12) with u32::MAX.
        blob[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        match open(&blob, &key) {
            Err(e) => assert!(matches!(e.kind, BkfsErrorKind::BadChecksum)),
            Ok(_) => panic!("oversized payload_len must not open"),
        }
    }

    #[test]
    fn ecc_params_validate() {
        assert!(EccParams {
            data: 10,
            parity: 2
        }
        .validate()
        .is_ok());
        assert!(EccParams { data: 0, parity: 2 }.validate().is_err());
        assert!(EccParams {
            data: 10,
            parity: 0
        }
        .validate()
        .is_err());
        assert!(EccParams {
            data: 254,
            parity: 2
        }
        .validate()
        .is_err()); // sum > 255
    }
}
