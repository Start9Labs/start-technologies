use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::root::VerifiedMounts;
use crate::disk::mount::filesystem::bind::{Bind, FileType};
use crate::disk::mount::filesystem::loop_dev::LoopDev;
use crate::disk::mount::filesystem::overlayfs::OverlayFs;
use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard};
use crate::os_overlay::VerityMount;
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::future::complete_owned_operation;

/// Read-only public providers with the packaged build runtime as fallback.
/// The signed public-rootfs mapping outlives the runtime and source mounts.
pub struct BuildSource {
    layers: VerifiedMounts<VerityMount>,
    path: PathBuf,
}

fn invalid_source() -> Error {
    Error::new(
        eyre!("{}", t!("fde.boot.invalid-descriptor")),
        ErrorKind::InvalidSignature,
    )
}

async fn verified_path(public: &Path, relative: &Path) -> Result<PathBuf, Error> {
    let public = crate::util::io::canonicalize(public, true).await?;
    let path = crate::util::io::canonicalize(public.join(relative), true).await?;
    if !path.starts_with(&public) {
        return Err(invalid_source());
    }
    Ok(path)
}

async fn runtime_image(public: &Path) -> Result<PathBuf, Error> {
    verified_path(
        public,
        Path::new("usr/lib/startos/container-runtime/rootfs.squashfs"),
    )
    .await
}

async fn source_mountpoint(root: &Path, relative: &Path) -> Result<PathBuf, Error> {
    let target = root.join(relative);
    if verified_path(root, relative).await? != target {
        return Err(invalid_source());
    }
    Ok(target)
}

async fn script_path(root: &Path, script: &Path) -> Result<PathBuf, Error> {
    let root = crate::util::io::canonicalize(root, true).await?;
    let relative = script.strip_prefix("/").map_err(|_| invalid_source())?;
    let script = verified_path(&root, relative).await?;
    if !script.starts_with(root.join("usr")) || !tokio::fs::metadata(&script).await?.is_file() {
        return Err(invalid_source());
    }
    Ok(Path::new("/").join(script.strip_prefix(&root).unwrap()))
}

