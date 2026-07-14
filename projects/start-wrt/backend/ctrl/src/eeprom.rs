//! On-board EEPROM access (BPI-F3: I²C bus 2, address 0x50, 24c02).
//!
//! The EEPROM is programmed by the hardware vendor with an ONIE-format TLV
//! blob (`TlvInfo` magic, version, total_length, records, trailing CRC-32).
//! StartWRT reads tag 0x2F as the WiFi PMK (12 ASCII bytes from
//! `PASSWORD_CHARS`). All access is read-only; programming is a manufacturing
//! responsibility.

use std::fs;

use crate::prelude::*;
use crate::PASSWORD_CHARS;

pub const EEPROM_PATH: &str = "/sys/bus/i2c/devices/2-0050/eeprom";
pub const TLV_TAG_WIFI_PMK: u8 = 0x2F;
pub const PMK_LEN: usize = 12;

mod tlv {
    pub const MAGIC: &[u8; 8] = b"TlvInfo\0";
    pub const TAG_CRC: u8 = 0xFE;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Record<'a> {
        pub tag: u8,
        pub value: &'a [u8],
    }

    #[derive(Debug, PartialEq, Eq)]
    pub enum TlvError {
        BadMagic,
        TruncatedHeader,
        TruncatedRecord,
        BadCrc,
        MissingCrcRecord,
    }

    /// Parse an ONIE TLV blob. Returns the records preceding the trailing CRC
    /// record (the CRC itself is consumed by validation, not returned).
    ///
    /// Validates: magic, header length, total_length consistency, presence and
    /// correctness of the trailing CRC-32 record.
    pub fn parse(blob: &[u8]) -> Result<Vec<Record<'_>>, TlvError> {
        if blob.len() < 11 {
            return Err(TlvError::TruncatedHeader);
        }
        if &blob[0..8] != MAGIC {
            return Err(TlvError::BadMagic);
        }
        let total_len = u16::from_be_bytes([blob[9], blob[10]]) as usize;
        let end = 11 + total_len;
        if blob.len() < end {
            return Err(TlvError::TruncatedHeader);
        }
        if total_len < 6 {
            return Err(TlvError::MissingCrcRecord);
        }

        let crc_record_start = end - 6;
        if blob[crc_record_start] != TAG_CRC || blob[crc_record_start + 1] != 4 {
            return Err(TlvError::MissingCrcRecord);
        }
        let stored_crc = u32::from_be_bytes([
            blob[crc_record_start + 2],
            blob[crc_record_start + 3],
            blob[crc_record_start + 4],
            blob[crc_record_start + 5],
        ]);
        let computed_crc = crc32(&blob[..crc_record_start + 2]);
        if stored_crc != computed_crc {
            return Err(TlvError::BadCrc);
        }

        let mut records = Vec::new();
        let mut i = 11;
        while i < crc_record_start {
            if i + 2 > crc_record_start {
                return Err(TlvError::TruncatedRecord);
            }
            let tag = blob[i];
            let len = blob[i + 1] as usize;
            if i + 2 + len > crc_record_start {
                return Err(TlvError::TruncatedRecord);
            }
            records.push(Record {
                tag,
                value: &blob[i + 2..i + 2 + len],
            });
            i += 2 + len;
        }
        Ok(records)
    }

    pub fn find<'a>(records: &'a [Record<'_>], tag: u8) -> Option<&'a [u8]> {
        records.iter().find(|r| r.tag == tag).map(|r| r.value)
    }

    /// Standard reflected CRC-32 (polynomial 0xEDB88320) — same algorithm as
    /// zlib and IEEE 802.3. Verified against the vendor's known-good blob.
    pub fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // Vendor's known-good 78-byte TLV blob (from TLV.pdf example dump,
        // confirmed against live hardware reading bus 2 / 0x50).
        const VENDOR_BLOB: &[u8] = &[
            0x54, 0x6c, 0x76, 0x49, 0x6e, 0x66, 0x6f, 0x00, 0x01, 0x00, 0x43, 0x21, 0x09, 0x6b,
            0x31, 0x2d, 0x78, 0x5f, 0x64, 0x65, 0x62, 0x31, 0x23, 0x13, 0x52, 0x54, 0x4b, 0x31,
            0x56, 0x31, 0x44, 0x41, 0x45, 0x59, 0x32, 0x36, 0x30, 0x34, 0x30, 0x30, 0x30, 0x30,
            0x39, 0x24, 0x06, 0xfc, 0xa2, 0xdf, 0x10, 0x17, 0xa9, 0x2a, 0x02, 0x00, 0x02, 0x41,
            0x01, 0x01, 0x2f, 0x0c, 0x4b, 0x66, 0x3f, 0x4a, 0x25, 0x33, 0x75, 0x5a, 0x67, 0x38,
            0x64, 0x6d, 0xfe, 0x04, 0x6b, 0xa1, 0xc3, 0x55,
        ];

        #[test]
        fn parses_vendor_blob() {
            let records = parse(VENDOR_BLOB).expect("vendor blob should parse");
            assert_eq!(records.len(), 6);
            assert_eq!(find(&records, 0x21), Some(b"k1-x_deb1".as_slice()));
            assert_eq!(
                find(&records, 0x23),
                Some(b"RTK1V1DAEY260400009".as_slice())
            );
            assert_eq!(
                find(&records, 0x24),
                Some([0xfc, 0xa2, 0xdf, 0x10, 0x17, 0xa9].as_slice())
            );
            assert_eq!(find(&records, 0x2F), Some(b"Kf?J%3uZg8dm".as_slice()));
        }

        #[test]
        fn bad_magic_rejected() {
            let mut bad = VENDOR_BLOB.to_vec();
            bad[0] = b'X';
            assert_eq!(parse(&bad), Err(TlvError::BadMagic));
        }

        #[test]
        fn tampered_payload_fails_crc() {
            let mut bad = VENDOR_BLOB.to_vec();
            bad[20] ^= 0x80;
            assert_eq!(parse(&bad), Err(TlvError::BadCrc));
        }

        #[test]
        fn unprogrammed_eeprom_rejected() {
            let blob = vec![0xFF; 256];
            assert_eq!(parse(&blob), Err(TlvError::BadMagic));
        }

        #[test]
        fn truncated_header_rejected() {
            assert_eq!(parse(&[0x54, 0x6c]), Err(TlvError::TruncatedHeader));
        }

        #[test]
        fn crc32_known_value() {
            // Sanity: zlib's CRC-32 of "123456789" is 0xCBF43926
            assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        }
    }
}

