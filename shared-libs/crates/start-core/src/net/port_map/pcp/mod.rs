//! PCP option extensions shared by client and server: HOSTNAME (SNI demux) and
//! PORT_SET (contiguous port ranges).

pub mod capability;
pub mod hostname;
pub mod portset;

/// PCP NO_RESOURCES result code (RFC 6887 §7.4).
pub const RESULT_NO_RESOURCES: u8 = 8;

/// Walk the PCP option area (RFC 6887 §7.3): each option is code(1),
/// reserved(1), length(2), value(length), padded to a 32-bit boundary. Yields
/// `(code, value)` per option; one `Err(())` then stops on a length overrun.
pub(super) fn pcp_options(mut tail: &[u8]) -> impl Iterator<Item = Result<(u8, &[u8]), ()>> {
    std::iter::from_fn(move || {
        if tail.len() < 4 {
            return None;
        }
        let code = tail[0];
        let len = u16::from_be_bytes([tail[2], tail[3]]) as usize;
        if 4 + len > tail.len() {
            tail = &[];
            return Some(Err(()));
        }
        let value = &tail[4..4 + len];
        // A trailing option may omit final padding; clamp to the buffer.
        let end = ((4 + len + 3) & !3).min(tail.len());
        tail = &tail[end..];
        Some(Ok((code, value)))
    })
}

/// Append a PCP option (RFC 6887 §7.3): code, reserved 0, BE length, value,
/// zero-padded to a 32-bit boundary.
pub(super) fn encode_pcp_option(buf: &mut Vec<u8>, code: u8, data: &[u8]) {
    buf.push(code);
    buf.push(0); // reserved
    buf.extend_from_slice(&(data.len() as u16).to_be_bytes());
    buf.extend_from_slice(data);
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
}
