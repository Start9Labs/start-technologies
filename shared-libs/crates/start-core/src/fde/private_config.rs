//! Private configuration remains on unlocked data; signed snapshots contain references.
//!
//! Callers retain the encrypted data mount, quiesce configuration writers and
//! protect transient credentials through preparation and publication.
//! Staging preserves legacy boot files. Their replacement requires durable compatible
//! protected EFI activation and an available runtime view. Backings are data placement,
//! not active-protection evidence. The generation/activation owner prunes abandoned
//! candidates only after their references retire and reclaims obsolete public extents.

#[cfg(test)]
#[path = "private_config_tests.rs"]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use openssl::pkey::{PKeyRef, Private};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use zeroize::Zeroizing;

use crate::os_overlay::{PreparedGeneration, sync_tree};
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::s9pk::merkle_archive::source::{DynFileSource, FileSource};
use crate::util::io::{
    AtomicFile, confined_relative, create_directory_beneath, open_file, to_tmp_path,
};
use crate::util::squashfs::{
    Entry, Metadata, Node, NodeContents, Squashfs, overlay_whiteout, resolve_hardlink,
    snapshot_entries,
};

const DIRECTORY: &str = "fde-private-config";
pub(crate) const RUNTIME: &str = "/run/startos-private-config";

/// Preserves shadow-group traversal; `disk::main::unmount_fs` releases the view.
pub(crate) async fn mount_runtime(data: &Path) -> Result<(), Error> {
    mount_view(data, Path::new(RUNTIME)).await
}

async fn mount_view(data: &Path, runtime: &Path) -> Result<(), Error> {
    let source = data.join(DIRECTORY);
    match tokio::fs::symlink_metadata(&source).await {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err(invalid_layout(&source)),
        Err(error) => return Err(error.into()),
    }
    require_data_filesystem(data, &source).await?;
    if crate::util::io::same_file(&source, runtime).await? {
        return Ok(());
    }
    if crate::disk::mount::util::is_mountpoint(runtime).await? {
        return Err(invalid_layout(runtime));
    }
    crate::disk::mount::util::bind(source, runtime, false).await
}

pub(crate) async fn unmount_runtime(data: &Path) -> Result<(), Error> {
    unmount_view(data, Path::new(RUNTIME)).await
}

async fn unmount_view(data: &Path, runtime: &Path) -> Result<(), Error> {
    let source = data.join(DIRECTORY);
    if crate::util::io::same_file(&source, runtime).await? {
        require_data_filesystem(data, &source).await?;
        crate::disk::mount::util::unmount(runtime, false).await?;
    }
    Ok(())
}

pub(crate) struct ProjectedConfig {
    snapshot: Squashfs<DynFileSource>,
    replacements: BTreeMap<PathBuf, PathBuf>,
}

fn invalid_layout(path: &Path) -> Error {
    Error::new(
        color_eyre::eyre::eyre!(t!(
            "fde.private-config.invalid-layout",
            path = path.display()
        )),
        ErrorKind::InvalidRequest,
    )
}

async fn require_data_filesystem(data: &Path, path: &Path) -> Result<(), Error> {
    if crate::util::io::file_identity(data).await?.0
        != crate::util::io::file_identity(path).await?.0
    {
        return Err(invalid_layout(path));
    }
    Ok(())
}

async fn backing_file(data: &Path, target: &Path) -> Result<PathBuf, Error> {
    let relative = target
        .strip_prefix(RUNTIME)
        .map_err(|_| invalid_layout(target))?;
    let relative = confined_relative(relative)?;
    let parent = relative.parent().ok_or_else(|| invalid_layout(target))?;
    crate::disk::mount::filesystem::syscall::open_beneath(
        data,
        &Path::new(DIRECTORY).join(parent),
    )?;
    let id = relative
        .iter()
        .next()
        .and_then(|id| id.to_str())
        .and_then(Guid::from);
    let file = data.join(DIRECTORY).join(&relative);
    if id.is_none()
        || relative.components().count() < 2
        || !tokio::fs::symlink_metadata(&file).await?.is_file()
    {
        return Err(invalid_layout(target));
    }
    require_data_filesystem(data, &file).await?;
    Ok(file)
}

