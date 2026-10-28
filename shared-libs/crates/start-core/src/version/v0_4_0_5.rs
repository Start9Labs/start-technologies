use exver::VersionRange;
use openssl::pkey::{PKey, Private};
use tokio::process::Command;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_4};
use crate::auth::AuthKeys;
use crate::context::RpcContext;
use crate::prelude::*;
use crate::sign::{AnyVerifyingKey, canonicalize_verifying_keys};
use crate::update::signers::TrustedOsUpdateSigners;
use crate::util::Invoke;
use crate::util::serde::{Der64, DerEncoding, Pem, PemEncoding};

lazy_static::lazy_static! {
    static ref V0_4_0_5: exver::Version = exver::Version::new([0, 4, 0, 5], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_4::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0_5.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn migration_revision(self) -> usize {
        2
    }
    fn up(self, db: &mut Value, _: ()) -> Result<Value, Error> {
        if let Some(keys) = field(db, &["private", "sessionPubkeys"]) {
            *keys = to_value(&from_value::<AuthKeys>(keys.clone())?)?;
        }
        if let Some(keys) = field(db, &["public", "serverInfo", "trustedOsUpdateSigners"]) {
            *keys = to_value(&canonicalize_verifying_keys(from_value::<
                TrustedOsUpdateSigners,
            >(keys.clone())?)?)?;
        }
        Ok(Value::Null)
    }
    async fn post_up(self, _ctx: &RpcContext, _input: Value) -> Result<(), Error> {
        start_initial_balance(Command::new("systemctl")).await;
        Ok(())
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        for path in [
            &["private", "sessionPubkeys"][..],
            &["public", "serverInfo", "trustedOsUpdateSigners"][..],
        ] {
            if let Some(keys) = field(db, path).and_then(Value::as_object_mut) {
                *keys = keys
                    .iter()
                    .map(|(key, value)| {
                        Ok((
                            InternedString::from_display(&key.parse::<AnyVerifyingKey>()?),
                            value.clone(),
                        ))
                    })
                    .collect::<Result<_, Error>>()?;
            }
        }
        restore_pem::<ed25519_dalek::SigningKey>(field(db, &["private", "developerKey"]))?;
        if let Some(packages) = field(db, &["public", "packageData"]).and_then(Value::as_object_mut)
        {
            for (_, package) in packages.iter_mut() {
                restore_pem::<ed25519_dalek::VerifyingKey>(package.get_mut("developerKey"))?;
            }
        }
        if let Some(local) = field(db, &["private", "keyStore", "localCerts"]) {
            for name in ["rootKey", "intKey"] {
                restore_pem::<PKey<Private>>(local.get_mut(name))?;
            }
            if let Some(leaves) = local.get_mut("leaves").and_then(Value::as_object_mut) {
                for (_, leaf) in leaves.iter_mut() {
                    for name in ["ed25519", "nistp256"] {
                        restore_pem::<PKey<Private>>(field(leaf, &["keys", name]))?;
                    }
                }
            }
        }
        if let Some(acme) = field(db, &["private", "keyStore", "acme"]) {
            if let Some(accounts) = acme.get_mut("accounts").and_then(Value::as_object_mut) {
                for (_, account) in accounts.iter_mut() {
                    let document: Der64<pkcs8::Document> = from_value(account.clone())?;
                    *account = Value::from(
                        der::pem::encode_string(
                            "EC PRIVATE KEY",
                            der::pem::LineEnding::LF,
                            document.as_bytes(),
                        )
                        .with_kind(ErrorKind::Serialization)?,
                    );
                }
            }
            if let Some(directories) = acme.get_mut("certs").and_then(Value::as_object_mut) {
                for (_, certificates) in directories.iter_mut() {
                    if let Some(certificates) = certificates.as_object_mut() {
                        for (_, certificate) in certificates.iter_mut() {
                            restore_pem::<PKey<Private>>(certificate.get_mut("key"))?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

async fn start_initial_balance(mut systemctl: Command) {
    if let Err(error) = systemctl
        .args(["start", "--no-block", "btrfs-balance.service"])
        .invoke(ErrorKind::DiskManagement)
        .await
    {
        tracing::warn!(?error, "{}", t!("migration.initial-balance-failed"));
    }
}

fn field<'a>(value: &'a mut Value, path: &[&str]) -> Option<&'a mut Value> {
    path.iter()
        .try_fold(value, |value, name| value.get_mut(*name))
}

fn restore_pem<T: DerEncoding + PemEncoding>(value: Option<&mut Value>) -> Result<(), Error> {
    if let Some(value) = value {
        let key: Der64<T> = from_value(value.clone())?;
        *value = to_value(&Pem(key.0))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use imbl_value::json;

    use super::*;
    use crate::auth::Session;
    use crate::version::Current;

    #[tokio::test]
    async fn prior_revisions_migrate_keys_and_requeue_post_up_once() {
        use patch_db::json_ptr::ROOT;
        let key = AnyVerifyingKey::Ed25519(
            ed25519_dalek::SigningKey::from_bytes(&[43; 32]).verifying_key(),
        );
        let canonical = key.interned_der64();
        let account =
            crate::account::AccountInfo::new("password", std::time::SystemTime::now(), None)
                .unwrap();
        for (revision, key_id) in [
            (0, key.to_string()),
            (1, key.to_string()),
            (1, canonical.to_string()),
        ] {
            let mut fixture =
                to_value(&crate::db::model::Database::init(&account, false, None, None).unwrap())
                    .unwrap();
            fixture["public"]["serverInfo"]["latestMigrationRevision"] = json!(revision);
            fixture["public"]["serverInfo"]["postInitMigrationTodos"] = json!({});
            fixture["public"]["serverInfo"]["trustedOsUpdateSigners"] =
                json!({ key_id.clone(): { "label": "retained signer" } });
            fixture["public"]["serverInfo"]["version"] = to_value(&Version.semver()).unwrap();
            let private = fixture["private"].as_object_mut().unwrap();
            private.remove("sessions");
            private.insert(
                "sessionPubkeys".into(),
                json!({ key_id: Session::default() }),
            );
            let db = PatchDb::in_memory();
            db.put(&ROOT, &fixture).await.unwrap();
            Current::default().pre_init(&db).await.unwrap();
            let migrated = db.dump(&ROOT).await.value;
            assert_eq!(
                migrated["public"]["serverInfo"]["latestMigrationRevision"],
                json!(Current::default().migration_revision())
            );
            assert_eq!(
                migrated["public"]["serverInfo"]["postInitMigrationTodos"],
                json!({ "0.4.0.5": null, "0.4.1": null })
            );
            assert!(
                migrated["private"]["sessions"]["pubkeys"]
                    .as_object()
                    .unwrap()
                    .contains_key(&canonical)
            );
            assert_eq!(
                migrated["public"]["serverInfo"]["trustedOsUpdateSigners"][&*canonical]["label"],
                json!("retained signer")
            );
            Current::default().pre_init(&db).await.unwrap();
            assert_eq!(db.dump(&ROOT).await.value, migrated);
            let mut completed = migrated;
            completed["public"]["serverInfo"]["postInitMigrationTodos"] = json!({});
            db.put(&ROOT, &completed).await.unwrap();
            Current::default().pre_init(&db).await.unwrap();
            assert_eq!(db.dump(&ROOT).await.value, completed);
        }
    }

    #[test]
    fn combined_migration_advances_the_published_revision() {
        assert_eq!(Version.migration_revision(), 2);
    }

    #[tokio::test]
    async fn initial_balance_queues_the_existing_service_without_failing_startup()
    -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let systemctl = temp.path().join("systemctl");
        let calls = temp.path().join("calls");
        std::fs::write(
            &systemctl,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CALL_LOG\"\nexit \"$EXIT_STATUS\"\n",
        )?;
        std::fs::set_permissions(&systemctl, std::fs::Permissions::from_mode(0o755))?;
        for status in [0, 7] {
            let mut command = Command::new(&systemctl);
            command
                .env("CALL_LOG", &calls)
                .env("EXIT_STATUS", status.to_string());
            let () = start_initial_balance(command).await;
            assert_eq!(
                std::fs::read_to_string(&calls)?,
                "start\n--no-block\nbtrfs-balance.service\n"
            );
        }
        Ok(())
    }

    #[test]
    fn downgrade_restores_tls_and_acme_private_keys() {
        use crate::net::ssl::PKeyPair;
        let ed25519 = PKey::generate_ed25519().unwrap();
        let nistp256 = crate::net::ssl::gen_nistp256().unwrap();
        let sec1 = nistp256.ec_key().unwrap().private_key_to_der().unwrap();
        let document = pkcs8::Document::try_from(sec1.as_slice()).unwrap();
        let mut db = json!({ "private": { "keyStore": {
            "localCerts": {
                "rootKey": Der64(nistp256.clone()),
                "intKey": Der64(nistp256.clone()),
                "leaves": { "names": { "keys": PKeyPair { ed25519: ed25519.clone(), nistp256: nistp256.clone() } } },
            },
            "acme": {
                "accounts": { "contacts": Der64(document) },
                "certs": { "directory": { "names": { "key": Der64(nistp256.clone()) } } },
            },
        } } });
        Version.down(&mut db).unwrap();
        for path in [
            &["private", "keyStore", "localCerts", "rootKey"][..],
            &["private", "keyStore", "localCerts", "intKey"][..],
            &[
                "private",
                "keyStore",
                "localCerts",
                "leaves",
                "names",
                "keys",
                "nistp256",
            ][..],
            &[
                "private",
                "keyStore",
                "acme",
                "certs",
                "directory",
                "names",
                "key",
            ][..],
        ] {
            let key: Pem<PKey<Private>> =
                from_value(field(&mut db, path).unwrap().clone()).unwrap();
            assert!(key.public_eq(&nistp256));
        }
        let key: Pem<PKey<Private>> = from_value(
            db["private"]["keyStore"]["localCerts"]["leaves"]["names"]["keys"]["ed25519"].clone(),
        )
        .unwrap();
        assert!(key.public_eq(&ed25519));
        let account = db["private"]["keyStore"]["acme"]["accounts"]["contacts"]
            .as_str()
            .unwrap();
        let (label, decoded) = der::pem::decode_vec(account.as_bytes()).unwrap();
        assert_eq!(label, "EC PRIVATE KEY");
        assert_eq!(decoded, sec1);
    }

    #[test]
    fn signer_indexes_upgrade_idempotently_and_downgrade_to_pem() {
        let signing = ed25519_dalek::SigningKey::from_bytes(&[43; 32]);
        let key = AnyVerifyingKey::Ed25519(signing.verifying_key());
        let session = Session {
            name: Some("retained sign-in".into()),
            logged_in: chrono::Utc::now(),
            last_active: chrono::Utc::now(),
            user_agent: Some("test".into()),
        };
        let mut db = json!({
            "private": {
                "sessionPubkeys": { key.to_string(): session },
                "developerKey": Pem(signing.clone()),
            },
            "public": {
                "serverInfo": { "trustedOsUpdateSigners": { key.to_string(): { "label": "retained signer" } } },
                "packageData": { "test": { "developerKey": Pem(signing.verifying_key()) } },
            },
        });
        let original = db.clone();
        Version.up(&mut db, ()).unwrap();
        let migrated = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(db, migrated);
        assert_eq!(
            db["private"]["sessionPubkeys"][&*key.interned_der64()],
            original["private"]["sessionPubkeys"][&key.to_string()]
        );
        db["private"]["developerKey"] = to_value(&Der64(signing.clone())).unwrap();
        db["public"]["packageData"]["test"]["developerKey"] =
            to_value(&Der64(signing.verifying_key())).unwrap();
        Version.down(&mut db).unwrap();
        assert_eq!(db, original);
    }
}
