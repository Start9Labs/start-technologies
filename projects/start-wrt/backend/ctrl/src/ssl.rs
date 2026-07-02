use std::collections::BTreeSet;
use std::fs;
use std::io::BufReader;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

use arc_swap::ArcSwap;
use imbl_value::InternedString;
use openssl::pkey::PKey;
use openssl::x509::X509;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::ClientHello;
use rustls::ServerConfig;
use rustls_pemfile::{certs, pkcs8_private_keys};
use startos::net::ssl::{
    CertBranding, SANInfo, gen_nistp256, make_int_cert, make_leaf_cert, make_root_cert,
    should_use_cert,
};
use startos::net::tls::{TlsHandler, TlsHandlerAction};
use startos::net::web_server::Accept;

use uciedit::openwrt::NetworkInterface;
use uciedit::{parse_all, Arena};

use crate::invoke::Invoke;
use crate::prelude::*;

/// Directory for CA and server certificates.
const SSL_CERT_DIR: &str = "/etc/ssl/certs";
const SSL_KEY_DIR: &str = "/etc/ssl/private";

const CA_CERT_FILENAME: &str = "startwrt-ca.pem";
const CA_KEY_FILENAME: &str = "startwrt-ca.key";
const INT_CERT_FILENAME: &str = "startwrt-int.pem";
const INT_KEY_FILENAME: &str = "startwrt-int.key";
const SERVER_CERT_FILENAME: &str = "startwrt-server.pem";
const SERVER_KEY_FILENAME: &str = "startwrt-server.key";

/// Default SAN hostname for the router.
const ROUTER_HOSTNAME: &str = "router.lan";

/// The IPv4 and optional IPv6 addresses to include in the server certificate SAN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanAddresses {
    pub ipv4: Ipv4Addr,
    pub ipv6: Option<Ipv6Addr>,
}

pub fn ca_cert_path() -> PathBuf {
    Path::new(SSL_CERT_DIR).join(CA_CERT_FILENAME)
}

fn ca_key_path() -> PathBuf {
    Path::new(SSL_KEY_DIR).join(CA_KEY_FILENAME)
}

fn int_cert_path() -> PathBuf {
    Path::new(SSL_CERT_DIR).join(INT_CERT_FILENAME)
}

fn int_key_path() -> PathBuf {
    Path::new(SSL_KEY_DIR).join(INT_KEY_FILENAME)
}

pub fn server_cert_path() -> PathBuf {
    Path::new(SSL_CERT_DIR).join(SERVER_CERT_FILENAME)
}

pub fn server_key_path() -> PathBuf {
    Path::new(SSL_KEY_DIR).join(SERVER_KEY_FILENAME)
}

/// Ensure SSL directories exist with appropriate permissions.
fn ensure_dirs() -> Result<(), Error> {
    use std::os::unix::fs::DirBuilderExt;

    // Create /etc/ssl/certs with world-readable
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o755);
    builder
        .create(SSL_CERT_DIR)
        .map_err(|e| Error::new(eyre!("failed to create {SSL_CERT_DIR}: {e}"), ErrorKind::Filesystem))?;

    // Create /etc/ssl/private with restricted access — rebuild builder
    // to avoid applying 0o700 to the intermediate /etc/ssl/ directory
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder
        .create(SSL_KEY_DIR)
        .map_err(|e| Error::new(eyre!("failed to create {SSL_KEY_DIR}: {e}"), ErrorKind::Filesystem))?;

    Ok(())
}

/// Branding baked into the Subject (CN/O/OU) of StartWRT-issued certs.
///
/// `root_ca_suffix` is appended to the Root CA CN so each freshly-minted Root CA
/// gets a unique Subject DN (see [`random_ca_suffix`] and [`generate_root_ca`]).
/// Baking it in here — rather than mutating the returned branding — mirrors
/// start-os's `CertBranding::start_os(hostname)`, which embeds its per-device
/// hostname in the CN at construction time. Pass `""` for the intermediate/leaf
/// generators; they never read `root_ca_cn`.
fn startwrt_branding(root_ca_suffix: &str) -> CertBranding {
    CertBranding {
        organization: InternedString::intern("Start9"),
        organizational_unit: InternedString::intern("StartWRT"),
        root_ca_cn: InternedString::intern(&format!(
            "StartWRT Local Root CA {root_ca_suffix}"
        )),
        intermediate_ca_cn: InternedString::intern("StartWRT Local Intermediate CA"),
    }
}