fn resolved<'a>(
    image: &'a Squashfs<DynFileSource>,
    path: &Path,
) -> Result<(PathBuf, &'a Node<DynFileSource>), Error> {
    Ok(resolve_hardlink(path, |path| {
        image.contents().get_path(path).map(|entry| match entry {
            Entry::Node(node) => Ok((path.to_owned(), node)),
            Entry::Hardlink(target) => Err(target.clone()),
        })
    })?)
}

async fn apply_metadata(file: &tokio::fs::File, metadata: &Metadata) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;

    let file = file.try_clone().await?.into_std().await;
    let metadata = metadata.clone();
    tokio::task::spawn_blocking(move || -> Result<(), Error> {
        nix::unistd::fchown(&file, Some(metadata.uid.into()), Some(metadata.gid.into()))
            .with_kind(ErrorKind::Filesystem)?;
        file.set_permissions(std::fs::Permissions::from_mode(metadata.mode as u32))?;
        crate::util::xattrs::Target::File(&file).replace(&metadata.xattrs)?;
        file.set_times(
            std::fs::FileTimes::new()
                .set_modified(UNIX_EPOCH + Duration::from_secs(metadata.modification_time.into())),
        )?;
        Ok(())
    })
    .await
    .with_kind(ErrorKind::Unknown)??;
    Ok(())
}

impl ProjectedConfig {
    /// Requires an unlocked encrypted data mount and quiescent source paths.
    /// Supply both native default and prospective HostKey paths from the admitted root.
    pub(crate) async fn stage(
        snapshot: Squashfs<DynFileSource>,
        data: &Path,
        host_keys: &[PathBuf],
    ) -> Result<Self, Error> {
        let key_directory = Path::new(crate::ssh::HOST_KEY_DIRECTORY);
        let relative_keys = confined_relative(key_directory)?;
        let mut paths = [crate::auth::SHADOW_PATH, crate::util::mok::DKMS_MOK_KEY]
            .into_iter()
            .map(PathBuf::from)
            .chain(host_keys.iter().cloned())
            .collect::<BTreeSet<_>>();
        if snapshot
            .contents()
            .get_path_checked(&relative_keys)
            .map_err(|_| invalid_layout(key_directory))?
            .is_some()
        {
            let (_, node) = resolved(&snapshot, &relative_keys)?;
            let NodeContents::Directory(keys) = &node.contents else {
                return Err(invalid_layout(key_directory));
            };
            for (name, _) in keys.iter() {
                if crate::ssh::is_installer_host_key(name) {
                    paths.insert(key_directory.join(name));
                }
            }
        }
        let mut private = BTreeSet::new();
        for path in &paths {
            if !path.is_absolute() {
                return Err(invalid_layout(path));
            }
            let path = confined_relative(path).map_err(|_| invalid_layout(path))?;
            if snapshot
                .contents()
                .get_path_checked(&path)
                .map_err(|_| invalid_layout(&path))?
                .is_none()
            {
                continue;
            }
            let (canonical, node) = resolved(&snapshot, &path)?;
            if overlay_whiteout(node) {
                if let NodeContents::File(source) = &node.contents
                    && source.size().await? != 0
                {
                    return Err(invalid_layout(&path));
                }
                continue;
            }
            match &node.contents {
                NodeContents::File(_) => {
                    private.insert(canonical);
                }
                NodeContents::Symlink(target) if target.starts_with(RUNTIME) => {}
                _ => return Err(invalid_layout(&path)),
            }
        }
        let mut aliases: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
        let mut replacements = BTreeMap::new();
        for (path, entry) in snapshot_entries(&snapshot) {
            if let Entry::Node(Node {
                contents: NodeContents::Symlink(target),
                ..
            }) = &entry
                && target.starts_with(RUNTIME)
            {
                let backing = backing_file(data, target).await?;
                open_file(&backing).await?.sync_all().await?;
                open_file(backing.parent().expect("credential parent"))
                    .await?
                    .sync_all()
                    .await?;
                replacements.insert(path.clone(), target.clone());
            }
            if matches!(
                entry,
                Entry::Hardlink(_)
                    | Entry::Node(Node {
                        contents: NodeContents::File(_),
                        ..
                    })
            ) {
                let (canonical, _) = resolved(&snapshot, &path)?;
                if private.contains(&canonical) {
                    aliases.entry(canonical).or_default().push(path);
                }
            }
        }
        let mut projected = Self {
            snapshot,
            replacements,
        };
        if aliases.is_empty() {
            return Ok(projected);
        }
        let id = Guid::new().to_string();
        let directory = Path::new(DIRECTORY).join(&id);
        create_directory_beneath(data, &directory)?;
        let backing = data.join(&directory);
        require_data_filesystem(data, &backing).await?;
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(data.join(DIRECTORY), std::fs::Permissions::from_mode(0o755))
            .await?;
        tokio::fs::set_permissions(&backing, std::fs::Permissions::from_mode(0o700)).await?;
        let mut directories = BTreeSet::from([PathBuf::new()]);
        for paths in aliases.values() {
            for path in paths {
                let mut parent = path.parent();
                while let Some(path) = parent {
                    directories.insert(path.to_owned());
                    parent = path.parent();
                }
            }
        }
        for path in &directories {
            create_directory_beneath(&backing, path)?;
        }
        for (canonical, paths) in aliases {
            let (_, node) = resolved(&projected.snapshot, &canonical)?;
            let NodeContents::File(source) = &node.contents else {
                unreachable!()
            };
            let size: usize = source
                .size()
                .await?
                .try_into()
                .map_err(|_| invalid_layout(&canonical))?;
            let mut secret = Zeroizing::new(vec![0; size]);
            source.reader().await?.read_exact(&mut secret).await?;
            let destination = backing.join(&canonical);
            let mut file = AtomicFile::new(&destination, None::<&Path>).await?;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .await?;
            file.write_all(&secret).await?;
            file.flush().await?;
            apply_metadata(&file, &node.metadata).await?;
            file.save().await?;
            let mut metadata = node.metadata.clone();
            metadata.mode = 0o777;
            metadata.xattrs.clear();
            for path in paths {
                if path != canonical {
                    tokio::fs::hard_link(&destination, backing.join(&path)).await?;
                }
                let target = Path::new(RUNTIME).join(&id).join(&path);
                projected.snapshot.contents_mut().insert_path(
                    &path,
                    Entry::new(metadata.clone(), NodeContents::Symlink(target.clone())),
                )?;
                projected.replacements.insert(path, target);
            }
        }
        for path in directories
            .iter()
            .rev()
            .filter(|path| !path.as_os_str().is_empty())
        {
            let (_, node) = resolved(&projected.snapshot, path)?;
            apply_metadata(&open_file(backing.join(path)).await?, &node.metadata).await?;
        }
        // The configuration container's mode does not govern the runtime namespace.
        let mut namespace = Metadata::new(0o755);
        namespace.uid = nix::unistd::geteuid().as_raw();
        namespace.gid = nix::unistd::getegid().as_raw();
        apply_metadata(&open_file(&backing).await?, &namespace).await?;
        sync_tree(&backing).await?;
        open_file(data.join(DIRECTORY)).await?.sync_all().await?;
        open_file(data).await?.sync_all().await?;
        Ok(projected)
    }

