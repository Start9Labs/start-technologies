use std::cmp::{Ordering, min};
use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Parser;
use futures::FutureExt;
use imbl_value::InternedString;
use libc::time_t;
use openssl::asn1::{Asn1Integer, Asn1Time, Asn1TimeRef};
use openssl::bn::{BigNum, MsbOption};
use openssl::ec::{EcGroup, EcKey};
use openssl::error::ErrorStack;
use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Private};
use openssl::x509::extension::{
    AuthorityKeyIdentifier, BasicConstraints, KeyUsage, SubjectAlternativeName,
    SubjectKeyIdentifier,
};
use openssl::x509::{X509, X509Builder, X509NameBuilder, X509Ref};
use openssl::*;
use patch_db::HasModel;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::CryptoProvider;
use tokio_rustls::rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::server::ClientHello;
use tracing::instrument;
use ts_rs::TS;
use visit_rs::Visit;

use crate::SOURCE_DATE;
use crate::account::AccountInfo;
use crate::context::{CliContext, RpcContext};
use crate::db::model::Database;
use crate::db::model::public::IpInfo;
use crate::db::{DbAccess, DbAccessMut};
use crate::init::check_time_is_synchronized;
use crate::net::gateway::GatewayInfo;
use crate::net::tls::{TlsHandler, TlsHandlerAction};
use crate::net::utils::is_global_ip;
use crate::net::web_server::{Accept, ExtractVisitor, TcpMetadata, extract};
use crate::prelude::*;
use crate::util::serde::{HandlerExtSerde, Pem};

pub fn should_use_cert(cert: &X509Ref) -> Result<bool, ErrorStack> {
    Ok(cert
        .not_before()
        .compare(Asn1Time::days_from_now(0)?.as_ref())?
        == Ordering::Less
        && cert
            .not_after()
            .compare(Asn1Time::days_from_now(30)?.as_ref())?
            == Ordering::Greater)
}

/// Inside its validity window. [`should_use_cert`] additionally demands 30
/// days of headroom, so a renewal is attempted well before this turns false.
pub fn cert_is_unexpired(cert: &X509Ref) -> Result<bool, ErrorStack> {
    Ok(cert
        .not_before()
        .compare(Asn1Time::days_from_now(0)?.as_ref())?
        == Ordering::Less
        && cert
            .not_after()
            .compare(Asn1Time::days_from_now(0)?.as_ref())?
            == Ordering::Greater)
}

