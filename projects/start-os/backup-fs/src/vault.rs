//! Versioned ChaCha20 / HMAC-SHA256 Encrypt-then-MAC blobs with Reed-Solomon recovery.
//!
//! The 28-byte header contains `BKV1`, version, data/parity counts, a reserved
//! byte, payload and shard lengths (u32 LE), and a 12-byte random nonce.
//! HMAC authenticates the entire header and ciphertext; ECC protects ciphertext
//! and the 32-byte tag together. Independent encryption and MAC subkeys are
//! derived from the master key by HMAC-SHA256 with distinct fixed labels.

use std::backtrace::Backtrace;

use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::{ChaCha20, Key, Nonce};
use pbkdf2::hmac::{Hmac, Mac};
use pbkdf2::pbkdf2;
use rand::rand_core::UnwrapErr;
use rand::{rng, Rng};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::ecc;
use crate::error::{BkfsError, BkfsErrorKind, BkfsResult};

/// Variant order is part of the on-disk key derivation.
#[derive(Clone, Copy, serde::Serialize)]
pub(crate) enum Binding {
    Superblock,
    Record,
    State,
    Transaction,
    Catalog,
    Legacy,
}

pub(crate) type AuthenticationTag = [u8; 32];

pub(crate) struct Sealed {
    pub bytes: Vec<u8>,
    pub tag: AuthenticationTag,
}

pub(crate) struct Authenticated {
    pub plaintext: Vec<u8>,
    pub tag: AuthenticationTag,
}

impl Authenticated {
    /// Requires a tag from an authenticated parent reference.
    pub fn verify_reference(self, expected: &AuthenticationTag) -> BkfsResult<Vec<u8>> {
        if &self.tag != expected {
            return Err(corrupt());
        }
        Ok(self.plaintext)
    }
}

const MAGIC: [u8; 4] = *b"BKV1";
const VERSION: u8 = 2;
const ENCRYPTION_LABEL: &[u8] = b"backup-fs/vault/v2/encryption";
const AUTHENTICATION_LABEL: &[u8] = b"backup-fs/vault/v2/authentication";
const HEADER_LEN: usize = 4 + 1 + 1 + 1 + 1 + 4 + 4 + 12; // = 28
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = std::mem::size_of::<AuthenticationTag>();
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

#[cfg(test)]
pub fn seal(plaintext: &[u8], key: &Key, ecc: EccParams) -> Vec<u8> {
    seal_with(plaintext, key, ecc.data as usize, ecc.parity as usize).bytes
}

fn binding_key(key: &Key, binding: Binding) -> Zeroizing<[u8; 32]> {
    let context = crate::serde::encode(&binding, crate::serde::data_config())
        .expect("fixed-size vault binding");
    subkey(
        key,
        &[b"backup-fs/v2/binding/".as_slice(), &context].concat(),
    )
}

pub(crate) fn seal_bound(plaintext: &[u8], key: &Key, ecc: EccParams, binding: Binding) -> Sealed {
    if matches!(binding, Binding::Legacy) {
        return Sealed {
            bytes: seal_legacy(plaintext, key, ecc),
            tag: Sha256::digest(plaintext).into(),
        };
    }
    let key = binding_key(key, binding);
    seal_with(
        plaintext,
        <&Key>::from(&*key),
        ecc.data as usize,
        ecc.parity as usize,
    )
}

pub(crate) fn open_bound(blob: &[u8], key: &Key, binding: Binding) -> BkfsResult<Authenticated> {
    if matches!(binding, Binding::Legacy) {
        return open_version(blob, key, 1);
    }
    let key = binding_key(key, binding);
    open_version(blob, <&Key>::from(&*key), VERSION)
}

fn subkey(key: &Key, label: &[u8]) -> Zeroizing<[u8; 32]> {
    let mut mac = <Hmac<Sha256> as pbkdf2::hmac::KeyInit>::new_from_slice(key.as_slice()).unwrap();
    mac.update(label);
    Zeroizing::new(mac.finalize().into_bytes().into())
}

fn seal_with(plaintext: &[u8], key: &Key, data: usize, parity: usize) -> Sealed {
    let enc = subkey(key, ENCRYPTION_LABEL);
    let auth = subkey(key, AUTHENTICATION_LABEL);
    let mut nonce = [0u8; NONCE_LEN];
    UnwrapErr(rng()).fill_bytes(&mut nonce);
    let payload_len = plaintext.len() + TAG_LEN;
    let shard_len = ecc::shard_len(payload_len, data);
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(data as u8);
    out.push(parity as u8);
    out.push(0);
    out.extend_from_slice(&(payload_len as u32).to_le_bytes());
    out.extend_from_slice(&(shard_len as u32).to_le_bytes());
    out.extend_from_slice(&nonce);

    let mut payload = plaintext.to_vec();
    ChaCha20::new(<&Key>::from(&*enc), &Nonce::from(nonce)).apply_keystream(&mut payload);
    let mut mac = <Hmac<Sha256> as pbkdf2::hmac::KeyInit>::new_from_slice(&*auth).unwrap();
    mac.update(&out);
    mac.update(&payload);
    let tag: [u8; 32] = mac.finalize().into_bytes().into();
    payload.extend_from_slice(&tag);
    let shards = ecc::encode(&payload, data, parity).expect("validated ecc params");
    for shard in &shards {
        out.extend_from_slice(&crc32fast::hash(shard).to_le_bytes());
        out.extend_from_slice(shard);
    }
    Sealed { bytes: out, tag }
}

