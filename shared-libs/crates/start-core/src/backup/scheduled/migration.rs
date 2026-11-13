use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::backup::target::{BackupTarget, BackupTargetId};
use crate::prelude::*;
use crate::shared_volumes::SharedVolumeSource;

pub(crate) fn register_legacy_disk_targets(db: &mut Value) -> Result<(), Error> {
    let mut wire = serde_json::to_value(&*db).with_kind(ErrorKind::Serialization)?;
    let mut legacy = BTreeSet::new();
    if let Some(state) = wire.pointer("/public/scheduledBackups") {
        collect_targets(state, &mut legacy);
    }
    if let Some(credentials) = wire
        .pointer("/private/scheduledBackupCredentials")
        .and_then(serde_json::Value::as_object)
    {
        legacy.extend(
            credentials
                .keys()
                .filter(|id| id.starts_with("disk-"))
                .cloned(),
        );
    }
    if legacy.is_empty() {
        return Ok(());
    }
    let targets = wire["public"]
        .as_object_mut()
        .expect("public database")
        .entry("backupTargets")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .expect("backup target catalog");
    let mut replacements = BTreeMap::new();
    for old in legacy {
        let logicalname = old.strip_prefix("disk-").expect("legacy disk identity");
        let existing = targets
            .iter()
            .find(|(_, target)| {
                target["source"]["kind"] == "disk"
                    && target["source"]["logicalname"] == logicalname
                    && target["source"]["path"]
                        .as_str()
                        .unwrap_or_default()
                        .is_empty()
            })
            .map(|(id, _)| id.clone());
        let id = existing.unwrap_or_else(|| BackupTargetId::new().to_string());
        targets.entry(id.clone()).or_insert(
            serde_json::to_value(BackupTarget {
                name: logicalname.to_owned(),
                source: SharedVolumeSource::Disk {
                    logicalname: PathBuf::from(logicalname),
                    path: PathBuf::new(),
                    drive: None,
                },
                inspection: Default::default(),
                start_os: Default::default(),
                legacy_backup: false,
            })
            .with_kind(ErrorKind::Serialization)?,
        );
        replacements.insert(old, id);
    }
    rewrite_targets(&mut wire["public"]["scheduledBackups"], &replacements);
    if let Some(credentials) = wire.pointer_mut("/private/scheduledBackupCredentials") {
        rewrite_targets(credentials, &replacements);
    }
    *db = to_value(&wire)?;
    Ok(())
}

fn collect_targets(value: &serde_json::Value, targets: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                if key == "targetId" {
                    if let Some(id) = value.as_str().filter(|id| id.starts_with("disk-")) {
                        targets.insert(id.to_owned());
                    }
                }
                collect_targets(value, targets);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_targets(value, targets);
            }
        }
        _ => (),
    }
}

fn rewrite_targets(value: &mut serde_json::Value, replacements: &BTreeMap<String, String>) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, mut value) in std::mem::take(object) {
                if key == "targetId" {
                    if let Some(id) = value.as_str().and_then(|id| replacements.get(id)) {
                        value = serde_json::Value::String(id.clone());
                    }
                }
                rewrite_targets(&mut value, replacements);
                let key = replacements
                    .get(&key)
                    .cloned()
                    .or_else(|| {
                        let (target, package) = key.split_once("::")?;
                        Some(format!("{}::{package}", replacements.get(target)?))
                    })
                    .unwrap_or(key);
                object.insert(key, value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                rewrite_targets(value, replacements);
            }
        }
        _ => (),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_registration_preserves_schedules_checkpoints_and_unlock_credentials() {
        let mut db = imbl_value::json!({"public": {"scheduledBackups": {
            "jobs": {"job": {"targetId": "disk-/dev/sda1", "name": "Nightly"}},
            "histories": {"disk-/dev/sda1::bitcoind": {"targetId": "disk-/dev/sda1", "snapshots": ["checkpoint"]}},
            "runs": {"run": {"targetId": "disk-/dev/sda1"}},
            "activities": {"activity": {"targetId": "disk-/dev/sda1"}},
            "targetFailures": {"disk-/dev/sda1": {"failures": 2}},
        }}, "private": {"scheduledBackupCredentials": {"disk-/dev/sda1": {"targetInstanceId": "original", "sealedKey": [1,2,3]}}}});
        register_legacy_disk_targets(&mut db).unwrap();
        let wire = serde_json::to_value(&db).unwrap();
        let id = wire["public"]["scheduledBackups"]["jobs"]["job"]["targetId"]
            .as_str()
            .unwrap();
        assert!(id.parse::<BackupTargetId>().is_ok());
        let target: BackupTarget =
            serde_json::from_value(wire["public"]["backupTargets"][id].clone()).unwrap();
        assert!(
            matches!(target.source, SharedVolumeSource::Disk { logicalname, .. } if logicalname == PathBuf::from("/dev/sda1"))
        );
        assert_eq!(
            wire["public"]["scheduledBackups"]["histories"][format!("{id}::bitcoind")]["snapshots"],
            serde_json::json!(["checkpoint"])
        );
        assert_eq!(
            wire["public"]["scheduledBackups"]["runs"]["run"]["targetId"],
            id
        );
        assert_eq!(
            wire["public"]["scheduledBackups"]["activities"]["activity"]["targetId"],
            id
        );
        assert_eq!(
            wire["public"]["scheduledBackups"]["targetFailures"][id]["failures"],
            2
        );
        assert_eq!(
            wire["private"]["scheduledBackupCredentials"][id]["targetInstanceId"],
            "original"
        );
        assert_eq!(
            wire["private"]["scheduledBackupCredentials"][id]["sealedKey"],
            serde_json::json!([1, 2, 3])
        );
        let saved = db.clone();
        register_legacy_disk_targets(&mut db).unwrap();
        assert_eq!(db, saved);
    }
    #[test]
    fn disk_credentials_are_registered_after_the_last_schedule_is_deleted() {
        let mut db = imbl_value::json!({
            "public": {"scheduledBackups": {}},
            "private": {"scheduledBackupCredentials": {
                "disk-/dev/sdb1": {"targetInstanceId": "saved", "sealedKey": [4,5,6]}
            }}
        });
        register_legacy_disk_targets(&mut db).unwrap();
        let wire = serde_json::to_value(&db).unwrap();
        let targets = wire["public"]["backupTargets"].as_object().unwrap();
        let (id, target) = targets.iter().next().unwrap();
        assert!(id.parse::<BackupTargetId>().is_ok());
        assert_eq!(target["source"]["logicalname"], "/dev/sdb1");
        assert_eq!(
            wire["private"]["scheduledBackupCredentials"][id]["targetInstanceId"],
            "saved"
        );
        assert!(
            wire["private"]["scheduledBackupCredentials"]
                .get("disk-/dev/sdb1")
                .is_none()
        );
    }
}