/// A short random hex token appended to each freshly-minted Root CA's CN.
///
/// Browsers key trusted CAs by Subject DN. Without a per-CA suffix every
/// fresh-flashed build mints a Root CA with an *identical* DN but a *different*
/// key; Firefox/NSS then tries to verify the new chain against the
/// previously-trusted CA's key and rejects it as SEC_ERROR_BAD_SIGNATURE
/// ("Bad Signature"). A unique suffix makes each CA a distinct trust anchor, so
/// an old trusted CA coexists harmlessly with a new one. Mirrors start-os, which
/// embeds its per-device random hostname in the Root CA CN.
fn random_ca_suffix() -> Result<String, Error> {
    let mut bytes = [0u8; 4];
    openssl::rand::rand_bytes(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Generate a new Root CA key pair and self-signed certificate.
/// Returns (cert_pem, key_pem).
fn generate_root_ca() -> Result<(String, String), Error> {
    let key = gen_nistp256()?;

    // Suffix the CN with a per-CA random token so a reflashed device's new Root
    // CA does not collide (same DN, different key) with one already trusted in a
    // browser — that collision surfaces as "Bad Signature". See random_ca_suffix.
    let suffix = random_ca_suffix()?;
    let branding = startwrt_branding(&suffix);

    let cert = make_root_cert(&key, &branding, SystemTime::now())?;

    let cert_pem = String::from_utf8(cert.to_pem()?)
        .map_err(|e| Error::new(eyre!("CA cert PEM is not UTF-8: {e}"), ErrorKind::OpenSsl))?;
    let key_pem = String::from_utf8(key.private_key_to_pem_pkcs8()?)
        .map_err(|e| Error::new(eyre!("CA key PEM is not UTF-8: {e}"), ErrorKind::OpenSsl))?;
    Ok((cert_pem, key_pem))
}

/// Generate an intermediate CA signed by the root CA.
/// Returns (cert_pem, key_pem).
fn generate_intermediate_ca(
    ca_cert_pem: &str,
    ca_key_pem: &str,
) -> Result<(String, String), Error> {
    let ca_cert = X509::from_pem(ca_cert_pem.as_bytes())?;
    let ca_key = PKey::private_key_from_pem(ca_key_pem.as_bytes())?;

    let key = gen_nistp256()?;
    let cert = make_int_cert((&ca_key, &ca_cert), &key, &startwrt_branding(""))?;

    let cert_pem = String::from_utf8(cert.to_pem()?).map_err(|e| {
        Error::new(eyre!("intermediate cert PEM is not UTF-8: {e}"), ErrorKind::OpenSsl)
    })?;
    let key_pem = String::from_utf8(key.private_key_to_pem_pkcs8()?).map_err(|e| {
        Error::new(eyre!("intermediate key PEM is not UTF-8: {e}"), ErrorKind::OpenSsl)
    })?;
    Ok((cert_pem, key_pem))
}

/// Generate a leaf certificate signed by the intermediate CA.
/// SANs include the given LAN IPs and `router.lan`.
/// Returns (leaf_cert_pem + int_cert_pem + ca_cert_pem chain, key_pem).
fn generate_leaf_cert(
    int_cert_pem: &str,
    int_key_pem: &str,
    ca_cert_pem: &str,
    addrs: &LanAddresses,
) -> Result<(String, String), Error> {
    let int_cert = X509::from_pem(int_cert_pem.as_bytes())?;
    let int_key = PKey::private_key_from_pem(int_key_pem.as_bytes())?;

    let leaf_key = gen_nistp256()?;

    // SANInfo partitions by parseability: strings that parse as IPs become IP
    // SANs, others become DNS SANs.
    let mut san_hostnames: BTreeSet<InternedString> = BTreeSet::new();
    san_hostnames.insert(InternedString::intern(ROUTER_HOSTNAME));
    san_hostnames.insert(InternedString::from_display(&addrs.ipv4));
    if let Some(ipv6) = addrs.ipv6 {
        san_hostnames.insert(InternedString::from_display(&ipv6));
    }
    let san_info = SANInfo::new(&san_hostnames);

    let leaf_cert = make_leaf_cert(
        (&int_key, &int_cert),
        (&leaf_key, &san_info),
        &startwrt_branding(""),
    )?;

    let leaf_pem = String::from_utf8(leaf_cert.to_pem()?)
        .map_err(|e| Error::new(eyre!("leaf cert PEM is not UTF-8: {e}"), ErrorKind::OpenSsl))?;
    let key_pem = String::from_utf8(leaf_key.private_key_to_pem_pkcs8()?)
        .map_err(|e| Error::new(eyre!("leaf key PEM is not UTF-8: {e}"), ErrorKind::OpenSsl))?;

    // Full chain: leaf + intermediate + root CA.
    let chain_pem = format!("{leaf_pem}{int_cert_pem}{ca_cert_pem}");
    Ok((chain_pem, key_pem))
}

/// Write a PEM file atomically, with the given permissions.
async fn write_pem(path: &Path, content: &str, mode: u32) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;

    let mut file = startos::util::io::AtomicFile::new(path, None::<&Path>)
        .await
        .map_err(Error::from)?;

    // Set mode on the underlying temp file before writing
    let perms = std::fs::Permissions::from_mode(mode);
    file.set_permissions(perms)
        .await
        .map_err(|e| Error::new(eyre!("failed to set temp file permissions: {e}"), ErrorKind::Filesystem))?;

    file.write_all(content.as_bytes())
        .await
        .map_err(|e| Error::new(eyre!("failed to write cert: {e}"), ErrorKind::Filesystem))?;

    file.save().await.map_err(Error::from)?;

    Ok(())
}

/// Read the current LAN gateway IPv4 from UCI config.
pub async fn read_lan_ip(uci_root: &Path) -> Ipv4Addr {
    let arena = Arena::new();
    let Ok(cfgs) = parse_all(uci_root, &arena, &["network"]).await else {
        return Ipv4Addr::new(192, 168, 0, 1);
    };

    for section in &cfgs["network"].sections {
        if section.name().as_deref() == Some("lan") {
            if let Ok(Some(iface)) = section.get_typed::<NetworkInterface>() {
                if let Some(ip) = iface.ipaddr {
                    return ip;
                }
            }
        }
    }

    Ipv4Addr::new(192, 168, 0, 1)
}

/// Read the current LAN gateway IPv4 from UCI config and IPv6 from the live system.
pub async fn read_lan_addresses(uci_root: &Path) -> LanAddresses {
    LanAddresses {
        ipv4: read_lan_ip(uci_root).await,
        ipv6: read_lan_ipv6_from_ubus().await,
    }
}

/// Read the LAN interface's IPv6 address from ubus.
///
/// Queries `network.interface.lan` for assigned IPv6 addresses, preferring
/// ULA (fd00::/8) over other scopes. Returns None if IPv6 is not configured
/// or the interface has no IPv6 address.
pub async fn read_lan_ipv6_from_ubus() -> Option<Ipv6Addr> {
    let output = tokio::process::Command::new("ubus")
        .args(["call", "network.interface.lan", "status"])
        .invoke(ErrorKind::Network.into())
        .await
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&output).ok()?;

    let mut addrs: Vec<Ipv6Addr> = Vec::new();

    // Collect from ipv6-address array
    if let Some(arr) = json.get("ipv6-address").and_then(|v| v.as_array()) {
        for entry in arr {
            if let Some(addr) = entry
                .get("address")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
            {
                addrs.push(addr);
            }
        }
    }

    // Collect from ipv6-prefix-assignment local addresses
    if let Some(arr) = json.get("ipv6-prefix-assignment").and_then(|v| v.as_array()) {
        for entry in arr {
            if let Some(addr) = entry
                .pointer("/local-address/address")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
            {
                addrs.push(addr);
            }
        }
    }

    // Prefer ULA (fd00::/8), then any non-link-local address
    addrs
        .iter()
        .find(|a| (a.segments()[0] & 0xff00) == 0xfd00)
        .or_else(|| addrs.iter().find(|a| (a.segments()[0] & 0xffc0) != 0xfe80))
        .copied()
}

