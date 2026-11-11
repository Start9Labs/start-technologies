use tokio_rustls::rustls::Side;
use tokio_rustls::rustls::crypto::ring;
use tokio_rustls::rustls::quic::{DirectionalKeys, Keys, Version};

use super::{HelloStatus, inspect_client_hello};

const CRYPTO_CAP: usize = 65535;
const V1: u32 = 1;
const V2: u32 = 0x6b3343cf;

/// Reassembles at most 65535 CRYPTO bytes from one version and destination ID.
/// The caller owns datagram and lifetime limits.
#[derive(Default)]
pub(super) struct InitialHello {
    initial: Option<Initial>,
    terminal: Option<HelloStatus>,
}

struct Initial {
    version: u32,
    dcid: Vec<u8>,
    keys: DirectionalKeys,
    largest_pn: Option<u64>,
    crypto: Vec<u8>,
    present: Vec<bool>,
    contiguous: usize,
    closed: bool,
}

impl InitialHello {
    pub(super) fn ingest(&mut self, datagram: &[u8]) -> HelloStatus {
        if let Some(status) = &self.terminal {
            return status.clone();
        }
        let status = self.ingest_inner(datagram).unwrap_or(HelloStatus::Invalid);
        if status != HelloStatus::Pending {
            self.terminal = Some(status.clone());
        }
        status
    }

    fn ingest_inner(&mut self, mut datagram: &[u8]) -> Result<HelloStatus, ()> {
        if datagram.is_empty() || datagram.len() > u16::MAX as usize {
            return Err(());
        }
        while !datagram.is_empty() {
            let packet = datagram;
            let first = packet[0];
            if first & 0xc0 != 0xc0 {
                if self.initial.is_some() {
                    break;
                }
                return Err(());
            }
            take(&mut datagram, 1)?;
            let wire_version = u32::from_be_bytes(take(&mut datagram, 4)?.try_into().unwrap());
            let (version, initial_type) = match wire_version {
                V1 => (Version::V1, 0),
                V2 => (Version::V2, 1),
                _ if self.initial.is_some() => break,
                _ => return Err(()),
            };
            if (first >> 4) & 3 != initial_type {
                if self.initial.is_some() {
                    break;
                }
                return Err(());
            }
            let dcid_len = take(&mut datagram, 1)?[0] as usize;
            if dcid_len > 20 {
                return Err(());
            }
            let dcid = take(&mut datagram, dcid_len)?;
            let scid_len = take(&mut datagram, 1)?[0] as usize;
            if scid_len > 20 {
                return Err(());
            }
            take(&mut datagram, scid_len)?;
            let token_len = size(&mut datagram)?;
            take(&mut datagram, token_len)?;
            let packet_len = size(&mut datagram)?;
            let pn_offset = packet.len() - datagram.len();
            take(&mut datagram, packet_len)?;
            let packet = &packet[..pn_offset + packet_len];

            let initial = self.initial.get_or_insert_with(|| Initial {
                version: wire_version,
                dcid: dcid.to_vec(),
                keys: initial_keys(version, dcid, Side::Server).remote,
                largest_pn: None,
                crypto: Vec::new(),
                present: Vec::new(),
                contiguous: 0,
                closed: false,
            });
            if initial.version != wire_version || initial.dcid != dcid {
                return Err(());
            }
            initial.packet(packet, pn_offset)?;
        }
        self.initial.as_ref().ok_or(())?.status()
    }
}

fn initial_keys(version: Version, dcid: &[u8], side: Side) -> Keys {
    let suite = ring::cipher_suite::TLS13_AES_128_GCM_SHA256
        .tls13()
        .unwrap();
    Keys::initial(version, suite, suite.quic.unwrap(), dcid, side)
}

