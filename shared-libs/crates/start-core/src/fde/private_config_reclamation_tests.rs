use std::sync::Arc;

use super::*;
use crate::util::future::complete_owned_operation;

pub(super) async fn prepare(projected: &ProjectedConfig, root: &Path) -> PreparedGeneration {
    let store = root.join("generations");
    tokio::fs::create_dir_all(&store).await.unwrap();
    let rootfs = root.join("rootfs");
    tokio::fs::write(
        &rootfs,
        vec![42; crate::os_overlay::VERITY_BLOCK_SIZE as usize],
    )
    .await
    .unwrap();
    projected
        .prepare_generation(
            &rootfs,
            &store,
            &crate::os_overlay::generate_signing_key().unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn generation_disposal_preserves_fresh_backing_for_preexisting_ancestor_readers() {
    use std::io::Read;

    if crate::util::file_lock_tests::isolated_test("fde::private_config::tests::reclamation::generation_disposal_preserves_fresh_backing_for_preexisting_ancestor_readers").await { return; }
    let (root, raw, data) = fixture().await;
    let namespace = data.join(DIRECTORY);
    tokio::fs::create_dir(&namespace).await.unwrap();
    let ancestor = std::fs::File::open(&namespace).unwrap();
    let projected = project(&raw, &data, &[]).await;
    let backing = backing_directory(&data, target(&projected, "etc/shadow")).await;
    let credential = backing_file(&data, target(&projected, "etc/shadow"))
        .await
        .unwrap();
    let relative = credential.strip_prefix(&namespace).unwrap().to_owned();
    let prepared = prepare(&projected, root.path()).await;
    let staging = prepared.staging_path().to_owned();
    projected.discard_generation(prepared).await.unwrap();
    assert!(!staging.exists());
    assert!(backing.is_dir());
    drop(
        reserve_backing(&backing, FileLockAccess::Exclusive)
            .await
            .unwrap(),
    );
    tokio::fs::write(&credential, b"live-after-disposal")
        .await
        .unwrap();
    let fd = nix::fcntl::openat(
        &ancestor,
        relative.as_path(),
        nix::fcntl::OFlag::O_RDONLY | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .unwrap();
    let mut contents = String::new();
    std::fs::File::from(fd)
        .read_to_string(&mut contents)
        .unwrap();
    assert_eq!(contents, "live-after-disposal");
    assert!(raw.join("etc/shadow").is_file());
    assert!(data.join(DIRECTORY).is_dir());
}

#[tokio::test]
async fn resumed_disposal_preserves_published_backings_and_unrelated_files() {
    if crate::util::file_lock_tests::isolated_test("fde::private_config::tests::reclamation::resumed_disposal_preserves_published_backings_and_unrelated_files").await { return; }
    let (root, raw, data) = fixture().await;
    let initial = project(&raw, &data, &[]).await;
    let retained = backing_file(&data, target(&initial, "etc/shadow"))
        .await
        .unwrap();
    let published = prepare(&initial, root.path())
        .await
        .publish()
        .await
        .unwrap();
    let unrelated = data.join(DIRECTORY).join("unrelated");
    tokio::fs::write(&unrelated, b"keep").await.unwrap();
    let snapshot = initial.snapshot.clone();
    let resumed = ProjectedConfig::stage(snapshot.clone(), &data, &[])
        .await
        .unwrap();
    assert!(
        crate::util::io::same_file(
            &backing_file(&data, target(&resumed, "etc/shadow"))
                .await
                .unwrap(),
            &retained
        )
        .await
        .unwrap()
    );
    drop(resumed);
    let mut snapshot = snapshot;
    snapshot
        .contents_mut()
        .insert_path(
            Path::new("etc/fresh-secret"),
            Entry::new(
                Metadata::new(0o600),
                NodeContents::File(DynFileSource::new(Arc::<[u8]>::from(&b"fresh-secret"[..]))),
            ),
        )
        .unwrap();
    let resumed = ProjectedConfig::stage(snapshot, &data, &[]).await.unwrap();
    let fresh = backing_file(&data, target(&resumed, "etc/fresh-secret"))
        .await
        .unwrap();
    drop(resumed);
    assert_eq!(tokio::fs::read(fresh).await.unwrap(), b"fresh-secret");
    assert_eq!(tokio::fs::read(&retained).await.unwrap(), b"private-shadow");
    assert_eq!(tokio::fs::read(unrelated).await.unwrap(), b"keep");
    assert!(published.join(crate::os_overlay::OVERLAY_FILE).is_file());
}

#[tokio::test]
async fn failed_generation_retirement_preserves_its_fresh_private_backing() {
    if crate::util::file_lock_tests::isolated_test("fde::private_config::tests::reclamation::failed_generation_retirement_preserves_its_fresh_private_backing").await { return; }
    let (root, raw, data) = fixture().await;
    let projected = project(&raw, &data, &[]).await;
    let backing = backing_file(&data, target(&projected, "etc/shadow"))
        .await
        .unwrap();
    let prepared = prepare(&projected, root.path()).await;
    let staging = prepared.staging_path().to_owned();
    tokio::fs::rename(&staging, root.path().join("retained-stage"))
        .await
        .unwrap();
    tokio::fs::write(&staging, b"unexpected replacement")
        .await
        .unwrap();
    assert!(projected.discard_generation(prepared).await.is_err());
    assert_eq!(tokio::fs::read(backing).await.unwrap(), b"private-shadow");
    assert_eq!(
        tokio::fs::read(staging).await.unwrap(),
        b"unexpected replacement"
    );
}

struct TruncatedSource;

impl FileSource for TruncatedSource {
    type Reader = std::io::Cursor<&'static [u8]>;
    type SliceReader = Self::Reader;

    async fn size(&self) -> Result<u64, Error> {
        Ok(2)
    }
    async fn reader(&self) -> Result<Self::Reader, Error> {
        Ok(std::io::Cursor::new(&b"x"[..]))
    }
    async fn slice(&self, _: u64, _: u64) -> Result<Self::SliceReader, Error> {
        Err(std::io::Error::from(std::io::ErrorKind::Unsupported).into())
    }
}

#[tokio::test]
async fn failed_private_projection_retains_its_partially_written_backing() {
    if crate::util::file_lock_tests::isolated_test("fde::private_config::tests::reclamation::failed_private_projection_retains_its_partially_written_backing").await { return; }
    let (_root, raw, data) = fixture().await;
    let initial = project(&raw, &data, &[]).await;
    let retained = backing_file(&data, target(&initial, "etc/shadow"))
        .await
        .unwrap();
    let mut snapshot = initial.snapshot.clone();
    for (path, source) in [
        (
            "etc/a-secret",
            DynFileSource::new(Arc::<[u8]>::from(&b"fresh-secret"[..])),
        ),
        ("etc/z-truncated", DynFileSource::new(TruncatedSource)),
    ] {
        snapshot
            .contents_mut()
            .insert_path(
                Path::new(path),
                Entry::new(Metadata::new(0o600), NodeContents::File(source)),
            )
            .unwrap();
    }
    let result = ProjectedConfig::stage(snapshot, &data, &[]).await;
    assert!(result.is_err());
    assert_eq!(tokio::fs::read(&retained).await.unwrap(), b"private-shadow");
    let initial_backing = backing_directory(&data, target(&initial, "etc/shadow")).await;
    let backings: Vec<_> = backing_ids(&data)
        .await
        .unwrap()
        .into_iter()
        .map(|id| data.join(DIRECTORY).join(id.to_string()))
        .collect();
    assert_eq!(backings.len(), 2);
    let partial = backings
        .iter()
        .find(|path| **path != initial_backing)
        .unwrap();
    assert_eq!(
        tokio::fs::read(partial.join("etc/a-secret")).await.unwrap(),
        b"fresh-secret"
    );
    assert_eq!(
        tokio::fs::metadata(partial)
            .await
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

#[tokio::test]
async fn requester_loss_keeps_the_existing_lease_through_candidate_disposal() {
    if crate::util::file_lock_tests::isolated_test("fde::private_config::tests::reclamation::requester_loss_keeps_the_existing_lease_through_candidate_disposal").await { return; }
    let (root, raw, data) = fixture().await;
    let projected = project(&raw, &data, &[]).await;
    let backing = backing_directory(&data, target(&projected, "etc/shadow")).await;
    let prepared = prepare(&projected, root.path()).await;
    let staging = prepared.staging_path().to_owned();
    let lock_path = root.path().join("images.lock");
    let lease = crate::util::FileLock::new(&lock_path, false).await.unwrap();
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, proceed) = tokio::sync::oneshot::channel();
    let (finished, done) = tokio::sync::oneshot::channel();
    let requester = tokio::spawn(complete_owned_operation(async move {
        entered.send(()).unwrap();
        proceed.await.unwrap();
        let result = projected.discard_generation(prepared).await;
        drop(lease);
        finished.send(result.is_ok()).unwrap();
        result
    }));
    started.await.unwrap();
    requester.abort();
    let _ = requester.await;
    assert!(crate::util::FileLock::new(&lock_path, false).await.is_err());
    assert!(backing.exists());
    assert!(staging.exists());
    release.send(()).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(10), done)
            .await
            .unwrap()
            .unwrap()
    );
    assert!(backing.is_dir());
    assert!(!staging.exists());
    crate::util::FileLock::new(&lock_path, false).await.unwrap();
}
