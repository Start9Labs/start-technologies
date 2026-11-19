use std::sync::Arc;

use super::*;
use crate::util::future::complete_owned_operation;

async fn prepare(projected: &ProjectedConfig, root: &Path) -> PreparedGeneration {
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
async fn unpublished_generation_and_fresh_backing_retire_together() {
    let (root, raw, data) = fixture().await;
    let projected = project(&raw, &data, &[]).await;
    let backing = projected.new_backing.clone().unwrap();
    let prepared = prepare(&projected, root.path()).await;
    let staging = prepared.staging_path().to_owned();
    projected.discard_generation(prepared).await.unwrap();
    assert!(!staging.exists());
    assert!(!backing.exists());
    assert!(raw.join("etc/shadow").is_file());
    assert!(data.join(DIRECTORY).is_dir());
}

#[tokio::test]
async fn resumed_disposal_preserves_published_backings_and_unrelated_files() {
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
    assert!(resumed.new_backing.is_none());
    resumed.discard().await.unwrap();
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
    let fresh = resumed.new_backing.clone().unwrap();
    resumed.discard().await.unwrap();
    assert!(!fresh.exists());
    assert_eq!(tokio::fs::read(&retained).await.unwrap(), b"private-shadow");
    assert_eq!(tokio::fs::read(unrelated).await.unwrap(), b"keep");
    assert!(published.join(crate::os_overlay::OVERLAY_FILE).is_file());
}

#[tokio::test]
async fn failed_generation_retirement_preserves_its_fresh_private_backing() {
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
async fn failed_private_projection_reclaims_its_partially_written_backing() {
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
    let mut directories = tokio::fs::read_dir(data.join(DIRECTORY)).await.unwrap();
    assert_eq!(
        directories.next_entry().await.unwrap().unwrap().path(),
        initial.new_backing.clone().unwrap()
    );
    assert!(directories.next_entry().await.unwrap().is_none());
}

#[tokio::test]
async fn requester_loss_keeps_the_existing_lease_through_candidate_disposal() {
    let (root, raw, data) = fixture().await;
    let projected = project(&raw, &data, &[]).await;
    let backing = projected.new_backing.clone().unwrap();
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
    assert!(!backing.exists());
    assert!(!staging.exists());
    crate::util::FileLock::new(&lock_path, false).await.unwrap();
}
