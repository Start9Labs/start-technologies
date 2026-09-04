use std::path::{Path, PathBuf};

use clap::Parser;
use imbl_value::{from_value, to_value};
use itertools::Itertools;
use openssl::hash::MessageDigest;
use openssl::x509::X509;
use rpc_toolkit::HandlerArgs;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::Mutex;
use x509_parser::parse_x509_certificate;

use crate::context::{CliContext, RpcContext};
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::{open_file, write_file_atomic};
use crate::util::serde::{WithIoFormat, display_serializable};

const MAX_CERTIFICATE_SIZE: usize = crate::CAP_1_MiB;
const LIVE_CA_DIRECTORY: &str = "/usr/local/share/ca-certificates/startos-custom";
const PERSISTENT_CA_DIRECTORY: &str =
    "/media/startos/config/overlay/usr/local/share/ca-certificates/startos-custom";
const PEM_BEGIN: &str = "-----BEGIN CERTIFICATE-----";
const PEM_END: &str = "-----END CERTIFICATE-----";

static TRUST_STORE_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct TrustCaCliParams {
    #[arg(help = "help.arg.ca-certificate-path")]
    certificate: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustCaRpcParams {
    pem: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedCa {
    subject: String,
    fingerprint: String,
}

#[derive(Debug)]
struct ParsedCa {
    canonical_pem: Vec<u8>,
    fingerprint_id: String,
    result: TrustedCa,
}

#[derive(Debug)]
struct FileSnapshot {
    path: PathBuf,
    contents: Option<Vec<u8>>,
}

#[derive(Debug)]
struct TrustStoreSnapshot {
    live: FileSnapshot,
    persistent: FileSnapshot,
}

pub async fn cli(
    HandlerArgs {
        context,
        parent_method,
        method,
        params,
        ..
    }: HandlerArgs<CliContext, TrustCaCliParams>,
) -> Result<TrustedCa, Error> {
    let pem = if params.certificate == Path::new("-") {
        read_limited(tokio::io::stdin()).await?
    } else {
        read_limited(open_file(&params.certificate).await?).await?
    };
    let pem = String::from_utf8(pem).map_err(invalid_certificate)?;

    Ok(from_value(
        context
            .call_remote::<RpcContext>(
                &parent_method.into_iter().chain(method).join("."),
                to_value(&TrustCaRpcParams { pem })?,
            )
            .await?,
    )?)
}

pub async fn install(
    _: RpcContext,
    TrustCaRpcParams { pem }: TrustCaRpcParams,
) -> Result<TrustedCa, Error> {
    let parsed = parse_ca(&pem)?;
    let _guard = TRUST_STORE_LOCK.lock().await;

    let snapshot = store_ca(
        &parsed,
        Path::new(LIVE_CA_DIRECTORY),
        Path::new(PERSISTENT_CA_DIRECTORY),
    )
    .await?;
    if let Err(error) = update_trust_store().await {
        let rollback_error = snapshot.restore().await.err();
        let refresh_error = update_trust_store().await.err();
        return Err(installation_error(error, rollback_error, refresh_error));
    }

    Ok(parsed.result)
}

pub fn display(params: WithIoFormat<TrustCaCliParams>, result: TrustedCa) -> Result<(), Error> {
    if let Some(format) = params.format {
        return display_serializable(format, result);
    }
    println!("{}: {}", t!("system.trust-ca.subject"), result.subject);
    println!(
        "{}: {}",
        t!("system.trust-ca.fingerprint"),
        result.fingerprint
    );
    Ok(())
}

async fn read_limited(reader: impl AsyncRead + Unpin) -> Result<Vec<u8>, Error> {
    let mut contents = Vec::new();
    reader
        .take((MAX_CERTIFICATE_SIZE + 1) as u64)
        .read_to_end(&mut contents)
        .await?;
    ensure_code!(
        contents.len() <= MAX_CERTIFICATE_SIZE,
        ErrorKind::InvalidRequest,
        "{}",
        t!("system.trust-ca.input-too-large")
    );
    Ok(contents)
}

fn parse_ca(pem: &str) -> Result<ParsedCa, Error> {
    ensure_code!(
        pem.len() <= MAX_CERTIFICATE_SIZE,
        ErrorKind::InvalidRequest,
        "{}",
        t!("system.trust-ca.input-too-large")
    );
    let trimmed = pem.trim();
    ensure_code!(
        trimmed.starts_with(PEM_BEGIN)
            && trimmed.ends_with(PEM_END)
            && trimmed.matches(PEM_BEGIN).count() == 1
            && trimmed.matches(PEM_END).count() == 1,
        ErrorKind::InvalidRequest,
        "{}",
        t!("system.trust-ca.invalid-certificate")
    );

    let certificate = X509::from_pem(trimmed.as_bytes()).map_err(invalid_certificate)?;
    let der = certificate.to_der().map_err(invalid_certificate)?;
    let (remainder, parsed) =
        parse_x509_certificate(&der).map_err(|error| invalid_certificate(error.to_string()))?;
    ensure_code!(
        remainder.is_empty(),
        ErrorKind::InvalidRequest,
        "{}",
        t!("system.trust-ca.invalid-certificate")
    );
    let basic_constraints = parsed
        .basic_constraints()
        .map_err(|error| invalid_certificate(error.to_string()))?;
    ensure_code!(
        basic_constraints.is_some_and(|extension| extension.value.ca),
        ErrorKind::InvalidRequest,
        "{}",
        t!("system.trust-ca.not-ca")
    );
    let key_usage = parsed
        .key_usage()
        .map_err(|error| invalid_certificate(error.to_string()))?;
    ensure_code!(
        key_usage.is_none_or(|extension| extension.value.key_cert_sign()),
        ErrorKind::InvalidRequest,
        "{}",
        t!("system.trust-ca.not-ca")
    );

    let fingerprint = certificate
        .digest(MessageDigest::sha256())
        .map_err(invalid_certificate)?;
    let fingerprint_id = hex::encode(fingerprint.as_ref());
    let fingerprint = fingerprint
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":");

    Ok(ParsedCa {
        canonical_pem: certificate.to_pem().map_err(invalid_certificate)?,
        fingerprint_id,
        result: TrustedCa {
            subject: parsed.subject().to_string(),
            fingerprint,
        },
    })
}

async fn store_ca(
    parsed: &ParsedCa,
    live: &Path,
    persistent: &Path,
) -> Result<TrustStoreSnapshot, Error> {
    let filename = format!("{}.crt", parsed.fingerprint_id);
    let snapshot = TrustStoreSnapshot {
        live: FileSnapshot::capture(live.join(&filename)).await?,
        persistent: FileSnapshot::capture(persistent.join(&filename)).await?,
    };
    write_file_atomic(&snapshot.persistent.path, &parsed.canonical_pem).await?;
    if let Err(error) = write_file_atomic(&snapshot.live.path, &parsed.canonical_pem).await {
        return Err(installation_error(
            error,
            snapshot.persistent.restore().await.err(),
            None,
        ));
    }
    Ok(snapshot)
}

async fn update_trust_store() -> Result<(), Error> {
    Command::new("update-ca-certificates")
        .invoke(ErrorKind::OpenSsl)
        .await?;
    Ok(())
}

impl FileSnapshot {
    async fn capture(path: PathBuf) -> Result<Self, Error> {
        let contents = match tokio::fs::read(&path).await {
            Ok(contents) => Some(contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(Error::new(error, ErrorKind::Filesystem)),
        };
        Ok(Self { path, contents })
    }

    async fn restore(self) -> Result<(), Error> {
        if let Some(contents) = self.contents {
            write_file_atomic(self.path, contents).await
        } else {
            match tokio::fs::remove_file(&self.path).await {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(Error::new(error, ErrorKind::Filesystem)),
            }
        }
    }
}

impl TrustStoreSnapshot {
    async fn restore(self) -> Result<(), Error> {
        let live = self.live.restore().await;
        let persistent = self.persistent.restore().await;
        live?;
        persistent
    }
}

fn installation_error(
    error: Error,
    rollback_error: Option<Error>,
    refresh_error: Option<Error>,
) -> Error {
    let mut failures = Vec::new();
    if let Some(error) = rollback_error {
        failures.push(format!("certificate rollback failed: {error}"));
    }
    if let Some(error) = refresh_error {
        failures.push(format!(
            "trust-store refresh after rollback failed: {error}"
        ));
    }
    if failures.is_empty() {
        return error;
    }
    let kind = error.kind;
    Error::new(error.source.wrap_err(failures.join("; ")), kind)
}

fn invalid_certificate(error: impl std::fmt::Display) -> Error {
    Error::new(
        eyre!("{}: {error}", t!("system.trust-ca.invalid-certificate")),
        ErrorKind::InvalidRequest,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::time::SystemTime;

    use openssl::asn1::Asn1Time;
    use openssl::bn::BigNum;
    use openssl::x509::extension::{BasicConstraints, KeyUsage};
    use openssl::x509::{X509Builder, X509NameBuilder};

    use super::*;
    use crate::net::ssl::{CertBranding, SANInfo, gen_nistp256, make_root_cert, make_self_signed};
    use crate::util::io::TmpDir;

    fn root_ca_pem() -> Vec<u8> {
        let key = gen_nistp256().unwrap();
        make_root_cert(&key, &CertBranding::start_os("test"), SystemTime::now())
            .unwrap()
            .to_pem()
            .unwrap()
    }

    fn ca_without_key_cert_sign_pem() -> Vec<u8> {
        let key = gen_nistp256().unwrap();
        let mut builder = X509Builder::new().unwrap();
        builder.set_version(2).unwrap();
        let serial = BigNum::from_u32(1).unwrap().to_asn1_integer().unwrap();
        builder.set_serial_number(&serial).unwrap();
        builder
            .set_not_before(&Asn1Time::days_from_now(0).unwrap())
            .unwrap();
        builder
            .set_not_after(&Asn1Time::days_from_now(1).unwrap())
            .unwrap();
        let mut name = X509NameBuilder::new().unwrap();
        name.append_entry_by_text("CN", "invalid CA").unwrap();
        let name = name.build();
        builder.set_subject_name(&name).unwrap();
        builder.set_issuer_name(&name).unwrap();
        builder.set_pubkey(&key).unwrap();
        builder
            .append_extension(BasicConstraints::new().critical().ca().build().unwrap())
            .unwrap();
        builder
            .append_extension(
                KeyUsage::new()
                    .critical()
                    .digital_signature()
                    .build()
                    .unwrap(),
            )
            .unwrap();
        builder.sign(&key, MessageDigest::sha256()).unwrap();
        builder.build().to_pem().unwrap()
    }

    #[test]
    fn parses_ca_and_reports_stable_identity() {
        let canonical_pem = root_ca_pem();
        let pem = format!("\n{}\n", String::from_utf8(canonical_pem.clone()).unwrap());
        let first = parse_ca(&pem).unwrap();
        let second = parse_ca(&pem).unwrap();

        assert!(first.result.subject.contains("CN=test Local Root CA"));
        assert_eq!(first.result, second.result);
        assert_eq!(first.fingerprint_id.len(), 64);
        assert_eq!(first.result.fingerprint.len(), 95);
        assert_eq!(first.canonical_pem, canonical_pem);
    }

    #[test]
    fn rejects_non_ca_certificate() {
        let key = gen_nistp256().unwrap();
        let names = BTreeSet::from([InternedString::intern("leaf.local")]);
        let cert = make_self_signed(
            (&key, &SANInfo::new(&names)),
            &CertBranding::start_os("test"),
        )
        .unwrap();
        let pem = String::from_utf8(cert.to_pem().unwrap()).unwrap();

        assert_eq!(parse_ca(&pem).unwrap_err().kind, ErrorKind::InvalidRequest);
    }

    #[test]
    fn rejects_ca_without_certificate_signing_usage() {
        let pem = String::from_utf8(ca_without_key_cert_sign_pem()).unwrap();

        assert_eq!(parse_ca(&pem).unwrap_err().kind, ErrorKind::InvalidRequest);
    }

    #[test]
    fn rejects_malformed_and_multiple_certificates() {
        assert_eq!(
            parse_ca("not a certificate").unwrap_err().kind,
            ErrorKind::InvalidRequest
        );

        let pem = String::from_utf8(root_ca_pem()).unwrap();
        assert_eq!(
            parse_ca(&format!("{pem}{pem}")).unwrap_err().kind,
            ErrorKind::InvalidRequest
        );
        assert_eq!(
            parse_ca(&format!("{pem}trailing data")).unwrap_err().kind,
            ErrorKind::InvalidRequest
        );
    }

    #[tokio::test]
    async fn rejects_oversized_certificate_input() {
        let input = vec![0; MAX_CERTIFICATE_SIZE + 1];
        assert_eq!(
            read_limited(input.as_slice()).await.unwrap_err().kind,
            ErrorKind::InvalidRequest
        );
        assert_eq!(
            parse_ca(&String::from_utf8(input).unwrap())
                .unwrap_err()
                .kind,
            ErrorKind::InvalidRequest
        );
    }

    #[tokio::test]
    async fn stores_ca_idempotently_without_replacing_other_roots() {
        let tmp = TmpDir::new().await.unwrap();
        let live = tmp.join("live");
        let persistent = tmp.join("persistent");
        write_file_atomic(live.join("startos-root-ca.crt"), b"generated")
            .await
            .unwrap();
        write_file_atomic(persistent.join("distribution.crt"), b"distribution")
            .await
            .unwrap();
        let parsed = parse_ca(&String::from_utf8(root_ca_pem()).unwrap()).unwrap();

        let initial_snapshot = store_ca(&parsed, &live, &persistent).await.unwrap();
        store_ca(&parsed, &live, &persistent).await.unwrap();

        assert_eq!(
            tokio::fs::read(live.join("startos-root-ca.crt"))
                .await
                .unwrap(),
            b"generated"
        );
        assert_eq!(
            tokio::fs::read(persistent.join("distribution.crt"))
                .await
                .unwrap(),
            b"distribution"
        );
        assert_eq!(
            tokio::fs::read(live.join(format!("{}.crt", parsed.fingerprint_id)))
                .await
                .unwrap(),
            parsed.canonical_pem
        );
        assert_eq!(entry_count(&live).await, 2);
        assert_eq!(entry_count(&persistent).await, 2);

        initial_snapshot.restore().await.unwrap();
        assert_eq!(entry_count(&live).await, 1);
        assert_eq!(entry_count(&persistent).await, 1);
        tmp.delete().await.unwrap();
    }

    async fn entry_count(path: &Path) -> usize {
        let mut entries = tokio::fs::read_dir(path).await.unwrap();
        let mut count = 0;
        while entries.next_entry().await.unwrap().is_some() {
            count += 1;
        }
        count
    }
}