/// Controls the strings baked into generated certificates (Subject CN, O, OU).
/// Exposed so downstream consumers can issue certs with their own branding
/// without having to reimplement the X.509 builders. Not persisted — callers
/// construct this on demand (typically from hostname + their own labels).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertBranding {
    pub organization: InternedString,
    pub organizational_unit: InternedString,
    pub root_ca_cn: InternedString,
    pub intermediate_ca_cn: InternedString,
}
impl CertBranding {
    pub fn start_os(hostname: &str) -> Self {
        Self {
            organization: InternedString::intern("Start9"),
            organizational_unit: InternedString::intern("StartOS"),
            root_ca_cn: InternedString::from_display(&lazy_format::lazy_format!(
                "{hostname} Local Root CA"
            )),
            intermediate_ca_cn: InternedString::intern("StartOS Local Intermediate CA"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, HasModel)]
#[model = "Model<Self>"]
#[serde(rename_all = "camelCase")]
pub struct CertStore {
    pub root_key: Pem<PKey<Private>>,
    pub root_cert: Pem<X509>,
    pub int_key: Pem<PKey<Private>>,
    pub int_cert: Pem<X509>,
    pub leaves: BTreeMap<JsonKey<BTreeSet<InternedString>>, CertData>,
}
impl CertStore {
    pub fn new(account: &AccountInfo, branding: &CertBranding) -> Result<Self, Error> {
        let int_key = gen_nistp256()?;
        let int_cert = make_int_cert(
            (&account.root_ca_key, &account.root_ca_cert),
            &int_key,
            branding,
        )?;
        Ok(Self {
            root_key: Pem::new(account.root_ca_key.clone()),
            root_cert: Pem::new(account.root_ca_cert.clone()),
            int_key: Pem::new(int_key),
            int_cert: Pem::new(int_cert),
            leaves: BTreeMap::new(),
        })
    }
}
impl Model<CertStore> {
    /// The caller must establish entitlement to every requested name.
    pub fn cert_for(
        &mut self,
        hostnames: &BTreeSet<InternedString>,
        branding: &CertBranding,
    ) -> Result<FullchainCertData, Error> {
        let keys = if let Some(cert_data) = self
            .as_leaves()
            .as_idx(JsonKey::new_ref(hostnames))
            .map(|m| m.de())
            .transpose()?
        {
            if should_use_cert(&cert_data.certs.ed25519)?
                && should_use_cert(&cert_data.certs.nistp256)?
            {
                return Ok(FullchainCertData {
                    root: self.as_root_cert().de()?.0,
                    int: self.as_int_cert().de()?.0,
                    leaf: cert_data,
                });
            }
            cert_data.keys
        } else {
            PKeyPair {
                ed25519: PKey::generate_ed25519()?,
                nistp256: gen_nistp256()?,
            }
        };
        let int_key = self.as_int_key().de()?.0;
        let int_cert = self.as_int_cert().de()?.0;
        let cert_data = CertData {
            certs: CertPair {
                ed25519: make_leaf_cert(
                    (&int_key, &int_cert),
                    (&keys.ed25519, &SANInfo::new(hostnames)),
                    branding,
                )?,
                nistp256: make_leaf_cert(
                    (&int_key, &int_cert),
                    (&keys.nistp256, &SANInfo::new(hostnames)),
                    branding,
                )?,
            },
            keys,
        };
        self.as_leaves_mut()
            .insert(JsonKey::new_ref(hostnames), &cert_data)?;
        Ok(FullchainCertData {
            root: self.as_root_cert().de()?.0,
            int: self.as_int_cert().de()?.0,
            leaf: cert_data,
        })
    }
}
impl DbAccess<CertStore> for Database {
    fn access<'a>(db: &'a Model<Self>) -> &'a Model<CertStore> {
        db.as_private().as_key_store().as_local_certs()
    }
}
impl DbAccessMut<CertStore> for Database {
    fn access_mut<'a>(db: &'a mut Model<Self>) -> &'a mut Model<CertStore> {
        db.as_private_mut().as_key_store_mut().as_local_certs_mut()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CertData {
    pub keys: PKeyPair,
    pub certs: CertPair,
}
impl CertData {
    pub fn expiration(&self) -> Result<SystemTime, Error> {
        self.certs.expiration()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullchainCertData {
    pub root: X509,
    pub int: X509,
    pub leaf: CertData,
}
impl FullchainCertData {
    pub fn fullchain_ed25519(&self) -> Vec<&X509> {
        vec![&self.leaf.certs.ed25519, &self.int, &self.root]
    }
    pub fn fullchain_nistp256(&self) -> Vec<&X509> {
        vec![&self.leaf.certs.nistp256, &self.int, &self.root]
    }
    pub fn expiration(&self) -> Result<SystemTime, Error> {
        [
            asn1_time_to_system_time(self.root.not_after())?,
            asn1_time_to_system_time(self.int.not_after())?,
            self.leaf.expiration()?,
        ]
        .into_iter()
        .min()
        .ok_or_else(|| Error::new(eyre!("{}", t!("net.ssl.unreachable")), ErrorKind::Unknown))
    }
}

static CERTIFICATE_VERSION: i32 = 2; // X509 version 3 is actually encoded as '2' in the cert because fuck you.

fn unix_time(time: SystemTime) -> time_t {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as time_t)
        .or_else(|_| UNIX_EPOCH.elapsed().map(|d| -(d.as_secs() as time_t)))
        .unwrap_or_default()
}

lazy_static::lazy_static! {
    static ref ASN1_UNIX_EPOCH: Asn1Time = Asn1Time::from_unix(0).unwrap();
}

fn asn1_time_to_system_time(time: &Asn1TimeRef) -> Result<SystemTime, Error> {
    let diff = ASN1_UNIX_EPOCH.diff(time)?;
    let mut res = UNIX_EPOCH;
    if diff.days >= 0 {
        res += Duration::from_secs(diff.days as u64 * 86400);
    } else {
        res -= Duration::from_secs((-1 * diff.days) as u64 * 86400);
    }
    if diff.secs >= 0 {
        res += Duration::from_secs(diff.secs as u64);
    } else {
        res -= Duration::from_secs((-1 * diff.secs) as u64);
    }
    Ok(res)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PKeyPair {
    #[serde(with = "crate::util::serde::pem")]
    pub ed25519: PKey<Private>,
    #[serde(with = "crate::util::serde::pem")]
    pub nistp256: PKey<Private>,
}
impl PartialEq for PKeyPair {
    fn eq(&self, other: &Self) -> bool {
        self.ed25519.public_eq(&other.ed25519) && self.nistp256.public_eq(&other.nistp256)
    }
}
impl Eq for PKeyPair {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct CertPair {
    #[serde(with = "crate::util::serde::pem")]
    pub ed25519: X509,
    #[serde(with = "crate::util::serde::pem")]
    pub nistp256: X509,
}
impl CertPair {
    pub fn expiration(&self) -> Result<SystemTime, Error> {
        Ok(min(
            asn1_time_to_system_time(self.ed25519.not_after())?,
            asn1_time_to_system_time(self.nistp256.not_after())?,
        ))
    }
}

pub async fn root_ca_start_time() -> SystemTime {
    if check_time_is_synchronized()
        .await
        .log_err()
        .unwrap_or(false)
    {
        SystemTime::now()
    } else {
        *SOURCE_DATE
    }
}

const EC_CURVE_NAME: nid::Nid = nid::Nid::X9_62_PRIME256V1;
lazy_static::lazy_static! {
    static ref EC_GROUP: EcGroup = EcGroup::from_curve_name(EC_CURVE_NAME).unwrap();
}

pub async fn export_key(key: &PKey<Private>, target: &Path) -> Result<(), Error> {
    tokio::fs::write(target, key.private_key_to_pem_pkcs8()?)
        .map(|res| res.with_ctx(|_| (ErrorKind::Filesystem, target.display().to_string())))
        .await?;
    Ok(())
}
pub async fn export_cert(chain: &[&X509], target: &Path) -> Result<(), Error> {
    tokio::fs::write(
        target,
        chain
            .into_iter()
            .flat_map(|c| c.to_pem().unwrap())
            .collect::<Vec<u8>>(),
    )
    .await?;
    Ok(())
}

#[instrument(skip_all)]
fn rand_serial() -> Result<Asn1Integer, Error> {
    let mut bn = BigNum::new()?;
    bn.rand(64, MsbOption::MAYBE_ZERO, false)?;
    let asn1 = Asn1Integer::from_bn(&bn)?;
    Ok(asn1)
}
#[instrument(skip_all)]
pub fn gen_nistp256() -> Result<PKey<Private>, Error> {
    Ok(PKey::from_ec_key(EcKey::generate(EC_GROUP.as_ref())?)?)
}

#[instrument(skip_all)]
pub fn make_root_cert(
    root_key: &PKey<Private>,
    branding: &CertBranding,
    start_time: SystemTime,
) -> Result<X509, Error> {
    let mut builder = X509Builder::new()?;
    builder.set_version(CERTIFICATE_VERSION)?;

    let unix_start_time = unix_time(start_time);

    let embargo = Asn1Time::from_unix(unix_start_time - 86400)?;
    builder.set_not_before(&embargo)?;

    let expiration = Asn1Time::from_unix(unix_start_time + (10 * 364 * 86400))?;
    builder.set_not_after(&expiration)?;

    builder.set_serial_number(&*rand_serial()?)?;

    let mut subject_name_builder = X509NameBuilder::new()?;
    subject_name_builder.append_entry_by_text("CN", &branding.root_ca_cn)?;
    subject_name_builder.append_entry_by_text("O", &branding.organization)?;
    subject_name_builder.append_entry_by_text("OU", &branding.organizational_unit)?;
    let subject_name = subject_name_builder.build();
    builder.set_subject_name(&subject_name)?;

    builder.set_issuer_name(&subject_name)?;

    builder.set_pubkey(&root_key)?;

    // Extensions
    let cfg = conf::Conf::new(conf::ConfMethod::default())?;
    let ctx = builder.x509v3_context(None, Some(&cfg));
    // subjectKeyIdentifier = hash
    let subject_key_identifier = SubjectKeyIdentifier::new().build(&ctx)?;
    // authorityKeyIdentifier = keyid,issuer:always
    let authority_key_identifier = AuthorityKeyIdentifier::new()
        .keyid(false)
        .issuer(true)
        .build(&ctx)?;
    // basicConstraints = critical, CA:true, pathlen:0
    let basic_constraints = BasicConstraints::new().critical().ca().build()?;
    // keyUsage = critical, digitalSignature, cRLSign, keyCertSign
    let key_usage = KeyUsage::new()
        .critical()
        .digital_signature()
        .crl_sign()
        .key_cert_sign()
        .build()?;
    builder.append_extension(subject_key_identifier)?;
    builder.append_extension(authority_key_identifier)?;
    builder.append_extension(basic_constraints)?;
    builder.append_extension(key_usage)?;
    builder.sign(&root_key, MessageDigest::sha256())?;
    let cert = builder.build();
    Ok(cert)
}
#[instrument(skip_all)]
pub fn make_int_cert(
    signer: (&PKey<Private>, &X509),
    applicant: &PKey<Private>,
    branding: &CertBranding,
) -> Result<X509, Error> {
    let mut builder = X509Builder::new()?;
    builder.set_version(CERTIFICATE_VERSION)?;

    builder.set_not_before(signer.1.not_before())?;

    builder.set_not_after(signer.1.not_after())?;

    builder.set_serial_number(&*rand_serial()?)?;

    let mut subject_name_builder = X509NameBuilder::new()?;
    subject_name_builder.append_entry_by_text("CN", &branding.intermediate_ca_cn)?;
    subject_name_builder.append_entry_by_text("O", &branding.organization)?;
    subject_name_builder.append_entry_by_text("OU", &branding.organizational_unit)?;
    let subject_name = subject_name_builder.build();
    builder.set_subject_name(&subject_name)?;

    builder.set_issuer_name(signer.1.subject_name())?;

    builder.set_pubkey(&applicant)?;

    let cfg = conf::Conf::new(conf::ConfMethod::default())?;
    let ctx = builder.x509v3_context(Some(&signer.1), Some(&cfg));

    // subjectKeyIdentifier = hash
    let subject_key_identifier = SubjectKeyIdentifier::new().build(&ctx)?;
    // authorityKeyIdentifier = keyid:always,issuer:always
    let authority_key_identifier = AuthorityKeyIdentifier::new()
        .keyid(true)
        .issuer(true)
        .build(&ctx)?;
    // basicConstraints = critical, CA:true, pathlen:0
    let basic_constraints = BasicConstraints::new().critical().ca().pathlen(0).build()?;
    // keyUsage = critical, digitalSignature, cRLSign, keyCertSign
    let key_usage = KeyUsage::new()
        .critical()
        .digital_signature()
        .crl_sign()
        .key_cert_sign()
        .build()?;
    builder.append_extension(subject_key_identifier)?;
    builder.append_extension(authority_key_identifier)?;
    builder.append_extension(basic_constraints)?;
    builder.append_extension(key_usage)?;
    builder.sign(&signer.0, MessageDigest::sha256())?;
    let cert = builder.build();
    Ok(cert)
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MaybeWildcard {
    WithWildcard(String),
    WithoutWildcard(InternedString),
}
impl MaybeWildcard {
    pub fn as_str(&self) -> &str {
        match self {
            MaybeWildcard::WithWildcard(s) => s.as_str(),
            MaybeWildcard::WithoutWildcard(s) => &**s,
        }
    }
}
impl std::fmt::Display for MaybeWildcard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaybeWildcard::WithWildcard(dns) => write!(f, "DNS:{dns},DNS:*.{dns}"),
            MaybeWildcard::WithoutWildcard(dns) => write!(f, "DNS:{dns}"),
        }
    }
}

#[derive(Debug)]
pub struct SANInfo {
    pub dns: BTreeSet<MaybeWildcard>,
    pub ips: BTreeSet<IpAddr>,
}
impl SANInfo {
    pub fn new(hostnames: &BTreeSet<InternedString>) -> Self {
        let mut dns = BTreeSet::new();
        let mut ips = BTreeSet::new();
        for hostname in hostnames {
            if let Ok(ip) = hostname.parse::<IpAddr>() {
                ips.insert(ip);
            } else {
                dns.insert(MaybeWildcard::WithoutWildcard(hostname.clone())); // TODO: wildcards?
            }
        }
        Self { dns, ips }
    }
    pub fn x509_extension(&self) -> SubjectAlternativeName {
        let mut san = SubjectAlternativeName::new();
        for h in &self.dns {
            san.dns(&h.as_str());
        }
        for ip in &self.ips {
            san.ip(&ip.to_string());
        }
        san
    }
}
impl std::fmt::Display for SANInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut written = false;
        for dns in &self.dns {
            if written {
                write!(f, ",")?;
            }
            written = true;
            write!(f, "{dns}")?;
        }
        for ip in &self.ips {
            if written {
                write!(f, ",")?;
            }
            written = true;
            write!(f, "IP:{ip}")?;
        }
        Ok(())
    }
}

