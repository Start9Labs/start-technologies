use exver::VersionRange;
use imbl_value::json;

use super::v0_3_5::V0_3_0_COMPAT;
use super::v0_4_0_2::for_each_host;
use super::{VersionT, v0_4_0_5};
use crate::prelude::*;

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
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        let private = db["private"].as_object_mut().or_not_found("private")?;
        let pubkeys = private
            .remove("sessionPubkeys")
            .unwrap_or_else(|| json!({}));
        private.insert(
            "sessions".into(),
            json!({ "pubkeys": pubkeys, "tokens": {} }),
        );
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        restore_legacy_http_bindings(db)?;
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
mod test {
    use imbl_value::json;

    use super::*;
    use crate::auth::Session;
    use crate::sign::AnyVerifyingKey;
    use crate::update::signers::initial_signers;
    use crate::version::{Upgrade, rollback_to_unchecked, v0_4_0_3, v0_4_0_4};

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

        let mut previous = db.clone();
        rollback_to_unchecked(&Version, &v0_4_0_5::Version, &mut previous).unwrap();
        assert_eq!(
            previous["public"]["serverInfo"]["version"],
            json!("0.4.0.5")
        );
        assert_eq!(
            previous["private"],
            json!({ "sessionPubkeys": { canonical.clone(): session.clone() } })
        );
        assert_eq!(
            previous["public"]["serverInfo"]["trustedOsUpdateSigners"],
            json!({ canonical: { "label": "retained signer" } })
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
            assert_eq!(
                db["public"]["serverInfo"]["trustedOsUpdateSigners"],
                expected,
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
        let original =
            json!({ "private": { "password": "hash", "sessionPubkeys": pubkeys.clone() } });
        let mut db = original.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db,
            json!({ "private": {
                "password": "hash",
                "sessions": { "pubkeys": pubkeys, "tokens": {} },
            } })
        );
        db["private"]["sessions"]["tokens"] = json!({ "hash": {} });
        Version.down(&mut db).unwrap();
        assert_eq!(db, original);
    }
}
