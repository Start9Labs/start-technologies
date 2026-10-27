use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::v0_4_0_2::for_each_host;
use super::{VersionT, v0_4_0_4};
use crate::prelude::*;

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
    fn up(self, _db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        let mut result = Ok(());
        for_each_host(db, |host| {
            if result.is_err() {
                return;
            }
            let Some(bindings) = host.get_mut("bindings").and_then(Value::as_object_mut) else {
                return;
            };
            for (_, binding) in bindings.iter_mut() {
                let Some(options) = binding.get_mut("options").and_then(Value::as_object_mut)
                else {
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
    use imbl_value::json;

    use super::*;

    fn database(options: Value) -> Value {
        json!({ "public": {
            "serverInfo": { "network": { "host": {
                "bindings": { "80": { "options": options.clone(), "addresses": { "lan": true } } }
            } } },
            "packageData": { "service": { "hosts": {
                "web": { "bindings": { "8080": { "options": options.clone() } } },
                "api": { "bindings": { "9000": { "options": options } } }
            } } }
        } })
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
            assert_eq!(db, database(legacy));

            let mut restored_ssl = db["public"]["serverInfo"]["network"]["host"]["bindings"]["80"]
                ["options"]["addSsl"]
                .clone();
            let restored = restored_ssl.as_object_mut().unwrap();
            assert_eq!(restored.remove("auth"), Some(auth));
            assert_eq!(restored.remove("addXForwardedHeaders"), Some(json!(true)));
            assert_eq!(restored_ssl, ssl);

            let once = db.clone();
            Version.down(&mut db).unwrap();
            assert_eq!(db, once);
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
            assert_eq!(db, database(options));
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
            let error = Version.down(&mut db).unwrap_err();
            assert_eq!(error.kind, ErrorKind::MigrationFailed);
            assert_eq!(db, before);
        }
    }

    #[test]
    fn up_is_idempotent_and_leaves_normalization_to_deserialization() {
        let mut db = database(json!({
            "preferredExternalPort": 80,
            "addSsl": { "preferredExternalPort": 443, "addXForwardedHeaders": true, "auth": { "type": "bearer", "tokens": ["secret"] } }
        }));
        let before = db.clone();
        for _ in 0..2 {
            assert_eq!(Version.up(&mut db, ()).unwrap(), Value::Null);
            assert_eq!(db, before);
        }
    }

    #[test]
    fn down_preserves_raw_bindings() {
        let options =
            json!({ "preferredExternalPort": 443, "secure": { "ssl": true }, "addSsl": null });
        let mut db = database(options);
        let before = db.clone();
        Version.down(&mut db).unwrap();
        assert_eq!(db, before);

        for_each_host(&mut db, |host| {
            for (_, binding) in host["bindings"].as_object_mut().unwrap().iter_mut() {
                binding["options"]["proxy"] = Value::Null;
            }
        });
        Version.down(&mut db).unwrap();
        assert_eq!(db, before);
    }
}