#[instrument(skip_all)]
pub fn make_leaf_cert(
    signer: (&PKey<Private>, &X509),
    applicant: (&PKey<Private>, &SANInfo),
    branding: &CertBranding,
) -> Result<X509, Error> {
    let mut builder = X509Builder::new()?;
    builder.set_version(CERTIFICATE_VERSION)?;

    let embargo = Asn1Time::from_unix(unix_time(SystemTime::now()) - 86400)?;
    builder.set_not_before(&embargo)?;

    // Google Apple and Mozilla reject certificate horizons longer than 398 days
    // https://techbeacon.com/security/google-apple-mozilla-enforce-1-year-max-security-certifications
    let expiration = Asn1Time::days_from_now(397)?;
    builder.set_not_after(&expiration)?;

    builder.set_serial_number(&*rand_serial()?)?;

    let mut subject_name_builder = X509NameBuilder::new()?;
    subject_name_builder.append_entry_by_text(
        "CN",
        applicant
            .1
            .dns
            .first()
            .map(MaybeWildcard::as_str)
            .unwrap_or("localhost"),
    )?;
    subject_name_builder.append_entry_by_text("O", &branding.organization)?;
    subject_name_builder.append_entry_by_text("OU", &branding.organizational_unit)?;
    let subject_name = subject_name_builder.build();
    builder.set_subject_name(&subject_name)?;

    builder.set_issuer_name(signer.1.subject_name())?;

    builder.set_pubkey(&applicant.0)?;

    // Extensions
    let cfg = conf::Conf::new(conf::ConfMethod::default())?;
    let ctx = builder.x509v3_context(Some(&signer.1), Some(&cfg));

    let subject_key_identifier = SubjectKeyIdentifier::new().build(&ctx)?;
    let authority_key_identifier = AuthorityKeyIdentifier::new()
        .keyid(true)
        .issuer(false)
        .build(&ctx)?;
    let subject_alt_name = applicant.1.x509_extension().build(&ctx)?;
    let basic_constraints = BasicConstraints::new().build()?;
    let key_usage = KeyUsage::new()
        .critical()
        .digital_signature()
        .key_encipherment()
        .build()?;

    builder.append_extension(subject_key_identifier)?;
    builder.append_extension(authority_key_identifier)?;
    builder.append_extension(subject_alt_name)?;
    builder.append_extension(basic_constraints)?;
    builder.append_extension(key_usage)?;

    builder.sign(&signer.0, MessageDigest::sha256())?;

    let cert = builder.build();
    Ok(cert)
}