/// Read the full EEPROM blob from sysfs (256 bytes for the BPI-F3's 24c02).
pub fn read_blob() -> Result<Vec<u8>, Error> {
    fs::read(EEPROM_PATH).map_err(|e| {
        Error::new(
            eyre!("failed to read {EEPROM_PATH}: {e}"),
            ErrorKind::Filesystem,
        )
    })
}

/// Why an EEPROM blob doesn't yield a usable WiFi password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PmkRejection {
    /// The blob isn't a valid ONIE TLV (unprogrammed or corrupt EEPROM).
    InvalidTlv(&'static str),
    /// The TLV parses but carries no `TLV_TAG_WIFI_PMK` record.
    MissingTag,
    /// The tag's value violates the password constraints. Length and charset
    /// are independent checks, so a doubly-bad value reports both.
    BadValue {
        /// The actual length, when it isn't `PMK_LEN`.
        wrong_length: Option<usize>,
        /// Every byte of the value not in `PASSWORD_CHARS`, in order.
        invalid_bytes: Vec<u8>,
    },
}

impl std::fmt::Display for PmkRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTlv(why) => write!(f, "not a valid ONIE TLV blob: {why}"),
            Self::MissingTag => write!(f, "no tag 0x{TLV_TAG_WIFI_PMK:02X} record in TLV"),
            Self::BadValue {
                wrong_length,
                invalid_bytes,
            } => {
                write!(f, "tag 0x{TLV_TAG_WIFI_PMK:02X} value ")?;
                if let Some(len) = wrong_length {
                    write!(f, "is {len} bytes (expected {PMK_LEN})")?;
                }
                if !invalid_bytes.is_empty() {
                    if wrong_length.is_some() {
                        write!(f, "; ")?;
                    }
                    write!(f, "has bytes outside the password charset: ")?;
                    for (i, b) in invalid_bytes.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        if b.is_ascii_graphic() {
                            write!(f, "'{}'", *b as char)?;
                        } else {
                            write!(f, "0x{b:02X}")?;
                        }
                    }
                }
                Ok(())
            }
        }
    }
}

