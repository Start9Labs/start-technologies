use std::path::Path;

use exver::{PreReleaseSegment, VersionRange};
use imbl_value::json;
use tokio::process::Command;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_beta_9};
use crate::context::RpcContext;
use crate::prelude::*;
use crate::util::Invoke;

lazy_static::lazy_static! {
    static ref V0_4_0_beta_10: exver::Version = exver::Version::new(
        [0, 4, 0],
        [PreReleaseSegment::String("beta".into()), 10.into()]
    );
}

const TOR_VOLUMES: &str = "/media/startos/data/package-data/volumes/tor/data";

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_beta_9::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0_beta_10.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    #[instrument(skip_all)]
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        // The Root CA fingerprint was formatted with `{:X}` (no width), dropping
        // the leading zero of any byte < 0x10 (e.g. `A3:3:D2` instead of
        // `A3:03:D2`), so the value shown in the UI didn't match what devices
        // report. Each `{:X}` byte is 1 char (< 0x10) or 2 chars, so left-padding
        // every 1-char segment to two digits exactly reconstructs the `{:02X}`
        // form. Idempotent: already-correct (all-2-char) fingerprints are unchanged.
        let Some(server_info) = db["public"]["serverInfo"].as_object_mut() else {
            return Err(Error::new(
                eyre!("db.public.serverInfo is not an object"),
                ErrorKind::Database,
            ));
        };
        let Some(fingerprint) = server_info.get("caFingerprint").and_then(|v| v.as_str()) else {
            return Err(Error::new(
                eyre!("db.public.serverInfo.caFingerprint is not a string"),
                ErrorKind::Database,
            ));
        };
        let repaired = fingerprint
            .split(':')
            .map(|seg| {
                if seg.len() < 2 {
                    format!("0{seg}")
                } else {
                    seg.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(":");
        server_info.insert("caFingerprint".into(), json!(repaired));
        heal_empty_server_host_id(db);
        Ok(Value::Null)
    }
    async fn post_up(self, _ctx: &RpcContext, _input: Value) -> Result<(), Error> {
        // Older installs copied /media/startos into the persistent config overlay as
        // root:root, shadowing the squashfs's root:startos on every boot. Fix the
        // persisted entry so migrated nodes match fresh installs (#3311).
        let overlay_media_startos = "/media/startos/config/overlay/media/startos";
        if tokio::fs::metadata(overlay_media_startos).await.is_ok() {
            Command::new("chown")
                .arg("root:startos")
                .arg(overlay_media_startos)
                .invoke(ErrorKind::Filesystem)
                .await?;
            Command::new("chmod")
                .arg("750")
                .arg(overlay_media_startos)
                .invoke(ErrorKind::Filesystem)
                .await?;
        }
        migrate_tor_service_identity().await?;
        Ok(())
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}

/// Dev builds between #3366 and #3387 persisted the server host's
/// then-sentinel id — the empty string — in the `startos-ui` interface's
/// `addressInfo.hostId`. Strict deserialization rejects an empty `Id`, so
/// every full-db read (e.g. `validate_db` at init) failed and the box booted
/// into the diagnostic UI. The server host's id is now `admin`.
fn heal_empty_server_host_id(db: &mut Value) {
    let Some(bindings) = db["public"]["serverInfo"]["network"]["host"]["bindings"].as_object_mut()
    else {
        return;
    };
    for (_, binding) in bindings.iter_mut() {
        let Some(interfaces) = binding
            .get_mut("interfaces")
            .and_then(|i| i.as_object_mut())
        else {
            continue;
        };
        for (_, interface) in interfaces.iter_mut() {
            let Some(address_info) = interface
                .get_mut("addressInfo")
                .and_then(|a| a.as_object_mut())
            else {
                continue;
            };
            if address_info.get("hostId").and_then(|h| h.as_str()) == Some("") {
                address_info.insert("hostId".into(), json!("admin"));
            }
        }
    }
}

/// The StartOS UI is now addressed like any service interface — package id
/// `start-os`, host id `admin` — replacing the tor package's legacy
/// `STARTOS`/`startos-ui` sentinels. Re-point tor's persisted state at the new
/// identity so the UI's existing .onion address (and its key) survives instead
/// of being regenerated: move the hidden-service key dir, rewrite the torrc
/// annotations, and update a not-yet-imported onion-migration.json. All steps
/// are idempotent and skipped when tor isn't installed.
async fn migrate_tor_service_identity() -> Result<(), Error> {
    let hs_root = Path::new(TOR_VOLUMES).join("tor/hidden_services");
    let legacy = hs_root.join("STARTOS/startos-ui");
    let target = hs_root.join("start-os/admin");
    if tokio::fs::metadata(&legacy).await.is_ok() && tokio::fs::metadata(&target).await.is_err() {
        let target_parent = target.parent().expect("has parent");
        tokio::fs::create_dir_all(target_parent).await?;
        copy_ownership(&hs_root, target_parent).await?;
        tokio::fs::rename(&legacy, &target).await?;
        let _ = tokio::fs::remove_dir(hs_root.join("STARTOS")).await;
    }

    let torrc_path = Path::new(TOR_VOLUMES).join("tor/torrc");
    if let Ok(torrc) = tokio::fs::read_to_string(&torrc_path).await {
        let migrated = migrate_torrc(&torrc);
        if migrated != torrc {
            let owner = tokio::fs::metadata(&torrc_path).await?;
            crate::util::io::write_file_atomic(&torrc_path, migrated).await?;
            restore_ownership(&owner, &torrc_path).await?;
        }
    }

    let onion_migration_path = Path::new(TOR_VOLUMES).join("startos/onion-migration.json");
    if let Ok(raw) = tokio::fs::read_to_string(&onion_migration_path).await {
        if let Ok(mut migration) = serde_json::from_str::<serde_json::Value>(&raw) {
            let mut changed = false;
            if let Some(addresses) = migration
                .get_mut("addresses")
                .and_then(|a| a.as_array_mut())
            {
                for entry in addresses {
                    if entry.get("packageId").and_then(|p| p.as_str()) == Some("STARTOS") {
                        entry["packageId"] = "start-os".into();
                        entry["hostId"] = "admin".into();
                        changed = true;
                    }
                }
            }
            if changed {
                let owner = tokio::fs::metadata(&onion_migration_path).await?;
                crate::util::io::write_file_atomic(
                    &onion_migration_path,
                    serde_json::to_string(&migration).with_kind(ErrorKind::Serialization)?,
                )
                .await?;
                restore_ownership(&owner, &onion_migration_path).await?;
            }
        }
    }

    Ok(())
}

fn migrate_torrc(torrc: &str) -> String {
    torrc
        .replace("# @service STARTOS startos-ui", "# @service start-os admin")
        .replace(
            "/hidden_services/STARTOS/startos-ui/",
            "/hidden_services/start-os/admin/",
        )
}

/// Files in a package volume belong to the container's (uid-mapped) user;
/// anything this migration creates as host root must be handed back or the
/// tor daemon can't read its keys / rewrite its config.
async fn copy_ownership(from: &Path, to: &Path) -> Result<(), Error> {
    let meta = tokio::fs::metadata(from).await?;
    restore_ownership(&meta, to).await
}

async fn restore_ownership(meta: &std::fs::Metadata, path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::MetadataExt;
    Command::new("chown")
        .arg(format!("{}:{}", meta.uid(), meta.gid()))
        .arg(path)
        .invoke(ErrorKind::Filesystem)
        .await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn heals_empty_host_id_and_leaves_valid_ids_alone() {
        let mut db = json!({
            "public": {
                "serverInfo": {
                    "network": {
                        "host": {
                            "bindings": {
                                "80": {
                                    "interfaces": {
                                        "startos-ui": {
                                            "addressInfo": { "hostId": "" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        heal_empty_server_host_id(&mut db);
        assert_eq!(
            db["public"]["serverInfo"]["network"]["host"]["bindings"]["80"]["interfaces"]["startos-ui"]
                ["addressInfo"]["hostId"],
            json!("admin")
        );
        // idempotent + already-valid ids untouched
        heal_empty_server_host_id(&mut db);
        assert_eq!(
            db["public"]["serverInfo"]["network"]["host"]["bindings"]["80"]["interfaces"]["startos-ui"]
                ["addressInfo"]["hostId"],
            json!("admin")
        );
        // absent interfaces / hosts are a no-op
        let mut empty = json!({ "public": { "serverInfo": {} } });
        heal_empty_server_host_id(&mut empty);
    }

    #[test]
    fn migrates_torrc_annotations() {
        let torrc = "SocksPort 0.0.0.0:9050\n\
            # @service STARTOS startos-ui\n\
            HiddenServiceDir /var/lib/tor/hidden_services/STARTOS/startos-ui/hs_0/\n\
            # @internalPort 80\n\
            HiddenServicePort 80 startos:80\n\
            # @ssl 80\n\
            HiddenServicePort 443 startos:443\n\
            \n\
            # @service bitcoind default\n\
            HiddenServiceDir /var/lib/tor/hidden_services/bitcoind/default/hs_0/\n\
            # @internalPort 8332\n\
            HiddenServicePort 8332 bitcoind.startos:8332\n";
        let migrated = migrate_torrc(torrc);
        assert!(migrated.contains("# @service start-os admin"));
        assert!(
            migrated.contains("HiddenServiceDir /var/lib/tor/hidden_services/start-os/admin/hs_0/")
        );
        assert!(!migrated.contains("STARTOS"));
        // package entries and port targets untouched
        assert!(migrated.contains("# @service bitcoind default"));
        assert!(migrated.contains("HiddenServicePort 80 startos:80"));
        assert!(migrated.contains("HiddenServicePort 8332 bitcoind.startos:8332"));
        // idempotent
        assert_eq!(migrate_torrc(&migrated), migrated);
    }
}