#[instrument(skip_all)]
pub fn make_self_signed(
    applicant: (&PKey<Private>, &SANInfo),
    branding: &CertBranding,
) -> Result<X509, Error> {
    let mut builder = X509Builder::new()?;
    builder.set_version(CERTIFICATE_VERSION)?;

    let embargo = Asn1Time::from_unix(unix_time(SystemTime::now()) - 86400)?;
    builder.set_not_before(&embargo)?;

    // Google Apple and Mozilla reject certificate horizons longer than 398 days
    // https://techbeacon.com/security/google-apple-mozilla-enforce-1-year-max-security-certifications
    let expiration = Asn1Time::days_from_now(397)?;
    builder.set_not_after(&expiration)?;

    builder.set_serial_number(&*rand_serial()?)?;

    let mut subject_name_builder = X509NameBuilder::new()?;
    subject_name_builder.append_entry_by_text(
        "CN",
        applicant
            .1
            .dns
            .first()
            .map(MaybeWildcard::as_str)
            .unwrap_or("localhost"),
    )?;
    subject_name_builder.append_entry_by_text("O", &branding.organization)?;
    subject_name_builder.append_entry_by_text("OU", &branding.organizational_unit)?;
    let subject_name = subject_name_builder.build();
    builder.set_subject_name(&subject_name)?;

    builder.set_issuer_name(&subject_name)?;

    builder.set_pubkey(&applicant.0)?;

    // Extensions
    let cfg = conf::Conf::new(conf::ConfMethod::default())?;
    let ctx = builder.x509v3_context(None, Some(&cfg));

    let subject_key_identifier = SubjectKeyIdentifier::new().build(&ctx)?;
    let authority_key_identifier = AuthorityKeyIdentifier::new()
        .keyid(false)
        .issuer(true)
        .build(&ctx)?;
    let subject_alt_name = applicant.1.x509_extension().build(&ctx)?;
    let basic_constraints = BasicConstraints::new().build()?;
    let key_usage = KeyUsage::new()
        .critical()
        .digital_signature()
        .key_encipherment()
        .build()?;

    builder.append_extension(subject_key_identifier)?;
    builder.append_extension(authority_key_identifier)?;
    builder.append_extension(subject_alt_name)?;
    builder.append_extension(basic_constraints)?;
    builder.append_extension(key_usage)?;

    builder.sign(&applicant.0, MessageDigest::sha256())?;

    let cert = builder.build();
    Ok(cert)
}

