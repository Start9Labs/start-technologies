use std::cmp::Ordering;
use std::future::Future;
use std::path::{Path, PathBuf};

use clap::Parser;
use imbl_value::{from_value, to_value};
use itertools::Itertools;
use openssl::asn1::{Asn1Time, Asn1TimeRef};
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::x509::{X509, X509NameRef};
use rpc_toolkit::HandlerArgs;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use x509_parser::parse_x509_certificate;
use x509_parser::x509::X509Version;

use crate::context::{CliContext, RpcContext};
use crate::net::ssl::x509_sha256_fingerprint;
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::{delete_file_durable, maybe_open_file, open_file, write_file_atomic_durable};
use crate::util::serde::{WithIoFormat, display_serializable};

const MAX_CERTIFICATE_SIZE: usize = crate::CAP_1_MiB;
const LIVE_CA_DIRECTORY: &str = "/usr/local/share/ca-certificates/startos-custom";
const PERSISTENT_CA_DIRECTORY: &str =
    "/media/startos/config/overlay/usr/local/share/ca-certificates/startos-custom";
const PEM_BEGIN: &str = "-----BEGIN CERTIFICATE-----";
const PEM_END: &str = "-----END CERTIFICATE-----";

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub(crate) struct TrustCaCliParams {
    #[arg(help = "help.arg.ca-certificate-path")]
    certificate: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrustCaRpcParams {
    pem: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrustedCa {
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

pub(crate) async fn cli(
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

pub(crate) async fn install(
    context: RpcContext,
    TrustCaRpcParams { pem }: TrustCaRpcParams,
) -> Result<TrustedCa, Error> {
    let parsed = tokio::task::spawn_blocking(move || parse_ca(&pem))
        .await
        .map_err(|error| Error::new(error, ErrorKind::Unknown))??;
    let Some(guard) = context.admit_trust_ca_install().await else {
        return Err(Error::new(
            eyre!(t!("context.rpc.rpc-context-shutdown")),
            ErrorKind::InvalidRequest,
        ));
    };
    run_detached_transaction(async move {
        let _guard = guard;
        install_transaction(
            &context,
            parsed,
            Path::new(LIVE_CA_DIRECTORY),
            Path::new(PERSISTENT_CA_DIRECTORY),
        )
        .await
    })
    .await
}

async fn install_transaction(
    context: &RpcContext,
    parsed: ParsedCa,
    live: &Path,
    persistent: &Path,
) -> Result<TrustedCa, Error> {
    let filename = format!("{}.crt", parsed.fingerprint_id);
    let snapshot = TrustStoreSnapshot {
        live: FileSnapshot::capture(live.join(&filename)).await?,
        persistent: FileSnapshot::capture(persistent.join(&filename)).await?,
    };
    if snapshot.matches(&parsed.canonical_pem) {
        return Ok(parsed.result);
    }
    run_install_stages(
        || write_file_atomic_durable(&snapshot.live.path, &parsed.canonical_pem),
        update_trust_store,
        || async { context.reload_http_client() },
        || write_file_atomic_durable(&snapshot.persistent.path, &parsed.canonical_pem),
        |error| rollback(context, &snapshot, error),
    )
    .await?;
    Ok(parsed.result)
}

async fn run_install_stages(
    write_live: impl AsyncFnOnce() -> Result<(), Error>,
    refresh: impl AsyncFnOnce() -> Result<(), Error>,
    reload: impl AsyncFnOnce() -> Result<(), Error>,
    write_persistent: impl AsyncFnOnce() -> Result<(), Error>,
    rollback: impl AsyncFnOnce(Error) -> Error,
) -> Result<(), Error> {
    if let Err(error) = write_live().await {
        return Err(rollback(error).await);
    }
    if let Err(error) = refresh().await {
        return Err(rollback(error).await);
    }
    if let Err(error) = reload().await {
        return Err(rollback(error).await);
    }
    if let Err(error) = write_persistent().await {
        return Err(rollback(error).await);
    }
    Ok(())
}

async fn rollback(context: &RpcContext, snapshot: &TrustStoreSnapshot, error: Error) -> Error {
    let rollback_error = snapshot.restore().await.err();
    let refresh_error = update_trust_store().await.err();
    let reload_error = context.reload_http_client().err();
    installation_error(error, rollback_error, refresh_error, reload_error)
}

async fn run_detached_transaction<T, F>(transaction: F) -> Result<T, Error>
where
    T: Send + 'static,
    F: Future<Output = Result<T, Error>> + Send + 'static,
{
    tokio::spawn(transaction)
        .await
        .map_err(|error| Error::new(error, ErrorKind::Unknown))?
}

pub(crate) fn display(
    params: WithIoFormat<TrustCaCliParams>,
    result: TrustedCa,
) -> Result<(), Error> {
    if let Some(format) = params.format {
        return display_serializable(format, result);
    }
    println!(
        "{}: {}",
        t!("system.trust-ca.subject"),
        human_readable_subject(&result.subject)
    );
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
    let (_, parsed) =
        parse_x509_certificate(&der).map_err(|error| invalid_certificate(error.to_string()))?;
    let now = Asn1Time::days_from_now(0).map_err(invalid_certificate)?;
    ensure_code!(
        is_currently_valid(&certificate, &now)?,
        ErrorKind::InvalidRequest,
        "{}",
        t!("system.trust-ca.not-currently-valid")
    );
    let basic_constraints = parsed
        .basic_constraints()
        .map_err(|error| invalid_certificate(error.to_string()))?;
    ensure_code!(
        basic_constraints.map_or(parsed.version() == X509Version::V1, |extension| {
            extension.value.ca
        }),
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
    let names_match = certificate
        .issuer_name()
        .try_cmp(certificate.subject_name())
        .map_err(invalid_certificate)?
        == Ordering::Equal;
    let public_key = certificate.public_key().map_err(invalid_certificate)?;
    let verifies_itself = certificate
        .verify(&public_key)
        .map_err(invalid_certificate)?;
    ensure_code!(
        names_match && verifies_itself,
        ErrorKind::InvalidRequest,
        "{}",
        t!("system.trust-ca.not-self-signed-root")
    );

    let fingerprint_id = hex::encode(
        certificate
            .digest(MessageDigest::sha256())
            .map_err(invalid_certificate)?,
    );
    let fingerprint = x509_sha256_fingerprint(&certificate).map_err(invalid_certificate)?;

    Ok(ParsedCa {
        canonical_pem: certificate.to_pem().map_err(invalid_certificate)?,
        fingerprint_id,
        result: TrustedCa {
            subject: render_subject(certificate.subject_name()),
            fingerprint,
        },
    })
}

fn is_currently_valid(certificate: &X509, now: &Asn1TimeRef) -> Result<bool, Error> {
    Ok(certificate
        .not_before()
        .compare(now)
        .map_err(invalid_certificate)?
        != Ordering::Greater
        && certificate
            .not_after()
            .compare(now)
            .map_err(invalid_certificate)?
            != Ordering::Less)
}

async fn update_trust_store() -> Result<(), Error> {
    Command::new("update-ca-certificates")
        .invoke(ErrorKind::OpenSsl)
        .await?;
    Ok(())
}

fn render_subject(subject: &X509NameRef) -> String {
    subject
        .entries()
        .map(|entry| {
            let object = entry.object();
            let nid = object.nid();
            let name = if nid == Nid::UNDEF {
                object.to_string()
            } else {
                nid.short_name()
                    .map(str::to_owned)
                    .unwrap_or_else(|_| object.to_string())
            };
            let value = entry
                .data()
                .as_utf8()
                .map(|value| escape_subject_value(&value))
                .unwrap_or_else(|_| format!("#{}", hex::encode_upper(entry.data().as_slice())));
            format!("{name}={value}")
        })
        .join(", ")
}

fn escape_subject_value(value: &str) -> String {
    let last = value.chars().count().saturating_sub(1);
    value
        .chars()
        .enumerate()
        .fold(String::new(), |mut output, (index, character)| {
            if is_bidi_formatting_control(character) {
                output.push_str(&format!("\\u{{{:x}}}", character as u32));
            } else if character.is_control() {
                let mut encoded = [0; 4];
                for byte in character.encode_utf8(&mut encoded).as_bytes() {
                    output.push_str(&format!("\\{byte:02X}"));
                }
            } else if matches!(character, '\\' | ',' | '=' | '+' | '"' | '<' | '>' | ';')
                || (index == 0 && character == '#')
                || ((index == 0 || index == last) && character == ' ')
            {
                output.push('\\');
                output.push(character);
            } else {
                output.push(character);
            }
            output
        })
}

fn is_bidi_formatting_control(character: char) -> bool {
    matches!(
        character,
        '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
}

fn human_readable_subject(subject: &str) -> String {
    subject
        .chars()
        .fold(String::new(), |mut output, character| {
            if character.is_control() {
                output.extend(character.escape_default());
            } else {
                output.push(character);
            }
            output
        })
}

impl FileSnapshot {
    async fn capture(path: PathBuf) -> Result<Self, Error> {
        let contents = if let Some(mut file) = maybe_open_file(&path).await? {
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).await?;
            Some(contents)
        } else {
            None
        };
        Ok(Self { path, contents })
    }

    async fn restore(&self) -> Result<(), Error> {
        if let Some(contents) = &self.contents {
            write_file_atomic_durable(&self.path, contents).await
        } else {
            delete_file_durable(&self.path).await
        }
    }
}

impl TrustStoreSnapshot {
    fn matches(&self, contents: &[u8]) -> bool {
        self.live.contents.as_deref() == Some(contents)
            && self.persistent.contents.as_deref() == Some(contents)
    }

    async fn restore(&self) -> Result<(), Error> {
        let mut errors = ErrorCollection::new();
        errors.handle(self.persistent.restore().await);
        errors.handle(self.live.restore().await);
        errors.into_result()
    }
}

fn installation_error(
    error: Error,
    rollback_error: Option<Error>,
    refresh_error: Option<Error>,
    reload_error: Option<Error>,
) -> Error {
    let mut failures = Vec::new();
    if let Some(error) = rollback_error {
        failures.push(t!("system.trust-ca.certificate-rollback-failed", error = error).to_string());
    }
    if let Some(error) = refresh_error {
        failures.push(
            t!(
                "system.trust-ca.trust-store-refresh-after-rollback-failed",
                error = error
            )
            .to_string(),
        );
    }
    if let Some(error) = reload_error {
        failures.push(
            t!(
                "system.trust-ca.http-client-reload-after-rollback-failed",
                error = error
            )
            .to_string(),
        );
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
    use std::sync::{Arc, Mutex as StdMutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use openssl::asn1::{Asn1Time, Asn1Type};
    use openssl::bn::BigNum;
    use openssl::pkey::{PKey, Private};
    use openssl::x509::extension::{BasicConstraints, KeyUsage};
    use openssl::x509::{X509Builder, X509NameBuilder};

    use super::*;
    use crate::net::ssl::{CertBranding, SANInfo, gen_nistp256, make_root_cert, make_self_signed};
    use crate::util::io::TmpDir;
    use crate::util::io::write_file_atomic;

    fn root_ca_pem() -> Vec<u8> {
        let key = gen_nistp256().unwrap();
        make_root_cert(&key, &CertBranding::start_os("test"), SystemTime::now())
            .unwrap()
            .to_pem()
            .unwrap()
    }

    fn root_ca_with_validity_at(now: i64, not_before: i64, not_after: i64) -> X509 {
        let key = gen_nistp256().unwrap();
        ca_certificate(
            &key,
            &key,
            "dated CA",
            "dated CA",
            now + not_before,
            now + not_after,
        )
    }

    fn root_ca_pem_with_validity(not_before: i64, not_after: i64) -> Vec<u8> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        root_ca_with_validity_at(now, not_before, not_after)
            .to_pem()
            .unwrap()
    }

    fn ca_certificate(
        key: &PKey<Private>,
        signer: &PKey<Private>,
        subject: &str,
        issuer: &str,
        not_before: i64,
        not_after: i64,
    ) -> X509 {
        let mut builder = X509Builder::new().unwrap();
        builder.set_version(2).unwrap();
        let serial = BigNum::from_u32(1).unwrap().to_asn1_integer().unwrap();
        builder.set_serial_number(&serial).unwrap();
        builder
            .set_not_before(&Asn1Time::from_unix(not_before).unwrap())
            .unwrap();
        builder
            .set_not_after(&Asn1Time::from_unix(not_after).unwrap())
            .unwrap();
        let mut subject_name = X509NameBuilder::new().unwrap();
        subject_name.append_entry_by_text("CN", subject).unwrap();
        builder.set_subject_name(&subject_name.build()).unwrap();
        let mut issuer_name = X509NameBuilder::new().unwrap();
        issuer_name.append_entry_by_text("CN", issuer).unwrap();
        builder.set_issuer_name(&issuer_name.build()).unwrap();
        builder.set_pubkey(key).unwrap();
        builder
            .append_extension(BasicConstraints::new().critical().ca().build().unwrap())
            .unwrap();
        builder
            .append_extension(KeyUsage::new().critical().key_cert_sign().build().unwrap())
            .unwrap();
        builder.sign(signer, MessageDigest::sha256()).unwrap();
        builder.build()
    }

    fn extensionless_ca_pem(version: i32) -> Vec<u8> {
        let key = gen_nistp256().unwrap();
        let mut builder = X509Builder::new().unwrap();
        builder.set_version(version).unwrap();
        let serial = BigNum::from_u32(1).unwrap().to_asn1_integer().unwrap();
        builder.set_serial_number(&serial).unwrap();
        builder
            .set_not_before(&Asn1Time::days_from_now(0).unwrap())
            .unwrap();
        builder
            .set_not_after(&Asn1Time::days_from_now(1).unwrap())
            .unwrap();
        let mut name = X509NameBuilder::new().unwrap();
        name.append_entry_by_text("CN", "extensionless CA").unwrap();
        let name = name.build();
        builder.set_subject_name(&name).unwrap();
        builder.set_issuer_name(&name).unwrap();
        builder.set_pubkey(&key).unwrap();
        builder.sign(&key, MessageDigest::sha256()).unwrap();
        builder.build().to_pem().unwrap()
    }

    fn legacy_subject_ca_pem() -> Vec<u8> {
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
        name.append_entry_by_text_with_type("CN", "Legacy CA", Asn1Type::T61STRING)
            .unwrap();
        name.append_entry_by_text("1.2.3.4", "custom").unwrap();
        let name = name.build();
        builder.set_subject_name(&name).unwrap();
        builder.set_issuer_name(&name).unwrap();
        builder.set_pubkey(&key).unwrap();
        builder
            .append_extension(BasicConstraints::new().critical().ca().build().unwrap())
            .unwrap();
        builder
            .append_extension(KeyUsage::new().critical().key_cert_sign().build().unwrap())
            .unwrap();
        builder.sign(&key, MessageDigest::sha256()).unwrap();
        builder.build().to_pem().unwrap()
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
    fn renders_legacy_subject_encoding_and_unknown_oid() {
        let parsed = parse_ca(&String::from_utf8(legacy_subject_ca_pem()).unwrap()).unwrap();

        assert_eq!(parsed.result.subject, "CN=Legacy CA, 1.2.3.4=custom");
    }

    #[test]
    fn escapes_controls_in_human_readable_subject() {
        assert_eq!(
            human_readable_subject("CN=普通\n\u{1b}[31mCA\u{7f}"),
            "CN=普通\\n\\u{1b}[31mCA\\u{7f}"
        );
    }

    #[test]
    fn subject_escaping_prevents_distinguished_name_collisions() {
        assert_eq!(
            escape_subject_value(" #a,b=c+d\\e\"f<g>h;i\n "),
            "\\ #a\\,b\\=c\\+d\\\\e\\\"f\\<g\\>h\\;i\\0A\\ "
        );
        assert_eq!(escape_subject_value("#root"), "\\#root");
        assert_ne!(
            format!("CN={}", escape_subject_value("a, OU=b")),
            "CN=a, OU=b"
        );
    }

    #[test]
    fn subject_escaping_neutralizes_bidi_formatting_controls() {
        let escaped = escape_subject_value("safe\u{202e}txt\u{2066}end");

        assert_eq!(escaped, "safe\\u{202e}txt\\u{2066}end");
        assert!(!escaped.chars().any(is_bidi_formatting_control));
        assert_ne!(
            escaped,
            escape_subject_value("safe\\u{202e}txt\\u{2066}end")
        );
    }

    #[test]
    fn accepts_each_validity_boundary() {
        let now = 1_700_000_000;
        let current = Asn1Time::from_unix(now).unwrap();

        assert!(is_currently_valid(&root_ca_with_validity_at(now, 0, 60), &current).unwrap());
        assert!(is_currently_valid(&root_ca_with_validity_at(now, -60, 0), &current).unwrap());
    }

    #[test]
    fn rejects_each_invalid_validity_boundary() {
        let now = 1_700_000_000;
        let current = Asn1Time::from_unix(now).unwrap();

        assert!(!is_currently_valid(&root_ca_with_validity_at(now, 1, 60), &current).unwrap());
        assert!(!is_currently_valid(&root_ca_with_validity_at(now, -60, -1), &current).unwrap());
    }

    #[test]
    fn rejects_ca_outside_validity_window() {
        for pem in [
            root_ca_pem_with_validity(-172_800, -86_400),
            root_ca_pem_with_validity(86_400, 172_800),
        ] {
            let error = parse_ca(&String::from_utf8(pem).unwrap()).unwrap_err();
            assert_eq!(error.kind, ErrorKind::InvalidRequest);
            assert!(error.to_string().contains("expired or not yet valid"));
        }
    }

    #[test]
    fn accepts_extensionless_v1_ca() {
        let pem = String::from_utf8(extensionless_ca_pem(0)).unwrap();

        parse_ca(&pem).unwrap();
    }

    #[test]
    fn rejects_extensionless_v3_ca() {
        let pem = String::from_utf8(extensionless_ca_pem(2)).unwrap();

        assert_eq!(parse_ca(&pem).unwrap_err().kind, ErrorKind::InvalidRequest);
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
    fn rejects_ca_intermediate_signed_by_another_key() {
        let issuer_key = gen_nistp256().unwrap();
        let intermediate_key = gen_nistp256().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let pem = ca_certificate(
            &intermediate_key,
            &issuer_key,
            "intermediate CA",
            "root CA",
            now - 60,
            now + 60,
        )
        .to_pem()
        .unwrap();
        let error = parse_ca(&String::from_utf8(pem).unwrap()).unwrap_err();

        assert_eq!(error.kind, ErrorKind::InvalidRequest);
        assert!(error.to_string().contains("self-signed root CA"));
    }

    #[test]
    fn rejects_matching_names_with_signature_from_another_key() {
        let subject_key = gen_nistp256().unwrap();
        let signer_key = gen_nistp256().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let pem = ca_certificate(
            &subject_key,
            &signer_key,
            "forged root CA",
            "forged root CA",
            now - 60,
            now + 60,
        )
        .to_pem()
        .unwrap();

        assert_eq!(
            parse_ca(&String::from_utf8(pem).unwrap()).unwrap_err().kind,
            ErrorKind::InvalidRequest
        );
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

    #[test]
    fn identical_snapshots_match_canonical_certificate() {
        let canonical = b"canonical".to_vec();
        let snapshot = TrustStoreSnapshot {
            live: FileSnapshot {
                path: PathBuf::from("live"),
                contents: Some(canonical.clone()),
            },
            persistent: FileSnapshot {
                path: PathBuf::from("persistent"),
                contents: Some(canonical.clone()),
            },
        };

        assert!(snapshot.matches(&canonical));
        assert!(!snapshot.matches(b"different"));
    }

    #[tokio::test]
    async fn persistent_ca_is_published_after_live_refresh_and_reload() {
        let stages = Arc::new(StdMutex::new(Vec::new()));
        run_install_stages(
            record_stage(&stages, "write-live", Ok(())),
            record_stage(&stages, "refresh", Ok(())),
            record_stage(&stages, "reload", Ok(())),
            record_stage(&stages, "write-persistent", Ok(())),
            |error| async move { error },
        )
        .await
        .unwrap();

        assert_eq!(
            *stages.lock().unwrap(),
            ["write-live", "refresh", "reload", "write-persistent"]
        );
    }

    #[tokio::test]
    async fn persistent_write_failure_rolls_back_live_state() {
        let stages = Arc::new(StdMutex::new(Vec::new()));
        let rollback_stages = stages.clone();
        let error = Error::new(eyre!("persistent write failed"), ErrorKind::Filesystem);
        let result = run_install_stages(
            record_stage(&stages, "write-live", Ok(())),
            record_stage(&stages, "refresh", Ok(())),
            record_stage(&stages, "reload", Ok(())),
            record_stage(&stages, "write-persistent", Err(error)),
            move |error| async move {
                rollback_stages.lock().unwrap().push("rollback");
                error
            },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(
            *stages.lock().unwrap(),
            [
                "write-live",
                "refresh",
                "reload",
                "write-persistent",
                "rollback"
            ]
        );
    }

    #[tokio::test]
    async fn snapshot_rollback_restores_live_and_persistent_roots() {
        let tmp = TmpDir::new().await.unwrap();
        let live = tmp.join("live.crt");
        let persistent = tmp.join("persistent.crt");
        write_file_atomic(&live, b"old-live").await.unwrap();
        write_file_atomic(&persistent, b"old-persistent")
            .await
            .unwrap();
        let snapshot = TrustStoreSnapshot {
            live: FileSnapshot::capture(live.clone()).await.unwrap(),
            persistent: FileSnapshot::capture(persistent.clone()).await.unwrap(),
        };
        write_file_atomic(&live, b"new-live").await.unwrap();
        write_file_atomic(&persistent, b"new-persistent")
            .await
            .unwrap();

        snapshot.restore().await.unwrap();

        assert_eq!(tokio::fs::read(live).await.unwrap(), b"old-live");
        assert_eq!(
            tokio::fs::read(persistent).await.unwrap(),
            b"old-persistent"
        );
        tmp.delete().await.unwrap();
    }

    #[tokio::test]
    async fn detached_transaction_finishes_after_caller_is_dropped() {
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
        let caller = tokio::spawn(run_detached_transaction(async move {
            started_tx.send(()).unwrap();
            release_rx.await.unwrap();
            finished_tx.send(()).unwrap();
            Ok(())
        }));
        started_rx.await.unwrap();

        caller.abort();
        release_tx.send(()).unwrap();

        tokio::time::timeout(Duration::from_secs(1), finished_rx)
            .await
            .expect("transaction did not finish")
            .unwrap();
    }

    #[tokio::test]
    async fn retains_both_snapshot_restore_errors() {
        let tmp = TmpDir::new().await.unwrap();
        let live = tmp.join("live");
        let persistent = tmp.join("persistent");
        tokio::fs::create_dir_all(&live).await.unwrap();
        tokio::fs::create_dir_all(&persistent).await.unwrap();
        let snapshot = TrustStoreSnapshot {
            live: FileSnapshot {
                path: live,
                contents: Some(Vec::new()),
            },
            persistent: FileSnapshot {
                path: persistent,
                contents: Some(Vec::new()),
            },
        };

        let error = snapshot.restore().await.unwrap_err().to_string();
        assert!(error.contains("live"), "{error}");
        assert!(error.contains("persistent"), "{error}");
        tmp.delete().await.unwrap();
    }

    fn record_stage(
        stages: &Arc<StdMutex<Vec<&'static str>>>,
        stage: &'static str,
        result: Result<(), Error>,
    ) -> impl AsyncFnOnce() -> Result<(), Error> {
        let stages = stages.clone();
        async move || {
            stages.lock().unwrap().push(stage);
            result
        }
    }
}