impl Initial {
    fn packet(&mut self, packet: &[u8], pn_offset: usize) -> Result<(), ()> {
        let sample_start = pn_offset + 4;
        let sample = packet
            .get(sample_start..sample_start + self.keys.header.sample_len())
            .ok_or(())?;
        let mut first = packet[0];
        let mut pn_bytes: [u8; 4] = packet[pn_offset..sample_start].try_into().unwrap();
        self.keys
            .header
            .decrypt_in_place(sample, &mut first, &mut pn_bytes)
            .map_err(|_| ())?;
        if first & 0x0c != 0 {
            return Err(());
        }
        let pn_len = (first & 3) as usize + 1;
        let truncated = pn_bytes[..pn_len]
            .iter()
            .fold(0u64, |n, &b| (n << 8) | u64::from(b));
        let pn = packet_number(truncated, pn_len, self.largest_pn)?;
        let header_len = pn_offset + pn_len;
        let mut header = packet[..header_len].to_vec();
        header[0] = first;
        header[pn_offset..].copy_from_slice(&pn_bytes[..pn_len]);
        let mut payload = packet[header_len..].to_vec();
        let plaintext = self
            .keys
            .packet
            .decrypt_in_place(pn, &header, &mut payload)
            .map_err(|_| ())?;
        self.largest_pn = Some(self.largest_pn.map_or(pn, |previous| previous.max(pn)));
        self.frames(plaintext)
    }

    fn frames(&mut self, mut payload: &[u8]) -> Result<(), ()> {
        while !payload.is_empty() {
            match varint(&mut payload)? {
                0 | 1 => {}
                frame @ (2 | 3) => {
                    let mut largest = varint(&mut payload)?;
                    varint(&mut payload)?;
                    let ranges = varint(&mut payload)?;
                    let first = varint(&mut payload)?;
                    largest = largest.checked_sub(first).ok_or(())?;
                    for _ in 0..ranges {
                        let gap = varint(&mut payload)?;
                        let range = varint(&mut payload)?;
                        largest = largest.checked_sub(gap + 2).ok_or(())?;
                        largest = largest.checked_sub(range).ok_or(())?;
                    }
                    if frame == 3 {
                        for _ in 0..3 {
                            varint(&mut payload)?;
                        }
                    }
                }
                6 => {
                    let offset = size(&mut payload)?;
                    let len = size(&mut payload)?;
                    let end = offset
                        .checked_add(len)
                        .filter(|&n| n <= CRYPTO_CAP)
                        .ok_or(())?;
                    let bytes = take(&mut payload, len)?;
                    self.crypto.resize(self.crypto.len().max(end), 0);
                    self.present.resize(self.present.len().max(end), false);
                    for (i, &byte) in (offset..end).zip(bytes) {
                        if self.present[i] && self.crypto[i] != byte {
                            return Err(());
                        }
                        self.crypto[i] = byte;
                        self.present[i] = true;
                    }
                    while self.contiguous < self.present.len() && self.present[self.contiguous] {
                        self.contiguous += 1;
                    }
                }
                0x1c => {
                    varint(&mut payload)?;
                    varint(&mut payload)?;
                    let reason_len = size(&mut payload)?;
                    take(&mut payload, reason_len)?;
                    self.closed = true;
                }
                _ => return Err(()),
            }
        }
        Ok(())
    }

    fn status(&self) -> Result<HelloStatus, ()> {
        if self.contiguous > 0 && self.crypto[0] != 1 {
            return Err(());
        }
        if self.contiguous >= 4 {
            let len =
                u32::from_be_bytes([0, self.crypto[1], self.crypto[2], self.crypto[3]]) as usize;
            let end = len.checked_add(4).filter(|&n| n <= CRYPTO_CAP).ok_or(())?;
            if self.contiguous >= end {
                let mut records = Vec::with_capacity(end + end.div_ceil(16384) * 5);
                for chunk in self.crypto[..end].chunks(16384) {
                    records.extend_from_slice(&[22, 3, 3]);
                    records.extend_from_slice(&(chunk.len() as u16).to_be_bytes());
                    records.extend_from_slice(chunk);
                }
                return Ok(inspect_client_hello(&records));
            }
        }
        if self.closed {
            return Err(());
        }
        Ok(HelloStatus::Pending)
    }
}