pub fn ssl_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand(
        "generate-certificate",
        from_fn_async(generate_certificate)
            .with_display_serializable()
            .with_custom_display_fn(|_, res: GenerateCertificateResponse| {
                println!("Private Key:");
                print!("{}", res.key);
                println!("\nCertificate Chain:");
                print!("{}", res.fullchain);
                Ok(())
            })
            .with_about("about.ssl-generate-certificate")
            .with_call_remote::<CliContext>(),
    )
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser, TS)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
#[group(skip)]
#[ts(export)]
pub struct GenerateCertificateParams {
    #[arg(help = "help.arg.hostnames")]
    pub hostnames: Vec<String>,
    #[arg(long, help = "help.arg.ed25519")]
    #[serde(default)]
    pub ed25519: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GenerateCertificateResponse {
    pub key: String,
    pub fullchain: String,
}

pub async fn generate_certificate(
    ctx: RpcContext,
    GenerateCertificateParams { hostnames, ed25519 }: GenerateCertificateParams,
) -> Result<GenerateCertificateResponse, Error> {
    let peek = ctx.db.peek().await;
    let cert_store = peek.as_private().as_key_store().as_local_certs();
    let int_key = cert_store.as_int_key().de()?.0;
    let int_cert = cert_store.as_int_cert().de()?.0;
    let root_cert = cert_store.as_root_cert().de()?.0;
    let server_hostname = peek.as_public().as_server_info().as_hostname().de()?;
    drop(peek);

    let branding = CertBranding::start_os(&server_hostname);
    let hostnames: BTreeSet<InternedString> =
        hostnames.into_iter().map(InternedString::from).collect();
    let san_info = SANInfo::new(&hostnames);

    let (key, cert) = if ed25519 {
        let key = PKey::generate_ed25519()?;
        let cert = make_leaf_cert((&int_key, &int_cert), (&key, &san_info), &branding)?;
        (key, cert)
    } else {
        let key = gen_nistp256()?;
        let cert = make_leaf_cert((&int_key, &int_cert), (&key, &san_info), &branding)?;
        (key, cert)
    };

    let key_pem = String::from_utf8(key.private_key_to_pem_pkcs8()?).with_kind(ErrorKind::Utf8)?;
    let fullchain_pem = String::from_utf8(
        [&cert, &int_cert, &root_cert]
            .into_iter()
            .map(|c| c.to_pem())
            .collect::<Result<Vec<_>, _>>()?
            .concat(),
    )
    .with_kind(ErrorKind::Utf8)?;

    Ok(GenerateCertificateResponse {
        key: key_pem,
        fullchain: fullchain_pem,
    })
}

fn served_name(
    server_name: Option<&str>,
    tcp: Option<TcpMetadata>,
    gateway: Option<&IpInfo>,
) -> Option<InternedString> {
    if let Some(host) = server_name {
        return Some(InternedString::from(host));
    }
    let tcp = tcp?;
    let dst = tcp.local_addr.ip().to_canonical();
    let src = tcp.peer_addr.ip().to_canonical();
    let translated = !is_global_ip(dst)
        && is_global_ip(src)
        && !gateway.is_some_and(|gw| gw.subnets.iter().any(|s| s.contains(&src)));
    let wan = gateway
        .and_then(|gw| gw.wan_ip)
        .filter(|wan| is_global_ip(IpAddr::V4(*wan)));
    let dialed = match (dst, wan) {
        (IpAddr::V4(_), Some(wan)) if translated => IpAddr::V4(wan),
        _ => dst,
    };
    Some(InternedString::from_display(&dialed))
}

pub struct RootCaTlsHandler<M: HasModel> {
    pub db: TypedPatchDb<M>,
    pub crypto_provider: Arc<CryptoProvider>,
    pub branding: CertBranding,
}
impl<M: HasModel> Clone for RootCaTlsHandler<M> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            crypto_provider: self.crypto_provider.clone(),
            branding: self.branding.clone(),
        }
    }
}

