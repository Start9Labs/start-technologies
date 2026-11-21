use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use openssl::pkey::{PKey, Private, Public};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::db::model::DatabaseModel;
use crate::hostname::ServerHostname;
use crate::net::acme::{AcmeProvider, BootstrapAcme};
use crate::net::ssl::{CertificateIdentity, LocalIssuer};
use crate::prelude::*;
use crate::util::io::delete_file;

const RUNTIME_PROFILES: &str = "/run/startos-fde/system-connections";
pub const SEALED_CREDENTIALS_FILE: &str = "etc/startos/fde-bootstrap.json";

/// Encrypted bootstrap identities and their TPM-sealed encryption key.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SealedCredentials {
    key: super::tpm::SealedKey,
    envelope: Vec<u8>,
}

/// The reserved bootstrap file cannot own unrelated hardlink aliases.
fn admit_bootstrap_path(
    snapshot: &crate::util::squashfs::Squashfs<crate::s9pk::merkle_archive::source::DynFileSource>,
) -> Result<(), Error> {
    use crate::util::squashfs::{Entry, snapshot_entries};
    let path = Path::new(SEALED_CREDENTIALS_FILE);
    if snapshot
        .contents()
        .get_path_checked(path)?
        .is_some_and(|entry| !entry.is_file_or_hardlink())
        || snapshot_entries(snapshot)
            .iter()
            .any(|(_, entry)| matches!(entry, Entry::Hardlink(target) if target == path))
    {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput).into());
    }
    Ok(())
}

impl SealedCredentials {
    /// The caller protects transient keys and retains a tmpfs workspace base.
    pub async fn seal(
        credentials: &Credentials,
        authority: &PKey<Private>,
        workspace_base: &Path,
    ) -> Result<Self, Error> {
        let key = super::bootstrap::generate_key()?;
        let envelope = credentials.encrypt(&key)?;
        Ok(Self {
            key: super::tpm::seal(&key, authority, workspace_base).await?,
            envelope,
        })
    }

    pub(super) fn clear_previous(
        snapshot: &mut crate::util::squashfs::Squashfs<
            crate::s9pk::merkle_archive::source::DynFileSource,
        >,
    ) -> Result<(), Error> {
        admit_bootstrap_path(snapshot)?;
        snapshot
            .contents_mut()
            .remove_path(SEALED_CREDENTIALS_FILE)?;
        Ok(())
    }

    pub(crate) fn insert_into(
        self,
        snapshot: &mut crate::util::squashfs::Squashfs<
            crate::s9pk::merkle_archive::source::DynFileSource,
        >,
    ) -> Result<(), Error> {
        use crate::s9pk::merkle_archive::source::DynFileSource;
        use crate::util::squashfs::{Entry, Metadata, Node, NodeContents};

        admit_bootstrap_path(snapshot)?;
        let bytes: std::sync::Arc<[u8]> = serde_json::to_vec(&self)
            .with_kind(ErrorKind::Serialization)?
            .into();
        snapshot.contents_mut().insert_path(
            Path::new(SEALED_CREDENTIALS_FILE),
            Entry::Node(Node {
                metadata: Metadata::new(0o600),
                contents: NodeContents::File(DynFileSource::new(bytes)),
            }),
        )?;
        Ok(())
    }

    pub(super) async fn unseal(
        &self,
        authorization: &super::tpm::Authorization,
        authority: &PKey<Public>,
    ) -> Result<Credentials, Error> {
        let key = super::tpm::unseal(&self.key, authorization, authority).await?;
        Credentials::decrypt(&self.envelope, &key)
    }
}

/// Bootstrap connection identities, without data-volume unlock credentials.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Credentials {
    pub disk_guid: InternedString,
    pub hostname: ServerHostname,
    pub identities: BTreeMap<JsonKey<BTreeSet<InternedString>>, CertificateIdentity>,
    #[serde(default)]
    pub local_issuer: Option<LocalIssuer>,
    #[serde(default)]
    pub acme: BootstrapAcme,
    pub network_profiles: crate::net::wifi::NetworkProfiles,
    pub network: Option<super::network::BootstrapNetwork>,
}

