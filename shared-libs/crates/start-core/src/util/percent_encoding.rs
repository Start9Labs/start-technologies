/// Rejects incomplete or non-hexadecimal escapes and preserves literal plus signs.
pub fn decode(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut bytes = bytes.iter().copied();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = (bytes.next()? as char).to_digit(16)?;
            let low = (bytes.next()? as char).to_digit(16)?;
            decoded.push(((high << 4) | low) as u8);
        } else {
            decoded.push(byte);
        }
    }
    Some(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_bytes_without_form_urlencoded_plus_normalization() {
        assert_eq!(
            decode(b"r%C3%A9sum%C3%A9+%2B"),
            Some("résumé++".as_bytes().to_vec())
        );
        assert_eq!(decode(b"%00%ff"), Some(vec![0, 255]));
        assert_eq!(decode(b""), Some(vec![]));
    }

    #[test]
    fn rejects_malformed_escapes() {
        for encoded in [b"%".as_slice(), b"%0", b"%GG", b"%0G"] {
            assert_eq!(decode(encoded), None);
        }
    }
}
