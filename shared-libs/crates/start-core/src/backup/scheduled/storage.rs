use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use color_eyre::eyre::eyre;
use serde::{Deserialize, Serialize};

use super::{BackupRun, RetentionPolicy, ServiceSnapshot, ServiceSnapshotId, ServiceTargetHistory};
use crate::auth::check_password;
use crate::disk::BACKUP_DIR_NAME;
use crate::disk::mount::filesystem::ReadWrite;
use crate::disk::mount::filesystem::backupfs::BackupFS;
use crate::disk::mount::guard::{GenericMountGuard, SubPath, TmpMountGuard};
use crate::disk::util::{MAX_BACKUP_RECOVERY_METADATA_BYTES, MAX_BACKUP_TARGET_METADATA_BYTES};
use crate::hostname::ServerHostname;
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::util::crypto::{decrypt_slice, encrypt_slice};
use crate::util::io::{delete_dir, delete_file, dir_copy, dir_size, rename, write_file_atomic};
use crate::util::serde::{IoFormat, read_json_file_bounded};
use crate::version::VersionT;
use crate::{PackageId, SYSTEM_PACKAGE_ID};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(super) struct TargetIdentityMismatch(pub(super) String);

/// Unencrypted recovery metadata needed to unlock a scheduled backup target.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledBackupRecoveryInfo {
    pub target_instance_id: String,
    pub hostname: ServerHostname,
    pub version: exver::Version,
    pub timestamp: DateTime<Utc>,
    pub password_hash: String,
    pub wrapped_key: String,
    #[serde(default)]
    pub has_system_backup: Option<bool>,
}

impl ScheduledBackupRecoveryInfo {
    fn encryption_key(&self, password: &str) -> Result<String, Error> {
        check_password(&self.password_hash, password)?;
        let wrapped_key = base32::decode(
            base32::Alphabet::Rfc4648 { padding: true },
            &self.wrapped_key,
        )
        .ok_or_else(|| {
            Error::new(
                eyre!("{}", t!("backup.scheduled.decode-key-failed")),
                ErrorKind::Backup,
            )
        })?;
        Ok(String::from_utf8(decrypt_slice(wrapped_key, password))?)
    }
}

/// Encrypted metadata describing all scheduled histories on a target.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledBackupOnTargetMetadata {
    pub target_instance_id: String,
    pub services: BTreeMap<PackageId, OnTargetServiceHistory>,
}

impl ScheduledBackupOnTargetMetadata {
    pub(super) fn refresh_history(&self, local: &mut ServiceTargetHistory) {
        if local.target_instance_id != self.target_instance_id {
            return;
        }
        let Some(saved) = self.services.get(&local.package_id) else {
            local.snapshots.clear();
            return;
        };
        let archived: BTreeSet<_> = local
            .snapshots
            .iter()
            .filter(|snapshot| snapshot.archived)
            .map(|snapshot| &snapshot.id)
            .collect();
        local.snapshots = saved
            .snapshots
            .iter()
            .cloned()
            .map(|mut snapshot| {
                snapshot.archived |= local.archived || archived.contains(&snapshot.id);
                snapshot
            })
            .collect();
        local.timezone = saved.timezone.clone();
        local.policy = saved.policy.clone();
    }

