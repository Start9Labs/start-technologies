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
    fn crc32(data: &[u8]) -> u32 {
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
            0x54, 0x6c, 0x76, 0x49, 0x6e, 0x66, 0x6f, 0x00,
            0x01, 0x00, 0x43,
            0x21, 0x09, 0x6b, 0x31, 0x2d, 0x78, 0x5f, 0x64, 0x65, 0x62, 0x31,
            0x23, 0x13, 0x52, 0x54, 0x4b, 0x31, 0x56, 0x31, 0x44, 0x41, 0x45,
            0x59, 0x32, 0x36, 0x30, 0x34, 0x30, 0x30, 0x30, 0x30, 0x39,
            0x24, 0x06, 0xfc, 0xa2, 0xdf, 0x10, 0x17, 0xa9,
            0x2a, 0x02, 0x00, 0x02,
            0x41, 0x01, 0x01,
            0x2f, 0x0c, 0x4b, 0x66, 0x3f, 0x4a, 0x25, 0x33, 0x75, 0x5a, 0x67, 0x38, 0x64, 0x6d,
            0xfe, 0x04, 0x6b, 0xa1, 0xc3, 0x55,
        ];

        #[test]
        fn parses_vendor_blob() {
            let records = parse(VENDOR_BLOB).expect("vendor blob should parse");
            assert_eq!(records.len(), 6);
            assert_eq!(find(&records, 0x21), Some(b"k1-x_deb1".as_slice()));
            assert_eq!(find(&records, 0x23), Some(b"RTK1V1DAEY260400009".as_slice()));
            assert_eq!(find(&records, 0x24), Some([0xfc, 0xa2, 0xdf, 0x10, 0x17, 0xa9].as_slice()));
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

/// Read the WiFi PMK from EEPROM tag 0x2F.
///
/// Returns `Ok(Some(password))` only when the blob is a valid ONIE TLV with
/// a 12-byte ASCII value at tag 0x2F whose every character is in
/// `PASSWORD_CHARS`. Returns `Ok(None)` for any kind of "EEPROM doesn't
/// have a usable password" — uninitialized, corrupt, missing tag, wrong
/// length, non-charset bytes — leaving the caller responsible for the
/// "no AP comes up" path. Returns `Err` only on sysfs I/O failure.
pub fn read_wifi_password() -> Result<Option<String>, Error> {
    let blob = read_blob()?;
    let records = match tlv::parse(&blob) {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };
    let value = match tlv::find(&records, TLV_TAG_WIFI_PMK) {
        Some(v) => v,
        None => return Ok(None),
    };
    if value.len() != PMK_LEN {
        return Ok(None);
    }
    let password = match std::str::from_utf8(value) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    if !password.chars().all(|c| PASSWORD_CHARS.contains(c)) {
        return Ok(None);
    }
    Ok(Some(password.to_string()))
}
