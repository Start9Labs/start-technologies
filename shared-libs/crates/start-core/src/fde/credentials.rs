use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use openssl::pkey::{PKey, Private, Public};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::db::model::DatabaseModel;
use crate::hostname::ServerHostname;
use crate::net::acme::{AcmeProvider, BootstrapAcme};
use crate::net::ssl::{CertificateIdentity, LocalIssuer};
use crate::prelude::*;
use crate::util::io::{delete_file, write_file_private_atomic};

const RUNTIME_PROFILES: &str = "/run/startos-fde/system-connections";
pub const SEALED_CREDENTIALS_FILE: &str = "etc/startos/fde-bootstrap.json";

/// Encrypted bootstrap identities and their TPM-sealed encryption key.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SealedCredentials {
    key: super::tpm::SealedKey,
    envelope: Vec<u8>,
}

impl SealedCredentials {
    /// The caller protects transient key handling and the TPM workspace.
    pub async fn seal(credentials: &Credentials, authority: &PKey<Private>) -> Result<Self, Error> {
        let key = super::bootstrap::generate_key()?;
        let envelope = credentials.encrypt(&key)?;
        Ok(Self {
            key: super::tpm::seal(&key, authority).await?,
            envelope,
        })
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
    pub network_profiles: BTreeMap<String, String>,
    pub network: Option<super::network::BootstrapNetwork>,
}

impl Credentials {
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

    pub fn capture_network(&mut self, db: &DatabaseModel) -> Result<(), Error> {
        self.network = Some(super::network::BootstrapNetwork::capture(db)?);
        Ok(())
    }

    pub async fn capture_network_profiles(&mut self) -> Result<(), Error> {
        let profiles = crate::net::wifi::capture_network_profiles().await?;
        for contents in self.network_profiles.values_mut() {
            contents.zeroize();
        }
        self.network_profiles = profiles;
        Ok(())
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

    async fn install_profiles_at(&self, directory: &Path) -> Result<(), Error> {
        tokio::fs::create_dir_all(directory).await?;
        for (name, contents) in &self.network_profiles {
            write_file_private_atomic(profile_path(directory, name), contents).await?;
        }
        Ok(())
    }

    async fn remove_profiles_at(&self, directory: &Path) -> Result<(), Error> {
        for name in self.network_profiles.keys() {
            delete_file(profile_path(directory, name)).await?;
        }
        Ok(())
    }
}

impl Drop for Credentials {
    fn drop(&mut self) {
        for profile in self.network_profiles.values_mut() {
            profile.zeroize();
        }
    }
}

fn profile_path(directory: &Path, name: &str) -> PathBuf {
    directory.join(format!(
        "startos-unlock-{}.nmconnection",
        blake3::hash(name.as_bytes()).to_hex(),
    ))
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
            "[wireguard]\nprivate-key=test-secret\n".to_owned(),
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
        let sealed = SealedCredentials::seal(&original, &authority)
            .await
            .unwrap();
        let mut decoded: SealedCredentials =
            serde_json::from_slice(&serde_json::to_vec(&sealed).unwrap()).unwrap();
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

    #[test]
    fn encrypted_credentials_round_trip_without_a_data_password() {
        let original = credentials();
        let key = super::super::bootstrap::generate_key().unwrap();
        let envelope = original.encrypt(&key).unwrap();
        let restored = Credentials::decrypt(&envelope, &key).unwrap();
        assert_eq!(restored.disk_guid, original.disk_guid);
        assert_eq!(restored.hostname.as_ref(), original.hostname.as_ref());
        assert_eq!(restored.network_profiles, original.network_profiles);
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
        assert_eq!(credentials.network_profiles.len(), 1);
        credentials.install_profiles_at(&runtime).await.unwrap();
        let path = profile_path(&runtime, "wired.nmconnection");
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

    #[test]
    fn profile_names_cannot_escape_the_runtime_directory() {
        let directory = Path::new("/run/NetworkManager/system-connections");
        for name in [
            "../outside",
            "/etc/shadow",
            "normal.nmconnection",
            &"a".repeat(1000),
        ] {
            let path = profile_path(directory, name);
            assert_eq!(path.parent(), Some(directory));
            assert!(path.file_name().unwrap().len() < 255);
        }
    }
}