    pub(crate) fn reconcile_histories(
        &mut self,
        histories: impl IntoIterator<Item = ServiceTargetHistory>,
    ) {
        for local in histories {
            if local.target_instance_id != self.target_instance_id {
                continue;
            }
            let history = match self.services.entry(local.package_id.clone()) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) if local.snapshots.is_empty() => {
                    entry.insert(OnTargetServiceHistory {
                        timezone: local.timezone.clone(),
                        policy: local.policy.clone(),
                        archived: local.archived,
                        snapshots: Vec::new(),
                    })
                }
                _ => continue,
            };
            if history.snapshots.is_empty() {
                history.timezone = local.timezone;
                history.policy = local.policy;
            }
            set_archive_state(history, local.archived);
            let archived: BTreeSet<_> = local
                .snapshots
                .iter()
                .filter(|snapshot| snapshot.archived)
                .map(|snapshot| &snapshot.id)
                .collect();
            for snapshot in &mut history.snapshots {
                snapshot.archived |= archived.contains(&snapshot.id);
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnTargetServiceHistory {
    pub timezone: String,
    pub policy: RetentionPolicy,
    pub archived: bool,
    pub snapshots: Vec<ServiceSnapshot>,
}

impl OnTargetServiceHistory {
    fn change_policy(
        &mut self,
        timezone: String,
        policy: RetentionPolicy,
        confirmed_removals: &BTreeSet<ServiceSnapshotId>,
    ) -> Result<(), Error> {
        let parsed_timezone = timezone.parse().map_err(|_| {
            Error::new(
                eyre!("{}", t!("backup.scheduled.stored-timezone-invalid")),
                ErrorKind::Backup,
            )
        })?;
        let removals = if self.archived {
            BTreeSet::new()
        } else {
            policy
                .preview(&self.snapshots, parsed_timezone)?
                .removed
                .into_iter()
                .map(|snapshot| snapshot.id)
                .collect()
        };
        if &removals != confirmed_removals {
            return Err(Error::new(
                eyre!("{}", t!("backup.scheduled.prune-confirmation-stale")),
                ErrorKind::InvalidRequest,
            ));
        }
        self.timezone = timezone;
        self.policy = policy;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ScheduledBackupMountGuard<G: GenericMountGuard> {
    target_guard: Option<G>,
    encrypted_guard: Option<TmpMountGuard>,
    recovery_path: PathBuf,
    pub recovery: ScheduledBackupRecoveryInfo,
    pub metadata: ScheduledBackupOnTargetMetadata,
}

impl<G: GenericMountGuard> ScheduledBackupMountGuard<G> {
    pub(crate) fn target_path(&self) -> &Path {
        self.target_guard
            .as_ref()
            .expect("scheduled backup target is mounted")
            .path()
    }

    pub async fn initialize(
        target_guard: G,
        server_id: &str,
        hostname: ServerHostname,
        password: &str,
        old_password: Option<&str>,
    ) -> Result<(Self, String), Error> {
        let root = scheduled_root(target_guard.path(), server_id);
        let recovery_path = root.join("unencrypted-metadata.json");
        let initializing = match tokio::fs::metadata(&root).await {
            Ok(_) => false,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(error) => return Err(error.into()),
        };
        let (recovery, encryption_key) = if !initializing {
            let password = old_password.unwrap_or(password);
            let recovery: ScheduledBackupRecoveryInfo =
                read_json_file_bounded(&recovery_path, MAX_BACKUP_RECOVERY_METADATA_BYTES).await?;
            let key = recovery.encryption_key(password)?;
            (recovery, key)
        } else {
            let encryption_key = base32::encode(
                base32::Alphabet::Rfc4648 { padding: false },
                &rand::random::<[u8; 32]>(),
            );
            let recovery = ScheduledBackupRecoveryInfo {
                target_instance_id: Guid::new().to_string(),
                hostname,
                version: crate::version::Current::default().semver(),
                timestamp: Utc::now(),
                password_hash: argon2::hash_encoded(
                    password.as_bytes(),
                    &rand::random::<[u8; 16]>(),
                    &argon2::Config::rfc9106_low_mem(),
                )
                .with_kind(ErrorKind::PasswordHashGeneration)?,
                wrapped_key: base32::encode(
                    base32::Alphabet::Rfc4648 { padding: true },
                    &encrypt_slice(&encryption_key, password),
                ),
                has_system_backup: Some(false),
            };
            (recovery, encryption_key)
        };
        let guard = Self::mount_inner(
            target_guard,
            server_id,
            recovery,
            recovery_path,
            &encryption_key,
            initializing,
        )
        .await?;
        Ok((guard, encryption_key))
    }

    pub async fn mount_with_key(
        target_guard: G,
        server_id: &str,
        expected_target_instance_id: &str,
        encryption_key: &str,
    ) -> Result<Self, Error> {
        let recovery_path =
            scheduled_root(target_guard.path(), server_id).join("unencrypted-metadata.json");
        let recovery: ScheduledBackupRecoveryInfo =
            read_json_file_bounded(&recovery_path, MAX_BACKUP_RECOVERY_METADATA_BYTES).await?;
        if recovery.target_instance_id != expected_target_instance_id {
            return Err(Error::new(
                TargetIdentityMismatch(t!("backup.scheduled.target-identity-mismatch").to_string()),
                ErrorKind::InvalidRequest,
            ));
        }
        Self::mount_inner(
            target_guard,
            server_id,
            recovery,
            recovery_path,
            encryption_key,
            false,
        )
        .await
    }

    pub async fn mount_with_password(
        target_guard: G,
        server_id: &str,
        expected_target_instance_id: &str,
        password: &str,
    ) -> Result<(Self, String), Error> {
        let (guard, encryption_key) =
            Self::discover_with_password(target_guard, server_id, password).await?;
        if guard.recovery.target_instance_id != expected_target_instance_id {
            return Err(Error::new(
                TargetIdentityMismatch(t!("backup.scheduled.target-identity-mismatch").to_string()),
                ErrorKind::InvalidRequest,
            ));
        }
        Ok((guard, encryption_key))
    }

    pub async fn discover_with_password(
        target_guard: G,
        server_id: &str,
        password: &str,
    ) -> Result<(Self, String), Error> {
        let recovery_path =
            scheduled_root(target_guard.path(), server_id).join("unencrypted-metadata.json");
        let recovery: ScheduledBackupRecoveryInfo =
            read_json_file_bounded(&recovery_path, MAX_BACKUP_RECOVERY_METADATA_BYTES).await?;
        let encryption_key = recovery.encryption_key(password)?;
        let guard = Self::mount_inner(
            target_guard,
            server_id,
            recovery,
            recovery_path,
            &encryption_key,
            false,
        )
        .await?;
        Ok((guard, encryption_key))
    }

    async fn mount_inner(
        target_guard: G,
        server_id: &str,
        recovery: ScheduledBackupRecoveryInfo,
        recovery_path: PathBuf,
        encryption_key: &str,
        initializing: bool,
    ) -> Result<Self, Error> {
        let crypt_path = scheduled_root(target_guard.path(), server_id).join("crypt");
        tokio::fs::create_dir_all(&crypt_path)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, crypt_path.display()))?;
        let encrypted_guard =
            TmpMountGuard::mount(&BackupFS::new(&crypt_path, encryption_key), ReadWrite).await?;
        let metadata_path = encrypted_guard.path().join("metadata.json");
        let metadata = read_target_metadata(
            &metadata_path,
            initializing.then_some(recovery.target_instance_id.as_str()),
        )
        .await?;
        if metadata.target_instance_id != recovery.target_instance_id {
            return Err(Error::new(
                TargetIdentityMismatch(
                    t!("backup.scheduled.metadata-identity-mismatch").to_string(),
                ),
                ErrorKind::InvalidRequest,
            ));
        }
        Ok(Self {
            target_guard: Some(target_guard),
            encrypted_guard: Some(encrypted_guard),
            recovery_path,
            recovery,
            metadata,
        })
    }

    pub async fn staging(
        self: &Arc<Self>,
        run_id: &Guid,
        package_id: &PackageId,
    ) -> Result<SubPath<Arc<Self>>, Error> {
        let relative = PathBuf::from("staging")
            .join(run_id.as_ref())
            .join(&**package_id);
        let staging_path = self.path().join(&relative);
        delete_dir(&staging_path).await?;
        if let Some(previous) = self.latest_snapshot(package_id) {
            dir_copy(
                self.snapshot_path(package_id, &previous.id),
                &staging_path,
                None,
            )
            .await?;
        } else {
            tokio::fs::create_dir_all(&staging_path).await?;
        }
        Ok(SubPath::new(self.clone(), relative))
    }

    pub fn latest_snapshot(&self, package_id: &PackageId) -> Option<&ServiceSnapshot> {
        self.metadata.services.get(package_id).and_then(|history| {
            history
                .snapshots
                .iter()
                .filter(|s| !s.archived)
                .max_by_key(|s| s.completed_at)
        })
    }

    pub fn snapshot_path(
        &self,
        package_id: &PackageId,
        snapshot_id: &ServiceSnapshotId,
    ) -> PathBuf {
        self.path()
            .join("services")
            .join(&**package_id)
            .join("snapshots")
            .join(snapshot_id.as_ref())
    }

    pub fn snapshot(
        self: &Arc<Self>,
        package_id: &PackageId,
        snapshot_id: &ServiceSnapshotId,
    ) -> SubPath<Arc<Self>> {
        SubPath::new(
            self.clone(),
            PathBuf::from("services")
                .join(&**package_id)
                .join("snapshots")
                .join(snapshot_id.as_ref()),
        )
    }

    pub async fn promote(
        &mut self,
        run_id: &Guid,
        mut snapshot: ServiceSnapshot,
        timezone: String,
        policy: RetentionPolicy,
    ) -> Result<ServiceSnapshot, Error> {
        let staging_path = self
            .path()
            .join("staging")
            .join(run_id.as_ref())
            .join(&*snapshot.package_id);
        snapshot.logical_size = dir_size(&staging_path, None).await?;
        let destination = self.snapshot_path(&snapshot.package_id, &snapshot.id);
        rename(&staging_path, &destination).await?;

        self.recovery.timestamp = snapshot.completed_at;
        self.recovery.version = crate::version::Current::default().semver();
        if snapshot.package_id == *SYSTEM_PACKAGE_ID {
            self.recovery.has_system_backup = Some(true);
        }

        let history = self
            .metadata
            .services
            .entry(snapshot.package_id.clone())
            .or_insert_with(|| OnTargetServiceHistory {
                timezone: timezone.clone(),
                policy: policy.clone(),
                archived: false,
                snapshots: Vec::new(),
            });
        history.timezone = timezone;
        history.policy = policy;
        history.snapshots.push(snapshot.clone());

        self.save().await?;
        self.prune(&snapshot.package_id).await?;
        self.remove_unreferenced_runs().await?;
        self.save().await?;
        Ok(snapshot)
    }

    async fn prune(&mut self, package_id: &PackageId) -> Result<(), Error> {
        let Some(history) = self.metadata.services.get(package_id) else {
            return Ok(());
        };
        if history.archived {
            return Ok(());
        }
        let timezone = history.timezone.parse().map_err(|_| {
            Error::new(
                eyre!("{}", t!("backup.scheduled.stored-timezone-invalid")),
                ErrorKind::Backup,
            )
        })?;
        let retained = history
            .policy
            .retained_snapshot_ids(&history.snapshots, timezone)?;
        let removed: BTreeSet<_> = history
            .snapshots
            .iter()
            .filter(|snapshot| !snapshot.archived && !retained.contains(&snapshot.id))
            .map(|snapshot| snapshot.id.clone())
            .collect();
        remove_snapshots(
            &self.path().to_owned(),
            &mut self.metadata,
            &self.recovery_path,
            &mut self.recovery,
            &BTreeMap::from([(package_id.clone(), removed)]),
        )
        .await
    }

    pub async fn apply_policy(
        &mut self,
        package_id: &PackageId,
        timezone: String,
        policy: RetentionPolicy,
        confirmed_removals: &BTreeSet<ServiceSnapshotId>,
    ) -> Result<(), Error> {
        let history = self
            .metadata
            .services
            .get_mut(package_id)
            .or_not_found(package_id)?;
        history.change_policy(timezone, policy, confirmed_removals)?;
        self.prune(package_id).await?;
        self.remove_unreferenced_runs().await?;
        self.save().await
    }

    pub async fn delete_archived_snapshots_bulk(
        &mut self,
        snapshots: &BTreeMap<PackageId, BTreeSet<ServiceSnapshotId>>,
    ) -> Result<(), Error> {
        for (package_id, snapshot_ids) in snapshots {
            let history = self
                .metadata
                .services
                .get(package_id)
                .or_not_found(package_id)?;
            let existing: BTreeSet<_> = history
                .snapshots
                .iter()
                .filter(|snapshot| snapshot.archived)
                .map(|snapshot| snapshot.id.clone())
                .collect();
            if !snapshot_ids.is_subset(&existing) {
                return Err(Error::new(
                    eyre!("{}", t!("backup.scheduled.snapshot-delete-stale")),
                    ErrorKind::InvalidRequest,
                ));
            }
        }

        remove_snapshots(
            &self.path().to_owned(),
            &mut self.metadata,
            &self.recovery_path,
            &mut self.recovery,
            snapshots,
        )
        .await?;
        self.remove_unreferenced_runs().await
    }

    async fn remove_unreferenced_runs(&self) -> Result<(), Error> {
        let referenced: std::collections::BTreeSet<_> = self
            .metadata
            .services
            .values()
            .flat_map(|history| history.snapshots.iter())
            .map(|snapshot| snapshot.run_id.to_string())
            .collect();
        let runs_path = self.path().join("runs");
        let mut entries = match tokio::fs::read_dir(&runs_path).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let run_id = name.split('.').next().unwrap_or_default();
            if !referenced.contains(run_id) {
                delete_file(entry.path()).await?;
            }
        }
        Ok(())
    }

    pub async fn save_run(&self, run: &BackupRun) -> Result<(), Error> {
        write_json(
            &self.path().join("runs").join(format!("{}.json", run.id)),
            run,
        )
        .await
    }

    pub async fn save(&mut self) -> Result<(), Error> {
        self.recovery.has_system_backup = Some(contains_system_backup(&self.metadata));
        write_json(&self.path().join("metadata.json"), &self.metadata).await?;
        write_json(&self.recovery_path, &self.recovery).await
    }

    pub(super) async fn reload_metadata(&mut self) -> Result<(), Error> {
        self.metadata = read_target_metadata(&self.path().join("metadata.json"), None).await?;
        Ok(())
    }

    pub async fn save_and_unmount(mut self) -> Result<(), Error> {
        self.save().await?;
        self.unmount().await
    }
}

impl<G: GenericMountGuard> GenericMountGuard for ScheduledBackupMountGuard<G> {
    fn path(&self) -> &Path {
        self.encrypted_guard
            .as_ref()
            .expect("scheduled backup is mounted")
            .path()
    }

