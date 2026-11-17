use std::os::unix::fs::MetadataExt;

use chrono::Utc;
use ed25519_dalek::SigningKey;

use super::*;
use crate::sign::ed25519::Ed25519ph;
use crate::sign::{AnySignature, AnyVerifyingKey, SignatureMap, SignatureScheme};

fn release(commit: &str) -> Release {
    Release {
        version: "0.4.3".parse().unwrap(),
        commit: commit.into(),
        platform: "x86_64".into(),
    }
}

fn policy(key: &SigningKey) -> AcceptSigners {
    AcceptSigners::Signer(AnyVerifyingKey::Ed25519(key.verifying_key()))
}

async fn signed_asset(path: &Path, key: &SigningKey) -> RegistryAsset<Blake3Commitment> {
    let file = open_file(path).await.unwrap();
    let commitment = Blake3Commitment::create(&MultiCursorFile::open(&file).await.unwrap())
        .await
        .unwrap();
    let signature = Ed25519ph
        .sign_commitment(key, &commitment, SIG_CONTEXT)
        .unwrap();
    RegistryAsset {
        published_at: Utc::now(),
        urls: vec!["http://127.0.0.1:9/unavailable".parse().unwrap()],
        commitment,
        signatures: SignatureMap::from([(
            AnyVerifyingKey::Ed25519(key.verifying_key()),
            AnySignature::Ed25519ph(signature),
        )]),
    }
}

async fn fixture(root: &Path, commit: &str, compression: &str) -> PathBuf {
    let directory = root.join(commit);
    let metadata = directory.join("usr/lib/startos");
    tokio::fs::create_dir_all(&metadata).await.unwrap();
    for (file, value) in [
        ("VERSION.txt", "0.4.3"),
        ("GIT_HASH.txt", commit),
        ("PLATFORM.txt", "x86_64"),
    ] {
        tokio::fs::write(metadata.join(file), format!("{value}\n"))
            .await
            .unwrap();
    }
    let image = root.join(format!("{commit}.{compression}.squashfs"));
    Command::new("/usr/bin/mksquashfs")
        .arg(&directory)
        .arg(&image)
        .args(["-noappend", "-comp", compression, "-no-progress"])
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    image
}

fn cache(root: &Path) -> SourceCache {
    std::fs::create_dir_all(root.join("images")).unwrap();
    SourceCache::new(root)
}

#[tokio::test]
async fn retained_gzip_source_survives_original_unlink_without_a_registry() {
    let root = tempfile::tempdir().unwrap();
    let cache = cache(root.path());
    let key = SigningKey::from_bytes(&[11; 32]);
    let image = fixture(root.path(), "running", "gzip").await;
    let inode = tokio::fs::metadata(&image).await.unwrap().ino();
    let asset = signed_asset(&image, &key).await;
    let retained = cache.retain(asset, &image).await.unwrap();
    assert_eq!(
        tokio::fs::metadata(&retained.path).await.unwrap().ino(),
        inode
    );
    tokio::fs::remove_file(&image).await.unwrap();
    let found = cache
        .find_approved(&release("running"), policy(&key))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.path, retained.path);
    assert_eq!(found.release, release("running"));
}