/// Read the LAN's current delegated *global* IPv6 prefix (base address + length)
/// from ubus. Unlike [`read_lan_ipv6_from_ubus`] (which prefers the ULA for cert
/// SANs), this returns the GUA prefix (2000::/3) so the published-ports reconcile
/// can recompute a device's forward address as `prefix ++ hostid` after the ISP
/// rotates the delegation. Returns `None` when the router currently has no global
/// prefix.
pub async fn read_lan_gua_prefix() -> Option<(Ipv6Addr, u8)> {
    let output = tokio::process::Command::new("ubus")
        .args(["call", "network.interface.lan", "status"])
        .invoke(ErrorKind::Network.into())
        .await
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&output).ok()?;

    let arr = json.get("ipv6-prefix-assignment")?.as_array()?;
    for entry in arr {
        let Some(addr) = entry
            .get("address")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<Ipv6Addr>().ok())
        else {
            // A malformed entry shouldn't mask a valid GUA later in the list.
            continue;
        };
        // Global Unicast (2000::/3) is the only WAN-reachable delegation.
        if crate::system::has_global_ipv6(std::slice::from_ref(&addr)) {
            let mask = entry.get("mask").and_then(|v| v.as_u64()).unwrap_or(64) as u8;
            return Some((addr, mask));
        }
    }
    None
}