    async fn unmount(mut self) -> Result<(), Error> {
        if let Some(guard) = self.encrypted_guard.take() {
            crate::disk::mount::util::sync_directory(guard.path()).await?;
            guard.unmount().await?;
        }
        if let Some(guard) = self.target_guard.take() {
            guard.unmount().await?;
        }
        Ok(())
    }
}

impl<G: GenericMountGuard> Drop for ScheduledBackupMountGuard<G> {
    fn drop(&mut self) {
        let encrypted = self.encrypted_guard.take();
        let target = self.target_guard.take();
        tokio::spawn(async move {
            if let Some(guard) = encrypted {
                crate::disk::mount::util::sync_directory(guard.path())
                    .await
                    .log_err();
                guard.unmount().await.log_err();
            }
            if let Some(guard) = target {
                guard.unmount().await.log_err();
            }
        });
    }
}

fn scheduled_root(target_path: &Path, server_id: &str) -> PathBuf {
    target_path
        .join(BACKUP_DIR_NAME)
        .join(format!("{server_id}.automatic"))
}

fn set_archive_state(history: &mut OnTargetServiceHistory, archived: bool) {
    history.archived = archived;
    if archived {
        for snapshot in &mut history.snapshots {
            snapshot.archived = true;
        }
    }
}

fn contains_system_backup(metadata: &ScheduledBackupOnTargetMetadata) -> bool {
    metadata
        .services
        .get(&*SYSTEM_PACKAGE_ID)
        .is_some_and(|history| !history.snapshots.is_empty())
}

async fn read_target_metadata(
    path: &Path,
    new_target_instance_id: Option<&str>,
) -> Result<ScheduledBackupOnTargetMetadata, Error> {
    if let Some(target_instance_id) = new_target_instance_id {
        return Ok(ScheduledBackupOnTargetMetadata {
            target_instance_id: target_instance_id.to_owned(),
            services: BTreeMap::new(),
        });
    }
    read_json_file_bounded(path, MAX_BACKUP_TARGET_METADATA_BYTES).await
}

async fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    write_file_atomic(path, IoFormat::Json.to_vec(value)?).await
}