impl BuildSource {
    pub(super) async fn mount(
        verified: VerityMount,
        runtime: &Path,
        root: &Path,
    ) -> Result<Self, Error> {
        let mut layers = VerifiedMounts::new(verified);
        let result = async {
            let public = layers.lower().path().to_owned();
            layers.push(
                MountGuard::mount(
                    &LoopDev::new(runtime_image(&public).await?, 0, 0),
                    runtime,
                    ReadOnly,
                )
                .await?,
            );
            layers.push(
                MountGuard::mount(
                    &OverlayFs::read_only(vec![public, runtime.to_owned()]),
                    root,
                    ReadOnly,
                )
                .await?,
            );
            Ok::<_, Error>(())
        }
        .await;
        if let Err(error) = result {
            layers.unmount().await.log_err();
            return Err(error);
        }
        Ok(Self {
            layers,
            path: root.to_owned(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Executes a public `/usr` script with private scratch, devices and namespaces.
    /// Caller cancellation retains source ownership through command completion and teardown.
    /// Script arguments and input are data; the script owns their interpretation.
    pub async fn run_script(
        mut self,
        script: PathBuf,
        arguments: Vec<OsString>,
        input: Vec<u8>,
    ) -> Result<Vec<u8>, Error> {
        complete_owned_operation(async move {
            let result = self.run_script_inner(&script, &arguments, input).await;
            let cleanup = self.unmount().await;
            match result {
                Ok(output) => {
                    cleanup?;
                    Ok(output)
                }
                Err(error) => {
                    cleanup.log_err();
                    Err(error)
                }
            }
        })
        .await
    }

    async fn run_script_inner(
        &mut self,
        script: &Path,
        arguments: &[OsString],
        input: Vec<u8>,
    ) -> Result<Vec<u8>, Error> {
        let root = crate::util::io::canonicalize(&self.path, true).await?;
        let script = script_path(&root, script).await?;
        let scratch = ["dev", "run", "tmp", "var/tmp"];
        for path in scratch.into_iter().chain(["proc"]) {
            source_mountpoint(&root, Path::new(path)).await?;
        }
        for path in scratch {
            self.layers.push(
                MountGuard::mount(&PrivateTmpfs::new(), self.path.join(path), ReadWrite).await?,
            );
        }
        for device in ["null", "zero", "random", "urandom"] {
            self.layers.push(
                MountGuard::mount(
                    &Bind::new(Path::new("/dev").join(device)).with_type(FileType::File),
                    self.path.join("dev").join(device),
                    ReadOnly,
                )
                .await?,
            );
        }
        let mut input = std::io::Cursor::new(input);
        tokio::process::Command::new("/usr/bin/unshare")
            .args([
                "--mount",
                "--pid",
                "--fork",
                "--kill-child=SIGKILL",
                "--net",
                "--ipc",
                "--uts",
                "--propagation=private",
                "--mount-proc",
            ])
            .arg("--root")
            .arg(&root)
            .arg("--wd=/")
            .args(["--", "/usr/bin/node", "--"])
            .arg(script)
            .args(arguments)
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .env("HOME", "/var/tmp")
            .env("TMPDIR", "/tmp")
            .env("LC_ALL", "C")
            .input(Some(&mut input))
            .invoke(ErrorKind::Filesystem)
            .await
    }

    pub async fn unmount(self) -> Result<(), Error> {
        self.layers.unmount().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scripts_must_remain_in_unshadowed_public_usr() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("usr/lib")).unwrap();
        std::fs::create_dir_all(root.join("var/tmp")).unwrap();
        std::fs::write(root.join("usr/lib/build.mjs"), "export {};").unwrap();
        std::fs::write(root.join("var/tmp/build.mjs"), "export {};").unwrap();
        assert_eq!(
            script_path(root, Path::new("/usr/lib/build.mjs"))
                .await
                .unwrap(),
            Path::new("/usr/lib/build.mjs")
        );
        assert!(
            script_path(root, Path::new("usr/lib/build.mjs"))
                .await
                .is_err()
        );
        assert!(
            script_path(root, Path::new("/var/tmp/build.mjs"))
                .await
                .is_err()
        );
        assert!(
            script_path(root, Path::new("/usr/../var/tmp/build.mjs"))
                .await
                .is_err()
        );
        assert!(script_path(root, Path::new("/usr/lib")).await.is_err());
        std::os::unix::fs::symlink("../../var/tmp/build.mjs", root.join("usr/lib/work.mjs"))
            .unwrap();
        assert!(
            script_path(root, Path::new("/usr/lib/work.mjs"))
                .await
                .is_err()
        );
        std::os::unix::fs::symlink("/etc/passwd", root.join("usr/lib/outside.mjs")).unwrap();
        assert!(
            script_path(root, Path::new("/usr/lib/outside.mjs"))
                .await
                .is_err()
        );
        assert_eq!(
            source_mountpoint(root, Path::new("var/tmp")).await.unwrap(),
            root.join("var/tmp")
        );
        std::os::unix::fs::symlink("usr/lib", root.join("tmp")).unwrap();
        assert!(source_mountpoint(root, Path::new("tmp")).await.is_err());
        std::os::unix::fs::symlink("/tmp", root.join("run")).unwrap();
        assert!(source_mountpoint(root, Path::new("run")).await.is_err());
    }

    #[tokio::test]
    async fn runtime_image_must_remain_inside_the_verified_public_filesystem() {
        let temp = tempfile::tempdir().unwrap();
        let public = temp.path().join("public");
        let image = public.join("usr/lib/startos/container-runtime/rootfs.squashfs");
        std::fs::create_dir_all(image.parent().unwrap()).unwrap();
        std::fs::write(&image, b"image data").unwrap();
        assert_eq!(
            runtime_image(&public).await.unwrap(),
            std::fs::canonicalize(&image).unwrap()
        );
        let outside = temp.path().join("outside");
        std::fs::write(&outside, b"not an admitted provider").unwrap();
        std::fs::remove_file(&image).unwrap();
        std::os::unix::fs::symlink(&outside, &image).unwrap();
        assert!(runtime_image(&public).await.is_err());
        std::fs::remove_file(&image).unwrap();
        std::fs::remove_dir(image.parent().unwrap()).unwrap();
        std::fs::create_dir(temp.path().join("container-runtime")).unwrap();
        std::fs::write(
            temp.path().join("container-runtime/rootfs.squashfs"),
            b"outside data",
        )
        .unwrap();
        std::os::unix::fs::symlink(
            temp.path().join("container-runtime"),
            image.parent().unwrap(),
        )
        .unwrap();
        assert!(runtime_image(&public).await.is_err());
    }
}