/// Check whether the server cert should be renewed (missing, corrupt, not
/// yet valid, or expiring within 30 days — the threshold baked into
/// [`startos::net::ssl::should_use_cert`]).
fn cert_needs_renewal() -> bool {
    let Ok(pem_data) = fs::read(server_cert_path()) else {
        return true;
    };
    let Ok(cert) = X509::from_pem(&pem_data) else {
        return true;
    };
    !should_use_cert(&cert).unwrap_or(false)
}

/// Ensure Root CA exists (generate if missing). Returns the CA cert PEM.
pub async fn ensure_root_ca() -> Result<String, Error> {
    ensure_dirs()?;

    let cert_path = ca_cert_path();
    let key_path = ca_key_path();

    if cert_path.exists() && key_path.exists() {
        return fs::read_to_string(&cert_path)
            .map_err(|e| Error::new(eyre!("failed to read CA cert: {e}"), ErrorKind::Filesystem));
    }

    tracing::info!("generating new Root CA");
    let (cert_pem, key_pem) = generate_root_ca()?;
    write_pem(&cert_path, &cert_pem, 0o644).await?;
    write_pem(&key_path, &key_pem, 0o600).await?;

    Ok(cert_pem)
}

/// Ensure intermediate CA exists (generate if missing). Returns the intermediate cert PEM.
pub async fn ensure_intermediate_ca() -> Result<String, Error> {
    ensure_dirs()?;

    let cert_path = int_cert_path();
    let key_path = int_key_path();

    if cert_path.exists() && key_path.exists() {
        return fs::read_to_string(&cert_path)
            .map_err(|e| Error::new(eyre!("failed to read intermediate cert: {e}"), ErrorKind::Filesystem));
    }

    let ca_cert_pem = fs::read_to_string(ca_cert_path())
        .map_err(|e| Error::new(eyre!("failed to read CA cert: {e}"), ErrorKind::Filesystem))?;
    let ca_key_pem = fs::read_to_string(ca_key_path())
        .map_err(|e| Error::new(eyre!("failed to read CA key: {e}"), ErrorKind::Filesystem))?;

    tracing::info!("generating new intermediate CA");
    let (cert_pem, key_pem) = generate_intermediate_ca(&ca_cert_pem, &ca_key_pem)?;
    write_pem(&cert_path, &cert_pem, 0o644).await?;
    write_pem(&key_path, &key_pem, 0o600).await?;

    Ok(cert_pem)
}

/// Read the IP addresses from the current server cert's SAN extension.
fn read_cert_san_addresses() -> Option<LanAddresses> {
    let pem_data = fs::read(server_cert_path()).ok()?;
    let cert = X509::from_pem(&pem_data).ok()?;
    let sans = cert.subject_alt_names()?;

    let mut ipv4 = None;
    let mut ipv6 = None;

    for name in sans.iter() {
        if let Some(bytes) = name.ipaddress() {
            match bytes.len() {
                4 => {
                    ipv4 = Some(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]));
                }
                16 => {
                    let mut octets = [0u8; 16];
                    octets.copy_from_slice(bytes);
                    ipv6 = Some(Ipv6Addr::from(octets));
                }
                _ => {}
            }
        }
    }

    Some(LanAddresses { ipv4: ipv4?, ipv6 })
}