impl<'a, A, M> TlsHandler<'a, A> for RootCaTlsHandler<M>
where
    A: Accept + 'a,
    <A as Accept>::Metadata: Visit<ExtractVisitor<TcpMetadata>>
        + Visit<ExtractVisitor<GatewayInfo>>
        + Clone
        + Send
        + Sync
        + 'static,
    M: HasModel<Model = Model<M>> + DbAccessMut<CertStore> + Send + Sync,
{
    async fn get_config(
        &mut self,
        hello: &ClientHello<'_>,
        metadata: &<A as Accept>::Metadata,
    ) -> Option<TlsHandlerAction> {
        let gateway = extract::<GatewayInfo, _>(metadata).and_then(|i| i.info.ip_info);
        let hostnames = [served_name(
            hello.server_name(),
            extract::<TcpMetadata, _>(metadata),
            gateway.as_deref(),
        )?]
        .into_iter()
        .collect();
        let branding = self.branding.clone();
        let cert = self
            .db
            .mutate(move |db| M::access_mut(db).cert_for(&hostnames, &branding))
            .await
            .result
            .log_err()?;
        let cfg = ServerConfig::builder_with_provider(self.crypto_provider.clone())
            .with_safe_default_protocol_versions()
            .log_err()?
            .with_no_client_auth();
        if hello
            .signature_schemes()
            .contains(&tokio_rustls::rustls::SignatureScheme::ED25519)
        {
            cfg.with_single_cert(
                cert.fullchain_ed25519()
                    .into_iter()
                    .map(|c| {
                        Ok(tokio_rustls::rustls::pki_types::CertificateDer::from(
                            c.to_der()?,
                        ))
                    })
                    .collect::<Result<_, Error>>()
                    .log_err()?,
                PrivateKeyDer::from(PrivatePkcs8KeyDer::from(
                    cert.leaf.keys.ed25519.private_key_to_pkcs8().log_err()?,
                )),
            )
        } else {
            cfg.with_single_cert(
                cert.fullchain_nistp256()
                    .into_iter()
                    .map(|c| {
                        Ok(tokio_rustls::rustls::pki_types::CertificateDer::from(
                            c.to_der()?,
                        ))
                    })
                    .collect::<Result<_, Error>>()
                    .log_err()?,
                PrivateKeyDer::from(PrivatePkcs8KeyDer::from(
                    cert.leaf.keys.nistp256.private_key_to_pkcs8().log_err()?,
                )),
            )
        }
        .log_err()
        .map(TlsHandlerAction::Tls)
    }
}