    pub(crate) async fn prepare_generation(
        &self,
        rootfs: &Path,
        store: &Path,
        key: &PKeyRef<Private>,
    ) -> Result<PreparedGeneration, Error> {
        PreparedGeneration::prepare_snapshot(self.snapshot.clone(), rootfs, store, key).await
    }

    /// Requires durable compatible protected EFI, the runtime view and excluded legacy writers.
    /// Obsolete public extent reclamation remains the activation owner's obligation.
    pub(crate) async fn replace_legacy(&self, raw: &Path) -> Result<(), Error> {
        for (path, target) in &self.replacements {
            let destination = raw.join(path);
            match tokio::fs::symlink_metadata(&destination).await {
                Ok(metadata) if metadata.is_symlink() => {
                    if tokio::fs::read_link(&destination).await? == *target {
                        continue;
                    }
                    return Err(invalid_layout(path));
                }
                Ok(metadata) if metadata.is_file() => {}
                Ok(_) => return Err(invalid_layout(path)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            }
            let temporary = to_tmp_path(&destination)?;
            tokio::fs::symlink(target, &temporary).await?;
            if let Err(error) = tokio::fs::rename(&temporary, &destination).await {
                tokio::fs::remove_file(&temporary).await.log_err();
                return Err(error.into());
            }
            open_file(destination.parent().expect("credential parent"))
                .await?
                .sync_all()
                .await?;
        }
        Ok(())
    }
}