async fn remove_snapshots(
    target: &Path,
    metadata: &mut ScheduledBackupOnTargetMetadata,
    recovery_path: &Path,
    recovery: &mut ScheduledBackupRecoveryInfo,
    removals: &BTreeMap<PackageId, BTreeSet<ServiceSnapshotId>>,
) -> Result<(), Error> {
    if removals.values().all(BTreeSet::is_empty) {
        return Ok(());
    }
    let mut updated = metadata.clone();
    for (package_id, snapshot_ids) in removals {
        updated
            .services
            .get_mut(package_id)
            .or_not_found(package_id)?
            .snapshots
            .retain(|snapshot| !snapshot_ids.contains(&snapshot.id));
    }
    // Persist removals before deleting checkpoint data.
    write_json(&target.join("metadata.json"), &updated).await?;
    recovery.has_system_backup = Some(contains_system_backup(&updated));
    write_json(recovery_path, recovery).await?;
    crate::disk::mount::util::sync_directory(recovery_path.parent().expect("recovery directory"))
        .await?;
    crate::disk::mount::util::sync_directory(target).await?;
    *metadata = updated;
    for (package_id, snapshot_ids) in removals {
        let snapshots = target
            .join("services")
            .join(&**package_id)
            .join("snapshots");
        for snapshot_id in snapshot_ids {
            delete_dir(snapshots.join(snapshot_id.as_ref())).await?;
        }
    }
    crate::disk::mount::util::sync_directory(target).await
}

