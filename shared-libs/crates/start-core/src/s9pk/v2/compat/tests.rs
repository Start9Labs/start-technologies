use std::ffi::OsStr;
use std::io::Cursor;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use super::*;
use crate::s9pk::merkle_archive::source::FileSource;
use crate::s9pk::v1::builder::S9pkPacker;
use crate::s9pk::v1::docker::DockerMultiArch;
use crate::util::Invoke;
use crate::util::squashfs::{Entry as SqfsEntry, NodeContents};

const SCRIPT: &[u8] = b"export const main = () => 'legacy script sentinel';\n";
const RAW_NAME: &[u8] = b"private/file\xff";

fn legacy_manifest() -> ManifestV1 {
    serde_json::from_value(serde_json::json!({
        "eos-version": "0.3.5.1", "id": "conversion-test", "title": "Conversion test",
        "version": "1.2.3", "description": {"short": "Fixture", "long": "Fixture"},
        "release-notes": "Fixture", "license": "MIT",
        "wrapper-repo": "https://example.com/package",
        "upstream-repo": "https://example.com/upstream",
        "main": {"type": "script", "entrypoint": "main"},
        "health-checks": {}, "volumes": {},
        "backup": {
            "create": {"type": "script", "entrypoint": "backup"},
            "restore": {"type": "script", "entrypoint": "restore"}
        }
    }))
    .unwrap()
}

fn append(
    tar: &mut tar::Builder<Vec<u8>>,
    path: &[u8],
    kind: tar::EntryType,
    mode: u32,
    data: &[u8],
    link: Option<&[u8]>,
) {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(kind);
    header.set_mode(mode);
    header.set_uid(1234);
    header.set_gid(5678);
    header.set_mtime(123456789);
    header.set_size(data.len() as u64);
    if let Some(link) = link {
        header
            .set_link_name(Path::new(OsStr::from_bytes(link)))
            .unwrap();
    }
    tar.append_data(&mut header, Path::new(OsStr::from_bytes(path)), data)
        .unwrap();
}

fn payload() -> Vec<u8> {
    (0..300_017).map(|i| (i % 251) as u8).collect()
}

fn asset_tar() -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    append(
        &mut tar,
        b".",
        tar::EntryType::Directory,
        0o40710,
        b"",
        None,
    );
    append(
        &mut tar,
        b"pax",
        tar::EntryType::XHeader,
        0o600,
        b"40 SCHILY.xattr.user.legacy=asset-value\n",
        None,
    );
    append(
        &mut tar,
        RAW_NAME,
        tar::EntryType::Regular,
        0o104600,
        &payload(),
        None,
    );
    append(
        &mut tar,
        b"private/z-hard",
        tar::EntryType::Link,
        0o100777,
        b"",
        Some(RAW_NAME),
    );
    append(
        &mut tar,
        b"symbolic",
        tar::EntryType::Symlink,
        0o120777,
        b"",
        Some(RAW_NAME),
    );
    append(
        &mut tar,
        b"private",
        tar::EntryType::Directory,
        0o42700,
        b"",
        None,
    );
    tar.into_inner().unwrap()
}

async fn legacy_package(assets: &[u8], scripts: Option<&[u8]>) -> Vec<u8> {
    let manifest = legacy_manifest();
    let mut docker = tar::Builder::new(Vec::new());
    let mut multiarch = Vec::new();
    serde_cbor::ser::into_writer(&DockerMultiArch::default(), &mut multiarch).unwrap();
    append(
        &mut docker,
        b"multiarch.cbor",
        tar::EntryType::Regular,
        0o644,
        &multiarch,
        None,
    );
    let docker = docker.into_inner().unwrap();
    pack_legacy(&manifest, &docker, assets, scripts).await
}

async fn pack_legacy(
    manifest: &ManifestV1,
    docker: &[u8],
    assets: &[u8],
    scripts: Option<&[u8]>,
) -> Vec<u8> {
    let mut writer = Cursor::new(Vec::new());
    S9pkPacker::builder()
        .writer(&mut writer)
        .manifest(manifest)
        .license(&b"MIT\n"[..])
        .instructions(&b"Legacy instructions\n"[..])
        .icon(&b"fixture icon"[..])
        .docker_images(docker)
        .assets(assets)
        .scripts(scripts)
        .build()
        .pack(&ed25519_dalek::SigningKey::from_bytes(&[19; 32]))
        .await
        .unwrap();
    writer.into_inner()
}

