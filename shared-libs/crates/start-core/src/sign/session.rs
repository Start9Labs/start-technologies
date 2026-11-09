#![doc = include_str!("../../signed-session.md")]

use std::future::Future;

use hkdf::Hkdf;
use openssl::symm::{Cipher, decrypt_aead, encrypt_aead};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

use super::commitment::request::{RequestCommitment, RequestProof};
use super::{AnyVerifyingKey, Signer};
use crate::prelude::*;
use crate::util::serde::Base64;

const PROTOCOL: &str = "Start-RPC-Session";
pub(crate) const VERSION: u8 = 1;
const CIPHER_KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
pub(crate) const MAX_HANDSHAKE_BYTES: usize = 1024;
pub(crate) const TAG_BYTES: usize = 16;
const MAX_RECORDS_PER_KEY: u64 = 1 << 32;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    payload: Base64<Vec<u8>>,
    proof: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Message {
    protocol: String,
    version: u8,
    hello: Hello,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "camelCase", deny_unknown_fields)]
enum Hello {
    Client {
        ephemeral: Base64<[u8; 32]>,
    },
    Server {
        ephemeral: Base64<[u8; 32]>,
        #[serde(rename = "clientHello")]
        client_hello: Base64<[u8; 32]>,
    },
}

fn invalid() -> Error {
    Error::new(
        eyre!("{}", t!("middleware.auth.invalid-request-signature")),
        ErrorKind::InvalidSignature,
    )
}

fn ephemeral() -> StaticSecret {
    StaticSecret::from(rand::random::<[u8; 32]>())
}

async fn encode(signer: &Signer, context: &str, hello: Hello) -> Result<Vec<u8>, Error> {
    let payload = serde_json::to_vec(&Message {
        protocol: PROTOCOL.into(),
        version: VERSION,
        hello,
    })
    .with_kind(ErrorKind::Serialization)?;
    let proof = RequestProof::sign_commitment(signer, &payload, context).await?;
    let wire = serde_json::to_vec(&Envelope {
        payload: Base64(payload),
        proof: proof
            .to_header()
            .to_str()
            .with_kind(ErrorKind::Serialization)?
            .into(),
    })
    .with_kind(ErrorKind::Serialization)?;
    if wire.len() > MAX_HANDSHAKE_BYTES {
        return Err(invalid());
    }
    Ok(wire)
}

async fn decode(wire: &[u8], context: &str) -> Result<(Hello, RequestProof), Error> {
    if wire.len() > MAX_HANDSHAKE_BYTES {
        return Err(invalid());
    }
    let envelope: Envelope = serde_json::from_slice(wire).map_err(|_| invalid())?;
    let proof = RequestProof::from_header(
        &http::HeaderValue::from_str(&envelope.proof).map_err(|_| invalid())?,
    )?;
    proof.verify_commitment(&envelope.payload, context).await?;
    let message: Message = serde_json::from_slice(&envelope.payload).map_err(|_| invalid())?;
    if message.protocol != PROTOCOL || message.version != VERSION {
        return Err(invalid());
    }
    Ok((message.hello, proof))
}

fn transcript(client: &[u8], server: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    for message in [client, server] {
        digest.update((message.len() as u32).to_be_bytes());
        digest.update(message);
    }
    digest.finalize().into()
}

/// A single connection's signed initiation, prepared before network deadlines.
pub struct PreparedClient {
    secret: StaticSecret,
    context: String,
    message: Vec<u8>,
}

impl PreparedClient {
    /// Uses a fresh DH key and request commitment for every connection.
    pub async fn new(signer: &Signer, context: &str) -> Result<Self, Error> {
        let secret = ephemeral();
        let message = encode(
            signer,
            context,
            Hello::Client {
                ephemeral: Base64(PublicKey::from(&secret).to_bytes()),
            },
        )
        .await?;
        Ok(Self {
            secret,
            context: context.into(),
            message,
        })
    }

    /// The signed client envelope, without the transport's length prefix.
    pub fn message(&self) -> &[u8] {
        &self.message
    }

    /// Authenticates the server response bound to this exact initiation.
    pub async fn finish(self, response: &[u8]) -> Result<(AnyVerifyingKey, SessionCipher), Error> {
        let (
            Hello::Server {
                ephemeral,
                client_hello,
            },
            proof,
        ) = decode(response, &self.context).await?
        else {
            return Err(invalid());
        };
        if *client_hello != <[u8; 32]>::from(Sha256::digest(&self.message)) {
            return Err(invalid());
        }
        let shared = self.secret.diffie_hellman(&PublicKey::from(*ephemeral));
        if !shared.was_contributory() {
            return Err(invalid());
        }
        let cipher = SessionCipher::derive(
            shared.as_bytes(),
            &transcript(&self.message, response),
            Side::Client,
        )?;
        Ok((proof.signer, cipher))
    }
}