#[cfg(test)]
mod cert_validity_tests {
    use openssl::x509::X509Builder;

    use super::*;

    fn cert_expiring_in(days: i64) -> X509 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let key = gen_nistp256().unwrap();
        let mut builder = X509Builder::new().unwrap();
        builder
            .set_not_before(&Asn1Time::from_unix(now - 86400).unwrap())
            .unwrap();
        builder
            .set_not_after(&Asn1Time::from_unix(now + days * 86400).unwrap())
            .unwrap();
        builder.set_pubkey(&key).unwrap();
        builder.sign(&key, MessageDigest::sha256()).unwrap();
        builder.build()
    }

    /// The renewal window: too old to serve without trying to renew, still
    /// good enough to serve while that renewal is failing.
    #[test]
    fn the_renewal_window_is_unexpired_but_due_for_renewal() {
        let fresh = cert_expiring_in(60);
        assert!(should_use_cert(&fresh).unwrap());
        assert!(cert_is_unexpired(&fresh).unwrap());

        let renewing = cert_expiring_in(10);
        assert!(!should_use_cert(&renewing).unwrap());
        assert!(cert_is_unexpired(&renewing).unwrap());

        let expired = cert_expiring_in(-1);
        assert!(!should_use_cert(&expired).unwrap());
        assert!(!cert_is_unexpired(&expired).unwrap());
    }
}

#[cfg(test)]
mod served_name_tests {
    use std::net::Ipv4Addr;

    use ipnet::IpNet;

    use super::*;

    const LAN: &str = "192.168.0.5:443";
    const LAN_V6: &str = "[2001:db8::5]:443";
    const ULA_V6: &str = "[fd00:3::1]:443";
    const WAN: Ipv4Addr = Ipv4Addr::new(203, 0, 113, 7);

    fn from(peer: &str, local: &str) -> Option<TcpMetadata> {
        Some(TcpMetadata {
            peer_addr: peer.parse().unwrap(),
            local_addr: local.parse().unwrap(),
        })
    }

    fn gateway(subnets: &[&str], wan_ip: Option<Ipv4Addr>) -> IpInfo {
        IpInfo {
            subnets: subnets
                .iter()
                .map(|s| s.parse::<IpNet>().unwrap())
                .collect(),
            wan_ip,
            ..Default::default()
        }
    }

    fn name(sni: Option<&str>, tcp: Option<TcpMetadata>, gw: Option<&IpInfo>) -> Option<String> {
        served_name(sni, tcp, gw).map(|n| n.to_string())
    }

    #[test]
    fn a_host_is_certified_by_name_alone_on_every_listener() {
        let gw = gateway(&["192.168.0.5/24"], Some(WAN));
        let sni = Some("mybox.local");
        for local in [LAN, LAN_V6, "127.0.0.1:443", "10.0.3.1:443"] {
            let tcp = from("198.51.100.9:51000", local);
            assert_eq!(
                name(sni, tcp, Some(&gw)).as_deref(),
                Some("mybox.local"),
                "listener {local}"
            );
        }
        assert_eq!(name(sni, None, None).as_deref(), Some("mybox.local"));
    }

    #[test]
    fn an_address_dialed_from_its_own_network_is_certified_as_itself() {
        let gw = gateway(&["192.168.0.5/24"], Some(WAN));
        assert_eq!(
            name(None, from("192.168.0.90:51000", LAN), Some(&gw)).as_deref(),
            Some("192.168.0.5")
        );
        let bridge = gateway(&["10.0.3.1/24"], Some(WAN));
        assert_eq!(
            name(None, from("10.0.3.20:51000", "10.0.3.1:443"), Some(&bridge)).as_deref(),
            Some("10.0.3.1")
        );
    }

    #[test]
    fn an_ipv4_address_dialed_from_the_internet_is_certified_as_the_wan_address() {
        let gw = gateway(&["192.168.0.5/24"], Some(WAN));
        assert_eq!(
            name(None, from("198.51.100.9:51000", LAN), Some(&gw)).as_deref(),
            Some("203.0.113.7")
        );
    }

    #[test]
    fn an_address_dialed_over_a_vpn_is_certified_as_itself() {
        let gw = gateway(&["192.168.0.5/24"], Some(WAN));
        assert_eq!(
            name(None, from("10.59.88.2:51000", LAN), Some(&gw)).as_deref(),
            Some("192.168.0.5")
        );
    }

    #[test]
    fn an_address_dialed_from_a_public_subnet_of_this_gateway_is_certified_as_itself() {
        let gw = gateway(&["192.168.0.5/24", "198.51.100.2/29"], Some(WAN));
        assert_eq!(
            name(None, from("198.51.100.3:51000", LAN), Some(&gw)).as_deref(),
            Some("192.168.0.5")
        );
    }