/// Ensure server leaf cert exists and is valid (generate if missing, corrupt, expiring,
/// or if the SAN addresses don't match the current LAN addresses).
pub async fn ensure_server_cert(addrs: &LanAddresses) -> Result<(), Error> {
    ensure_dirs()?;

    let cert_path = server_cert_path();
    let key_path = server_key_path();

    let needs_gen = !cert_path.exists()
        || !key_path.exists()
        || cert_needs_renewal()
        || read_cert_san_addresses().as_ref() != Some(addrs);
    if !needs_gen {
        return Ok(());
    }

    generate_and_write_server_cert(addrs).await
}

/// Force-regenerate the server leaf cert (e.g. after LAN IP or IPv6 change).
async fn generate_and_write_server_cert(addrs: &LanAddresses) -> Result<(), Error> {
    let int_cert_pem = fs::read_to_string(int_cert_path())
        .map_err(|e| Error::new(eyre!("failed to read intermediate cert: {e}"), ErrorKind::Filesystem))?;
    let int_key_pem = fs::read_to_string(int_key_path())
        .map_err(|e| Error::new(eyre!("failed to read intermediate key: {e}"), ErrorKind::Filesystem))?;
    let ca_cert_pem = fs::read_to_string(ca_cert_path())
        .map_err(|e| Error::new(eyre!("failed to read CA cert: {e}"), ErrorKind::Filesystem))?;

    if let Some(ipv6) = addrs.ipv6 {
        tracing::info!("generating server certificate for {}, {}, and {}", addrs.ipv4, ipv6, ROUTER_HOSTNAME);
    } else {
        tracing::info!("generating server certificate for {} and {}", addrs.ipv4, ROUTER_HOSTNAME);
    }
    let (cert_pem, key_pem) = generate_leaf_cert(&int_cert_pem, &int_key_pem, &ca_cert_pem, addrs)?;

    write_pem(&server_cert_path(), &cert_pem, 0o644).await?;
    write_pem(&server_key_path(), &key_pem, 0o600).await?;

    Ok(())
}

/// Regenerate the server leaf cert (e.g. after LAN IP or IPv6 change). If a
/// hot-reload swap has been initialized, the freshly written cert is loaded
/// and atomically swapped in for new TLS handshakes.
pub async fn regenerate_server_cert(addrs: &LanAddresses) -> Result<(), Error> {
    generate_and_write_server_cert(addrs).await?;
    if let Err(e) = reload_tls_materials() {
        tracing::error!("regenerated server cert but failed to reload TLS materials: {e}");
    }
    Ok(())
}

/// Parsed cert chain + key, ready to plug into a `rustls::ServerConfig`.
pub struct TlsMaterials {
    pub chain: Vec<CertificateDer<'static>>,
    pub key: PrivateKeyDer<'static>,
}

impl TlsMaterials {
    /// Read and parse the on-disk server cert and key. Used at startup to
    /// validate the files and to refresh them after regeneration.
    pub fn load_from_disk() -> Result<Self, Error> {
        let cert_file = fs::File::open(server_cert_path())
            .map_err(|e| Error::new(eyre!("failed to open server cert: {e}"), ErrorKind::Filesystem))?;
        let key_file = fs::File::open(server_key_path())
            .map_err(|e| Error::new(eyre!("failed to open server key: {e}"), ErrorKind::Filesystem))?;

        let chain: Vec<_> = certs(&mut BufReader::new(cert_file))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Error::new(eyre!("failed to parse server cert: {e}"), ErrorKind::OpenSsl))?;

        let key = pkcs8_private_keys(&mut BufReader::new(key_file))
            .next()
            .ok_or_else(|| Error::new(eyre!("no private key found in server key file"), ErrorKind::OpenSsl))?
            .map_err(|e| Error::new(eyre!("failed to parse server key: {e}"), ErrorKind::OpenSsl))?;

        Ok(Self {
            chain,
            key: PrivateKeyDer::Pkcs8(key),
        })
    }
}

/// Process-wide hot-swappable TLS materials. Initialized once at startup;
/// updated by `regenerate_server_cert` so existing servers see new certs on
/// the next handshake without a daemon restart.
static TLS_MATERIALS: OnceLock<Arc<ArcSwap<TlsMaterials>>> = OnceLock::new();