#[tokio::test]
async fn reuse_rechecks_current_signers_metadata_and_complete_bytes() {
    let root = tempfile::tempdir().unwrap();
    let cache = cache(root.path());
    let key = SigningKey::from_bytes(&[12; 32]);
    let other = SigningKey::from_bytes(&[13; 32]);
    let image = fixture(root.path(), "running", "gzip").await;
    let retained = cache
        .retain(signed_asset(&image, &key).await, &image)
        .await
        .unwrap();
    assert!(
        cache
            .find_approved(&release("running"), policy(&other))
            .await
            .unwrap()
            .is_none()
    );
    let directory = retained.path.parent().unwrap();
    let record_bytes = tokio::fs::read(directory.join(RECORD)).await.unwrap();
    let mut record: Record = serde_json::from_slice(&record_bytes).unwrap();
    record.release = release("different");
    tokio::fs::write(directory.join(RECORD), serde_json::to_vec(&record).unwrap())
        .await
        .unwrap();
    assert!(
        cache
            .find_approved(&release("different"), policy(&key))
            .await
            .unwrap()
            .is_none()
    );
    tokio::fs::write(directory.join(RECORD), &record_bytes)
        .await
        .unwrap();
    assert!(
        cache
            .find_approved(&release("running"), policy(&key))
            .await
            .unwrap()
            .is_some()
    );
    record = serde_json::from_slice(&record_bytes).unwrap();
    record.asset.signatures.clear();
    tokio::fs::write(directory.join(RECORD), serde_json::to_vec(&record).unwrap())
        .await
        .unwrap();
    assert!(
        cache
            .find_approved(&release("running"), policy(&key))
            .await
            .unwrap()
            .is_none()
    );
    tokio::fs::write(directory.join(RECORD), record_bytes)
        .await
        .unwrap();
    use tokio::io::AsyncWriteExt;
    tokio::fs::OpenOptions::new()
        .append(true)
        .open(&retained.path)
        .await
        .unwrap()
        .write_all(b"trailing bytes")
        .await
        .unwrap();
    assert!(
        cache
            .find_approved(&release("running"), policy(&key))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn pruning_keeps_running_and_pending_then_reclaims_replaced_inputs() {
    let root = tempfile::tempdir().unwrap();
    let cache = cache(root.path());
    let key = SigningKey::from_bytes(&[14; 32]);
    let mut paths = Vec::new();
    for commit in ["running", "pending", "abandoned"] {
        let image = fixture(root.path(), commit, "gzip").await;
        let source = cache
            .retain(signed_asset(&image, &key).await, &image)
            .await
            .unwrap();
        tokio::fs::remove_file(image).await.unwrap();
        paths.push(source.path);
    }
    cache
        .prune(&BTreeSet::from([release("running"), release("pending")]))
        .await
        .unwrap();
    assert!(paths[0].is_file());
    assert!(paths[1].is_file());
    assert!(!paths[2].exists());
    cache
        .prune(&BTreeSet::from([release("pending")]))
        .await
        .unwrap();
    assert!(!paths[0].exists());
    assert!(
        cache
            .find_approved(&release("pending"), policy(&key))
            .await
            .unwrap()
            .is_some()
    );
    let foreign = cache.directory().join("foreign");
    tokio::fs::create_dir(&foreign).await.unwrap();
    tokio::fs::write(foreign.join("keep"), b"not owned")
        .await
        .unwrap();
    let incomplete = cache.directory().join("f".repeat(64));
    tokio::fs::create_dir(&incomplete).await.unwrap();
    tokio::fs::write(incomplete.join(IMAGE), b"incomplete")
        .await
        .unwrap();
    tokio::fs::write(
        crate::util::io::to_tmp_path(incomplete.join(RECORD)).unwrap(),
        b"{",
    )
    .await
    .unwrap();
    cache
        .prune(&BTreeSet::from([release("pending")]))
        .await
        .unwrap();
    assert!(!incomplete.exists());
    assert_eq!(
        tokio::fs::read(foreign.join("keep")).await.unwrap(),
        b"not owned"
    );
}

#[tokio::test]
async fn boot_retention_reads_running_and_next_without_importing_a_private_repack() {
    let root = tempfile::tempdir().unwrap();
    let cache = cache(root.path());
    let key = SigningKey::from_bytes(&[18; 32]);
    let running = fixture(root.path(), "running", "gzip").await;
    let pending = fixture(root.path(), "pending", "gzip").await;
    let signed = cache
        .retain(signed_asset(&pending, &key).await, &pending)
        .await
        .unwrap();
    let private = root.path().join("pending/usr/lib/startos/private");
    tokio::fs::write(&private, b"never public").await.unwrap();
    let repack = root.path().join("private.rootfs");
    Command::new("/usr/bin/mksquashfs")
        .arg(root.path().join("pending"))
        .arg(&repack)
        .args(["-noappend", "-comp", "gzip", "-no-progress"])
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    assert!(
        cache
            .retain(signed_asset(&pending, &key).await, &repack)
            .await
            .is_err()
    );
    let pointer = root.path().join("current.rootfs");
    tokio::fs::symlink(&repack, &pointer).await.unwrap();
    let pins = boot_releases(&root.path().join("running"), &pointer)
        .await
        .unwrap();
    assert_eq!(
        pins,
        BTreeSet::from([release("running"), release("pending")])
    );
    assert_eq!(
        Release::read(&root.path().join("running")).await.unwrap(),
        release("running")
    );
    cache.prune(&pins).await.unwrap();
    let found = cache
        .find_approved(&release("pending"), policy(&key))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.path, signed.path);
    assert_ne!(
        tokio::fs::metadata(&found.path).await.unwrap().ino(),
        tokio::fs::metadata(&repack).await.unwrap().ino()
    );
    let bytes = Command::new("/usr/bin/unsquashfs")
        .arg("-cat")
        .arg(&found.path)
        .arg("usr/lib/startos/private")
        .invoke(ErrorKind::Filesystem)
        .await;
    assert!(bytes.is_err());
    tokio::fs::remove_file(&pointer).await.unwrap();
    tokio::fs::symlink(root.path().join("pending"), &pointer)
        .await
        .unwrap();
    assert_eq!(
        boot_releases(&root.path().join("running"), &pointer)
            .await
            .unwrap(),
        pins
    );
    tokio::fs::remove_file(&pointer).await.unwrap();
    assert!(
        boot_releases(&root.path().join("running"), &pointer)
            .await
            .is_err()
    );
    assert!(running.exists());
}

#[tokio::test]
async fn retention_merges_valid_signatures_and_repairs_interrupted_publication() {
    let root = tempfile::tempdir().unwrap();
    let cache = cache(root.path());
    let first = SigningKey::from_bytes(&[15; 32]);
    let second = SigningKey::from_bytes(&[16; 32]);
    let image = fixture(root.path(), "running", "zstd").await;
    let source = cache
        .retain(signed_asset(&image, &first).await, &image)
        .await
        .unwrap();
    cache
        .retain(signed_asset(&image, &second).await, &image)
        .await
        .unwrap();
    for key in [&first, &second] {
        assert!(
            cache
                .find_approved(&release("running"), policy(key))
                .await
                .unwrap()
                .is_some()
        );
    }
    let directory = source.path.parent().unwrap();
    tokio::fs::remove_file(directory.join(RECORD))
        .await
        .unwrap();
    tokio::fs::write(
        crate::util::io::to_tmp_path(directory.join(RECORD)).unwrap(),
        b"{",
    )
    .await
    .unwrap();
    assert!(
        cache
            .find_approved(&release("running"), policy(&second))
            .await
            .unwrap()
            .is_none()
    );
    cache
        .retain(signed_asset(&image, &second).await, &image)
        .await
        .unwrap();
    assert!(
        cache
            .find_approved(&release("running"), policy(&second))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        !crate::util::io::to_tmp_path(directory.join(RECORD))
            .unwrap()
            .exists()
    );
}

#[tokio::test]
async fn receipt_refresh_reclaims_preexisting_temporary_aliases_without_writing_them() {
    let root = tempfile::tempdir().unwrap();
    let cache = cache(root.path());
    let key = SigningKey::from_bytes(&[19; 32]);
    let image = fixture(root.path(), "running", "gzip").await;
    let asset = signed_asset(&image, &key).await;
    let source = cache.retain(asset.clone(), &image).await.unwrap();
    let temporary =
        crate::util::io::to_tmp_path(source.path.parent().unwrap().join(RECORD)).unwrap();
    let foreign = root.path().join("foreign");
    tokio::fs::write(&foreign, b"keep").await.unwrap();
    tokio::fs::symlink(&foreign, &temporary).await.unwrap();
    cache.retain(asset.clone(), &image).await.unwrap();
    assert_eq!(tokio::fs::read(&foreign).await.unwrap(), b"keep");
    tokio::fs::hard_link(&foreign, &temporary).await.unwrap();
    cache.retain(asset, &image).await.unwrap();
    assert_eq!(tokio::fs::read(&foreign).await.unwrap(), b"keep");
    assert!(!temporary.exists());
}

#[tokio::test]
async fn redirected_cache_directories_never_publish_into_foreign_storage() {
    let root = tempfile::tempdir().unwrap();
    let cache = cache(root.path());
    let key = SigningKey::from_bytes(&[17; 32]);
    let image = fixture(root.path(), "running", "gzip").await;
    let asset = signed_asset(&image, &key).await;
    let foreign = root.path().join("foreign");
    tokio::fs::create_dir(&foreign).await.unwrap();
    tokio::fs::write(foreign.join(RECORD), b"keep")
        .await
        .unwrap();
    tokio::fs::symlink(&foreign, cache.directory())
        .await
        .unwrap();
    assert!(cache.retain(asset.clone(), &image).await.is_err());
    assert!(
        cache
            .find_approved(&release("running"), policy(&key))
            .await
            .is_err()
    );
    tokio::fs::remove_file(cache.directory()).await.unwrap();
    tokio::fs::create_dir(cache.directory()).await.unwrap();
    tokio::fs::symlink(&foreign, cache.directory().join(asset_key(&asset)))
        .await
        .unwrap();
    assert!(cache.retain(asset, &image).await.is_err());
    assert_eq!(
        tokio::fs::read(foreign.join(RECORD)).await.unwrap(),
        b"keep"
    );
}

async fn shell_prune(
    lease: &crate::util::FileLock,
    volume: &Path,
    binary: &Path,
) -> Result<Vec<u8>, Error> {
    let executable = tempfile::tempdir()?;
    tokio::fs::symlink(binary, executable.path().join("startbox")).await?;
    let path = std::env::join_paths(
        std::iter::once(executable.path().to_path_buf())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let scripts =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../projects/start-os/build/lib/scripts");
    let variable = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../projects/start-os/build/lib/scripts/os-image-lock-fd-name"
    ))
    .trim();
    let mut command = Command::new("timeout");
    command
        .args([
            "--kill-after=2s",
            "15s",
            "/bin/bash",
            "-euc",
            "source \"$1/os-image-store\"; startos_prune_images \"$2\" 0 0",
            "source-cache-prune",
        ])
        .arg(scripts)
        .arg(volume)
        .env("PATH", path);
    lease.inherit(&mut command, variable)?;
    command.invoke(ErrorKind::Filesystem).await
}

