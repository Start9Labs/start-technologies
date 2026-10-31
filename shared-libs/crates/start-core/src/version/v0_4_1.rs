use exver::VersionRange;
use imbl_value::json;

use super::v0_3_5::V0_3_0_COMPAT;
use super::v0_4_0_2::for_each_host;
use super::{VersionT, v0_4_0_5};
use crate::prelude::*;
use crate::trusted_keys::{migrate_signers, os_signers};

lazy_static::lazy_static! {
    static ref V0_4_1: exver::Version = exver::Version::new([0, 4, 1], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_5::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_1.clone()
    }
    fn migration_revision(self) -> usize {
        2
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        let server = db
            .get_mut("public")
            .and_then(|public| public.get_mut("serverInfo"))
            .and_then(Value::as_object_mut)
            .or_not_found("serverInfo")?;
        if !server.contains_key("trustedKeys") {
            let legacy = server
                .get("trustedOsUpdateSigners")
                .cloned()
                .map(from_value)
                .transpose()?
                .unwrap_or_default();
            server.insert("trustedKeys".into(), to_value(&migrate_signers(legacy)?)?);
        }
        server.remove("trustedOsUpdateSigners");
        let private = db["private"].as_object_mut().or_not_found("private")?;
        if !private.contains_key("sessions") {
            let pubkeys = private
                .remove("sessionPubkeys")
                .unwrap_or_else(|| json!({}));
            private.insert(
                "sessions".into(),
                json!({ "pubkeys": pubkeys, "tokens": {} }),
            );
        }
        v0_4_0_5::initialize_security_notifications(db)?;
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        restore_legacy_http_bindings(db)?;
        let server = db
            .get_mut("public")
            .and_then(|public| public.get_mut("serverInfo"))
            .and_then(Value::as_object_mut)
            .or_not_found("serverInfo")?;
        if let Some(keys) = server.get("trustedKeys") {
            let signers = to_value(&os_signers(&from_value(keys.clone())?))?;
            server.remove("trustedKeys");
            server.insert("trustedOsUpdateSigners".into(), signers);
        }
        let private = db["private"].as_object_mut().or_not_found("private")?;
        let pubkeys = private
            .remove("sessions")
            .map(|sessions| sessions["pubkeys"].clone())
            .unwrap_or_else(|| json!({}));
        private.insert("sessionPubkeys".into(), pubkeys);
        Ok(())
    }
}

fn restore_legacy_http_bindings(db: &mut Value) -> Result<(), Error> {
    let mut result = Ok(());
    for_each_host(db, |host| {
        if result.is_err() {
            return;
        }
        let Some(bindings) = host.get_mut("bindings").and_then(Value::as_object_mut) else {
            return;
        };
        for (_, binding) in bindings.iter_mut() {
            let Some(options) = binding.get_mut("options").and_then(Value::as_object_mut) else {
                continue;
            };
            if let Err(error) = restore_legacy_http_options(options) {
                result = Err(error);
                return;
            }
        }
    });
    result
}