/// Initialize the global TLS materials swap from the on-disk cert and key.
/// Returns a clone of the inner `Arc<ArcSwap<…>>` so callers can hand it to
/// `StaticTlsHandler`. Must be called after `ensure_server_cert` succeeds.
pub fn init_tls_materials() -> Result<Arc<ArcSwap<TlsMaterials>>, Error> {
    let materials = TlsMaterials::load_from_disk()?;
    Ok(TLS_MATERIALS
        .get_or_init(|| Arc::new(ArcSwap::from(Arc::new(materials))))
        .clone())
}

fn reload_tls_materials() -> Result<(), Error> {
    if let Some(swap) = TLS_MATERIALS.get() {
        let materials = TlsMaterials::load_from_disk()?;
        swap.store(Arc::new(materials));
    }
    Ok(())
}

/// `TlsHandler` impl for use with `startos::net::tls::TlsListener`. Serves a
/// single ArcSwap-backed cert chain and is cheap to clone (just clones the
/// `Arc`s). New handshakes pick up the latest cert; in-flight connections
/// keep their original cert until they close.
#[derive(Clone)]
pub struct StaticTlsHandler {
    materials: Arc<ArcSwap<TlsMaterials>>,
    crypto_provider: Arc<CryptoProvider>,
}

impl StaticTlsHandler {
    pub fn new(materials: Arc<ArcSwap<TlsMaterials>>) -> Self {
        Self {
            materials,
            crypto_provider: Arc::new(rustls::crypto::ring::default_provider()),
        }
    }
}

impl<'a, A: Accept + 'a> TlsHandler<'a, A> for StaticTlsHandler
where
    A::Metadata: Sync,
{
    async fn get_config(
        &'a mut self,
        _hello: &'a ClientHello<'a>,
        _metadata: &'a A::Metadata,
    ) -> Option<TlsHandlerAction> {
        let materials = self.materials.load_full();
        let mut cfg = ServerConfig::builder_with_provider(self.crypto_provider.clone())
            .with_safe_default_protocol_versions()
            .ok()?
            .with_no_client_auth()
            .with_single_cert(materials.chain.clone(), materials.key.clone_key())
            .ok()?;
        cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Some(TlsHandlerAction::Tls(cfg))
    }
}

/// Read the Root CA certificate PEM from disk.
pub fn read_root_ca_pem() -> Result<String, Error> {
    fs::read_to_string(ca_cert_path())
        .map_err(|e| Error::new(eyre!("failed to read CA cert: {e}"), ErrorKind::Filesystem))
}

#[cfg(test)]
mod tests {
    use openssl::nid::Nid;

    use super::*;

    #[test]
    fn test_generate_root_ca() {
        let (cert_pem, key_pem) = generate_root_ca().unwrap();
        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(key_pem.contains("BEGIN PRIVATE KEY"));

        // Verify it parses back and the CN carries the base label.
        let cert = X509::from_pem(cert_pem.as_bytes()).unwrap();
        let cn = cert.subject_name().entries_by_nid(Nid::COMMONNAME).next().unwrap();
        assert!(
            cn.data().as_utf8().unwrap().to_string().starts_with("StartWRT Local Root CA "),
            "Root CA CN missing base label"
        );
    }

    #[test]
    fn test_generate_root_ca_unique_subject_dn() {
        // Each freshly-minted Root CA must get a distinct Subject DN (random suffix)
        // so a reflashed device's new CA does not collide with one already trusted in
        // a browser. See random_ca_suffix / startwrt_branding.
        let cn = |pem: &str| {
            X509::from_pem(pem.as_bytes())
                .unwrap()
                .subject_name()
                .entries_by_nid(Nid::COMMONNAME)
                .next()
                .unwrap()
                .data()
                .as_utf8()
                .unwrap()
                .to_string()
        };
        let (a, _) = generate_root_ca().unwrap();
        let (b, _) = generate_root_ca().unwrap();
        assert_ne!(cn(&a), cn(&b), "two Root CAs must have distinct CNs");
    }