    #[test]
    fn a_routable_address_is_certified_as_itself() {
        let gw = gateway(&["192.168.0.5/24", "198.51.100.2/29"], Some(WAN));
        assert_eq!(
            name(
                None,
                from("203.0.113.99:51000", "198.51.100.3:443"),
                Some(&gw)
            )
            .as_deref(),
            Some("198.51.100.3")
        );
    }

    #[test]
    fn an_address_is_certified_as_itself_while_the_wan_address_is_unreachable() {
        let gw = gateway(&["192.168.0.5/24"], Some(Ipv4Addr::new(100, 88, 0, 1)));
        assert_eq!(
            name(None, from("198.51.100.9:51000", LAN), Some(&gw)).as_deref(),
            Some("192.168.0.5")
        );
    }

    #[test]
    fn an_address_dialed_from_shared_address_space_is_certified_as_itself() {
        let gw = gateway(&["192.168.0.5/24"], Some(WAN));
        assert_eq!(
            name(None, from("100.100.1.5:51000", LAN), Some(&gw)).as_deref(),
            Some("192.168.0.5")
        );
    }

    #[test]
    fn an_address_dialed_from_the_internet_onto_shared_address_space_is_certified_as_the_wan_address()
     {
        let gw = gateway(&["100.64.0.5/24"], Some(WAN));
        assert_eq!(
            name(
                None,
                from("198.51.100.9:51000", "100.64.0.5:443"),
                Some(&gw)
            )
            .as_deref(),
            Some("203.0.113.7")
        );
    }

    #[test]
    fn an_address_is_certified_as_itself_while_the_wan_address_is_unknown() {
        let gw = gateway(&["192.168.0.5/24"], None);
        assert_eq!(
            name(None, from("198.51.100.9:51000", LAN), Some(&gw)).as_deref(),
            Some("192.168.0.5")
        );
        assert_eq!(
            name(None, from("198.51.100.9:51000", LAN), None).as_deref(),
            Some("192.168.0.5")
        );
    }

    #[test]
    fn an_ipv6_address_is_certified_as_itself_whatever_the_source() {
        let gw = gateway(&["2001:db8::5/64"], Some(WAN));
        for (peer, local, expected) in [
            ("[2001:db8::90]:51000", LAN_V6, "2001:db8::5"),
            ("[2001:db8:1::9]:51000", LAN_V6, "2001:db8::5"),
            ("[2001:db8:1::9]:51000", ULA_V6, "fd00:3::1"),
        ] {
            assert_eq!(
                name(None, from(peer, local), Some(&gw)).as_deref(),
                Some(expected),
                "peer {peer} listener {local}"
            );
        }
    }

    #[test]
    fn an_ip_literal_sni_is_certified_as_the_named_address() {
        let gw = gateway(&["192.168.0.5/24"], None);
        for address in ["203.0.113.7", "2001:db8::5"] {
            assert_eq!(
                name(Some(address), from("192.168.0.90:51000", LAN), Some(&gw)).as_deref(),
                Some(address)
            );
        }
    }

    #[test]
    fn a_mapped_ipv4_dial_is_certified_as_the_ipv4_address_it_carries() {
        let gw = gateway(&["192.168.0.5/24"], Some(WAN));
        assert_eq!(
            name(
                None,
                from("[::ffff:192.168.0.90]:51000", "[::ffff:192.168.0.5]:443"),
                Some(&gw)
            )
            .as_deref(),
            Some("192.168.0.5")
        );
        assert_eq!(
            name(
                None,
                from("[::ffff:198.51.100.9]:51000", "[::ffff:192.168.0.5]:443"),
                Some(&gw)
            )
            .as_deref(),
            Some("203.0.113.7")
        );
    }

    #[test]
    fn a_connection_naming_nothing_gets_no_certificate() {
        let gw = gateway(&["192.168.0.5/24"], Some(WAN));
        assert_eq!(name(None, None, Some(&gw)), None);
    }
}

#[cfg(test)]
mod cert_store_tests {
    use super::*;

    #[test]
    fn a_singleton_certificate_reuses_its_key() {
        let account = AccountInfo::new("password", SystemTime::now(), None).unwrap();
        let branding = CertBranding::start_os(account.hostname.as_ref());
        let mut store =
            Model::<CertStore>::new(&CertStore::new(&account, &branding).unwrap()).unwrap();
        let hostnames = [InternedString::from("mybox.local")].into_iter().collect();

        let first = store.cert_for(&hostnames, &branding).unwrap();
        let second = store.cert_for(&hostnames, &branding).unwrap();

        assert_eq!(
            first.leaf.keys.ed25519.private_key_to_der().unwrap(),
            second.leaf.keys.ed25519.private_key_to_der().unwrap()
        );
    }
}