async fn volume_lease(volume: &Path) -> crate::util::FileLock {
    let configuration = volume.join("config");
    tokio::fs::create_dir_all(&configuration).await.unwrap();
    let name = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../projects/start-os/build/lib/scripts/os-image-lock-name"
    ))
    .trim();
    crate::util::FileLock::new(configuration.join(name), true)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires the actual built startbox in FDE_SOURCE_GC_STARTBOX"]
async fn shell_space_admission_prunes_public_sources_with_the_native_owner_and_lease() {
    use std::os::unix::ffi::OsStringExt;

    let binary = PathBuf::from(std::env::var_os("FDE_SOURCE_GC_STARTBOX").unwrap());
    let root = tempfile::tempdir().unwrap();
    let volume = root
        .path()
        .join(std::ffi::OsString::from_vec(b"volume [one] \xff".to_vec()));
    let cache = cache(&volume);
    let lease = volume_lease(&volume).await;
    let key = SigningKey::from_bytes(&[21; 32]);
    let mut sources = Vec::new();
    for commit in ["running", "pending", "abandoned"] {
        let image = fixture(root.path(), commit, "gzip").await;
        let source = cache
            .retain(signed_asset(&image, &key).await, &image)
            .await
            .unwrap();
        if commit == "pending" {
            tokio::fs::symlink(&image, volume.join("config/current.rootfs"))
                .await
                .unwrap();
        } else {
            tokio::fs::remove_file(&image).await.unwrap();
        }
        sources.push(source.path);
    }
    tokio::fs::rename(root.path().join("running"), volume.join("current"))
        .await
        .unwrap();
    let pending = tokio::fs::read_link(volume.join("config/current.rootfs"))
        .await
        .unwrap();
    tokio::fs::remove_file(volume.join("config/current.rootfs"))
        .await
        .unwrap();
    tokio::fs::symlink(
        root.path().join("missing.rootfs"),
        volume.join("config/current.rootfs"),
    )
    .await
    .unwrap();
    assert!(shell_prune(&lease, &volume, &binary).await.is_err());
    assert!(sources.iter().all(|path| path.is_file()));
    tokio::fs::remove_file(volume.join("config/current.rootfs"))
        .await
        .unwrap();
    tokio::fs::symlink(&pending, volume.join("config/current.rootfs"))
        .await
        .unwrap();
    shell_prune(&lease, &volume, &binary).await.unwrap();
    assert!(sources[0].is_file());
    assert!(sources[1].is_file());
    assert!(!sources[2].exists());
    assert_eq!(
        tokio::fs::read_link(volume.join("config/current.rootfs"))
            .await
            .unwrap(),
        pending
    );
    assert!(
        cache
            .find_approved(&release("running"), policy(&key))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        cache
            .find_approved(&release("pending"), policy(&key))
            .await
            .unwrap()
            .is_some()
    );
    lease.unlock().await.unwrap();
}