impl Credentials {
    /// Reads bootstrap UI authority and saved intent without issuing certificates or mutating state.
    pub(crate) fn capture(db: &DatabaseModel, disk_guid: InternedString) -> Result<Self, Error> {
        let hostname = ServerHostname::load(db.as_public().as_server_info())?;
        let mut network = super::network::BootstrapNetwork::capture(db)?;
        let mut host = Model::new(&network.host)?;
        host.update_addresses(&hostname, &network.gateways, &network.ports)?;
        network.host = host.de()?;
        let root: crate::util::serde::Pem<openssl::x509::X509> = db
            .as_private()
            .as_key_store()
            .as_local_certs()
            .as_root_cert()
            .de()?;
        let client = std::sync::Arc::new(crate::net::tls::client_config(
            std::sync::Arc::new(tokio_rustls::rustls::crypto::ring::default_provider()),
            [&*root.0],
        )?);
        let plan = crate::net::net_controller::plan_host_exposure(
            &network.host,
            &network.gateways,
            std::net::Ipv4Addr::LOCALHOST,
            None,
            |_| client.clone(),
        );
        let providers = plan
            .vhosts
            .into_iter()
            .filter_map(|((name, _, _), target)| {
                Some((JsonKey::new(BTreeSet::from([name?])), target.acme?))
            })
            .collect();
        let mut credentials = Self {
            disk_guid,
            hostname,
            identities: BTreeMap::new(),
            local_issuer: None,
            acme: BootstrapAcme::default(),
            network_profiles: BTreeMap::new(),
            network: Some(network),
        };
        credentials.capture_tls(db, providers)?;
        Ok(credentials)
    }

    /// The caller supplies enabled ACME bindings for the bootstrap host.
    pub fn capture_tls(
        &mut self,
        db: &DatabaseModel,
        providers: BTreeMap<JsonKey<BTreeSet<InternedString>>, AcmeProvider>,
    ) -> Result<(), Error> {
        let issuer = db.as_private().as_key_store().as_local_certs().issuer()?;
        let acme = BootstrapAcme::capture(db, providers)?;
        self.local_issuer = Some(issuer);
        self.acme = acme;
        Ok(())
    }

    pub fn encrypt(&self, key: &[u8; 32]) -> Result<Vec<u8>, Error> {
        let plaintext =
            Zeroizing::new(serde_json::to_vec(self).with_kind(ErrorKind::Serialization)?);
        super::bootstrap::encrypt(&plaintext, key)
    }

    pub fn decrypt(envelope: &[u8], key: &[u8; 32]) -> Result<Self, Error> {
        let plaintext = super::bootstrap::decrypt(envelope, key)?;
        serde_json::from_slice(&plaintext).with_kind(ErrorKind::Deserialization)
    }

    /// Writes credentials beneath `/run`; the saved data-drive profiles remain authoritative.
    pub async fn install_network_profiles(&self) -> Result<PathBuf, Error> {
        let directory = Path::new(RUNTIME_PROFILES);
        self.install_profiles_at(directory).await?;
        Ok(directory.to_owned())
    }

    pub async fn remove_network_profiles(&self) -> Result<(), Error> {
        self.remove_profiles_at(Path::new(RUNTIME_PROFILES)).await
    }

    pub(super) async fn install_profiles_at(&self, directory: &Path) -> Result<(), Error> {
        tokio::fs::create_dir_all(directory).await?;
        for (name, profile) in &self.network_profiles {
            profile.install_at(&profile_path(directory, name)?).await?;
        }
        Ok(())
    }

    async fn remove_profiles_at(&self, directory: &Path) -> Result<(), Error> {
        for name in self.network_profiles.keys() {
            delete_file(profile_path(directory, name)?).await?;
        }
        Ok(())
    }
}

fn profile_path(directory: &Path, name: &str) -> Result<PathBuf, Error> {
    if !crate::net::wifi::is_network_profile_name(std::ffi::OsStr::new(name)) {
        return Err(Error::new(
            eyre!(t!("fde.bootstrap.invalid-profile-name")),
            ErrorKind::Filesystem,
        ));
    }
    Ok(directory.join(name))
}