fn packet_number(truncated: u64, len: usize, largest: Option<u64>) -> Result<u64, ()> {
    let expected = largest.map_or(0, |n| n + 1);
    let window = 1u64 << (len * 8);
    let half_window = window / 2;
    let mut candidate = (expected & !(window - 1)) | truncated;
    if candidate + half_window <= expected && candidate < (1 << 62) - window {
        candidate += window;
    } else if candidate > expected + half_window && candidate >= window {
        candidate -= window;
    }
    (candidate < 1 << 62).then_some(candidate).ok_or(())
}

fn take<'a>(input: &mut &'a [u8], len: usize) -> Result<&'a [u8], ()> {
    let bytes = input.get(..len).ok_or(())?;
    *input = &input[len..];
    Ok(bytes)
}

fn varint(input: &mut &[u8]) -> Result<u64, ()> {
    let first = *input.first().ok_or(())?;
    let bytes = take(input, 1 << (first >> 6))?;
    Ok(bytes[1..]
        .iter()
        .fold(u64::from(first & 0x3f), |n, &b| (n << 8) | u64::from(b)))
}

fn size(input: &mut &[u8]) -> Result<usize, ()> {
    usize::try_from(varint(input)?).map_err(|_| ())
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::Arc;

    use tokio_rustls::rustls::pki_types::ServerName;
    use tokio_rustls::rustls::{ClientConfig, RootCertStore, quic};

    use super::*;

    const DCID: &[u8] = &[0x83, 0x94, 0xc8, 0xf0, 0x3e, 0x51, 0x57, 0x08];

    fn version(wire: u32) -> Version {
        match wire {
            V1 => Version::V1,
            V2 => Version::V2,
            _ => panic!("unsupported test version"),
        }
    }

    pub(in crate::tunnel::forward::sni) fn client_hello(wire: u32, sni: bool) -> Vec<u8> {
        client_hello_with_params(wire, sni, Vec::new())
    }

    fn client_hello_with_params(wire: u32, sni: bool, params: Vec<u8>) -> Vec<u8> {
        let mut config = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
            .with_protocol_versions(&[&tokio_rustls::rustls::version::TLS13])
            .unwrap()
            .with_root_certificates(RootCertStore::empty())
            .with_no_client_auth();
        config.enable_sni = sni;
        config.alpn_protocols = vec![b"h3".to_vec()];
        let mut client = quic::ClientConnection::new(
            Arc::new(config),
            version(wire),
            ServerName::try_from("Quic.Example.com").unwrap().to_owned(),
            params,
        )
        .unwrap();
        let mut hello = Vec::new();
        assert!(client.write_hs(&mut hello).is_none());
        assert_eq!(hello[0], 1);
        hello
    }

    fn encode(n: u64, out: &mut Vec<u8>) {
        let (len, prefix) = match n {
            0..64 => (1, 0),
            64..16384 => (2, 0x40),
            16384..1073741824 => (4, 0x80),
            _ => (8, 0xc0),
        };
        let bytes = n.to_be_bytes();
        let start = out.len();
        out.extend_from_slice(&bytes[8 - len..]);
        out[start] |= prefix;
    }

    fn crypto(offset: u64, bytes: &[u8]) -> Vec<u8> {
        let mut frame = vec![6];
        encode(offset, &mut frame);
        encode(bytes.len() as u64, &mut frame);
        frame.extend_from_slice(bytes);
        frame
    }

    fn packet(wire: u32, dcid: &[u8], pn: u64, pn_len: usize, frames: &[u8]) -> Vec<u8> {
        let keys = initial_keys(version(wire), dcid, Side::Client).local;
        let mut payload = frames.to_vec();
        payload.resize(payload.len().max(4 - pn_len), 0);
        let first = (if wire == V1 { 0xc0 } else { 0xd0 }) | (pn_len as u8 - 1);
        let mut header = vec![first];
        header.extend_from_slice(&wire.to_be_bytes());
        header.push(dcid.len() as u8);
        header.extend_from_slice(dcid);
        header.extend_from_slice(&[2, 1, 2, 3, 5, 6, 7]);
        encode(
            (pn_len + payload.len() + keys.packet.tag_len()) as u64,
            &mut header,
        );
        let pn_offset = header.len();
        header.extend_from_slice(&pn.to_be_bytes()[8 - pn_len..]);
        let tag = keys
            .packet
            .encrypt_in_place(pn, &header, &mut payload)
            .unwrap();
        let mut packet = header;
        packet.extend_from_slice(&payload);
        packet.extend_from_slice(tag.as_ref());
        let sample = packet[pn_offset + 4..pn_offset + 4 + keys.header.sample_len()].to_vec();
        let (first, rest) = packet.split_at_mut(1);
        keys.header
            .encrypt_in_place(
                &sample,
                &mut first[0],
                &mut rest[pn_offset - 1..pn_offset - 1 + pn_len],
            )
            .unwrap();
        packet
    }

    pub(in crate::tunnel::forward::sni) fn initial(
        wire: u32,
        pn: u64,
        offset: usize,
        data: &[u8],
    ) -> Vec<u8> {
        packet(wire, DCID, pn, 2, &crypto(offset as u64, data))
    }

    fn complete() -> HelloStatus {
        HelloStatus::Complete(Some("quic.example.com".into()))
    }

    #[test]
    fn generated_hellos_with_and_without_sni() {
        for wire in [V1, V2] {
            for sni in [false, true] {
                let hello = client_hello(wire, sni);
                let packet = initial(wire, 0, 0, &hello);
                assert_eq!(
                    InitialHello::default().ingest(&packet),
                    if sni {
                        complete()
                    } else {
                        HelloStatus::Complete(None)
                    }
                );
            }
        }
    }

    #[test]
    fn client_hello_spanning_tls_records() {
        for wire in [V1, V2] {
            let hello = client_hello_with_params(wire, true, vec![0; 32000]);
            assert!(hello.len() > 16384 && hello.len() < CRYPTO_CAP);
            let mut decoder = InitialHello::default();
            for (i, chunk) in hello.chunks(10000).enumerate() {
                let status = decoder.ingest(&initial(wire, i as u64, i * 10000, chunk));
                assert_eq!(
                    status,
                    if (i + 1) * 10000 >= hello.len() {
                        complete()
                    } else {
                        HelloStatus::Pending
                    }
                );
            }
        }
    }

    #[test]
    fn fragmented_out_of_order_and_identical_overlaps() {
        for wire in [V1, V2] {
            let hello = client_hello(wire, true);
            let mut decoder = InitialHello::default();
            for (pn, offset, end) in [(2, 90, hello.len()), (0, 0, 2), (1, 1, 100)] {
                let status = decoder.ingest(&initial(wire, pn, offset, &hello[offset..end]));
                assert_eq!(
                    status,
                    if pn == 1 {
                        complete()
                    } else {
                        HelloStatus::Pending
                    }
                );
            }
        }
    }

    #[test]
    fn conflicting_overlap_is_terminal() {
        let hello = client_hello(V1, true);
        let mut decoder = InitialHello::default();
        let partial = initial(V1, 0, 0, &hello[..100]);
        assert_eq!(decoder.ingest(&partial), HelloStatus::Pending);
        assert_eq!(decoder.ingest(&partial), HelloStatus::Pending);
        let mut conflict = hello[50..110].to_vec();
        conflict[0] ^= 1;
        assert_eq!(
            decoder.ingest(&initial(V1, 1, 50, &conflict)),
            HelloStatus::Invalid
        );
        assert_eq!(
            decoder.ingest(&initial(V1, 2, 0, &hello)),
            HelloStatus::Invalid
        );
    }

    #[test]
    fn coalesced_initials_and_trailing_other_packet() {
        for wire in [V1, V2] {
            let hello = client_hello(wire, true);
            let mut datagram = initial(wire, 0, 0, &hello[..100]);
            datagram.extend(initial(wire, 1, 100, &hello[100..]));
            datagram.extend_from_slice(&[0x40, 1, 2, 3]);
            assert_eq!(InitialHello::default().ingest(&datagram), complete());
            let mut datagram = initial(wire, 0, 0, &hello[..100]);
            datagram.extend_from_slice(&[if wire == V1 { 0xe0 } else { 0xf0 }]);
            datagram.extend_from_slice(&wire.to_be_bytes());
            assert_eq!(
                InitialHello::default().ingest(&datagram),
                HelloStatus::Pending
            );
            let mut datagram = initial(wire, 0, 0, &hello);
            let mut conflict = hello.clone();
            conflict[5] ^= 1;
            datagram.extend(initial(wire, 1, 0, &conflict));
            assert_eq!(
                InitialHello::default().ingest(&datagram),
                HelloStatus::Invalid
            );
        }
    }

    #[test]
    fn mixed_version_and_dcid_are_rejected() {
        let hello = client_hello(V1, true);
        for second in [
            initial(V2, 1, 100, &hello[100..]),
            packet(V1, b"another-id", 1, 2, &crypto(100, &hello[100..])),
        ] {
            let mut decoder = InitialHello::default();
            assert_eq!(
                decoder.ingest(&initial(V1, 0, 0, &hello[..100])),
                HelloStatus::Pending
            );
            assert_eq!(decoder.ingest(&second), HelloStatus::Invalid);
        }
    }

    #[test]
    fn packet_numbers_wrap_and_arrive_out_of_order() {
        let hello = client_hello(V1, true);
        let mut decoder = InitialHello::default();
        assert_eq!(
            decoder.ingest(&packet(V1, DCID, 255, 1, &crypto(100, &hello[100..]))),
            HelloStatus::Pending
        );
        assert_eq!(
            decoder.ingest(&packet(V1, DCID, 256, 1, &[1])),
            HelloStatus::Pending
        );
        assert_eq!(
            decoder.ingest(&packet(V1, DCID, 254, 1, &crypto(0, &hello[..100]))),
            complete()
        );
        assert_eq!(packet_number(0, 1, Some(255)), Ok(256));
        assert_eq!(packet_number(254, 1, Some(256)), Ok(254));
        assert!(packet_number(0, 4, Some((1 << 62) - 1)).is_err());
    }

    #[test]
    fn rejects_truncated_packets_and_corrupt_tags() {
        for wire in [V1, V2] {
            let hello = client_hello(wire, true);
            let mut packet = initial(wire, 0, 0, &hello);
            for end in 0..packet.len() {
                assert_eq!(
                    InitialHello::default().ingest(&packet[..end]),
                    HelloStatus::Invalid,
                    "prefix {end}"
                );
            }
            *packet.last_mut().unwrap() ^= 1;
            assert_eq!(
                InitialHello::default().ingest(&packet),
                HelloStatus::Invalid
            );
        }
    }

    #[test]
    fn unknown_versions_and_non_initial_packets_need_a_candidate() {
        for packet in [
            vec![0x40, 1, 2, 3],
            vec![0xc0, 0xff, 0xff, 0xff, 0xff],
            vec![0xe0, 0, 0, 0, 1],
            vec![0xc0, 0x6b, 0x33, 0x43, 0xcf],
        ] {
            assert_eq!(
                InitialHello::default().ingest(&packet),
                HelloStatus::Invalid
            );
            let mut decoder = InitialHello::default();
            assert_eq!(
                decoder.ingest(&initial(V1, 0, 0, &[1, 0])),
                HelloStatus::Pending
            );
            assert_eq!(decoder.ingest(&packet), HelloStatus::Pending);
        }
    }

    #[test]
    fn crypto_and_handshake_bounds() {
        for (offset, data) in [
            (CRYPTO_CAP as u64, &[0][..]),
            ((1 << 62) - 1, &[0][..]),
            (0, &[1, 1, 0, 0][..]),
            (0, &[2, 0, 0, 0][..]),
        ] {
            let packet = packet(V1, DCID, 0, 2, &crypto(offset, data));
            let mut decoder = InitialHello::default();
            assert_eq!(decoder.ingest(&packet), HelloStatus::Invalid);
            assert!(decoder.initial.as_ref().unwrap().crypto.len() <= CRYPTO_CAP);
        }
        let mut decoder = InitialHello::default();
        assert_eq!(
            decoder.ingest(&initial(V1, 0, CRYPTO_CAP - 1, &[0])),
            HelloStatus::Pending
        );
        assert_eq!(decoder.initial.as_ref().unwrap().crypto.len(), CRYPTO_CAP);
        let partial = [6, 0, 20, 1];
        assert_eq!(
            InitialHello::default().ingest(&packet(V1, DCID, 0, 2, &partial)),
            HelloStatus::Invalid
        );
    }

    #[test]
    fn parses_initial_control_frames_and_rejects_bad_frames() {
        let hello = client_hello(V1, true);
        let mut frames = vec![0, 1, 2, 10, 0, 1, 1, 2, 1, 3, 10, 0, 0, 0, 1, 2, 3];
        frames.extend(crypto(0, &hello));
        frames.extend_from_slice(&[0x1c, 0, 0, 3, b'b', b'y', b'e']);
        assert_eq!(
            InitialHello::default().ingest(&packet(V1, DCID, 0, 2, &frames)),
            complete()
        );
        for frames in [
            vec![0x1c, 0, 0, 0],
            vec![0x1c, 0, 0, 20],
            vec![0x1d, 0, 0],
            vec![7],
            vec![2, 0, 0, 0, 1],
            vec![2, 0, 0, 1, 0, 0, 0],
            vec![3, 0, 0, 0, 0],
        ] {
            assert_eq!(
                InitialHello::default().ingest(&packet(V1, DCID, 0, 2, &frames)),
                HelloStatus::Invalid
            );
        }
    }

    #[test]
    fn varint_widths_and_truncation() {
        for value in [
            0,
            63,
            64,
            16383,
            16384,
            (1 << 30) - 1,
            1 << 30,
            (1 << 62) - 1,
        ] {
            let mut bytes = Vec::new();
            encode(value, &mut bytes);
            assert_eq!(varint(&mut bytes.as_slice()), Ok(value));
            for end in 0..bytes.len() {
                assert!(varint(&mut &bytes[..end]).is_err());
            }
        }
    }

    fn unhex(hex: &str) -> Vec<u8> {
        let hex: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn rfc9001_and_rfc9369_client_initials() {
        for vector in [RFC9001_CLIENT_INITIAL, RFC9369_CLIENT_INITIAL] {
            let packet = unhex(vector);
            assert_eq!(packet.len(), 1200);
            assert_eq!(
                InitialHello::default().ingest(&packet),
                HelloStatus::Complete(Some("example.com".into()))
            );
        }
    }

    // RFC 9001 A.2; Copyright (c) 2021 IETF Trust and the document authors.
    // Simplified BSD terms: quic-vectors.LICENSE.txt.
    const RFC9001_CLIENT_INITIAL: &str =
        "c000000001088394c8f03e5157080000 449e7b9aec34d1b1c98dd7689fb8ec11
   d242b123dc9bd8bab936b47d92ec356c 0bab7df5976d27cd449f63300099f399
   1c260ec4c60d17b31f8429157bb35a12 82a643a8d2262cad67500cadb8e7378c
   8eb7539ec4d4905fed1bee1fc8aafba1 7c750e2c7ace01e6005f80fcb7df6212
   30c83711b39343fa028cea7f7fb5ff89 eac2308249a02252155e2347b63d58c5
   457afd84d05dfffdb20392844ae81215 4682e9cf012f9021a6f0be17ddd0c208
   4dce25ff9b06cde535d0f920a2db1bf3 62c23e596d11a4f5a6cf3948838a3aec
   4e15daf8500a6ef69ec4e3feb6b1d98e 610ac8b7ec3faf6ad760b7bad1db4ba3
   485e8a94dc250ae3fdb41ed15fb6a8e5 eba0fc3dd60bc8e30c5c4287e53805db
   059ae0648db2f64264ed5e39be2e20d8 2df566da8dd5998ccabdae053060ae6c
   7b4378e846d29f37ed7b4ea9ec5d82e7 961b7f25a9323851f681d582363aa5f8
   9937f5a67258bf63ad6f1a0b1d96dbd4 faddfcefc5266ba6611722395c906556
   be52afe3f565636ad1b17d508b73d874 3eeb524be22b3dcbc2c7468d54119c74
   68449a13d8e3b95811a198f3491de3e7 fe942b330407abf82a4ed7c1b311663a
   c69890f4157015853d91e923037c227a 33cdd5ec281ca3f79c44546b9d90ca00
   f064c99e3dd97911d39fe9c5d0b23a22 9a234cb36186c4819e8b9c5927726632
   291d6a418211cc2962e20fe47feb3edf 330f2c603a9d48c0fcb5699dbfe58964
   25c5bac4aee82e57a85aaf4e2513e4f0 5796b07ba2ee47d80506f8d2c25e50fd
   14de71e6c418559302f939b0e1abd576 f279c4b2e0feb85c1f28ff18f58891ff
   ef132eef2fa09346aee33c28eb130ff2 8f5b766953334113211996d20011a198
   e3fc433f9f2541010ae17c1bf202580f 6047472fb36857fe843b19f5984009dd
   c324044e847a4f4a0ab34f719595de37 252d6235365e9b84392b061085349d73
   203a4a13e96f5432ec0fd4a1ee65accd d5e3904df54c1da510b0ff20dcc0c77f
   cb2c0e0eb605cb0504db87632cf3d8b4 dae6e705769d1de354270123cb11450e
   fc60ac47683d7b8d0f811365565fd98c 4c8eb936bcab8d069fc33bd801b03ade
   a2e1fbc5aa463d08ca19896d2bf59a07 1b851e6c239052172f296bfb5e724047
   90a2181014f3b94a4e97d117b4381303 68cc39dbb2d198065ae3986547926cd2
   162f40a29f0c3c8745c0f50fba3852e5 66d44575c29d39a03f0cda721984b6f4
   40591f355e12d439ff150aab7613499d bd49adabc8676eef023b15b65bfc5ca0
   6948109f23f350db82123535eb8a7433 bdabcb909271a6ecbcb58b936a88cd4e
   8f2e6ff5800175f113253d8fa9ca8885 c2f552e657dc603f252e1a8e308f76f0
   be79e2fb8f5d5fbbe2e30ecadd220723 c8c0aea8078cdfcb3868263ff8f09400
   54da48781893a7e49ad5aff4af300cd8 04a6b6279ab3ff3afb64491c85194aab
   760d58a606654f9f4400e8b38591356f bf6425aca26dc85244259ff2b19c41b9
   f96f3ca9ec1dde434da7d2d392b905dd f3d1f9af93d1af5950bd493f5aa731b4
   056df31bd267b6b90a079831aaf579be 0a39013137aac6d404f518cfd4684064
   7e78bfe706ca4cf5e9c5453e9f7cfd2b 8b4c8d169a44e55c88d4a9a7f9474241
   e221af44860018ab0856972e194cd934";

    // RFC 9369 A.2; Copyright (c) 2023 IETF Trust and the document authors.
    // Simplified BSD terms: quic-vectors.LICENSE.txt.
    const RFC9369_CLIENT_INITIAL: &str =
        "d76b3343cf088394c8f03e5157080000 449ea0c95e82ffe67b6abcdb4298b485
   dd04de806071bf03dceebfa162e75d6c 96058bdbfb127cdfcbf903388e99ad04
   9f9a3dd4425ae4d0992cfff18ecf0fdb 5a842d09747052f17ac2053d21f57c5d
   250f2c4f0e0202b70785b7946e992e58 a59ac52dea6774d4f03b55545243cf1a
   12834e3f249a78d395e0d18f4d766004 f1a2674802a747eaa901c3f10cda5500
   cb9122faa9f1df66c392079a1b40f0de 1c6054196a11cbea40afb6ef5253cd68
   18f6625efce3b6def6ba7e4b37a40f77 32e093daa7d52190935b8da58976ff33
   12ae50b187c1433c0f028edcc4c2838b 6a9bfc226ca4b4530e7a4ccee1bfa2a3
   d396ae5a3fb512384b2fdd851f784a65 e03f2c4fbe11a53c7777c023462239dd
   6f7521a3f6c7d5dd3ec9b3f233773d4b 46d23cc375eb198c63301c21801f6520
   bcfb7966fc49b393f0061d974a2706df 8c4a9449f11d7f3d2dcbb90c6b877045
   636e7c0c0fe4eb0f697545460c806910 d2c355f1d253bc9d2452aaa549e27a1f
   ac7cf4ed77f322e8fa894b6a83810a34 b361901751a6f5eb65a0326e07de7c12
   16ccce2d0193f958bb3850a833f7ae43 2b65bc5a53975c155aa4bcb4f7b2c4e5
   4df16efaf6ddea94e2c50b4cd1dfe060 17e0e9d02900cffe1935e0491d77ffb4
   fdf85290fdd893d577b1131a610ef6a5 c32b2ee0293617a37cbb08b847741c3b
   8017c25ca9052ca1079d8b78aebd4787 6d330a30f6a8c6d61dd1ab5589329de7
   14d19d61370f8149748c72f132f0fc99 f34d766c6938597040d8f9e2bb522ff9
   9c63a344d6a2ae8aa8e51b7b90a4a806 105fcbca31506c446151adfeceb51b91
   abfe43960977c87471cf9ad4074d30e1 0d6a7f03c63bd5d4317f68ff325ba3bd
   80bf4dc8b52a0ba031758022eb025cdd 770b44d6d6cf0670f4e990b22347a7db
   848265e3e5eb72dfe8299ad7481a4083 22cac55786e52f633b2fb6b614eaed18
   d703dd84045a274ae8bfa73379661388 d6991fe39b0d93debb41700b41f90a15
   c4d526250235ddcd6776fc77bc97e7a4 17ebcb31600d01e57f32162a8560cacc
   7e27a096d37a1a86952ec71bd89a3e9a 30a2a26162984d7740f81193e8238e61
   f6b5b984d4d3dfa033c1bb7e4f0037fe bf406d91c0dccf32acf423cfa1e70710
   10d3f270121b493ce85054ef58bada42 310138fe081adb04e2bd901f2f13458b
   3d6758158197107c14ebb193230cd115 7380aa79cae1374a7c1e5bbcb80ee23e
   06ebfde206bfb0fcbc0edc4ebec30966 1bdd908d532eb0c6adc38b7ca7331dce
   8dfce39ab71e7c32d318d136b6100671 a1ae6a6600e3899f31f0eed19e3417d1
   34b90c9058f8632c798d4490da498730 7cba922d61c39805d072b589bd52fdf1
   e86215c2d54e6670e07383a27bbffb5a ddf47d66aa85a0c6f9f32e59d85a44dd
   5d3b22dc2be80919b490437ae4f36a0a e55edf1d0b5cb4e9a3ecabee93dfc6e3
   8d209d0fa6536d27a5d6fbb17641cde2 7525d61093f1b28072d111b2b4ae5f89
   d5974ee12e5cf7d5da4d6a31123041f3 3e61407e76cffcdcfd7e19ba58cf4b53
   6f4c4938ae79324dc402894b44faf8af bab35282ab659d13c93f70412e85cb19
   9a37ddec600545473cfb5a05e08d0b20 9973b2172b4d21fb69745a262ccde96b
   a18b2faa745b6fe189cf772a9f84cbfc";
}
