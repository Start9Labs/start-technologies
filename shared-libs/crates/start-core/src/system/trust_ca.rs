use std::cmp::Ordering;
use std::path::Path;

use clap::Parser;
use itertools::Itertools;
use openssl::nid::Nid;
use openssl::x509::{X509, X509NameRef};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::Mutex;
use x509_parser::parse_x509_certificate;
use x509_parser::x509::X509Version;

use crate::context::RpcContext;
use crate::net::ssl::x509_sha256_fingerprint;
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::write_file_atomic;
use crate::util::serde::{Pem, WithIoFormat, display_serializable};

const LIVE_CA_DIRECTORY: &str = "/usr/local/share/ca-certificates/startos-custom";
const PERSISTENT_CA_DIRECTORY: &str =
    "/media/startos/config/overlay/usr/local/share/ca-certificates/startos-custom";
static INSTALL_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub(crate) struct TrustCaParams {
    #[arg(long, help = "help.arg.ca-certificate")]
    cert: Pem<X509>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrustedCa {
    subject: String,
    fingerprint: String,
}

#[derive(Debug)]
struct ParsedCa {
    pem: Vec<u8>,
    result: TrustedCa,
}

pub(crate) async fn install(
    ctx: RpcContext,
    TrustCaParams { cert }: TrustCaParams,
) -> Result<TrustedCa, Error> {
    let ca = validate_ca(&cert)?;
    tokio::spawn(async move {
        let _guard = INSTALL_LOCK.lock().await;
        let filename = format!(
            "{}.crt",
            ca.result.fingerprint.replace(':', "").to_lowercase()
        );
        write_file_atomic(Path::new(PERSISTENT_CA_DIRECTORY).join(&filename), &ca.pem).await?;
        write_file_atomic(Path::new(LIVE_CA_DIRECTORY).join(&filename), &ca.pem).await?;
        update_trust_store().await?;
        ctx.client.reload()?;
        Ok(ca.result)
    })
    .await
    .with_kind(ErrorKind::Unknown)?
}

pub(crate) fn display(params: WithIoFormat<TrustCaParams>, result: TrustedCa) -> Result<(), Error> {
    if let Some(format) = params.format {
        return display_serializable(format, result);
    }
    println!(
        "{}: {}",
        t!("system.trust-ca.subject"),
        escape_controls(&result.subject)
    );
    println!(
        "{}: {}",
        t!("system.trust-ca.fingerprint"),
        result.fingerprint
    );
    Ok(())
}

fn escape_controls(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() {
                c.escape_default().to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
}

pub(crate) async fn update_trust_store() -> Result<(), Error> {
    Command::new("update-ca-certificates")
        .invoke(ErrorKind::OpenSsl)
        .await?;
    Ok(())
}

fn validate_ca(certificate: &X509) -> Result<ParsedCa, Error> {
    let der = certificate.to_der().map_err(invalid_certificate)?;
    let (_, parsed) = parse_x509_certificate(&der).map_err(invalid_certificate)?;
    let is_ca = match parsed.basic_constraints().map_err(invalid_certificate)? {
        Some(constraints) => constraints.value.ca,
        None => parsed.version() == X509Version::V1,
    };
    let signs_certificates = parsed
        .key_usage()
        .map_err(invalid_certificate)?
        .is_none_or(|usage| usage.value.key_cert_sign());
    ensure_code!(
        is_ca && signs_certificates,
        ErrorKind::InvalidRequest,
        "{}",
        t!("system.trust-ca.not-ca")
    );

    let self_issued = certificate
        .issuer_name()
        .try_cmp(certificate.subject_name())
        .map_err(invalid_certificate)?
        == Ordering::Equal;
    let public_key = certificate.public_key().map_err(invalid_certificate)?;
    let self_signed = certificate
        .verify(&public_key)
        .map_err(invalid_certificate)?;
    ensure_code!(
        self_issued && self_signed,
        ErrorKind::InvalidRequest,
        "{}",
        t!("system.trust-ca.not-self-signed-root")
    );

    Ok(ParsedCa {
        pem: certificate.to_pem().map_err(invalid_certificate)?,
        result: TrustedCa {
            subject: render_subject(certificate.subject_name()),
            fingerprint: x509_sha256_fingerprint(&certificate).map_err(invalid_certificate)?,
        },
    })
}

fn render_subject(subject: &X509NameRef) -> String {
    subject
        .entries()
        .map(|entry| {
            let object = entry.object();
            let name = match object.nid() {
                Nid::UNDEF => object.to_string(),
                nid => nid
                    .short_name()
                    .map_or_else(|_| object.to_string(), str::to_owned),
            };
            let value = entry.data().as_utf8().map_or_else(
                |_| format!("#{}", hex::encode_upper(entry.data().as_slice())),
                |value| value.to_string(),
            );
            format!("{name}={value}")
        })
        .join(", ")
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

    use openssl::asn1::{Asn1Time, Asn1Type};
    use openssl::bn::BigNum;
    use openssl::hash::MessageDigest;
    use openssl::pkey::{PKey, Private};
    use openssl::x509::extension::{BasicConstraints, KeyUsage};
    use openssl::x509::{X509Builder, X509Name, X509NameBuilder};

    use super::*;
    use crate::net::ssl::{CertBranding, SANInfo, gen_nistp256, make_root_cert, make_self_signed};

    fn parse_ca(pem: &str) -> Result<ParsedCa, Error> {
        let cert = pem.parse::<Pem<X509>>().map_err(invalid_certificate)?;
        validate_ca(&cert)
    }

    fn name(entries: &[(&str, &str, Asn1Type)]) -> X509Name {
        let mut name = X509NameBuilder::new().unwrap();
        for (field, value, ty) in entries {
            name.append_entry_by_text_with_type(field, value, *ty)
                .unwrap();
        }
        name.build()
    }

    fn certificate(
        version: i32,
        key: &PKey<Private>,
        signer: &PKey<Private>,
        subject: &X509Name,
        issuer: &X509Name,
        key_usage: Option<KeyUsage>,
        ca: bool,
    ) -> String {
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
        builder.set_subject_name(subject).unwrap();
        builder.set_issuer_name(issuer).unwrap();
        builder.set_pubkey(key).unwrap();
        if ca {
            builder
                .append_extension(BasicConstraints::new().critical().ca().build().unwrap())
                .unwrap();
        }
        if let Some(mut key_usage) = key_usage {
            builder
                .append_extension(key_usage.critical().build().unwrap())
                .unwrap();
        }
        builder.sign(signer, MessageDigest::sha256()).unwrap();
        String::from_utf8(builder.build().to_pem().unwrap()).unwrap()
    }

    fn root(version: i32, ca: bool, key_usage: Option<KeyUsage>) -> String {
        let key = gen_nistp256().unwrap();
        let name = name(&[("CN", "test CA", Asn1Type::UTF8STRING)]);
        certificate(version, &key, &key, &name, &name, key_usage, ca)
    }

    fn key_cert_sign() -> Option<KeyUsage> {
        let mut usage = KeyUsage::new();
        usage.key_cert_sign();
        Some(usage)
    }

    #[test]
    fn accepts_root_ca_with_stable_identity() {
        let key = gen_nistp256().unwrap();
        let pem = make_root_cert(&key, &CertBranding::start_os("test"), SystemTime::now())
            .unwrap()
            .to_pem()
            .unwrap();
        let input = format!("\n{}\n", String::from_utf8(pem.clone()).unwrap());
        let first = parse_ca(&input).unwrap();

        assert!(first.result.subject.contains("CN=test Local Root CA"));
        assert_eq!(first.result, parse_ca(&input).unwrap().result);
        assert_eq!(first.result.fingerprint.len(), 95);
        assert_eq!(first.pem, pem);
    }

    #[test]
    fn accepts_extensionless_v1_root() {
        parse_ca(&root(0, false, None)).unwrap();
    }

    #[test]
    fn renders_legacy_string_types_and_unknown_oids() {
        let key = gen_nistp256().unwrap();
        let name = name(&[
            ("CN", "Legacy CA", Asn1Type::T61STRING),
            ("1.2.3.4", "custom", Asn1Type::UTF8STRING),
        ]);
        let pem = certificate(2, &key, &key, &name, &name, key_cert_sign(), true);

        assert_eq!(
            parse_ca(&pem).unwrap().result.subject,
            "CN=Legacy CA, 1.2.3.4=custom"
        );
    }

    #[test]
    fn rejects_non_ca_certificates() {
        let key = gen_nistp256().unwrap();
        let names = BTreeSet::from([InternedString::intern("leaf.local")]);
        let leaf = make_self_signed(
            (&key, &SANInfo::new(&names)),
            &CertBranding::start_os("test"),
        )
        .unwrap()
        .to_pem()
        .unwrap();
        let mut digital_signature = KeyUsage::new();
        digital_signature.digital_signature();

        for pem in [
            String::from_utf8(leaf).unwrap(),
            root(2, false, None),
            root(2, true, Some(digital_signature)),
        ] {
            assert_eq!(parse_ca(&pem).unwrap_err().kind, ErrorKind::InvalidRequest);
        }
    }

    #[test]
    fn rejects_certificates_not_signed_by_their_own_key() {
        let key = gen_nistp256().unwrap();
        let signer = gen_nistp256().unwrap();
        let root_name = name(&[("CN", "root CA", Asn1Type::UTF8STRING)]);
        let intermediate_name = name(&[("CN", "intermediate CA", Asn1Type::UTF8STRING)]);

        for (subject, issuer) in [(&intermediate_name, &root_name), (&root_name, &root_name)] {
            let pem = certificate(2, &key, &signer, subject, issuer, key_cert_sign(), true);
            let error = parse_ca(&pem).unwrap_err();
            assert_eq!(error.kind, ErrorKind::InvalidRequest);
            assert!(error.to_string().contains("self-signed root CA"));
        }
    }

    #[test]
    fn validates_the_first_certificate_in_a_bundle() {
        let pem = root(2, true, key_cert_sign());
        let first = parse_ca(&pem).unwrap();
        assert_eq!(
            parse_ca(&format!("{pem}{pem}")).unwrap().result,
            first.result
        );
        assert_eq!(
            parse_ca(&format!("{pem}trailing data")).unwrap().result,
            first.result
        );

        let key = gen_nistp256().unwrap();
        let names = BTreeSet::from([InternedString::intern("leaf.local")]);
        let leaf = String::from_utf8(
            make_self_signed(
                (&key, &SANInfo::new(&names)),
                &CertBranding::start_os("test"),
            )
            .unwrap()
            .to_pem()
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            parse_ca(&format!("{leaf}{pem}")).unwrap_err().kind,
            ErrorKind::InvalidRequest
        );
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(
            parse_ca("not a certificate").unwrap_err().kind,
            ErrorKind::InvalidRequest
        );
    }

    #[test]
    fn cli_cert_round_trips_to_rpc() {
        let pem = root(2, true, key_cert_sign());
        let params =
            TrustCaParams::try_parse_from(["trust-ca", &format!("--cert={}", pem.trim_end())])
                .unwrap();
        let remote: TrustCaParams =
            imbl_value::from_value(imbl_value::to_value(&params).unwrap()).unwrap();
        assert_eq!(
            validate_ca(&params.cert).unwrap().result,
            validate_ca(&remote.cert).unwrap().result
        );
    }
}