/// Authenticates a client and applies the caller's replay policy before responding.
pub async fn accept<F, Fut>(
    signer: &Signer,
    context: &str,
    request: &[u8],
    expected_peer: Option<&AnyVerifyingKey>,
    accept_nonce: F,
) -> Result<(AnyVerifyingKey, Vec<u8>, SessionCipher), Error>
where
    F: FnOnce(RequestCommitment) -> Fut,
    Fut: Future<Output = Result<(), Error>>,
{
    let (Hello::Client { ephemeral: remote }, proof) = decode(request, context).await? else {
        return Err(invalid());
    };
    if expected_peer.is_some_and(|expected| expected != &proof.signer) {
        return Err(invalid());
    }
    let secret = ephemeral();
    let shared = secret.diffie_hellman(&PublicKey::from(*remote));
    if !shared.was_contributory() {
        return Err(invalid());
    }
    accept_nonce(proof.commitment).await?;
    let response = encode(
        signer,
        context,
        Hello::Server {
            ephemeral: Base64(PublicKey::from(&secret).to_bytes()),
            client_hello: Base64(Sha256::digest(request).into()),
        },
    )
    .await?;
    let cipher = SessionCipher::derive(
        shared.as_bytes(),
        &transcript(request, &response),
        Side::Server,
    )?;
    Ok((proof.signer, response, cipher))
}

fn protocol_label() -> Vec<u8> {
    format!("{PROTOCOL} v{VERSION}\0").into_bytes()
}

#[derive(Clone, Copy)]
enum Side {
    Client,
    Server,
}

impl Side {
    fn code(self) -> u8 {
        match self {
            Self::Client => 0,
            Self::Server => 1,
        }
    }

    fn key_info(self) -> Vec<u8> {
        let direction = match self {
            Self::Client => b"client-to-server",
            Self::Server => b"server-to-client",
        };
        [protocol_label().as_slice(), direction].concat()
    }
}

struct RecordKey {
    key: Zeroizing<[u8; CIPHER_KEY_BYTES]>,
    aad: Vec<u8>,
    counter: u64,
}

impl RecordKey {
    fn nonce(&self) -> Result<[u8; NONCE_BYTES], Error> {
        if self.counter >= MAX_RECORDS_PER_KEY {
            return Err(invalid());
        }
        let mut nonce = [0; NONCE_BYTES];
        nonce[NONCE_BYTES - std::mem::size_of::<u64>()..]
            .copy_from_slice(&self.counter.to_be_bytes());
        Ok(nonce)
    }
}

/// Direction-separated AES-GCM records; framing and connection lifetime belong to the adapter.
pub struct SessionCipher {
    send: RecordKey,
    receive: RecordKey,
}

impl SessionCipher {
    fn derive(shared: &[u8; 32], transcript: &[u8; 32], side: Side) -> Result<Self, Error> {
        let hkdf = Hkdf::<Sha256>::new(Some(transcript), shared);
        let key = |direction: Side| -> Result<RecordKey, Error> {
            let mut key = Zeroizing::new([0; CIPHER_KEY_BYTES]);
            hkdf.expand(&direction.key_info(), &mut *key)
                .map_err(|_| invalid())?;
            Ok(RecordKey {
                key,
                aad: [protocol_label(), vec![direction.code()]].concat(),
                counter: 0,
            })
        };
        let client = key(Side::Client)?;
        let server = key(Side::Server)?;
        let (send, receive) = match side {
            Side::Client => (client, server),
            Side::Server => (server, client),
        };
        Ok(Self { send, receive })
    }

    /// Writes ciphertext and its tag; the caller supplies the record framing.
    pub fn write_message(&mut self, plaintext: &[u8], output: &mut [u8]) -> Result<usize, Error> {
        if output.len() < plaintext.len() + TAG_BYTES {
            return Err(invalid());
        }
        let mut tag = [0; TAG_BYTES];
        let ciphertext = encrypt_aead(
            Cipher::aes_256_gcm(),
            &*self.send.key,
            Some(&self.send.nonce()?),
            &self.send.aad,
            plaintext,
            &mut tag,
        )
        .map_err(|_| invalid())?;
        output[..ciphertext.len()].copy_from_slice(&ciphertext);
        output[ciphertext.len()..ciphertext.len() + TAG_BYTES].copy_from_slice(&tag);
        self.send.counter += 1;
        Ok(ciphertext.len() + TAG_BYTES)
    }

    /// Authentication failure requires the caller to terminate the channel.
    pub fn read_message(&mut self, ciphertext: &[u8], output: &mut [u8]) -> Result<usize, Error> {
        if ciphertext.len() < TAG_BYTES || output.len() < ciphertext.len() - TAG_BYTES {
            return Err(invalid());
        }
        let (data, tag) = ciphertext.split_at(ciphertext.len() - TAG_BYTES);
        let plaintext = decrypt_aead(
            Cipher::aes_256_gcm(),
            &*self.receive.key,
            Some(&self.receive.nonce()?),
            &self.receive.aad,
            data,
            tag,
        )
        .map_err(|_| invalid())?;
        output[..plaintext.len()].copy_from_slice(&plaintext);
        self.receive.counter += 1;
        Ok(plaintext.len())
    }
}

pub(crate) fn wire_profile() -> serde_json::Value {
    serde_json::json!({
        "protocol": PROTOCOL, "version": VERSION,
        "dh": "X25519", "kdf": "HKDF", "hash": "SHA-256", "cipher": "AES-GCM",
        "clientKeyInfo": Side::Client.key_info(), "serverKeyInfo": Side::Server.key_info(),
        "aadPrefix": protocol_label(), "tagBytes": TAG_BYTES, "maxRecordsPerKey": MAX_RECORDS_PER_KEY,
        "keyBytes": CIPHER_KEY_BYTES, "nonceBytes": NONCE_BYTES,
    })
}

#[cfg(test)]
mod tests;