    #[test]
    fn test_generate_intermediate_ca() {
        let (ca_cert, ca_key) = generate_root_ca().unwrap();
        let (int_cert_pem, int_key_pem) = generate_intermediate_ca(&ca_cert, &ca_key).unwrap();
        assert!(int_cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(int_key_pem.contains("BEGIN PRIVATE KEY"));

        let int_cert = X509::from_pem(int_cert_pem.as_bytes()).unwrap();

        // Verify CN
        let cn = int_cert.subject_name().entries_by_nid(Nid::COMMONNAME).next().unwrap();
        assert_eq!(cn.data().as_utf8().unwrap().to_string(), "StartWRT Local Intermediate CA");

        // Verify issuer matches root CA subject
        let ca = X509::from_pem(ca_cert.as_bytes()).unwrap();
        assert_eq!(
            int_cert.issuer_name().entries().next().unwrap().data().as_utf8().unwrap().to_string(),
            ca.subject_name().entries().next().unwrap().data().as_utf8().unwrap().to_string(),
        );

        // Verify it has AKI and SKI
        assert!(int_cert.subject_key_id().is_some(), "missing SKI");
        assert!(int_cert.authority_key_id().is_some(), "missing AKI");
    }

    #[test]
    fn test_generate_leaf_cert_includes_ca_in_chain() {
        let (ca_cert, ca_key) = generate_root_ca().unwrap();
        let (int_cert, int_key) = generate_intermediate_ca(&ca_cert, &ca_key).unwrap();
        let addrs = LanAddresses {
            ipv4: Ipv4Addr::new(192, 168, 0, 1),
            ipv6: None,
        };
        let (cert_pem, key_pem) = generate_leaf_cert(&int_cert, &int_key, &ca_cert, &addrs).unwrap();
        assert!(key_pem.contains("BEGIN PRIVATE KEY"));
        // Chain should contain three certificates: leaf + intermediate + root CA
        let cert_count = cert_pem.matches("BEGIN CERTIFICATE").count();
        assert_eq!(cert_count, 3, "chain should contain leaf + intermediate + root CA");
    }

    #[test]
    fn test_leaf_cert_has_expected_extensions() {
        let (ca_cert, ca_key) = generate_root_ca().unwrap();
        let (int_cert, int_key) = generate_intermediate_ca(&ca_cert, &ca_key).unwrap();
        let addrs = LanAddresses {
            ipv4: Ipv4Addr::new(192, 168, 0, 1),
            ipv6: None,
        };
        let (cert_pem, _) = generate_leaf_cert(&int_cert, &int_key, &ca_cert, &addrs).unwrap();

        // Parse just the leaf cert (first in chain)
        let leaf = X509::from_pem(cert_pem.as_bytes()).unwrap();

        // Verify key extensions are present
        assert!(leaf.subject_alt_names().is_some(), "missing SAN");
        assert!(leaf.subject_key_id().is_some(), "missing SKI");
        assert!(leaf.authority_key_id().is_some(), "missing AKI");
    }

    #[test]
    fn test_leaf_cert_with_ipv6_san() {
        let (ca_cert, ca_key) = generate_root_ca().unwrap();
        let (int_cert, int_key) = generate_intermediate_ca(&ca_cert, &ca_key).unwrap();
        let addrs = LanAddresses {
            ipv4: Ipv4Addr::new(192, 168, 0, 1),
            ipv6: Some("fda7:5549:a8c::1".parse().unwrap()),
        };
        let (cert_pem, _) = generate_leaf_cert(&int_cert, &int_key, &ca_cert, &addrs).unwrap();

        let leaf = X509::from_pem(cert_pem.as_bytes()).unwrap();
        let sans = leaf.subject_alt_names().expect("missing SAN");

        // Should have: router.lan (DNS), 192.168.0.1 (IP), fda7:5549:a8c::1 (IP)
        let mut dns_names = Vec::new();
        let mut ip_addrs = Vec::new();
        for name in sans.iter() {
            if let Some(dns) = name.dnsname() {
                dns_names.push(dns.to_string());
            }
            if let Some(ip) = name.ipaddress() {
                ip_addrs.push(ip.to_vec());
            }
        }

        assert!(dns_names.contains(&ROUTER_HOSTNAME.to_string()), "missing router.lan DNS SAN");
        assert_eq!(ip_addrs.len(), 2, "expected 2 IP SANs (IPv4 + IPv6), got {}", ip_addrs.len());
        // IPv4 is 4 bytes, IPv6 is 16 bytes
        assert!(ip_addrs.iter().any(|ip| ip.len() == 4), "missing IPv4 SAN");
        assert!(ip_addrs.iter().any(|ip| ip.len() == 16), "missing IPv6 SAN");
    }

    #[test]
    fn test_leaf_cert_without_ipv6_has_only_ipv4_san() {
        let (ca_cert, ca_key) = generate_root_ca().unwrap();
        let (int_cert, int_key) = generate_intermediate_ca(&ca_cert, &ca_key).unwrap();
        let addrs = LanAddresses {
            ipv4: Ipv4Addr::new(10, 0, 0, 1),
            ipv6: None,
        };
        let (cert_pem, _) = generate_leaf_cert(&int_cert, &int_key, &ca_cert, &addrs).unwrap();

        let leaf = X509::from_pem(cert_pem.as_bytes()).unwrap();
        let sans = leaf.subject_alt_names().expect("missing SAN");

        let ip_addrs: Vec<_> = sans.iter().filter_map(|n| n.ipaddress()).collect();
        assert_eq!(ip_addrs.len(), 1, "expected only 1 IP SAN (IPv4), got {}", ip_addrs.len());
        assert_eq!(ip_addrs[0].len(), 4, "expected IPv4 (4 bytes)");
    }

    #[tokio::test]
    async fn test_read_lan_ip_from_uci() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("network"),
            "\
config interface 'lan'
\toption device 'br-lan'
\toption proto 'static'
\toption ipaddr '10.0.5.1'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();

        let ip = read_lan_ip(dir.path()).await;
        assert_eq!(ip, Ipv4Addr::new(10, 0, 5, 1));
    }