/// Extract and validate the WiFi PMK from a raw EEPROM blob, reporting the
/// specific rejection on failure: valid ONIE TLV, tag `TLV_TAG_WIFI_PMK`
/// present, value exactly `PMK_LEN` bytes, every byte in `PASSWORD_CHARS`.
pub fn wifi_password_from_blob(blob: &[u8]) -> Result<String, PmkRejection> {
    let records = tlv::parse(blob).map_err(|e| {
        PmkRejection::InvalidTlv(match e {
            tlv::TlvError::BadMagic => "bad magic (EEPROM likely unprogrammed)",
            tlv::TlvError::TruncatedHeader => "truncated header",
            tlv::TlvError::TruncatedRecord => "truncated record",
            tlv::TlvError::BadCrc => "CRC mismatch",
            tlv::TlvError::MissingCrcRecord => "missing trailing CRC record",
        })
    })?;
    let value = tlv::find(&records, TLV_TAG_WIFI_PMK).ok_or(PmkRejection::MissingTag)?;

    // PASSWORD_CHARS is pure ASCII, so a byte-level check is equivalent to
    // the char-level constraint and needs no UTF-8 decode.
    let wrong_length = (value.len() != PMK_LEN).then_some(value.len());
    let invalid_bytes: Vec<u8> = value
        .iter()
        .copied()
        .filter(|b| !PASSWORD_CHARS.as_bytes().contains(b))
        .collect();
    if wrong_length.is_some() || !invalid_bytes.is_empty() {
        return Err(PmkRejection::BadValue {
            wrong_length,
            invalid_bytes,
        });
    }
    Ok(value.iter().map(|&b| b as char).collect())
}

/// Read the WiFi PMK from EEPROM tag 0x2F.
///
/// Returns `Ok(Some(password))` only when the blob passes every check in
/// [`wifi_password_from_blob`]. Returns `Ok(None)` for any kind of "EEPROM
/// doesn't have a usable password" — uninitialized, corrupt, missing tag,
/// wrong length, non-charset bytes — leaving the caller responsible for the
/// "no AP comes up" path. Returns `Err` only on sysfs I/O failure.
pub fn read_wifi_password() -> Result<Option<String>, Error> {
    Ok(wifi_password_from_blob(&read_blob()?).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a valid ONIE TLV blob (correct magic + trailing CRC) carrying a
    /// single record.
    fn blob_with_record(tag: u8, value: &[u8]) -> Vec<u8> {
        let mut blob = Vec::new();
        blob.extend_from_slice(tlv::MAGIC);
        blob.push(0x01); // version
        let total_len = (2 + value.len() + 6) as u16;
        blob.extend_from_slice(&total_len.to_be_bytes());
        blob.push(tag);
        blob.push(value.len() as u8);
        blob.extend_from_slice(value);
        blob.extend_from_slice(&[tlv::TAG_CRC, 4]);
        let crc = tlv::crc32(&blob);
        blob.extend_from_slice(&crc.to_be_bytes());
        blob
    }

    #[test]
    fn accepts_valid_pmk() {
        let blob = blob_with_record(TLV_TAG_WIFI_PMK, b"Kf?J%3uZg8dm");
        assert_eq!(
            wifi_password_from_blob(&blob).as_deref(),
            Ok("Kf?J%3uZg8dm")
        );
    }

    #[test]
    fn rejects_unprogrammed_eeprom() {
        assert_eq!(
            wifi_password_from_blob(&[0xFF; 256]),
            Err(PmkRejection::InvalidTlv(
                "bad magic (EEPROM likely unprogrammed)"
            ))
        );
    }

    #[test]
    fn rejects_missing_tag() {
        let blob = blob_with_record(0x21, b"k1-x_deb1");
        assert_eq!(
            wifi_password_from_blob(&blob),
            Err(PmkRejection::MissingTag)
        );
    }

    #[test]
    fn rejects_wrong_length() {
        let blob = blob_with_record(TLV_TAG_WIFI_PMK, b"Kf?J%3uZg8d");
        assert_eq!(
            wifi_password_from_blob(&blob),
            Err(PmkRejection::BadValue {
                wrong_length: Some(11),
                invalid_bytes: vec![],
            })
        );
    }

    #[test]
    fn rejects_invalid_bytes() {
        // 'l' is excluded from the non-ambiguous charset
        let blob = blob_with_record(TLV_TAG_WIFI_PMK, b"Kf?J%3uZg8dl");
        assert_eq!(
            wifi_password_from_blob(&blob),
            Err(PmkRejection::BadValue {
                wrong_length: None,
                invalid_bytes: vec![b'l'],
            })
        );
    }

    #[test]
    fn reports_length_and_charset_together() {
        // 13 bytes, containing a space and a NUL — both checks must surface
        let rejection =
            wifi_password_from_blob(&blob_with_record(TLV_TAG_WIFI_PMK, b"bad password\0"))
                .unwrap_err();
        assert_eq!(
            rejection,
            PmkRejection::BadValue {
                wrong_length: Some(13),
                invalid_bytes: vec![b' ', 0x00],
            }
        );
        let msg = rejection.to_string();
        assert!(msg.contains("13 bytes"), "missing length in: {msg}");
        // space isn't ascii-graphic, so it renders as hex like the NUL
        assert!(msg.contains("0x20"), "missing space byte in: {msg}");
        assert!(msg.contains("0x00"), "missing NUL byte in: {msg}");
    }
}