/// Requires exclusive ownership of backup operations.
pub(super) async fn remove_unreferenced_snapshots(
    target: &Path,
    metadata: &ScheduledBackupOnTargetMetadata,
) -> Result<(), Error> {
    let services = target.join("services");
    let mut packages = match tokio::fs::read_dir(&services).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let mut removed = false;
    while let Some(package) = packages.next_entry().await? {
        let package_id = package.file_name();
        let referenced: BTreeSet<_> = metadata
            .services
            .get(package_id.to_string_lossy().as_ref())
            .into_iter()
            .flat_map(|history| &history.snapshots)
            .map(|snapshot| snapshot.id.as_ref())
            .collect();
        let mut snapshots = match tokio::fs::read_dir(package.path().join("snapshots")).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        while let Some(snapshot) = snapshots.next_entry().await? {
            let id = snapshot.file_name();
            if !referenced.contains(id.to_string_lossy().as_ref()) {
                delete_dir(snapshot.path()).await?;
                removed = true;
            }
        }
    }
    if removed {
        crate::disk::mount::util::sync_directory(target).await?;
    }
    Ok(())
}

/// Requires exclusive ownership of backup operations.
pub(super) async fn remove_abandoned_staging(target: &Path) -> Result<(), Error> {
    let staging = target.join("staging");
    if tokio::fs::try_exists(&staging).await? {
        delete_dir(staging).await?;
        crate::disk::mount::util::sync_directory(target).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::scheduled::BackupSource;

    fn snapshot() -> ServiceSnapshot {
        ServiceSnapshot {
            id: Guid::new(),
            package_id: "test-service".parse().unwrap(),
            package_version: "1.0.0".into(),
            source: BackupSource::Scheduled,
            job_id: Guid::new(),
            job_name: "Daily".into(),
            run_id: Guid::new(),
            completed_at: Utc::now(),
            logical_size: 1,
            physical_size: None,
            changed_bytes: None,
            measured_at: Utc::now(),
            archived: false,
        }
    }

    #[tokio::test]
    async fn interrupted_deletion_keeps_only_intact_checkpoints_advertised() {
        let root = tempfile::tempdir().unwrap();
        let retained = snapshot();
        let removed = ServiceSnapshot {
            id: Guid::from("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap(),
            ..snapshot()
        };
        let failing = ServiceSnapshot {
            id: Guid::from("BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB").unwrap(),
            ..snapshot()
        };
        let package_id = retained.package_id.clone();
        let snapshots_path = root.path().join("services/test-service/snapshots");
        for snapshot in [&retained, &removed] {
            let path = snapshots_path.join(snapshot.id.as_ref());
            tokio::fs::create_dir_all(&path).await.unwrap();
            tokio::fs::write(path.join("data"), b"checkpoint")
                .await
                .unwrap();
        }
        let failing_path = snapshots_path.join(failing.id.as_ref());
        tokio::fs::write(&failing_path, b"not a directory")
            .await
            .unwrap();
        let mut metadata = ScheduledBackupOnTargetMetadata {
            target_instance_id: "target".into(),
            services: BTreeMap::from([(
                package_id.clone(),
                OnTargetServiceHistory {
                    timezone: "UTC".into(),
                    policy: RetentionPolicy::latest_only(),
                    archived: false,
                    snapshots: vec![retained.clone(), removed.clone(), failing.clone()],
                },
            )]),
        };
        write_json(&root.path().join("metadata.json"), &metadata)
            .await
            .unwrap();
        let removals = BTreeMap::from([(
            package_id.clone(),
            BTreeSet::from([removed.id.clone(), failing.id.clone()]),
        )]);
        let recovery_path = root.path().join("unencrypted-metadata.json");
        let mut recovery = ScheduledBackupRecoveryInfo {
            target_instance_id: "target".into(),
            hostname: ServerHostname::new("test-server".into()).unwrap(),
            version: "0.4.0".parse().unwrap(),
            timestamp: Utc::now(),
            password_hash: String::new(),
            wrapped_key: String::new(),
            has_system_backup: Some(true),
        };
        assert!(
            remove_snapshots(
                root.path(),
                &mut metadata,
                &recovery_path,
                &mut recovery,
                &removals
            )
            .await
            .is_err()
        );
        assert!(!snapshots_path.join(removed.id.as_ref()).exists());
        let reloaded_recovery: ScheduledBackupRecoveryInfo =
            read_json_file_bounded(&recovery_path, MAX_BACKUP_RECOVERY_METADATA_BYTES)
                .await
                .unwrap();
        assert_eq!(reloaded_recovery.has_system_backup, Some(false));

        let reloaded = read_target_metadata(&root.path().join("metadata.json"), None)
            .await
            .unwrap();
        let advertised = &reloaded.services[&package_id].snapshots;
        assert_eq!(advertised.len(), 1);
        assert_eq!(advertised[0].id, retained.id);
        assert_eq!(
            tokio::fs::read(snapshots_path.join(retained.id.as_ref()).join("data"))
                .await
                .unwrap(),
            b"checkpoint"
        );

        tokio::fs::remove_file(&failing_path).await.unwrap();
        tokio::fs::create_dir(&failing_path).await.unwrap();
        remove_unreferenced_snapshots(root.path(), &reloaded)
            .await
            .unwrap();
        remove_unreferenced_snapshots(root.path(), &reloaded)
            .await
            .unwrap();
        assert!(!failing_path.exists());
        assert!(!snapshots_path.join(removed.id.as_ref()).exists());
        assert!(
            snapshots_path
                .join(retained.id.as_ref())
                .join("data")
                .exists()
        );
    }

    #[test]
    fn retention_can_change_before_the_first_checkpoint() {
        let package_id: PackageId = "test-service".parse().unwrap();
        let mut metadata = ScheduledBackupOnTargetMetadata {
            target_instance_id: "target".into(),
            services: BTreeMap::new(),
        };
        let mut local = ServiceTargetHistory {
            target_id: "cifs-0".parse().unwrap(),
            target_instance_id: "target".into(),
            package_id: package_id.clone(),
            timezone: "UTC".into(),
            policy: RetentionPolicy::latest_only(),
            feeding_jobs: BTreeSet::from([Guid::new()]),
            snapshots: Vec::new(),
            archived: false,
        };
        metadata.reconcile_histories([local.clone()]);
        let policy = RetentionPolicy {
            tiers: vec![super::super::RetentionTier {
                interval_seconds: 3600,
                coverage_seconds: 86400,
            }],
        };
        metadata
            .services
            .get_mut(&package_id)
            .unwrap()
            .change_policy("UTC".into(), policy.clone(), &BTreeSet::new())
            .unwrap();
        assert_eq!(metadata.services[&package_id].policy, policy);

        metadata.services.clear();
        local.snapshots.push(snapshot());
        metadata.reconcile_histories([local.clone()]);
        assert!(metadata.services.is_empty());
        local.snapshots.clear();
        local.target_instance_id = "another-target".into();
        metadata.reconcile_histories([local]);
        assert!(metadata.services.is_empty());
    }

    #[test]
    fn saved_history_updates_preserve_archive_decisions_and_committed_policy() {
        let retained = snapshot();
        let removed = snapshot();
        let package_id = retained.package_id.clone();
        let policy = RetentionPolicy {
            tiers: vec![super::super::RetentionTier {
                interval_seconds: 3600,
                coverage_seconds: 86400,
            }],
        };
        let mut metadata = ScheduledBackupOnTargetMetadata {
            target_instance_id: "target".into(),
            services: BTreeMap::from([(
                package_id.clone(),
                OnTargetServiceHistory {
                    timezone: "UTC".into(),
                    policy: policy.clone(),
                    archived: false,
                    snapshots: vec![retained.clone()],
                },
            )]),
        };
        let mut local = ServiceTargetHistory {
            target_id: "cifs-0".parse().unwrap(),
            target_instance_id: "target".into(),
            package_id: package_id.clone(),
            timezone: "UTC".into(),
            policy: RetentionPolicy::latest_only(),
            feeding_jobs: BTreeSet::new(),
            snapshots: vec![retained.clone(), removed],
            archived: true,
        };
        metadata.refresh_history(&mut local);
        assert!(local.archived);
        assert_eq!(local.snapshots.len(), 1);
        assert_eq!(local.snapshots[0].id, retained.id);
        assert!(local.snapshots[0].archived);
        assert_eq!(local.policy, policy);

        local.archived = false;
        local.feeding_jobs.insert(Guid::new());
        metadata
            .services
            .get_mut(&package_id)
            .unwrap()
            .snapshots
            .push(snapshot());
        metadata.refresh_history(&mut local);
        assert!(!local.archived);
        assert!(local.snapshots[0].archived);
        assert!(!local.snapshots[1].archived);

        local.policy = RetentionPolicy::latest_only();
        metadata
            .services
            .get_mut(&package_id)
            .unwrap()
            .snapshots
            .clear();
        metadata.refresh_history(&mut local);
        assert!(local.snapshots.is_empty());
        assert_eq!(local.policy, policy);
    }

    #[tokio::test]
    async fn abandoned_staging_cleanup_preserves_checkpoints_and_metadata() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging/interrupted-run/test-service");
        let snapshot = root
            .path()
            .join("services/test-service/snapshots/checkpoint");
        tokio::fs::create_dir_all(&staging).await.unwrap();
        tokio::fs::create_dir_all(&snapshot).await.unwrap();
        tokio::fs::write(staging.join("partial"), b"incomplete")
            .await
            .unwrap();
        tokio::fs::write(snapshot.join("data"), b"retained")
            .await
            .unwrap();
        tokio::fs::write(root.path().join("metadata.json"), b"metadata")
            .await
            .unwrap();

        remove_abandoned_staging(root.path()).await.unwrap();
        remove_abandoned_staging(root.path()).await.unwrap();

        assert!(!root.path().join("staging").exists());
        assert_eq!(
            tokio::fs::read(snapshot.join("data")).await.unwrap(),
            b"retained"
        );
        assert_eq!(
            tokio::fs::read(root.path().join("metadata.json"))
                .await
                .unwrap(),
            b"metadata"
        );
    }

    #[test]
    fn existing_store_requires_its_original_password() {
        let original = "original password";
        let key = "target encryption key";
        let recovery = ScheduledBackupRecoveryInfo {
            target_instance_id: "target".into(),
            hostname: ServerHostname::new("test-server".into()).unwrap(),
            version: "0.4.0".parse().unwrap(),
            timestamp: Utc::now(),
            password_hash: argon2::hash_encoded(
                original.as_bytes(),
                b"test-password-salt",
                &argon2::Config::default(),
            )
            .unwrap(),
            wrapped_key: base32::encode(
                base32::Alphabet::Rfc4648 { padding: true },
                &encrypt_slice(key, original),
            ),
            has_system_backup: Some(false),
        };
        assert_eq!(recovery.encryption_key(original).unwrap(), key);
        assert_eq!(
            recovery
                .encryption_key("changed server password")
                .unwrap_err()
                .kind,
            ErrorKind::IncorrectPassword,
        );
    }

    #[test]
    fn reconnect_preserves_offline_archive_decisions_and_empty_history_settings() {
        let package_id: PackageId = "test-service".parse().unwrap();
        let now = Utc::now();
        let snapshot = ServiceSnapshot {
            id: ServiceSnapshotId::new(),
            package_id: package_id.clone(),
            package_version: "1.0.0".into(),
            source: BackupSource::Scheduled,
            job_id: Guid::new(),
            job_name: "Daily".into(),
            run_id: Guid::new(),
            completed_at: now,
            logical_size: 1,
            physical_size: None,
            changed_bytes: None,
            measured_at: now,
            archived: false,
        };
        let mut metadata = ScheduledBackupOnTargetMetadata {
            target_instance_id: "target".into(),
            services: BTreeMap::from([(
                package_id.clone(),
                OnTargetServiceHistory {
                    timezone: "UTC".into(),
                    policy: RetentionPolicy::latest_only(),
                    archived: false,
                    snapshots: vec![snapshot.clone()],
                },
            )]),
        };
        let mut local = ServiceTargetHistory {
            target_id: "cifs-0".parse().unwrap(),
            target_instance_id: "target".into(),
            package_id: package_id.clone(),
            timezone: "America/New_York".into(),
            policy: RetentionPolicy {
                tiers: vec![super::super::RetentionTier {
                    interval_seconds: 3600,
                    coverage_seconds: 86400,
                }],
            },
            feeding_jobs: BTreeSet::new(),
            snapshots: vec![ServiceSnapshot {
                archived: true,
                ..snapshot.clone()
            }],
            archived: true,
        };

        metadata.reconcile_histories([local.clone()]);
        let history = &metadata.services[&package_id];
        assert!(history.archived);
        assert!(history.snapshots[0].archived);
        assert_eq!(history.timezone, "UTC");
        assert_eq!(history.policy, RetentionPolicy::latest_only());

        local.archived = false;
        local.feeding_jobs.insert(Guid::new());
        metadata.reconcile_histories([local.clone()]);
        let history = metadata.services.get_mut(&package_id).unwrap();
        assert!(!history.archived);
        history.snapshots.push(ServiceSnapshot {
            id: ServiceSnapshotId::new(),
            completed_at: now + chrono::Duration::hours(1),
            ..snapshot
        });
        assert!(
            history
                .policy
                .preview(&history.snapshots, chrono_tz::UTC)
                .unwrap()
                .removed
                .is_empty()
        );

        history.snapshots.clear();
        metadata.reconcile_histories([local.clone()]);
        let history = &metadata.services[&package_id];
        assert_eq!(history.timezone, local.timezone);
        assert_eq!(history.policy, local.policy);

        local.target_instance_id = "another-target".into();
        local.timezone = "Asia/Tokyo".into();
        local.archived = true;
        metadata.reconcile_histories([local]);
        assert_eq!(metadata.services[&package_id].timezone, "America/New_York");
        assert!(!metadata.services[&package_id].archived);
    }

    #[tokio::test]
    async fn existing_target_requires_readable_metadata() {
        let root = std::env::temp_dir().join(format!("backup-metadata-{}", Guid::new()));
        tokio::fs::create_dir(&root).await.unwrap();
        let path = root.join("metadata.json");

        assert!(read_target_metadata(&path, None).await.is_err());
        let fresh = read_target_metadata(&path, Some("new-instance"))
            .await
            .unwrap();
        assert_eq!(fresh.target_instance_id, "new-instance");
        assert!(fresh.services.is_empty());

        tokio::fs::write(&path, b"invalid metadata").await.unwrap();
        assert!(read_target_metadata(&path, None).await.is_err());
        tokio::fs::write(&path, serde_json::to_vec(&fresh).unwrap())
            .await
            .unwrap();
        assert_eq!(
            read_target_metadata(&path, None)
                .await
                .unwrap()
                .target_instance_id,
            "new-instance"
        );
        tokio::fs::remove_file(&path).await.unwrap();
        tokio::fs::create_dir(&path).await.unwrap();
        assert!(read_target_metadata(&path, None).await.is_err());
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[test]
    fn policy_change_rejects_unconfirmed_target_snapshots() {
        let snapshot = |seconds| {
            let completed_at = DateTime::from_timestamp(seconds, 0).unwrap();
            ServiceSnapshot {
                id: ServiceSnapshotId::new(),
                package_id: "test-service".parse().unwrap(),
                package_version: "1.0.0".into(),
                source: BackupSource::Scheduled,
                job_id: Guid::new(),
                job_name: "Daily".into(),
                run_id: Guid::new(),
                completed_at,
                logical_size: 1,
                physical_size: None,
                changed_bytes: None,
                measured_at: completed_at,
                archived: false,
            }
        };
        let known = snapshot(1);
        let target_only = snapshot(2);
        let newest = snapshot(3);
        let mut history = OnTargetServiceHistory {
            timezone: "UTC".into(),
            policy: RetentionPolicy {
                tiers: vec![super::super::RetentionTier {
                    interval_seconds: 1,
                    coverage_seconds: 10,
                }],
            },
            archived: false,
            snapshots: vec![known.clone(), target_only.clone(), newest],
        };
        let original = history.policy.clone();
        assert!(
            history
                .change_policy(
                    "UTC".into(),
                    RetentionPolicy::latest_only(),
                    &BTreeSet::from([known.id.clone()]),
                )
                .is_err()
        );
        assert_eq!(history.policy, original);
        history
            .change_policy(
                "UTC".into(),
                RetentionPolicy::latest_only(),
                &BTreeSet::from([known.id, target_only.id]),
            )
            .unwrap();
        assert_eq!(history.policy, RetentionPolicy::latest_only());
    }

    #[test]
    fn scheduled_root_is_separate_from_the_manual_backup_set() {
        let target = Path::new("/target");
        let manual = target.join(BACKUP_DIR_NAME).join("server-id");
        let scheduled = scheduled_root(target, "server-id");
        assert_eq!(
            scheduled,
            target.join(BACKUP_DIR_NAME).join("server-id.automatic")
        );
        assert_ne!(scheduled, manual);
    }

    #[test]
    fn reactivating_history_does_not_unarchive_old_snapshots() {
        let now = Utc::now();
        let mut history = OnTargetServiceHistory {
            timezone: "UTC".into(),
            policy: RetentionPolicy::latest_only(),
            archived: true,
            snapshots: vec![ServiceSnapshot {
                id: ServiceSnapshotId::new(),
                package_id: "test-service".parse().unwrap(),
                package_version: "1.0.0".into(),
                source: BackupSource::Scheduled,
                job_id: Guid::new(),
                job_name: "Nightly".into(),
                run_id: Guid::new(),
                completed_at: now,
                logical_size: 1,
                physical_size: None,
                changed_bytes: None,
                measured_at: now,
                archived: true,
            }],
        };

        set_archive_state(&mut history, false);
        assert!(!history.archived);
        assert!(history.snapshots[0].archived);

        history.snapshots[0].archived = false;
        set_archive_state(&mut history, true);
        assert!(history.snapshots[0].archived);
    }

    #[test]
    fn recovery_eligibility_tracks_system_snapshots() {
        let package_id = SYSTEM_PACKAGE_ID.clone();
        let now = Utc::now();
        let mut metadata = ScheduledBackupOnTargetMetadata {
            target_instance_id: "target".to_owned(),
            services: BTreeMap::from([(
                package_id.clone(),
                OnTargetServiceHistory {
                    timezone: "UTC".to_owned(),
                    policy: RetentionPolicy::latest_only(),
                    archived: true,
                    snapshots: vec![ServiceSnapshot {
                        id: ServiceSnapshotId::new(),
                        package_id,
                        package_version: "1.0.0".to_owned(),
                        source: BackupSource::Scheduled,
                        job_id: Guid::new(),
                        job_name: "Nightly".to_owned(),
                        run_id: Guid::new(),
                        completed_at: now,
                        logical_size: 1,
                        physical_size: None,
                        changed_bytes: None,
                        measured_at: now,
                        archived: true,
                    }],
                },
            )]),
        };

        assert!(contains_system_backup(&metadata));
        metadata
            .services
            .get_mut(&*SYSTEM_PACKAGE_ID)
            .unwrap()
            .snapshots
            .clear();
        assert!(!contains_system_backup(&metadata));
    }
}