fn read_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// Decode, error-correct, decrypt, and integrity-verify a blob produced by
/// [`seal`]. Returns `BadChecksum` if the key is wrong or the data is
/// corrupt beyond what ECC can repair.
#[cfg(test)]
pub fn open(blob: &[u8], key: &Key) -> BkfsResult<Vec<u8>> {
    open_version(blob, key, VERSION).map(|opened| opened.plaintext)
}

/// Reads the pre-authentication filesystem format.
pub(crate) fn open_legacy(blob: &[u8], key: &Key) -> BkfsResult<Vec<u8>> {
    open_version(blob, key, 1).map(|opened| opened.plaintext)
}

pub(crate) fn is_legacy(blob: &[u8]) -> bool {
    blob.len() >= HEADER_LEN && blob[..4] == MAGIC && blob[4] == 1
}

fn open_version(blob: &[u8], key: &Key, version: u8) -> BkfsResult<Authenticated> {
    if blob.len() < HEADER_LEN || blob[..4] != MAGIC || blob[4] != version {
        return Err(corrupt());
    }
    let data = blob[5] as usize;
    let parity = blob[6] as usize;
    let payload_len = read_u32(&blob[8..12]) as usize;
    let shard_len = read_u32(&blob[12..16]) as usize;
    let total = data + parity;
    if data == 0 || parity == 0 || total > ecc::MAX_SHARDS || payload_len < TAG_LEN {
        return Err(corrupt());
    }
    // Bound allocations before authenticating the untrusted header.
    if payload_len > data.saturating_mul(shard_len) || payload_len > blob.len() {
        return Err(corrupt());
    }
    let mut shards = Vec::with_capacity(total);
    let mut cursor = HEADER_LEN;
    let stride = 4 + shard_len;
    for _ in 0..total {
        if cursor + stride > blob.len() {
            shards.push(None);
            continue;
        }
        let crc = read_u32(&blob[cursor..cursor + 4]);
        let bytes = &blob[cursor + 4..cursor + stride];
        shards.push((crc32fast::hash(bytes) == crc).then(|| bytes.to_vec()));
        cursor += stride;
    }
    let mut payload = ecc::decode(shards, data, parity, payload_len).map_err(|_| corrupt())?;
    let tag_start = payload.len() - TAG_LEN;
    let nonce = Nonce::try_from(&blob[16..28]).unwrap();
    if version == 1 {
        ChaCha20::new(key, &nonce).apply_keystream(&mut payload);
        if Sha256::digest(&payload[..tag_start]).as_slice() != &payload[tag_start..] {
            return Err(bad_checksum());
        }
    } else {
        let auth = subkey(key, AUTHENTICATION_LABEL);
        let mut mac = <Hmac<Sha256> as pbkdf2::hmac::KeyInit>::new_from_slice(&*auth).unwrap();
        mac.update(&blob[..HEADER_LEN]);
        mac.update(&payload[..tag_start]);
        mac.verify_slice(&payload[tag_start..])
            .map_err(|_| bad_checksum())?;
        let enc = subkey(key, ENCRYPTION_LABEL);
        ChaCha20::new(<&Key>::from(&*enc), &nonce).apply_keystream(&mut payload[..tag_start]);
    }
    let tag = payload[tag_start..].try_into().unwrap();
    payload.truncate(tag_start);
    Ok(Authenticated {
        plaintext: payload,
        tag,
    })
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

pub(crate) fn seal_legacy(plaintext: &[u8], key: &Key, ecc: EccParams) -> Vec<u8> {
    let data = ecc.data as usize;
    let parity = ecc.parity as usize;
    let mut secret = Vec::with_capacity(plaintext.len() + TAG_LEN);
    secret.extend_from_slice(plaintext);
    secret.extend_from_slice(&Sha256::digest(plaintext));

    let mut nonce = [0u8; NONCE_LEN];
    UnwrapErr(rng()).fill_bytes(&mut nonce);
    let mut cipher = ChaCha20::new(key, &Nonce::try_from(&nonce[..]).unwrap());
    cipher.apply_keystream(&mut secret);
    let payload_len = secret.len();

    let shards = ecc::encode(&secret, data, parity).expect("validated ecc params");
    let shard_len = shards.first().map_or(0, Vec::len);

    let mut out = Vec::with_capacity(HEADER_LEN + shards.len() * (4 + shard_len));
    out.extend_from_slice(&MAGIC);
    out.push(1);
    out.push(data as u8);
    out.push(parity as u8);
    out.push(0);
    out.extend_from_slice(&(payload_len as u32).to_le_bytes());
    out.extend_from_slice(&(shard_len as u32).to_le_bytes());
    out.extend_from_slice(&nonce);
    for shard in &shards {
        out.extend_from_slice(&crc32fast::hash(shard).to_le_bytes());
        out.extend_from_slice(shard);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> Key {
        Key::from([7u8; 32])
    }

    #[test]
    fn binding_rejects_other_metadata_roles() {
        let key = test_key();
        let sealed = seal_bound(
            b"immutable contents",
            &key,
            EccParams::default(),
            Binding::Record,
        );
        let opened = open_bound(&sealed.bytes, &key, Binding::Record).unwrap();
        assert_eq!(opened.plaintext, b"immutable contents");
        assert_eq!(opened.tag, sealed.tag);
        for wrong in [
            Binding::Superblock,
            Binding::State,
            Binding::Transaction,
            Binding::Catalog,
        ] {
            assert!(open_bound(&sealed.bytes, &key, wrong).is_err());
        }
        assert!(open(&sealed.bytes, &key).is_err());
        let unbound = seal(b"immutable contents", &key, EccParams::default());
        assert!(open_bound(&unbound, &key, Binding::Record).is_err());
    }

    #[test]
    fn authenticated_reference_tag_survives_ecc_repair() {
        let key = test_key();
        let mut sealed = seal_bound(b"recoverable", &key, EccParams::default(), Binding::Record);
        sealed.bytes[HEADER_LEN + 4] ^= 1;
        let opened = open_bound(&sealed.bytes, &key, Binding::Record).unwrap();
        assert_eq!(opened.plaintext, b"recoverable");
        assert_eq!(opened.tag, sealed.tag);
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
        let mut blob = seal_with(&pt, &key, 8, 3).bytes;

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
    fn payload(blob: &[u8]) -> Vec<u8> {
        let data = blob[5] as usize;
        let parity = blob[6] as usize;
        let shard_len = read_u32(&blob[12..16]) as usize;
        let shards = blob[HEADER_LEN..]
            .chunks_exact(4 + shard_len)
            .map(|s| Some(s[4..].to_vec()))
            .collect();
        ecc::decode(shards, data, parity, read_u32(&blob[8..12]) as usize).unwrap()
    }

    fn replace_payload(blob: &mut Vec<u8>, payload: &[u8]) {
        let shards = ecc::encode(payload, blob[5] as usize, blob[6] as usize).unwrap();
        blob.truncate(HEADER_LEN);
        for shard in shards {
            blob.extend_from_slice(&crc32fast::hash(&shard).to_le_bytes());
            blob.extend_from_slice(&shard);
        }
    }

    fn known_plaintext_forgery(blob: &mut Vec<u8>, known: &[u8], replacement: &[u8]) {
        let mut encrypted = payload(blob);
        let a = [known, Sha256::digest(known).as_slice()].concat();
        let b = [replacement, Sha256::digest(replacement).as_slice()].concat();
        for ((c, a), b) in encrypted.iter_mut().zip(a).zip(b) {
            *c ^= a ^ b;
        }
        replace_payload(blob, &encrypted);
    }

    #[test]
    fn known_plaintext_forgery_and_legacy_downgrade_are_rejected() {
        let key = test_key();
        let known = b"known original plaintext";
        let replacement = b"evil! modified plaintext";
        assert_eq!(known.len(), replacement.len());
        let mut legacy = seal_legacy(known, &key, EccParams::default());
        known_plaintext_forgery(&mut legacy, known, replacement);
        assert_eq!(open_legacy(&legacy, &key).unwrap(), replacement);
        assert!(open(&legacy, &key).is_err());
        let mut current = seal(known, &key, EccParams::default());
        known_plaintext_forgery(&mut current, known, replacement);
        assert!(open(&current, &key).is_err());
        current[4] = 1;
        assert!(open(&current, &key).is_err());
    }

    #[test]
    fn authenticates_every_header_byte_and_reencoded_payload() {
        let key = test_key();
        let original = seal(b"authenticated message", &key, EccParams::default());
        for i in 0..HEADER_LEN {
            let mut corrupt = original.clone();
            corrupt[i] ^= 1;
            assert!(open(&corrupt, &key).is_err(), "header byte {i}");
        }
        for i in [0, payload(&original).len() - 1] {
            let mut corrupt = original.clone();
            let mut bytes = payload(&corrupt);
            bytes[i] ^= 1;
            replace_payload(&mut corrupt, &bytes);
            assert!(open(&corrupt, &key).is_err());
        }
    }

    #[test]
    fn truncated_parity_shards_are_recovered() {
        let key = test_key();
        let mut blob = seal(b"recover ciphertext and MAC", &key, EccParams::default());
        let stride = 4 + read_u32(&blob[12..16]) as usize;
        blob.truncate(blob.len() - 2 * stride);
        assert_eq!(open(&blob, &key).unwrap(), b"recover ciphertext and MAC");
    }
}