#[cfg(test)]
pub(super) fn test_credentials() -> Credentials {
    Credentials {
        disk_guid: InternedString::intern("STARTOS_TEST"),
        hostname: ServerHostname::new_from_input(InternedString::intern("fde-test")).unwrap(),
        identities: BTreeMap::new(),
        local_issuer: None,
        acme: BootstrapAcme::default(),
        network: None,
        network_profiles: [(
            "private.nmconnection".to_owned(),
            crate::net::wifi::NetworkProfile {
                contents: Zeroizing::new("[wireguard]\nprivate-key=test-secret\n".to_owned()),
                modified: (1_700_000_000, 123_456_789),
            },
        )]
        .into_iter()
        .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::{test_credentials as credentials, *};

    #[tokio::test]
    #[ignore = "requires an exclusively owned software TPM and tpm2-tools"]
    async fn software_tpm_sealed_snapshot_round_trips_and_rejects_tampering() {
        let authority = crate::util::crypto::gen_nistp256().unwrap();
        let public = PKey::public_key_from_der(&authority.public_key_to_der().unwrap()).unwrap();
        let original = credentials();
        let sealed = SealedCredentials::seal(&original, &authority, Path::new("/var/tmp"))
            .await
            .unwrap();
        let bytes = serde_json::to_vec(&sealed).unwrap();
        let mut snapshot = crate::util::squashfs::Squashfs::default();
        serde_json::from_slice::<SealedCredentials>(&bytes)
            .unwrap()
            .insert_into(&mut snapshot)
            .unwrap();
        let entry = snapshot
            .contents()
            .get_path(Path::new(SEALED_CREDENTIALS_FILE))
            .unwrap();
        let crate::util::squashfs::Entry::Node(node) = entry else {
            panic!("sealed payload is a regular file")
        };
        let crate::util::squashfs::NodeContents::File(file) = &node.contents else {
            panic!("sealed payload is a regular file")
        };
        use crate::s9pk::merkle_archive::source::FileSource;
        let staged = file.to_vec(None).await.unwrap();
        assert_eq!(staged, bytes);
        assert_eq!(node.metadata.mode, 0o600);
        assert!(
            !staged
                .windows(b"test-secret".len())
                .any(|window| window == b"test-secret")
        );
        let mut decoded: SealedCredentials = serde_json::from_slice(&staged).unwrap();
        let authorization = super::super::tpm::authorize(
            &super::super::tpm::read_measurements().await.unwrap(),
            &authority,
        )
        .await
        .unwrap();
        let round_trip = decoded.unseal(&authorization, &public).await.unwrap();
        assert_eq!(
            serde_json::to_value(&round_trip).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        *decoded.envelope.last_mut().unwrap() ^= 1;
        assert!(decoded.unseal(&authorization, &public).await.is_err());
        let mut extra = serde_json::to_value(&sealed).unwrap();
        extra["diskPassword"] = serde_json::json!("not-a-bootstrap-field");
        assert!(serde_json::from_value::<SealedCredentials>(extra).is_err());
    }

    fn snapshot_database() -> crate::db::model::DatabaseModel {
        use crate::net::forward::AvailablePorts;
        use crate::net::host::address::PublicDomainConfig;
        use crate::net::host::binding::BindOptions;
        use crate::net::service_interface::{AddressInfo, ServiceInterface, ServiceInterfaceType};

        let account = crate::account::AccountInfo::new(
            "account-only-password",
            std::time::SystemTime::now(),
            None,
        )
        .unwrap();
        let mut db =
            Model::new(&crate::db::model::Database::init(&account, false, None, None).unwrap())
                .unwrap();
        let mut ports = AvailablePorts::new();
        let mut host = Model::new(&crate::net::host::Host::new()).unwrap();
        host.add_binding(&mut ports, 80, BindOptions::server_ui(), true)
            .unwrap();
        host.add_binding(&mut ports, 8080, BindOptions::server_ui(), true)
            .unwrap();
        let mut host = host.de().unwrap();
        let id = "admin-ui".parse().unwrap();
        host.bindings.get_mut(&80).unwrap().interfaces.insert(
            id,
            ServiceInterface {
                id: "admin-ui".parse().unwrap(),
                name: "StartOS UI".into(),
                description: String::new(),
                masked: false,
                address_info: AddressInfo {
                    username: None,
                    host_id: crate::HostId::admin(),
                    internal_port: 80,
                    scheme: Some("http".into()),
                    ssl_scheme: Some("https".into()),
                    suffix: String::new(),
                },
                interface_type: ServiceInterfaceType::Ui,
                preferred_launcher_address: None,
            },
        );
        for name in ["enabled.example", "disabled.example"] {
            host.public_domains.insert(
                name.into(),
                PublicDomainConfig {
                    gateway: "eth0".parse().unwrap(),
                    acme: Some("letsencrypt".parse().unwrap()),
                },
            );
        }
        let hostname = ServerHostname::load(db.as_public().as_server_info()).unwrap();
        let gateways = db
            .as_public()
            .as_server_info()
            .as_network()
            .as_gateways()
            .de()
            .unwrap();
        let mut derived = Model::new(&host).unwrap();
        derived
            .update_addresses(&hostname, &gateways, &ports)
            .unwrap();
        host = derived.de().unwrap();
        let ssl = host.bindings[&80].net.assigned_ssl_port.unwrap();
        let choices = &mut host.bindings.get_mut(&80).unwrap().addresses.disabled;
        choices.remove(&("enabled.example".into(), ssl));
        choices.insert(("disabled.example".into(), ssl));
        let mut derived = Model::new(&host).unwrap();
        derived
            .update_addresses(&hostname, &gateways, &ports)
            .unwrap();
        host = derived.de().unwrap();
        let network = db.as_public_mut().as_server_info_mut().as_network_mut();
        network.as_host_mut().ser(&host).unwrap();
        network
            .as_acme_mut()
            .insert(
                &"letsencrypt".parse().unwrap(),
                &crate::db::model::public::AcmeSettings {
                    contact: vec!["mailto:bootstrap@example.org".into()],
                },
            )
            .unwrap();
        db.as_private_mut()
            .as_available_ports_mut()
            .ser(&ports)
            .unwrap();
        db
    }

    #[test]
    fn captured_bootstrap_uses_ui_intent_and_intermediate_authority_without_mutating_database() {
        let db = snapshot_database();
        let before = serde_json::to_value(db.de().unwrap()).unwrap();
        let credentials = Credentials::capture(&db, "bootstrap-disk".into()).unwrap();
        assert_eq!(&*credentials.disk_guid, "bootstrap-disk");
        assert_eq!(
            credentials
                .acme
                .providers
                .keys()
                .map(|names| names.iter().map(|name| &**name).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            vec![vec!["enabled.example"]]
        );
        assert_eq!(credentials.acme.settings.len(), 1);
        assert_eq!(
            credentials
                .network
                .as_ref()
                .unwrap()
                .host
                .bindings
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![80]
        );
        assert!(credentials.network_profiles.is_empty());
        let captured = serde_json::to_value(&credentials).unwrap();
        let root_key = serde_json::to_string(
            &db.as_private()
                .as_key_store()
                .as_local_certs()
                .as_root_key()
                .de()
                .unwrap(),
        )
        .unwrap();
        assert!(
            !serde_json::to_string(&credentials)
                .unwrap()
                .contains(&root_key)
        );
        assert!(captured.get("password").is_none());
        assert!(captured.get("fdeEnrollment").is_none());
        assert!(captured.get("luksCredentials").is_none());
        assert!(credentials.local_issuer.is_some());
        assert_eq!(serde_json::to_value(db.de().unwrap()).unwrap(), before);
    }

    #[test]
    fn disabled_bootstrap_ui_does_not_capture_acme_provider_authority() {
        let mut db = snapshot_database();
        let mut bindings = db
            .as_public()
            .as_server_info()
            .as_network()
            .as_host()
            .as_bindings()
            .de()
            .unwrap();
        bindings.get_mut(&80).unwrap().enabled = false;
        db.as_public_mut()
            .as_server_info_mut()
            .as_network_mut()
            .as_host_mut()
            .as_bindings_mut()
            .ser(&bindings)
            .unwrap();
        let credentials = Credentials::capture(&db, "bootstrap-disk".into()).unwrap();
        assert!(credentials.acme.providers.is_empty());
        assert!(credentials.acme.settings.is_empty());
        assert!(credentials.acme.cache.accounts.is_empty());
    }

    #[test]
    fn bootstrap_file_admission_preserves_unrelated_nodes_and_hardlink_aliases() {
        use crate::s9pk::merkle_archive::source::DynFileSource;
        use crate::util::squashfs::{Entry, Metadata, NodeContents, Squashfs};
        let mut snapshot = Squashfs::<DynFileSource>::default();
        assert!(admit_bootstrap_path(&snapshot).is_ok());
        let file = Entry::new(
            Metadata::new(0o600),
            NodeContents::File(DynFileSource::new(std::sync::Arc::<[u8]>::from(
                &b"previous envelope"[..],
            ))),
        );
        snapshot
            .contents_mut()
            .insert_path(SEALED_CREDENTIALS_FILE, file.clone())
            .unwrap();
        assert!(admit_bootstrap_path(&snapshot).is_ok());
        snapshot
            .contents_mut()
            .insert_path(
                "etc/dependent",
                Entry::Hardlink(SEALED_CREDENTIALS_FILE.into()),
            )
            .unwrap();
        assert!(admit_bootstrap_path(&snapshot).is_err());
        for contents in [
            NodeContents::Directory(Default::default()),
            NodeContents::Symlink("/private/file".into()),
        ] {
            let mut snapshot = Squashfs::<DynFileSource>::default();
            snapshot
                .contents_mut()
                .insert_path(
                    SEALED_CREDENTIALS_FILE,
                    Entry::new(Metadata::new(0o700), contents),
                )
                .unwrap();
            assert!(admit_bootstrap_path(&snapshot).is_err());
        }
        let mut snapshot = Squashfs::<DynFileSource>::default();
        snapshot
            .contents_mut()
            .insert_path("etc/original", file)
            .unwrap();
        snapshot
            .contents_mut()
            .insert_path(
                SEALED_CREDENTIALS_FILE,
                Entry::Hardlink("etc/original".into()),
            )
            .unwrap();
        assert!(admit_bootstrap_path(&snapshot).is_ok());
    }

    #[test]
    fn encrypted_credentials_round_trip_without_a_data_password() {
        let original = credentials();
        let key = super::super::bootstrap::generate_key().unwrap();
        let envelope = original.encrypt(&key).unwrap();
        let restored = Credentials::decrypt(&envelope, &key).unwrap();
        assert_eq!(restored.disk_guid, original.disk_guid);
        assert_eq!(restored.hostname.as_ref(), original.hostname.as_ref());
        assert_eq!(
            restored
                .network_profiles
                .get("private.nmconnection")
                .unwrap()
                .contents
                .as_str(),
            "[wireguard]\nprivate-key=test-secret\n"
        );
        assert_eq!(
            restored.network_profiles.len(),
            original.network_profiles.len()
        );
        assert!(
            !envelope
                .windows(b"test-secret".len())
                .any(|bytes| bytes == b"test-secret")
        );
    }

    #[test]
    fn leaf_identity_and_issuer_survive_bootstrap_encryption_without_root_signing_key() {
        use std::sync::Arc;
        use std::time::SystemTime;

        use patch_db::ModelExt;

        use crate::account::AccountInfo;
        use crate::net::ssl::{CertBranding, CertStore, SANInfo, gen_nistp256, make_leaf_cert};
        use crate::util::serde::{Der64, Pem};

        let mut original = credentials();
        let account = AccountInfo::new("test-password", SystemTime::now(), None).unwrap();
        let branding = CertBranding::start_os(original.hostname.as_ref());
        let store = CertStore::new(&account, &branding).unwrap();
        let database = Model::<crate::db::model::Database>::from_value(imbl_value::json!({
            "private": {"keyStore": {"localCerts": store, "acme": crate::net::acme::AcmeCertStore::new()}},
            "public": {"serverInfo": {"network": {"acme": {}}}},
        }));
        original.capture_tls(&database, BTreeMap::new()).unwrap();
        let root_key = serde_json::to_string(&store.root_key).unwrap();
        assert!(
            !serde_json::to_string(&original)
                .unwrap()
                .contains(&root_key)
        );
        let leaf_key = gen_nistp256().unwrap();
        let names = [original.hostname.local_domain_name()]
            .into_iter()
            .collect();
        let leaf = make_leaf_cert(
            (&store.int_key.0, &store.int_cert.0),
            (&leaf_key, &SANInfo::new(&names)),
            &branding,
        )
        .unwrap();
        original.identities.insert(
            JsonKey::new(names),
            CertificateIdentity {
                key: Der64(leaf_key),
                fullchain: vec![Pem(leaf), store.int_cert, store.root_cert],
            },
        );
        let key = super::super::bootstrap::generate_key().unwrap();
        let restored = Credentials::decrypt(&original.encrypt(&key).unwrap(), &key).unwrap();
        assert_eq!(
            restored
                .local_issuer
                .as_ref()
                .unwrap()
                .intermediate_key
                .0
                .private_key_to_pkcs8()
                .unwrap(),
            store.int_key.0.private_key_to_pkcs8().unwrap(),
        );
        let identity = restored.identities.values().next().unwrap();
        assert_eq!(
            identity.key.0.public_key_to_der().unwrap(),
            original
                .identities
                .values()
                .next()
                .unwrap()
                .key
                .0
                .public_key_to_der()
                .unwrap(),
        );
        assert_eq!(identity.fullchain.len(), 3);
        identity
            .server_config(Arc::new(
                tokio_rustls::rustls::crypto::ring::default_provider(),
            ))
            .unwrap();
    }

    #[tokio::test]
    async fn bootstrap_profiles_are_private_and_cleanup_preserves_other_profiles() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = directory.path().join("runtime");
        let saved = directory.path().join("saved");
        tokio::fs::create_dir_all(&saved).await.unwrap();
        tokio::fs::write(
            saved.join("wired.nmconnection"),
            "[connection]\nuuid=saved\n",
        )
        .await
        .unwrap();
        tokio::fs::set_permissions(
            saved.join("wired.nmconnection"),
            std::fs::Permissions::from_mode(0o600),
        )
        .await
        .unwrap();
        tokio::fs::write(saved.join("ignored.txt"), "not a profile")
            .await
            .unwrap();
        symlink(
            saved.join("wired.nmconnection"),
            saved.join("link.nmconnection"),
        )
        .unwrap();
        let mut credentials = credentials();
        credentials.network_profiles = crate::net::wifi::read_network_profiles(&saved)
            .await
            .unwrap();
        assert_eq!(credentials.network_profiles.len(), 2);
        credentials.install_profiles_at(&runtime).await.unwrap();
        let path = profile_path(&runtime, "wired.nmconnection").unwrap();
        assert_eq!(path.file_name().unwrap(), "wired.nmconnection");
        assert_eq!(
            tokio::fs::read_to_string(&path).await.unwrap(),
            "[connection]\nuuid=saved\n"
        );
        assert_eq!(
            tokio::fs::metadata(&path)
                .await
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        tokio::fs::write(runtime.join("unrelated.nmconnection"), "keep")
            .await
            .unwrap();
        credentials.remove_profiles_at(&runtime).await.unwrap();
        assert!(!path.exists());
        assert_eq!(
            tokio::fs::read_to_string(runtime.join("unrelated.nmconnection"))
                .await
                .unwrap(),
            "keep"
        );
        assert!(saved.join("wired.nmconnection").exists());
    }

    #[tokio::test]
    async fn saved_profile_invalid_utf8_preserves_the_native_filesystem_error() {
        let directory = tempfile::tempdir().unwrap();
        tokio::fs::write(directory.path().join("invalid.nmconnection"), [0xff])
            .await
            .unwrap();
        tokio::fs::set_permissions(
            directory.path().join("invalid.nmconnection"),
            std::fs::Permissions::from_mode(0o600),
        )
        .await
        .unwrap();
        let error = crate::net::wifi::read_network_profiles(directory.path())
            .await
            .err()
            .unwrap();
        assert_eq!(error.kind, ErrorKind::Filesystem);
    }

    #[test]
    fn profile_names_cannot_escape_the_runtime_directory() {
        let directory = Path::new("/run/NetworkManager/system-connections");
        for name in [
            "../outside",
            "/etc/shadow",
            "",
            ".hidden",
            "../",
            "bad\0name",
        ] {
            assert!(profile_path(directory, name).is_err());
        }
        for name in ["normal.nmconnection", "legacy", "κλειδί"] {
            let path = profile_path(directory, name).unwrap();
            assert_eq!(path.parent(), Some(directory));
            assert_eq!(path.file_name().unwrap(), name);
        }
    }
}
