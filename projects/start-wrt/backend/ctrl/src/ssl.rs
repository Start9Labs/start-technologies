use std::fs;
use std::net::Ipv6Addr;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use imbl_value::InternedString;
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use startos::net::ssl::{gen_nistp256, make_int_cert, make_root_cert, CertBranding};

use crate::invoke::Invoke;
use crate::prelude::*;

/// Directory for the CA certificates.
const SSL_CERT_DIR: &str = "/etc/ssl/certs";
const SSL_KEY_DIR: &str = "/etc/ssl/private";

const CA_CERT_FILENAME: &str = "startwrt-ca.pem";
const CA_KEY_FILENAME: &str = "startwrt-ca.key";
const INT_CERT_FILENAME: &str = "startwrt-int.pem";
const INT_KEY_FILENAME: &str = "startwrt-int.key";

/// Default SAN hostname for the router.
pub(crate) const ROUTER_HOSTNAME: &str = "router.lan";

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

/// Ensure SSL directories exist with appropriate permissions.
fn ensure_dirs() -> Result<(), Error> {
    use std::os::unix::fs::DirBuilderExt;

    // Create /etc/ssl/certs with world-readable
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o755);
    builder.create(SSL_CERT_DIR).map_err(|e| {
        Error::new(
            eyre!("failed to create {SSL_CERT_DIR}: {e}"),
            ErrorKind::Filesystem,
        )
    })?;

    // Create /etc/ssl/private with restricted access — rebuild builder
    // to avoid applying 0o700 to the intermediate /etc/ssl/ directory
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(SSL_KEY_DIR).map_err(|e| {
        Error::new(
            eyre!("failed to create {SSL_KEY_DIR}: {e}"),
            ErrorKind::Filesystem,
        )
    })?;

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
pub(crate) fn startwrt_branding(root_ca_suffix: &str) -> CertBranding {
    CertBranding {
        organization: InternedString::intern("Start9"),
        organizational_unit: InternedString::intern("StartWRT"),
        root_ca_cn: InternedString::intern(&format!("StartWRT Local Root CA {root_ca_suffix}")),
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
        Error::new(
            eyre!("intermediate cert PEM is not UTF-8: {e}"),
            ErrorKind::OpenSsl,
        )
    })?;
    let key_pem = String::from_utf8(key.private_key_to_pem_pkcs8()?).map_err(|e| {
        Error::new(
            eyre!("intermediate key PEM is not UTF-8: {e}"),
            ErrorKind::OpenSsl,
        )
    })?;
    Ok((cert_pem, key_pem))
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
    file.set_permissions(perms).await.map_err(|e| {
        Error::new(
            eyre!("failed to set temp file permissions: {e}"),
            ErrorKind::Filesystem,
        )
    })?;

    file.write_all(content.as_bytes())
        .await
        .map_err(|e| Error::new(eyre!("failed to write cert: {e}"), ErrorKind::Filesystem))?;

    file.save().await.map_err(Error::from)?;

    Ok(())
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
    if let Some(arr) = json
        .get("ipv6-prefix-assignment")
        .and_then(|v| v.as_array())
    {
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

/// Every LAN-side interface's currently-assigned *global* IPv6 prefix, as
/// `(l3_device, prefix, mask)` — e.g. `("br-lan.101", 2001:db8:0:1::, 64)` —
/// read from `ubus call network.interface dump`. Unlike
/// [`read_lan_ipv6_from_ubus`] (which prefers the ULA), this returns GUA
/// prefixes (2000::/3) only.
///
/// The admin LAN is not the only bridge with a GUA /64: when the ISP delegates
/// more than a /64, every v6-capable profile interface gets its own slice, and
/// a published-port target on such a profile holds its address in *that* /64.
/// So election scoping must accept an address in any of these prefixes, and
/// suffix recombination must use the /64 of the device's own bridge — scoping
/// to `lan`'s assignment alone would rewrite a guest-profile rule into a /64
/// the device never owns. Empty when the router currently has no global prefix.
pub async fn read_gua_prefix_assignments() -> Vec<(String, Ipv6Addr, u8)> {
    let Ok(output) = tokio::process::Command::new("ubus")
        .args(["call", "network.interface", "dump"])
        .invoke(ErrorKind::Network.into())
        .await
    else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output) else {
        return Vec::new();
    };

    let mut assignments = Vec::new();
    for iface in json
        .get("interface")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        let Some(l3_device) = iface.get("l3_device").and_then(|v| v.as_str()) else {
            continue;
        };
        for entry in iface
            .get("ipv6-prefix-assignment")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
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
                assignments.push((l3_device.to_string(), addr, mask));
            }
        }
    }
    assignments
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
        return fs::read_to_string(&cert_path).map_err(|e| {
            Error::new(
                eyre!("failed to read intermediate cert: {e}"),
                ErrorKind::Filesystem,
            )
        });
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

pub(crate) type CaPair = (PKey<Private>, X509);

/// The root and intermediate CA, as written by [`ensure_root_ca`] and
/// [`ensure_intermediate_ca`].
pub(crate) fn read_ca_chain() -> Result<(CaPair, CaPair), Error> {
    let read = |path: PathBuf| {
        fs::read(&path).map_err(|e| {
            Error::new(
                eyre!("failed to read {}: {e}", path.display()),
                ErrorKind::Filesystem,
            )
        })
    };
    Ok((
        (
            PKey::private_key_from_pem(&read(ca_key_path())?)?,
            X509::from_pem(&read(ca_cert_path())?)?,
        ),
        (
            PKey::private_key_from_pem(&read(int_key_path())?)?,
            X509::from_pem(&read(int_cert_path())?)?,
        ),
    ))
}

/// Read the Root CA certificate PEM from disk.
pub fn read_root_ca_pem() -> Result<String, Error> {
    fs::read_to_string(ca_cert_path())
        .map_err(|e| Error::new(eyre!("failed to read CA cert: {e}"), ErrorKind::Filesystem))
}

/// Read the Root CA as an Apple configuration profile.
pub fn read_root_ca_mobileconfig() -> Result<String, Error> {
    let cert = X509::from_pem(read_root_ca_pem()?.as_bytes())?;
    startos::net::static_server::root_ca_mobileconfig(&cert, "StartWRT", ROUTER_HOSTNAME)
        .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::OpenSsl))
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
        let cn = cert
            .subject_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()
            .unwrap();
        assert!(
            cn.data()
                .to_string()
                .unwrap()
                .starts_with("StartWRT Local Root CA "),
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
                .to_string()
                .unwrap()
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
        let cn = int_cert
            .subject_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()
            .unwrap();
        assert_eq!(
            cn.data().to_string().unwrap(),
            "StartWRT Local Intermediate CA"
        );

        // Verify issuer matches root CA subject
        let ca = X509::from_pem(ca_cert.as_bytes()).unwrap();
        assert_eq!(
            int_cert
                .issuer_name()
                .entries()
                .next()
                .unwrap()
                .data()
                .to_string()
                .unwrap(),
            ca.subject_name()
                .entries()
                .next()
                .unwrap()
                .data()
                .to_string()
                .unwrap(),
        );

        // Verify it has AKI and SKI
        assert!(int_cert.subject_key_id().is_some(), "missing SKI");
        assert!(int_cert.authority_key_id().is_some(), "missing AKI");
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