#[tokio::test]
async fn legacy_sideload_checks_original_signer_before_resigning() {
    use crate::sign::AnyVerifyingKey;

    let source = Arc::<[u8]>::from(legacy_package(&asset_tar(), Some(SCRIPT)).await);
    let original = ed25519_dalek::SigningKey::from_bytes(&[19; 32]);
    let host = ed25519_dalek::SigningKey::from_bytes(&[23; 32]);
    assert!(
        crate::s9pk::load_with_signers(
            source.clone(),
            || panic!("untrusted legacy archive must not reach conversion"),
            None,
            Some(&[AnyVerifyingKey::Ed25519(host.verifying_key())]),
        )
        .await
        .is_err()
    );
    let converted = crate::s9pk::load_with_signers(
        source,
        || Ok(host.clone()),
        None,
        Some(&[AnyVerifyingKey::Ed25519(original.verifying_key())]),
    )
    .await
    .unwrap();
    assert_eq!(converted.as_archive().signer(), host.verifying_key());
}

#[tokio::test]
async fn assets_reader_ends_at_section_eof_before_scripts() {
    let assets = asset_tar();
    let bytes = legacy_package(&assets, Some(SCRIPT)).await;
    let mut reader = S9pkReader::from_reader(Cursor::new(bytes), true)
        .await
        .unwrap();
    assert!(reader.docker_arches().await.unwrap().is_empty());
    let mut handle = reader.assets().await.unwrap();
    let mut actual = vec![0; assets.len()];
    handle.read_exact(&mut actual).await.unwrap();
    assert_eq!(actual, assets);
    assert_eq!(handle.read(&mut [0; 512]).await.unwrap(), 0);
    drop(handle);
    let image = Squashfs::from_tar(reader.assets().await.unwrap())
        .await
        .unwrap();
    assert_eq!(image.contents().len(), 2);
    assert_eq!(
        reader
            .scripts()
            .await
            .unwrap()
            .unwrap()
            .to_vec()
            .await
            .unwrap(),
        SCRIPT
    );
}

async fn archive_image(
    package: &S9pk,
    workspace: &TmpDir,
    name: &str,
) -> (
    Squashfs<crate::util::squashfs::SquashfsFileSource<Arc<[u8]>>>,
    std::path::PathBuf,
) {
    let entry = package.as_archive().contents().get_path(name).unwrap();
    let bytes = entry.read_file_to_vec().await.unwrap();
    let path = workspace.join(name);
    tokio::fs::write(&path, &bytes).await.unwrap();
    let image = Squashfs::deserialize(Arc::<[u8]>::from(bytes))
        .await
        .unwrap();
    (image, path)
}

async fn external_bytes(image: &Path, path: &OsStr) -> Vec<u8> {
    Command::new("unsquashfs")
        .arg("-cat")
        .arg(image)
        .arg(path)
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap()
}

