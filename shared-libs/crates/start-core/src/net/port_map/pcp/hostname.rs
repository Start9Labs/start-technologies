//! PCP `HOSTNAME` option (RFC 6887 extension): associates FQDNs with a MAP
//! request so a gateway can SNI-demultiplex inbound TLS on a shared external
//! port (e.g. 443) by the ClientHello SNI.
//!
//! The code and result codes are not IANA-assigned, so we use the PCP Private
//! Use ranges (Options 224-254, Result Codes 192-254). Shared by the
//! StartTunnel PCP server (parses/echoes) and the StartOS client (emits via
//! `crab_nat::pcp::PcpOption`).

/// HOSTNAME option code (optional-to-process Private Use range).
pub const OPTION_HOSTNAME: u8 = 224;

/// Hostname already bound on this external address+port (short-lifetime error).
pub const RESULT_HOSTNAME_TAKEN: u8 = 192;
/// HOSTNAME requests an unsupported feature (long-lifetime error).
pub const RESULT_UNSUPP_HOSTNAME: u8 = 193;

/// Valid as an SNI demux key: 1-255 octets, ASCII labels of `[A-Za-z0-9-]`
/// (leading `*` label allowed), no leading/trailing dot, no empty labels.
pub fn validate_hostname(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 || name.starts_with('.') || name.ends_with('.') {
        return false;
    }
    name.split('.').enumerate().all(|(i, label)| {
        !label.is_empty()
            && label.bytes().enumerate().all(|(j, b)| {
                b.is_ascii_alphanumeric()
                    || b == b'-'
                    || (b == b'*' && i == 0 && j == 0 && label.len() == 1)
            })
    })
}

/// Append a HOSTNAME option (RFC 6887 §7.3 framing, zero-padded to 32 bits).
/// `buf` MUST already be 32-bit aligned (true after the fixed opcode payload).
pub fn encode_hostname_option(buf: &mut Vec<u8>, hostname: &str) {
    super::encode_pcp_option(buf, OPTION_HOSTNAME, hostname.as_bytes());
}

/// Lowercased hostnames carried by HOSTNAME options in a PCP opcode's option
/// area; other options skipped. `Err` on a truncated or invalid option so the
/// caller can reply MALFORMED_OPTION.
pub fn parse_hostname_options(tail: &[u8]) -> Result<Vec<String>, ()> {
    let mut names = Vec::new();
    for opt in super::pcp_options(tail) {
        let (code, value) = opt?;
        if code == OPTION_HOSTNAME {
            let value = std::str::from_utf8(value).map_err(|_| ())?;
            if !validate_hostname(value) {
                return Err(());
            }
            names.push(value.to_ascii_lowercase());
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate() {
        assert!(validate_hostname("git.example.com"));
        assert!(validate_hostname("a"));
        assert!(validate_hostname("*.example.com"));
        assert!(!validate_hostname(""));
        assert!(!validate_hostname(".example.com"));
        assert!(!validate_hostname("example.com."));
        assert!(!validate_hostname("ex ample.com"));
        assert!(!validate_hostname("ex*ample.com"));
        assert!(!validate_hostname("foo..bar"));
        assert!(!validate_hostname(&"a".repeat(256)));
    }

    #[test]
    fn round_trip_single() {
        let mut buf = Vec::new();
        encode_hostname_option(&mut buf, "git.example.com");
        // 4 header + 15 data = 19 -> padded to 20.
        assert_eq!(buf.len(), 20);
        assert_eq!(buf.len() % 4, 0);
        assert_eq!(buf[0], OPTION_HOSTNAME);
        assert_eq!(
            parse_hostname_options(&buf).unwrap(),
            vec!["git.example.com"]
        );
    }

    #[test]
    fn round_trip_multiple_and_lowercases() {
        let mut buf = Vec::new();
        encode_hostname_option(&mut buf, "First.Example.com");
        encode_hostname_option(&mut buf, "second.example.com");
        assert_eq!(
            parse_hostname_options(&buf).unwrap(),
            vec!["first.example.com", "second.example.com"]
        );
    }

    #[test]
    fn skips_unknown_options() {
        // An unknown 4-byte option (code 200, len 0) before a HOSTNAME.
        let mut buf = vec![200u8, 0, 0, 0];
        encode_hostname_option(&mut buf, "host.example.com");
        assert_eq!(
            parse_hostname_options(&buf).unwrap(),
            vec!["host.example.com"]
        );
    }

    #[test]
    fn rejects_truncated() {
        // code, reserved, length=10 but no data.
        assert!(parse_hostname_options(&[OPTION_HOSTNAME, 0, 0, 10]).is_err());
    }

    #[test]
    fn empty_tail_is_no_names() {
        assert_eq!(parse_hostname_options(&[]).unwrap(), Vec::<String>::new());
    }
}