#[tokio::test]
#[ignore = "requires the actual previous startbox in FDE_SOURCE_GC_OLD_STARTBOX"]
async fn shell_pruning_keeps_old_tools_without_a_cache_and_rejects_unsupported_cached_state() {
    let binary = PathBuf::from(std::env::var_os("FDE_SOURCE_GC_OLD_STARTBOX").unwrap());
    let root = tempfile::tempdir().unwrap();
    let volume = root.path().join("volume");
    let cache = cache(&volume);
    let lease = volume_lease(&volume).await;
    let image = fixture(root.path(), "running", "gzip").await;
    tokio::fs::rename(root.path().join("running"), volume.join("current"))
        .await
        .unwrap();
    tokio::fs::symlink(volume.join("current"), volume.join("config/current.rootfs"))
        .await
        .unwrap();
    shell_prune(&lease, &volume, &binary).await.unwrap();
    assert!(!cache.directory().exists());
    let key = SigningKey::from_bytes(&[22; 32]);
    let source = cache
        .retain(signed_asset(&image, &key).await, &image)
        .await
        .unwrap();
    assert!(shell_prune(&lease, &volume, &binary).await.is_err());
    assert!(source.path.is_file());
    assert!(image.is_file());
    lease.unlock().await.unwrap();
}

#[tokio::test]
async fn source_release_metadata_is_confined_and_includes_commit_identity() {
    let dir = tempfile::tempdir().unwrap();
    let image = fixture(dir.path(), "build-a", "gzip").await;
    let root = dir.path().join("build-a");
    let metadata = root.join("usr/lib/startos");
    assert_eq!(Release::read(&root).await.unwrap(), release("build-a"));
    assert_eq!(
        Release::read_image(&image).await.unwrap(),
        release("build-a")
    );
    tokio::fs::write(metadata.join("GIT_HASH.txt"), "build-b")
        .await
        .unwrap();
    assert_ne!(Release::read(&root).await.unwrap(), release("build-a"));
    tokio::fs::remove_file(metadata.join("GIT_HASH.txt"))
        .await
        .unwrap();
    tokio::fs::write(dir.path().join("outside"), "build-a")
        .await
        .unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside"), metadata.join("GIT_HASH.txt")).unwrap();
    assert!(Release::read(&root).await.is_err());
}
