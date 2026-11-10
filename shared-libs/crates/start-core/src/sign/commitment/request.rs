use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::Request;
use digest::Update;
use futures::TryStreamExt;
use http::HeaderValue;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWrite;
use tokio_util::io::StreamReader;
use url::Url;

use crate::prelude::*;
use crate::s9pk::merkle_archive::hash::VerifyingWriter;
use crate::sign::commitment::{Commitment, Digestable, SignedCommitment};
use crate::sign::{AnySignature, AnyVerifyingKey, Signer};
use crate::util::serde::{Base64, DerEncoding};

pub mod binary;

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, PartialEq, Eq, VisitFields)]
#[serde(rename_all = "camelCase", tag = "type", rename = "request")]
#[model = "Model<Self>"]
#[visit(ts(rename = "RequestCommitment"))]
pub struct RequestCommitment {
    pub timestamp: i64,
    pub nonce: u64,
    pub size: u64,
    pub blake3: Base64<[u8; 32]>,
}

rpc_toolkit::reflect_ts!(RequestCommitment);
rpc_toolkit::ts_export!(RequestCommitment, namespaces = [""]);
impl RequestCommitment {
    /// Commits the complete byte slice with a fresh timestamp and nonce.
    pub fn for_bytes(body: &[u8]) -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or_else(|e| e.duration().as_secs() as i64 * -1),
            nonce: rand::random(),
            size: body.len() as u64,
            blake3: Base64(*blake3::hash(body).as_bytes()),
        }
    }

    /// Checks all bytes, including any bytes beyond the declared size.
    pub async fn verify_bytes(&self, body: &[u8]) -> Result<(), Error> {
        use tokio::io::AsyncWriteExt;
        let mut writer = VerifyingWriter::new(
            tokio::io::sink(),
            Some((blake3::Hash::from_bytes(*self.blake3), self.size)),
        );
        writer
            .write_all(body)
            .await
            .with_kind(ErrorKind::InvalidSignature)?;
        writer.verify().await?;
        Ok(())
    }

    pub fn append_query(&self, url: &mut Url) {
        use base64::Engine;
        url.query_pairs_mut()
            .append_pair("commitment", Self::TYPE_NAME)
            .append_pair("timestamp", &self.timestamp.to_string())
            .append_pair("nonce", &self.nonce.to_string())
            .append_pair("size", &self.size.to_string())
            .append_pair(
                "blake3",
                // unpadded base64url, like every binary param in the header
                &base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&*self.blake3),
            );
    }
    pub fn from_query(query: &HeaderValue) -> Result<Self, Error> {
        use base64::Engine;
        let query: BTreeMap<_, _> = form_urlencoded::parse(query.as_bytes()).collect();
        if let Some(ty) = query.get("commitment") {
            if ty != Self::TYPE_NAME {
                return Err(Error::new(
                    eyre!(
                        "{}",
                        t!("sign.commitment.request.unsupported-type", ty = ty)
                    ),
                    ErrorKind::InvalidSignature,
                ));
            }
        }
        Ok(Self {
            timestamp: query.get("timestamp").or_not_found("timestamp")?.parse()?,
            nonce: query.get("nonce").or_not_found("nonce")?.parse()?,
            size: query.get("size").or_not_found("size")?.parse()?,
            blake3: Base64(
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(&**query.get("blake3").or_not_found("blake3")?)
                    .with_kind(ErrorKind::Deserialization)?
                    .try_into()
                    .map_err(|_| {
                        Error::new(eyre!("blake3 hash must be 32 bytes"), ErrorKind::ParseUrl)
                    })?,
            ),
        })
    }
}
/// A signed request commitment; the receiver separately enforces nonce freshness.
#[derive(Clone)]
pub struct RequestProof {
    pub commitment: RequestCommitment,
    pub signer: AnyVerifyingKey,
    pub signature: AnySignature,
}

impl RequestProof {
    /// Emits pure Ed25519 through the configured provider without exporting private keys.
    pub async fn sign_commitment(
        signer: &Signer,
        body: &[u8],
        context: &str,
    ) -> Result<Self, Error> {
        let commitment = RequestCommitment::for_bytes(body);
        let signature = signer
            .sign_message(&crate::sign::signing_message(
                crate::sign::COMMITMENT_SIG_TAG,
                &commitment,
                context,
            )?)
            .await?;
        Ok(Self {
            commitment,
            signer: signer.verifying_key().await?,
            signature,
        })
    }