fn restore_legacy_http_options(
    options: &mut imbl_value::InOMap<InternedString, Value>,
) -> Result<(), Error> {
    let Some(proxy) = options.get("proxy").filter(|p| !p.is_null()).cloned() else {
        options.remove("proxy");
        return Ok(());
    };
    let auth = proxy.get("auth").cloned().unwrap_or(Value::Null);
    let ssl = options.get_mut("addSsl").and_then(Value::as_object_mut);
    if let Some(ssl) = ssl {
        ssl.insert("auth".into(), auth);
        ssl.insert(
            "addXForwardedHeaders".into(),
            proxy
                .get("addXForwardedHeaders")
                .cloned()
                .unwrap_or(Value::Bool(false)),
        );
    } else if !auth.is_null() {
        return Err(Error::new(
            eyre!(t!("version.http-auth-downgrade-requires-tls")),
            ErrorKind::MigrationFailed,
        ));
    }
    options.remove("proxy");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Session;
    use crate::sign::AnyVerifyingKey;
    use crate::trusted_keys::TrustedKeys;
    use crate::update::signers::{TrustedOsUpdateSigners, initial_signers};
    use crate::version::{Current, Upgrade, rollback_to_unchecked, v0_4_0_3, v0_4_0_4};

    fn database(options: Value) -> Value {
        json!({
            "private": { "sessions": { "pubkeys": {}, "tokens": {} } },
            "public": {
                "serverInfo": { "network": { "host": {
                    "bindings": { "80": { "options": options.clone(), "addresses": { "lan": true } } }
                } } },
                "packageData": { "service": { "hosts": {
                    "web": { "bindings": { "8080": { "options": options.clone() } } },
                    "api": { "bindings": { "9000": { "options": options } } }
                } } }
            }
        })
    }

    fn legacy_database(options: Value) -> Value {
        let mut db = database(options);
        db["private"] = json!({ "sessionPubkeys": {} });
        db
    }

    #[test]
    fn down_restores_http_auth_and_headers_on_server_and_package_hosts() {
        for auth in [
            json!({ "type": "basic", "realm": "Private", "credentials": [{ "username": "alice", "password": "secret" }] }),
            json!({ "type": "bearer", "realm": "Private", "tokens": ["secret"] }),
        ] {
            let ssl = json!({
                "preferredExternalPort": 443,
                "alpn": ["http/1.1"],
                "upstreamCertValidation": "disable"
            });
            let options = json!({
                "preferredExternalPort": 80,
                "secure": { "ssl": true },
                "addSsl": ssl.clone(),
                "proxy": { "type": "http", "auth": auth.clone(), "addXForwardedHeaders": true }
            });
            let mut db = database(options.clone());
            Version.down(&mut db).unwrap();

            let mut legacy = options;
            legacy.as_object_mut().unwrap().remove("proxy");
            legacy["addSsl"]["auth"] = auth.clone();
            legacy["addSsl"]["addXForwardedHeaders"] = json!(true);
            assert_eq!(db, legacy_database(legacy));

            let mut restored_ssl = db["public"]["serverInfo"]["network"]["host"]["bindings"]["80"]
                ["options"]["addSsl"]
                .clone();
            let restored = restored_ssl.as_object_mut().unwrap();
            assert_eq!(restored.remove("auth"), Some(auth));
            assert_eq!(restored.remove("addXForwardedHeaders"), Some(json!(true)));
            assert_eq!(restored_ssl, ssl);
        }
    }

    #[test]
    fn down_restores_headers_only_tls_and_removes_plain_http_proxy() {
        for add_ssl in [Value::Null, json!({ "preferredExternalPort": 443 })] {
            let mut options = json!({
                "preferredExternalPort": 80,
                "secure": null,
                "addSsl": add_ssl.clone(),
                "proxy": { "type": "http", "auth": null, "addXForwardedHeaders": true }
            });
            let mut db = database(options.clone());
            Version.down(&mut db).unwrap();
            options.as_object_mut().unwrap().remove("proxy");
            if !add_ssl.is_null() {
                options["addSsl"]["auth"] = Value::Null;
                options["addSsl"]["addXForwardedHeaders"] = json!(true);
            }
            assert_eq!(db, legacy_database(options));
        }
    }

    #[test]
    fn down_refuses_plain_http_auth_on_server_and_package_hosts() {
        let options = json!({
            "preferredExternalPort": 80,
            "secure": null,
            "addSsl": null,
            "proxy": { "type": "http", "auth": { "type": "bearer", "tokens": ["secret"] }, "addXForwardedHeaders": false }
        });
        let db = database(options);
        for mut db in [db.clone(), {
            let mut package_only = db.clone();
            package_only["public"]["serverInfo"] = json!({});
            package_only
        }] {
            let before = db.clone();
            let error = rollback_to_unchecked(&Version, &v0_4_0_5::Version, &mut db).unwrap_err();
            assert_eq!(error.kind, ErrorKind::MigrationFailed);
            assert_eq!(db, before);
        }
    }

    #[test]
    fn down_preserves_raw_bindings() {
        let options =
            json!({ "preferredExternalPort": 443, "secure": { "ssl": true }, "addSsl": null });
        let mut db = database(options.clone());
        Version.down(&mut db).unwrap();
        assert_eq!(db, legacy_database(options.clone()));

        let mut db = database(options.clone());
        for_each_host(&mut db, |host| {
            for (_, binding) in host["bindings"].as_object_mut().unwrap().iter_mut() {
                binding["options"]["proxy"] = Value::Null;
            }
        });
        Version.down(&mut db).unwrap();
        assert_eq!(db, legacy_database(options));
    }

    #[test]
    fn downgrade_to_0_4_0_5_restores_http_policy_before_key_rollback() {
        let key = AnyVerifyingKey::Ed25519(
            ed25519_dalek::SigningKey::from_bytes(&[43; 32]).verifying_key(),
        );
        let canonical = key.interned_der64().to_string();
        let legacy_key = key.to_string();
        let session = to_value(&Session::default()).unwrap();
        let auth = json!({ "type": "bearer", "tokens": ["secret"], "realm": null });
        let mut db = database(json!({
            "preferredExternalPort": 80,
            "secure": null,
            "addSsl": { "preferredExternalPort": 443, "alpn": null, "upstreamCertValidation": null },
            "proxy": { "type": "http", "auth": auth.clone(), "addXForwardedHeaders": true }
        }));
        db["private"]["sessions"]["pubkeys"] = json!({ canonical.clone(): session.clone() });
        let info = &mut db["public"]["serverInfo"];
        info["version"] = json!("0.4.1");
        info["packageVersionCompat"] = json!("*");
        info["postInitMigrationTodos"] = json!({});
        info["trustedOsUpdateSigners"] =
            json!({ canonical.clone(): { "label": "retained signer" } });
        Version.up(&mut db, ()).unwrap();
        assert!(db["public"]["serverInfo"].get("trustedKeys").is_some());
        assert!(
            db["public"]["serverInfo"]
                .get("trustedOsUpdateSigners")
                .is_none()
        );

        let mut previous = db.clone();
        rollback_to_unchecked(&Version, &v0_4_0_5::Version, &mut previous).unwrap();
        assert_eq!(
            previous["public"]["serverInfo"]["version"],
            json!("0.4.0.5")
        );
        assert_eq!(
            previous["private"],
            json!({
                "sessionPubkeys": { canonical.clone(): session.clone() },
                "osSecurityNotifications": {},
            })
        );
        assert_eq!(
            previous["public"]["serverInfo"]["trustedOsUpdateSigners"],
            json!({ canonical: { "label": "retained signer" } })
        );
        assert!(
            previous["public"]["serverInfo"]
                .get("trustedKeys")
                .is_none()
        );
        for_each_host(&mut previous, |host| {
            for (_, binding) in host["bindings"].as_object_mut().unwrap().iter_mut() {
                let options = &binding["options"];
                assert!(options.get("proxy").is_none());
                assert_eq!(options["addSsl"]["auth"], auth);
                assert_eq!(options["addSsl"]["addXForwardedHeaders"], json!(true));
            }
        });

        rollback_to_unchecked(&Version, &v0_4_0_4::Version, &mut db).unwrap();
        assert_eq!(db["public"]["serverInfo"]["version"], json!("0.4.0.4"));
        assert_eq!(
            db["private"],
            json!({ "sessionPubkeys": { legacy_key.clone(): session } })
        );
        assert_eq!(
            db["public"]["serverInfo"]["trustedOsUpdateSigners"],
            json!({ legacy_key: { "label": "retained signer" } })
        );
        assert_eq!(
            db["public"]["serverInfo"]["network"]["host"],
            previous["public"]["serverInfo"]["network"]["host"]
        );
        assert_eq!(
            db["public"]["packageData"],
            previous["public"]["packageData"]
        );
    }

    #[test]
    fn malformed_legacy_keys_fail_without_changing_saved_trust() {
        let mut db = json!({
            "private": { "sessionPubkeys": {} },
            "public": { "serverInfo": { "trustedOsUpdateSigners": {
                "not a public key": { "label": "custom" },
            } } },
        });
        let saved = db.clone();
        assert!(Version.up(&mut db, ()).is_err());
        assert_eq!(db, saved);
    }

    #[test]
    fn migration_is_idempotent_and_downgrade_preserves_os_policy() {
        let legacy = initial_signers();
        let mut db = json!({
            "private": { "sessionPubkeys": {} },
            "public": { "serverInfo": { "trustedOsUpdateSigners": legacy } },
        });
        Version.up(&mut db, ()).unwrap();
        let migrated = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(db, migrated);
        assert!(
            db["public"]["serverInfo"]
                .get("trustedOsUpdateSigners")
                .is_none()
        );
        Version.down(&mut db).unwrap();
        let restored: TrustedOsUpdateSigners =
            from_value(db["public"]["serverInfo"]["trustedOsUpdateSigners"].clone()).unwrap();
        assert_eq!(
            restored.into_keys().collect::<Vec<_>>(),
            legacy.into_keys().collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn upgrade_from_0_4_0_3_reruns_its_stale_migration() {
        let initial = to_value(&initial_signers()).unwrap();
        let mut custom = initial_signers();
        custom.pop_first();
        custom.values_mut().next().unwrap().label = "Owner-managed".to_owned();
        let custom = to_value(&custom).unwrap();
        for (revision, saved, expected) in [
            (2, None, initial),
            (3, Some(json!({})), json!({})),
            (3, Some(custom.clone()), custom),
        ] {
            let previous = v0_4_0_3::Version;
            let mut db = json!({
                "private": { "sessionPubkeys": {} },
                "public": { "serverInfo": {
                    "version": previous.semver(),
                    "packageVersionCompat": "*",
                    "latestMigrationRevision": revision,
                    "postInitMigrationTodos": {},
                } },
            });
            if let Some(saved) = saved {
                db["public"]["serverInfo"]["trustedOsUpdateSigners"] = saved;
            }
            Upgrade::load(&previous, &Version, revision)
                .await
                .unwrap()
                .apply(&previous, &Version, &mut db)
                .unwrap();
            let keyrings: TrustedKeys =
                from_value(db["public"]["serverInfo"]["trustedKeys"].clone()).unwrap();
            let actual = os_signers(&keyrings);
            let expected: TrustedOsUpdateSigners = from_value(expected).unwrap();
            assert_eq!(
                actual.keys().collect::<Vec<_>>(),
                expected.keys().collect::<Vec<_>>()
            );
            for (key, signer) in expected {
                if signer.label == "Owner-managed" {
                    assert_eq!(actual[&key].label, signer.label);
                }
            }
            assert!(
                db["public"]["serverInfo"]
                    .get("trustedOsUpdateSigners")
                    .is_none()
            );
            assert_eq!(db["public"]["serverInfo"]["version"], json!("0.4.1"));
        }
    }

    #[test]
    fn sessions_split_by_credential_and_roll_back() {
        let pubkeys = json!({ "-----BEGIN PUBLIC KEY-----\nkey\n-----END PUBLIC KEY-----\n": {
            "name": null,
            "loggedIn": "2026-01-01T00:00:00Z",
            "lastActive": "2026-01-02T00:00:00Z",
            "userAgent": "agent",
        } });
        let original = json!({
            "public": { "serverInfo": { "trustedOsUpdateSigners": {} } },
            "private": {
                "password": "hash", "sessionPubkeys": pubkeys.clone(),
                "osSecurityNotifications": {},
            },
        });
        let mut db = original.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db["private"],
            json!({
                "password": "hash", "sessions": { "pubkeys": pubkeys, "tokens": {} },
                "osSecurityNotifications": {},
            })
        );
        db["private"]["sessions"]["tokens"] = json!({ "hash": {} });
        let migrated = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(db, migrated);
        Version.down(&mut db).unwrap();
        assert_eq!(db, original);
    }

    #[tokio::test]
    async fn published_keyrings_gain_notification_state_without_losing_trust_or_sessions() {
        use patch_db::json_ptr::ROOT;

        let account =
            crate::account::AccountInfo::new("password", std::time::SystemTime::now(), None)
                .unwrap();
        let mut fixture =
            to_value(&crate::db::model::Database::init(&account, false, None, None).unwrap())
                .unwrap();
        fixture["public"]["serverInfo"]["trustedKeys"]["start9"]["os"] = json!(false);
        fixture["public"]["serverInfo"]["latestMigrationRevision"] = json!(1);
        fixture["public"]["serverInfo"]["postInitMigrationTodos"] = json!({});
        fixture["private"]["sessions"]["tokens"] =
            json!({ "a".repeat(64): crate::auth::Session::default() });
        fixture["private"]
            .as_object_mut()
            .unwrap()
            .remove("osSecurityNotifications");
        let saved_trust = fixture["public"]["serverInfo"]["trustedKeys"].clone();
        let saved_sessions = fixture["private"]["sessions"].clone();
        let db = PatchDb::in_memory();
        db.put(&ROOT, &fixture).await.unwrap();
        Current::default().pre_init(&db).await.unwrap();
        let migrated = db.dump(&ROOT).await.value;
        assert_eq!(migrated["private"]["osSecurityNotifications"], json!({}));
        assert_eq!(migrated["private"]["sessions"], saved_sessions);
        assert_eq!(migrated["public"]["serverInfo"]["trustedKeys"], saved_trust);
        assert_eq!(
            migrated["public"]["serverInfo"]["latestMigrationRevision"],
            json!(2)
        );
        Current::default().pre_init(&db).await.unwrap();
        assert_eq!(db.dump(&ROOT).await.value, migrated);
    }

    #[tokio::test]
    async fn existing_0_4_1_tokens_and_custom_os_trust_survive_the_new_revision() {
        use patch_db::json_ptr::ROOT;

        let account =
            crate::account::AccountInfo::new("password", std::time::SystemTime::now(), None)
                .unwrap();
        let key = "MCowBQYDK2VwAyEAGaAYxsY7iO5GyQrCA5vcLWYIQyoKImp0du4WWvIzr2Y=";
        for legacy in [
            TrustedOsUpdateSigners::new(),
            from_value(json!({ key: { "label": "Owner-managed" } })).unwrap(),
        ] {
            let mut fixture =
                to_value(&crate::db::model::Database::init(&account, false, None, None).unwrap())
                    .unwrap();
            let server = fixture["public"]["serverInfo"].as_object_mut().unwrap();
            server.remove("trustedKeys");
            server.insert("trustedOsUpdateSigners".into(), to_value(&legacy).unwrap());
            server.insert("latestMigrationRevision".into(), json!(0));
            server.insert("postInitMigrationTodos".into(), json!({}));
            fixture["private"]["sessions"]["tokens"] =
                json!({ "a".repeat(64): crate::auth::Session::default() });
            let sessions = fixture["private"]["sessions"].clone();
            let db = PatchDb::in_memory();
            db.put(&ROOT, &fixture).await.unwrap();
            Current::default().pre_init(&db).await.unwrap();
            let migrated = db.dump(&ROOT).await.value;
            let keyrings: TrustedKeys =
                from_value(migrated["public"]["serverInfo"]["trustedKeys"].clone()).unwrap();
            assert_eq!(os_signers(&keyrings), legacy);
            assert_eq!(migrated["private"]["sessions"], sessions);
            assert_eq!(
                migrated["public"]["serverInfo"]["latestMigrationRevision"],
                json!(Version.migration_revision())
            );
            assert_eq!(
                migrated["public"]["serverInfo"]["postInitMigrationTodos"],
                json!({ "0.4.1": null })
            );
            Current::default().pre_init(&db).await.unwrap();
            assert_eq!(db.dump(&ROOT).await.value, migrated);
        }
    }
}
