use std::path::{Path, PathBuf};

use imbl_value::InternedString;
use tokio::process::Command;

use crate::ImageId;
use crate::disk::mount::filesystem::overlayfs::OverlayGuard;
use crate::disk::mount::guard::GenericMountGuard;
use crate::rpc_continuations::Guid;
use crate::service::effects::prelude::*;
use crate::service::persistent_container::Subcontainer;
use crate::util::Invoke;
use crate::util::io::write_file_owned_atomic;

pub const NVIDIA_OVERLAY_PATH: &str = "/var/tmp/startos/nvidia-overlay";
pub const NVIDIA_OVERLAY_DEBIAN: &str = "/var/tmp/startos/nvidia-overlay/debian";
pub const NVIDIA_OVERLAY_GENERIC: &str = "/var/tmp/startos/nvidia-overlay/generic";

#[cfg(target_os = "linux")]
mod sync;

#[cfg(not(target_os = "linux"))]
mod sync_dummy;

pub use sync::*;
#[cfg(not(target_os = "linux"))]
use sync_dummy as sync;

#[derive(Debug, Deserialize, Serialize, Parser, visit_rs::VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct DestroySubcontainerFsParams {
    guid: Guid,
}

rpc_toolkit::reflect_ts!(DestroySubcontainerFsParams);
rpc_toolkit::ts_export!(DestroySubcontainerFsParams, namespaces = [""]);
#[instrument(skip_all)]
pub async fn destroy_subcontainer_fs(
    context: EffectContext,
    DestroySubcontainerFsParams { guid }: DestroySubcontainerFsParams,
) -> Result<(), Error> {
    let context = context.deref()?;
    if let Some(overlay) = context
        .seed
        .persistent_container
        .subcontainers
        .lock()
        .await
        .remove(&guid)
    {
        #[cfg(target_os = "linux")]
        if tokio::fs::metadata(overlay.overlay.path().join("proc/1"))
            .await
            .is_ok()
        {
            let procfs = context
                .seed
                .persistent_container
                .lxc_container
                .get()
                .or_not_found("lxc container")?
                .rootfs_dir()
                .join("proc");
            let overlay_path = overlay.overlay.path().to_owned();
            tokio::task::spawn_blocking(move || sync::kill_init(&procfs, &overlay_path))
                .await
                .with_kind(ErrorKind::Unknown)??;
        }
        overlay.overlay.unmount(true).await?;
    } else {
        tracing::warn!(
            "Could not find a subcontainer fs to destroy; assumming that it already is destroyed and will be skipping"
        );
    }
    Ok(())
}

#[derive(Debug, Deserialize, Serialize, Parser, visit_rs::VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct CreateSubcontainerFsParams {
    image_id: ImageId,
    #[visit(ts(type = "string | null"), wire = "Option<rpc_toolkit::ts::Unknown>")]
    #[visit(opaque, type_attributes(visit::wire))]
    name: Option<InternedString>,
}

rpc_toolkit::reflect_ts!(CreateSubcontainerFsParams);
rpc_toolkit::ts_export!(CreateSubcontainerFsParams, namespaces = [""]);
#[instrument(skip_all)]
pub async fn create_subcontainer_fs(
    context: EffectContext,
    CreateSubcontainerFsParams { image_id, name }: CreateSubcontainerFsParams,
) -> Result<(PathBuf, Guid), Error> {
    let context = context.deref()?;
    if let Some(image) = context
        .seed
        .persistent_container
        .images
        .get(&image_id)
        .cloned()
    {
        let guid = Guid::new();
        let lxc_container = context
            .seed
            .persistent_container
            .lxc_container
            .get()
            .ok_or_else(|| {
                Error::new(
                    eyre!("PersistentContainer has been destroyed"),
                    ErrorKind::Incoherent,
                )
            })?;
        let container_guid = &lxc_container.guid;
        let rootfs_dir = lxc_container.rootfs_dir();
        let mountpoint = rootfs_dir
            .join("media/startos/subcontainers")
            .join(guid.as_ref());
        tokio::fs::create_dir_all(&mountpoint).await?;
        let container_mountpoint = Path::new("/").join(
            mountpoint
                .strip_prefix(rootfs_dir)
                .with_kind(ErrorKind::Incoherent)?,
        );
        tracing::info!("Mounting overlay {guid} for {image_id}");

        // Determine which nvidia overlay to use based on distro detection
        let nvidia_overlay: &[&str] = if context
            .seed
            .persistent_container
            .s9pk
            .as_manifest()
            .images
            .get(&image_id)
            .map_or(false, |i| i.nvidia_container)
        {
            // Check if image is debian-based by looking for /etc/debian_version
            let is_debian = tokio::fs::metadata(image.path().join("etc/debian_version"))
                .await
                .is_ok();
            if is_debian && tokio::fs::metadata(NVIDIA_OVERLAY_DEBIAN).await.is_ok() {
                &[NVIDIA_OVERLAY_DEBIAN]
            } else if tokio::fs::metadata(NVIDIA_OVERLAY_GENERIC).await.is_ok() {
                &[NVIDIA_OVERLAY_GENERIC]
            } else {
                &[]
            }
        } else {
            &[]
        };

        let subcontainer_wrapper = Subcontainer {
            overlay: OverlayGuard::mount_layers(&[], image, nvidia_overlay, &mountpoint).await?,
            name: name
                .unwrap_or_else(|| InternedString::intern(format!("subcontainer-{}", image_id))),
            image_id: image_id.clone(),
        };

        Command::new("chown")
            .arg("100000:100000")
            .arg(&mountpoint)
            .invoke(ErrorKind::Filesystem)
            .await?;
        write_file_owned_atomic(
            mountpoint.join("etc/hostname"),
            format!("{container_guid}\n"),
            100000,
            100000,
        )
        .await?;
        write_file_owned_atomic(
            mountpoint.join("etc/hosts"),
            format!("127.0.0.1\tlocalhost\n127.0.1.1\t{container_guid}\n::1\tlocalhost ip6-localhost ip6-loopback\n"),
            100000,
            100000,
        )
        .await?;
        tracing::info!("Mounted overlay {guid} for {image_id}");
        context
            .seed
            .persistent_container
            .subcontainers
            .lock()
            .await
            .insert(guid.clone(), subcontainer_wrapper);
        Ok((container_mountpoint, guid))
    } else {
        Err(Error::new(
            eyre!("image {image_id} not found in s9pk"),
            ErrorKind::NotFound,
        ))
    }
}
