//! bincode (de)serialization wrapped in the [`crate::vault`] seal/open
//! pipeline. Every persistent structure — inode attributes, seglog records,
//! directory buckets, the superblock body — is serialized to bytes, then
//! (for the data structures) sealed (encrypted + integrity-tagged +
//! erasure-coded) before it touches the backing store.
//!
//! ## Encoding contract (part of the on-disk format version)
//!
//! Everything uses **bincode v2 in `standard` configuration** (little-endian,
//! varint integers). Two distinct, *hardcoded* configurations exist:
//!
//! * [`superblock_config`] — used only for the superblock body. It is pinned
//!   forever for a given envelope version (see [`crate::superblock`]); the
//!   superblock must be decodable without first knowing any recorded
//!   constant, so its codec can never be routed through
//!   `Constants::bincode_encoding`.
//! * [`data_config`] — used for every other (data) structure. The active
//!   encoding is recorded in the superblock as `bincode_encoding` and
//!   validated for equality on mount.
//!
//! Both carry an explicit size limit so a corrupted-but-integrity-recovered
//! length prefix can't trigger an unbounded allocation during decode.
//!
//! Because a sealed payload is *exactly* the bytes that were encoded (the
//! vault tag covers the whole plaintext), [`decode`] hard-errors on any
//! trailing/unconsumed bytes: leftover bytes always indicate a struct-shape
//! skew or corruption, never legitimate data.

use bincode::config::Config;
use serde::de::DeserializeOwned;
pub use serde::{Deserialize, Serialize};

use crate::error::{BkfsError, BkfsErrorKind, BkfsResult};

/// Decoder read/allocation budget for persisted structures, including snapshot catalogs.
const DATA_LIMIT: usize = 64 * 1024 * 1024;
/// Decode limit for the superblock body — tiny and fixed-shape.
const SUPERBLOCK_LIMIT: usize = 64 * 1024;

/// Encoding for ordinary data structures (inodes, seglog records, dir
/// buckets). Recorded in the superblock as `bincode_encoding`.
pub fn data_config() -> impl Config {
    bincode::config::standard().with_limit::<DATA_LIMIT>()
}

/// Encoding for the superblock body. HARDCODED — never derived from a
/// recorded constant, so the superblock is always decodable.
pub fn superblock_config() -> impl Config {
    bincode::config::standard().with_limit::<SUPERBLOCK_LIMIT>()
}

/// Serialize `value` with `config` (bincode v2 serde-compat).
pub fn encode<T: Serialize>(value: &T, config: impl Config) -> BkfsResult<Vec<u8>> {
    Ok(bincode::serde::encode_to_vec(value, config)?)
}

/// Deserialize a value from the *exact* `bytes` with `config`, erroring if any
/// trailing bytes remain unconsumed (always a bug for a sealed payload).
pub fn decode<T: DeserializeOwned>(bytes: &[u8], config: impl Config) -> BkfsResult<T> {
    let (value, consumed): (T, usize) = bincode::serde::decode_from_slice(bytes, config)?;
    if consumed != bytes.len() {
        return Err(BkfsError {
            kind: BkfsErrorKind::Decode(bincode::error::DecodeError::Other(
                "trailing bytes after decode (struct-shape skew or corruption)",
            )),
            backtrace: None,
        });
    }
    Ok(value)
}
