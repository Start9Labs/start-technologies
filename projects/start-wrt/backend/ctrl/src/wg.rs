//! WireGuard keys and live interface state.

use base64::Engine;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::prelude::*;

/// WireGuard private key wrapper with native key generation.
#[derive(Clone)]
pub struct WgKey(StaticSecret);

impl WgKey {
    /// Generate a new random WireGuard private key.
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        Self(StaticSecret::from(bytes))
    }

    /// Derive the public key from this private key.
    pub fn public_key(&self) -> WgPublicKey {
        WgPublicKey(PublicKey::from(&self.0))
    }
}

impl AsRef<[u8]> for WgKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl TryFrom<Vec<u8>> for WgKey {
    type Error = Error;
    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let bytes: [u8; 32] = value.try_into().map_err(|_| {
            Error::new(
                eyre!("invalid key length: expected 32 bytes"),
                ErrorKind::InvalidValue,
            )
        })?;
        Ok(Self(bytes.into()))
    }
}

impl TryFrom<&str> for WgKey {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(value)
            .map_err(|e| Error::new(eyre!("invalid base64: {}", e), ErrorKind::Deserialization))?;
        Self::try_from(bytes)
    }
}

/// WireGuard public key wrapper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgPublicKey(PublicKey);

impl WgPublicKey {
    /// Get the raw bytes of the public key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl AsRef<[u8]> for WgPublicKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl TryFrom<Vec<u8>> for WgPublicKey {
    type Error = Error;
    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let bytes: [u8; 32] = value.try_into().map_err(|_| {
            Error::new(
                eyre!("invalid public key length: expected 32 bytes"),
                ErrorKind::InvalidValue,
            )
        })?;
        Ok(Self(PublicKey::from(bytes)))
    }
}

/// Base64-encoded wrapper for WireGuard keys and other binary data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Base64<T>(pub T);

impl<T> Base64<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: AsRef<[u8]>> Base64<T> {
    /// Encode to base64 string with padding (standard WireGuard format).
    pub fn to_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.0.as_ref())
    }
}

impl<T: AsRef<[u8]>> std::fmt::Display for Base64<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_base64())
    }
}

impl<T: TryFrom<Vec<u8>>> std::str::FromStr for Base64<T>
where
    T::Error: std::fmt::Display,
{
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(|e| Error::new(eyre!("invalid base64: {}", e), ErrorKind::Deserialization))?;
        let value =
            T::try_from(bytes).map_err(|e| Error::new(eyre!("{}", e), ErrorKind::InvalidValue))?;
        Ok(Self(value))
    }
}

impl<T: AsRef<[u8]>> Serialize for Base64<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_base64())
    }
}

impl<'de, T> Deserialize<'de> for Base64<T>
where
    Base64<T>: std::str::FromStr,
    <Base64<T> as std::str::FromStr>::Err: std::fmt::Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl<T> std::ops::Deref for Base64<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Base64<WgKey> {
    /// Derive the public key from this private key.
    pub fn public_key(&self) -> Base64<WgPublicKey> {
        Base64(self.0.public_key())
    }
}

/// Pre-shared key (32 bytes of random data).
pub type Psk = [u8; 32];

/// Generate a random pre-shared key.
pub fn generate_psk() -> Psk {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

/// Seconds after its latest handshake that a WireGuard session stops carrying data.
pub const REJECT_AFTER_SECS: u64 = 180;

/// Wire size of a handshake initiation; a session-less peer sends nothing else.
pub const HANDSHAKE_INITIATION_BYTES: u64 = 148;

/// A peer line of `wg show <interface> dump`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgPeerStats {
    pub public_key: String,
    pub endpoint: Option<String>,
    /// Unix seconds; 0 when no handshake has completed.
    pub latest_handshake: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// Peers of a `wg show <interface> dump`, skipping its interface line.
pub fn parse_dump(output: &str) -> Vec<WgPeerStats> {
    output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 8 {
                return None;
            }
            Some(WgPeerStats {
                public_key: fields[0].to_string(),
                endpoint: Some(fields[2])
                    .filter(|e| *e != "(none)")
                    .map(str::to_string),
                latest_handshake: fields[4].parse().unwrap_or(0),
                rx_bytes: fields[5].parse().unwrap_or(0),
                tx_bytes: fields[6].parse().unwrap_or(0),
            })
        })
        .collect()
}

/// `None` when the interface does not exist.
pub async fn show_dump(interface: &str) -> Option<Vec<WgPeerStats>> {
    use crate::invoke::Invoke;

    let out = tokio::process::Command::new("wg")
        .args(["show", interface, "dump"])
        .invoke(ErrorKind::Network.into())
        .await
        .ok()?;
    Some(parse_dump(&String::from_utf8_lossy(&out)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dump_reads_peers() {
        let dump = "priv\tpub\t60616\toff\n\
            peerA\t(none)\t192.0.2.1:51820\t0.0.0.0/0\t1700000000\t92\t1480\t25\n\
            peerB\t(none)\t(none)\t10.0.0.0/8\t0\t0\t0\toff\n";
        assert_eq!(
            parse_dump(dump),
            vec![
                WgPeerStats {
                    public_key: "peerA".into(),
                    endpoint: Some("192.0.2.1:51820".into()),
                    latest_handshake: 1_700_000_000,
                    rx_bytes: 92,
                    tx_bytes: 1480,
                },
                WgPeerStats {
                    public_key: "peerB".into(),
                    endpoint: None,
                    latest_handshake: 0,
                    rx_bytes: 0,
                    tx_bytes: 0,
                },
            ]
        );
    }

    #[test]
    fn parse_dump_of_peerless_interface_is_empty() {
        assert!(parse_dump("(none)\t(none)\t0\toff\n").is_empty());
    }

    #[test]
    fn test_key_generation() {
        let key = WgKey::generate();
        let public = key.public_key();

        // Keys should be 32 bytes
        assert_eq!(key.as_ref().len(), 32);
        assert_eq!(public.as_ref().len(), 32);
    }

    #[test]
    fn test_base64_roundtrip() {
        let key = WgKey::generate();
        let base64_key = Base64(key.clone());
        let encoded = base64_key.to_base64();

        // Should be valid base64 (44 chars for 32 bytes with padding)
        assert_eq!(encoded.len(), 44);

        // Should roundtrip
        let decoded: Base64<WgKey> = encoded.parse().unwrap();
        assert_eq!(decoded.as_ref(), key.as_ref());
    }

    #[test]
    fn test_psk_generation() {
        let psk = generate_psk();
        assert_eq!(psk.len(), 32);
    }
}