#[tokio::test]
async fn from_v1_native_roundtrip_under_umask_077_with_and_without_scripts() {
    const CHILD_ENV: &str = "STARTOS_NATIVE_COMPAT_UMASK_CHILD";
    const TEST: &str = "s9pk::v2::compat::tests::from_v1_native_roundtrip_under_umask_077_with_and_without_scripts";
    if std::env::var_os(CHILD_ENV).is_none() {
        Command::new("sh")
            .arg("-c")
            .arg(r#"umask 077; exec "$@""#)
            .arg("sh")
            .arg(std::env::current_exe().unwrap())
            .arg(TEST)
            .arg("--exact")
            .arg("--nocapture")
            .env(CHILD_ENV, "1")
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        return;
    }
    for scripts in [Some(SCRIPT), None] {
        let workspace = Arc::new(TmpDir::new().await.unwrap());
        let legacy = workspace.join("legacy.s9pk");
        tokio::fs::write(&legacy, legacy_package(&asset_tar(), scripts).await)
            .await
            .unwrap();
        let mut reader = S9pkReader::open(&legacy, true).await.unwrap();
        let expected_manifest = serde_json::to_vec(&reader.manifest().await.unwrap()).unwrap();
        assert!(reader.docker_arches().await.unwrap().is_empty());
        let mut converted = S9pk::from_v1(
            reader,
            workspace.clone(),
            ed25519_dalek::SigningKey::from_bytes(&[23; 32]),
        )
        .await
        .unwrap();
        assert!(converted.manifest.images.is_empty());
        let output = workspace.join("converted.s9pk");
        let mut sink = create_file(&output).await.unwrap();
        converted.serialize(&mut sink, true).await.unwrap();
        sink.flush().await.unwrap();
        drop(sink);
        drop(converted);
        let reopened = S9pk::open(&output, None).await.unwrap();
        assert_eq!(
            reopened.instructions().await.unwrap().unwrap(),
            "Legacy instructions\n"
        );
        let (assets, asset_path) = archive_image(&reopened, &workspace, "assets.squashfs").await;
        assert_eq!(assets.root_metadata().mode, 0o710);
        assert_eq!(assets.root_metadata().uid, 1234);
        assert_eq!(assets.root_metadata().gid, 5678);
        let private = assets
            .contents()
            .get_path("private")
            .unwrap()
            .as_node()
            .unwrap();
        assert_eq!(private.metadata.mode, 0o2700);
        assert_eq!(private.metadata.uid, 1234);
        assert_eq!(private.metadata.gid, 5678);
        let file = assets
            .contents()
            .get_path(Path::new(OsStr::from_bytes(RAW_NAME)))
            .unwrap()
            .as_node()
            .unwrap();
        assert_eq!(file.metadata.mode, 0o4600);
        assert_eq!(file.metadata.uid, 1234);
        assert_eq!(file.metadata.gid, 5678);
        assert_eq!(assets.root_metadata().modification_time, 0);
        assert_eq!(private.metadata.modification_time, 0);
        assert_eq!(file.metadata.modification_time, 0);
        assert_eq!(
            file.metadata.xattrs.get(OsStr::new("user.legacy")).unwrap(),
            b"asset-value"
        );
        assert!(
            matches!(assets.contents().get_path("private/z-hard").unwrap(), SqfsEntry::Hardlink(target) if target == Path::new(OsStr::from_bytes(RAW_NAME)))
        );
        assert!(
            matches!(&assets.contents().get_path("symbolic").unwrap().as_node().unwrap().contents, NodeContents::Symlink(target) if target == Path::new(OsStr::from_bytes(RAW_NAME)))
        );
        assert_eq!(
            external_bytes(&asset_path, OsStr::from_bytes(RAW_NAME)).await,
            payload()
        );
        assert_eq!(
            external_bytes(&asset_path, OsStr::new("private/z-hard")).await,
            payload()
        );
        let extracted = workspace.join("extracted");
        Command::new("sh")
            .arg("-c")
            .arg(r#"umask 000; exec unsquashfs -quiet -processors 1 -dest "$1" "$2""#)
            .arg("sh")
            .arg(&extracted)
            .arg(&asset_path)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        let original = std::fs::metadata(extracted.join(OsStr::from_bytes(RAW_NAME))).unwrap();
        let hard = std::fs::metadata(extracted.join("private/z-hard")).unwrap();
        assert_eq!(original.ino(), hard.ino());
        assert_eq!(original.nlink(), 2);
        assert_eq!(original.permissions().mode() & 0o777, 0o600);
        assert_eq!(
            std::fs::read_link(extracted.join("symbolic")).unwrap(),
            Path::new(OsStr::from_bytes(RAW_NAME))
        );
        let (javascript, javascript_path) =
            archive_image(&reopened, &workspace, "javascript.squashfs").await;
        assert_eq!(javascript.root_metadata().mode, 0o755);
        let manifest = javascript
            .contents()
            .get_path("embassyManifest.json")
            .unwrap()
            .as_node()
            .unwrap();
        assert_eq!(manifest.metadata.mode, 0o644);
        assert_eq!(
            manifest.metadata.uid,
            std::fs::metadata(workspace.join("javascript/embassyManifest.json"))
                .unwrap()
                .uid()
        );
        assert_eq!(
            external_bytes(&javascript_path, OsStr::new("embassyManifest.json")).await,
            expected_manifest
        );
        if let Some(scripts) = scripts {
            assert_eq!(
                javascript
                    .contents()
                    .get_path("embassy.js")
                    .unwrap()
                    .as_node()
                    .unwrap()
                    .metadata
                    .mode,
                0o644
            );
            assert_eq!(
                external_bytes(&javascript_path, OsStr::new("embassy.js")).await,
                scripts
            );
            assert_eq!(javascript.contents().len(), 2);
        } else {
            assert!(javascript.contents().get_path("embassy.js").is_none());
            assert_eq!(javascript.contents().len(), 1);
        }
        drop(assets);
        drop(javascript);
        drop(reopened);
        workspace.gc().await.unwrap();
    }
}

#[tokio::test]
async fn legacy_single_arch_images_convert_without_container_tools() {
    const CHILD: &str = "STARTOS_LEGACY_NATIVE_IMAGE_TEST";
    const TEST: &str =
        "s9pk::v2::compat::tests::legacy_single_arch_images_convert_without_container_tools";
    if std::env::var_os(CHILD).is_none() {
        Command::new(std::env::current_exe().unwrap())
            .arg(TEST)
            .arg("--exact")
            .env(CHILD, "1")
            .env("PATH", "/nonexistent-startos-test-bin")
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        return;
    }
    use std::io::Read;
    use std::os::unix::ffi::OsStrExt;
    let (saved, _) = super::super::image::tests::save_archive(&[asset_tar()], "amd64");
    let mut docker = Vec::new();
    for entry in tar::Archive::new(saved.as_slice()).entries().unwrap() {
        let mut entry = entry.unwrap();
        let path = entry.path().unwrap().into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if path == Path::new("manifest.json") {
            let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            manifest[0]["RepoTags"] = serde_json::json!(["start9/conversion-test/main:1.2.3"]);
            bytes = serde_json::to_vec(&manifest).unwrap();
        }
        crate::util::squashfs::test_support::append(
            &mut docker,
            path.as_os_str().as_bytes(),
            b'0',
            &bytes,
            None,
        );
    }
    docker.extend_from_slice(&[0; 1024]);
    assert_converts_without_container_tools(&docker).await;
    assert_converts_without_container_tools(&super::super::image::legacy_docker_save_fixture(
        &asset_tar(),
    ))
    .await;
}

async fn assert_converts_without_container_tools(docker: &[u8]) {
    let mut manifest = legacy_manifest();
    manifest.main = serde_json::from_value(
        serde_json::json!({"type":"docker", "image":"main", "entrypoint":"/bin/true"}),
    )
    .unwrap();
    let bytes = pack_legacy(&manifest, docker, &asset_tar(), None).await;
    let reader = S9pkReader::from_reader(Cursor::new(bytes), true)
        .await
        .unwrap();
    let workspace = Arc::new(TmpDir::new().await.unwrap());
    let converted = S9pk::from_v1(
        reader,
        workspace.clone(),
        ed25519_dalek::SigningKey::from_bytes(&[23; 32]),
    )
    .await
    .unwrap();
    assert!(
        converted.manifest.images[&"main".parse().unwrap()]
            .arch
            .contains("x86_64")
    );
    let source = converted
        .archive
        .contents()
        .get_path("images/x86_64/main.squashfs")
        .unwrap()
        .expect_file()
        .unwrap();
    let image = Squashfs::deserialize(Arc::<[u8]>::from(source.to_vec(None).await.unwrap()))
        .await
        .unwrap();
    assert_eq!(
        image
            .contents()
            .get_path(Path::new(OsStr::from_bytes(RAW_NAME)))
            .unwrap()
            .as_node()
            .unwrap()
            .metadata
            .mode,
        0o4600
    );
    drop(image);
    drop(converted);
    workspace.gc().await.unwrap();
}
