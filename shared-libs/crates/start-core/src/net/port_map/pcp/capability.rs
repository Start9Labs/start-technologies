//! Interim PCP capability marker: a gateway answers ANNOUNCE (opcode 0) with
//! this Start9 vendor option so a client confirms HOSTNAME support before
//! emitting OPTION_HOSTNAME (224, a Private-Use code that could collide with
//! another vendor). Once HOSTNAME has an IANA-assigned code this discovery is
//! moot, so it lives only in the implementation — NOT in the RFC draft. Shared
//! verbatim by every gateway (StartTunnel, StartWRT) and the StartOS client.

/// Start9 capability option code (Private Use range, next free after 224).
pub const OPTION_START9_CAPABILITY: u8 = 225;
/// Fixed "I speak HOSTNAME" marker: "S9" tag, a discriminator byte, and a
/// format version byte (lets a future format bump without reusing code 225).
pub const START9_CAPABILITY_MAGIC: [u8; 4] = *b"S9\x3b\x01";

/// Append the Start9 capability option (RFC 6887 §7.3 framing, 32-bit padded).
pub fn encode_start9_capability_option(buf: &mut Vec<u8>) {
    super::encode_pcp_option(buf, OPTION_START9_CAPABILITY, &START9_CAPABILITY_MAGIC);
}

/// True iff the option area carries the exact Start9 capability marker.
pub fn has_start9_capability(tail: &[u8]) -> bool {
    super::pcp_options(tail)
        .flatten()
        .any(|(code, value)| code == OPTION_START9_CAPABILITY && value == START9_CAPABILITY_MAGIC)
}

#[cfg(test)]
mod tests {
    use super::super::encode_pcp_option;
    use super::*;

    #[test]
    fn encode_size_and_framing() {
        let mut b = Vec::new();
        encode_start9_capability_option(&mut b);
        assert_eq!(b.len(), 8);
        assert_eq!(b.len() % 4, 0);
        assert_eq!(b[0], OPTION_START9_CAPABILITY);
        assert_eq!(b[1], 0);
        assert_eq!(u16::from_be_bytes([b[2], b[3]]), 4);
        assert_eq!(&b[4..8], &START9_CAPABILITY_MAGIC);
    }

    #[test]
    fn recognizes_exact_marker() {
        let mut b = Vec::new();
        encode_start9_capability_option(&mut b);
        assert!(has_start9_capability(&b));
    }

    #[test]
    fn rejects_wrong_value() {
        // Right code, wrong (future) version byte -> not a match.
        let mut b = Vec::new();
        encode_pcp_option(&mut b, OPTION_START9_CAPABILITY, b"S9\x3b\x02");
        assert!(!has_start9_capability(&b));
    }

    #[test]
    fn rejects_wrong_code() {
        let mut b = Vec::new();
        encode_pcp_option(&mut b, 224, &START9_CAPABILITY_MAGIC);
        assert!(!has_start9_capability(&b));
    }

    #[test]
    fn skips_unknown_option_then_matches() {
        let mut b = Vec::new();
        encode_pcp_option(&mut b, 200, &[1, 2, 3, 4]);
        encode_start9_capability_option(&mut b);
        assert!(has_start9_capability(&b));
    }

    #[test]
    fn rejects_empty_and_malformed() {
        assert!(!has_start9_capability(&[]));
        assert!(!has_start9_capability(&[
            OPTION_START9_CAPABILITY,
            0,
            0,
            10
        ]));
    }
}