    #[tokio::test]
    async fn test_read_lan_ip_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("network"), "").unwrap();

        let ip = read_lan_ip(dir.path()).await;
        assert_eq!(ip, Ipv4Addr::new(192, 168, 0, 1));
    }

    #[test]
    fn test_read_cert_san_addresses_ipv4_only() {
        let (ca_cert, ca_key) = generate_root_ca().unwrap();
        let (int_cert, int_key) = generate_intermediate_ca(&ca_cert, &ca_key).unwrap();
        let addrs = LanAddresses {
            ipv4: Ipv4Addr::new(10, 0, 0, 1),
            ipv6: None,
        };
        let (cert_pem, _) = generate_leaf_cert(&int_cert, &int_key, &ca_cert, &addrs).unwrap();

        // Parse the leaf cert (first in chain) and extract SANs
        let leaf = X509::from_pem(cert_pem.as_bytes()).unwrap();
        let sans = leaf.subject_alt_names().unwrap();
        let mut found_v4 = None;
        let mut found_v6 = None;
        for name in sans.iter() {
            if let Some(bytes) = name.ipaddress() {
                match bytes.len() {
                    4 => found_v4 = Some(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3])),
                    16 => {
                        let mut octets = [0u8; 16];
                        octets.copy_from_slice(bytes);
                        found_v6 = Some(Ipv6Addr::from(octets));
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(found_v4, Some(Ipv4Addr::new(10, 0, 0, 1)));
        assert_eq!(found_v6, None);
    }

    #[test]
    fn test_read_cert_san_addresses_with_ipv6() {
        let (ca_cert, ca_key) = generate_root_ca().unwrap();
        let (int_cert, int_key) = generate_intermediate_ca(&ca_cert, &ca_key).unwrap();
        let expected_v6: Ipv6Addr = "fda7:5549:a8c::1".parse().unwrap();
        let addrs = LanAddresses {
            ipv4: Ipv4Addr::new(192, 168, 1, 1),
            ipv6: Some(expected_v6),
        };
        let (cert_pem, _) = generate_leaf_cert(&int_cert, &int_key, &ca_cert, &addrs).unwrap();

        let leaf = X509::from_pem(cert_pem.as_bytes()).unwrap();
        let sans = leaf.subject_alt_names().unwrap();
        let mut found_v4 = None;
        let mut found_v6 = None;
        for name in sans.iter() {
            if let Some(bytes) = name.ipaddress() {
                match bytes.len() {
                    4 => found_v4 = Some(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3])),
                    16 => {
                        let mut octets = [0u8; 16];
                        octets.copy_from_slice(bytes);
                        found_v6 = Some(Ipv6Addr::from(octets));
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(found_v4, Some(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(found_v6, Some(expected_v6));
    }

    #[tokio::test]
    async fn test_write_pem_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pem");
        write_pem(&path, "test content", 0o644).await.unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "test content");

        // Verify permissions
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
    }

    #[tokio::test]
    async fn test_write_pem_key_restricted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.key");
        write_pem(&path, "secret key", 0o600).await.unwrap();

        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
