use std::ffi::CString;
use std::path::{Path, PathBuf};

use imbl_value::InternedString;
use nix::unistd::{Gid, Uid, fchown};

use crate::ImageId;
use crate::disk::mount::filesystem::overlayfs::OverlayGuard;
use crate::disk::mount::guard::GenericMountGuard;
use crate::rpc_continuations::Guid;
use crate::service::effects::prelude::*;
use crate::service::persistent_container::Subcontainer;
use crate::util::directory::{
    create_directory_at, create_directory_beneath, open_directory_nofollow,
};
use crate::util::io::write_file_owned_at;

fn chown_directory(directory: &std::fs::File) -> Result<(), Error> {
    fchown(
        directory,
        Some(Uid::from_raw(100000)),
        Some(Gid::from_raw(100000)),
    )
    .with_kind(ErrorKind::Filesystem)
}

const SUBCONTAINER_DIR: &str = "/media/startos/subcontainers";

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
        {
            let root = std::fs::File::open(overlay.overlay.path())?;
            tokio::task::spawn_blocking(move || sync::kill_init(&root))
                .await
                .with_kind(ErrorKind::Unknown)??;
        }
        overlay.unmount().await?;
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
        let rootfs = open_directory_nofollow(lxc_container.rootfs_dir())?;
        let parent =
            create_directory_beneath(&rootfs, Path::new(SUBCONTAINER_DIR.trim_start_matches('/')))?;
        let entry_name = CString::new(guid.as_ref()).unwrap();
        let container_mountpoint = Path::new(SUBCONTAINER_DIR).join(guid.as_ref());
        tracing::info!("Mounting overlay {guid} for {image_id}");

        let nvidia_overlay: &[&str] = if context
            .seed
            .persistent_container
            .s9pk
            .as_manifest()
            .images
            .get(&image_id)
            .map_or(false, |i| i.nvidia_container)
        {
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

        let overlay =
            OverlayGuard::mount_layers_at(&[], image, nvidia_overlay, &parent, &entry_name).await?;
        let root = std::fs::File::open(overlay.path())?;
        let proxy_mounts = if context
            .seed
            .persistent_container
            .s9pk
            .as_manifest()
            .metadata
            .plugins
            .contains(&crate::service::effects::plugin::PluginId::HttpProxyV0)
        {
            Some(
                context
                    .seed
                    .ctx
                    .net_controller
                    .http_proxy
                    .mount_provider_at(&context.seed.id, &root)
                    .await?,
            )
        } else {
            None
        };
        let subcontainer_wrapper = Subcontainer {
            overlay,
            proxy_mounts,
            name: name
                .unwrap_or_else(|| InternedString::intern(format!("subcontainer-{}", image_id))),
            image_id: image_id.clone(),
        };

        chown_directory(&root)?;
        let (etc, created) = create_directory_at(&root, c"etc")?;
        if created {
            chown_directory(&etc)?;
        }
        write_file_owned_at(
            &etc,
            c"hostname",
            format!("{container_guid}\n"),
            100000,
            100000,
        )
        .await?;
        write_file_owned_at(
            &etc,
            c"hosts",
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