    /// Verifies identity and captured bytes, independently of replay or authorization policy.
    pub async fn verify_commitment(
        &self,
        body: &[u8],
        contexts: &[InternedString],
    ) -> Result<InternedString, Error> {
        let context = contexts
            .iter()
            .find(|context| {
                self.signer
                    .verify_commitment(&self.commitment, context, &self.signature)
                    .is_ok()
            })
            .ok_or_else(|| {
                Error::new(
                    eyre!("{}", t!("middleware.auth.invalid-request-signature")),
                    ErrorKind::InvalidSignature,
                )
            })?;
        self.commitment.verify_bytes(body).await?;
        Ok(context.clone())
    }

    /// Encodes full-width commitment fields and algorithm-tagged DER identities/signatures.
    pub fn to_header(&self) -> HeaderValue {
        let mut url: Url = "http://localhost".parse().unwrap();
        self.commitment.append_query(&mut url);
        url.query_pairs_mut().append_pair(
            "signer",
            &encode_header_der(&self.signer).expect("encode verifying key"),
        );
        url.query_pairs_mut().append_pair(
            "signature",
            &encode_header_der(&self.signature).expect("encode signature"),
        );
        HeaderValue::from_str(url.query().unwrap_or_default()).unwrap()
    }

    /// Decodes the common request-proof wire fields without selecting a signing scheme.
    pub fn from_header(header: &HeaderValue) -> Result<Self, Error> {
        let query: BTreeMap<_, _> = form_urlencoded::parse(header.as_bytes()).collect();
        Ok(Self {
            commitment: RequestCommitment::from_query(header)?,
            signer: decode_header_der(query.get("signer").or_not_found("signer")?)?,
            signature: decode_header_der(query.get("signature").or_not_found("signature")?)?,
        })
    }
}

fn encode_header_der(value: &impl DerEncoding) -> Result<String, Error> {
    use base64::Engine;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        value
            .to_der::<serde_json::Error>()
            .with_kind(ErrorKind::Serialization)?,
    ))
}

fn decode_header_der<T: DerEncoding>(value: &str) -> Result<T, Error> {
    use base64::Engine;
    let der = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .with_kind(ErrorKind::Deserialization)?;
    T::from_der::<serde_json::Error>(&der).with_kind(ErrorKind::Deserialization)
}

impl SignedCommitment for RequestCommitment {
    const TYPE_ID: u8 = 3;
    const TYPE_NAME: &'static str = "request";
}
impl Digestable for RequestCommitment {
    fn update<D: Update>(&self, digest: &mut D) {
        digest.update(&i64::to_be_bytes(self.timestamp));
        digest.update(&u64::to_be_bytes(self.nonce));
        digest.update(&u64::to_be_bytes(self.size));
        digest.update(&*self.blake3);
    }
}
impl<'a> Commitment<&'a mut Request> for RequestCommitment {
    async fn create(resource: &'a mut Request) -> Result<Self, Error> {
        use http_body_util::BodyExt;

        let body = std::mem::replace(resource.body_mut(), Body::empty())
            .collect()
            .await
            .with_kind(ErrorKind::Network)?
            .to_bytes();
        let res = Self::for_bytes(&body);
        *resource.body_mut() = Body::from(body);
        Ok(res)
    }
    async fn copy_to<W: AsyncWrite + Unpin + Send>(
        &self,
        resource: &'a mut Request,
        writer: W,
    ) -> Result<(), Error> {
        use tokio::io::AsyncReadExt;

        let mut body = StreamReader::new(
            std::mem::replace(resource.body_mut(), Body::empty())
                .into_data_stream()
                .map_err(std::io::Error::other),
        )
        .take(self.size);

        let mut writer = VerifyingWriter::new(
            writer,
            Some((blake3::Hash::from_bytes(*self.blake3), self.size)),
        );
        tokio::io::copy(&mut body, &mut writer).await?;
        writer.verify().await?;

        Ok(())
    }
}
