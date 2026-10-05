use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::context::config::ClientConfig;
use crate::developer::write_signing_key;

const MANIFEST: &str = r#"module.exports.manifest = {
  id: "permissions", version: "1.0.0:0",
  canMigrateTo: "1.0.0:0", canMigrateFrom: "1.0.0:0",
  title: "Permissions", description: { short: "Test package", long: "Test package" },
  releaseNotes: "Initial test version", license: "MIT",
  packageRepo: "https://example.com/package", upstreamRepo: "https://example.com/upstream",
  images: {}, volumes: []
}
"#;

fn mode(path: impl AsRef<Path>) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

async fn extract(source: &impl FileSource, workspace: &Path, name: &str) -> PathBuf {
    let image = workspace.join(format!("{name}.squashfs"));
    source
        .copy(&mut create_file(&image).await.unwrap())
        .await
        .unwrap();
    let dest = workspace.join(name);
    Command::new("sh")
        .arg("-c")
        .arg(r#"umask 000; exec unsquashfs -quiet -processors 1 -dest "$1" "$2""#)
        .arg("sh")
        .arg(&dest)
        .arg(image)
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    dest
}

#[test]
fn javascript_access_preserves_executable_and_special_bits() {
    for (source, expected) in [
        (0o600, 0o644),
        (0o400, 0o444),
        (0o700, 0o744),
        (0o010, 0o454),
        (0o4500, 0o4544),
        (0o4000, 0o4444),
    ] {
        assert_eq!(javascript_mode(source, false), expected);
    }
    for (source, expected) in [(0, 0o555), (0o600, 0o755), (0o770, 0o775), (0o2750, 0o2755)] {
        assert_eq!(javascript_mode(source, true), expected);
    }
}

#[tokio::test]
async fn public_pack_under_umask_077_exposes_only_javascript_and_keeps_sources() {
    const CHILD_ENV: &str = "STARTOS_NATIVE_PACK_UMASK_CHILD";
    const TEST: &str = "s9pk::v2::pack::packaging_tests::public_pack_under_umask_077_exposes_only_javascript_and_keeps_sources";
    let workspace = Arc::new(TmpDir::new().await.unwrap());
    if std::env::var_os(CHILD_ENV).is_none() {
        let startos = workspace.join(".startos");
        tokio::fs::create_dir(&startos).await.unwrap();
        write_signing_key(
            &ed25519_dalek::SigningKey::from_bytes(&[7; 32]),
            startos.join("build.key.pem"),
        )
        .await
        .unwrap();
        Command::new("sh")
            .arg("-c")
            .arg(r#"umask 077; exec "$@""#)
            .arg("sh")
            .arg(std::env::current_exe().unwrap())
            .arg(TEST)
            .arg("--exact")
            .arg("--nocapture")
            .env(CHILD_ENV, "1")
            .current_dir(&*workspace)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        workspace.gc().await.unwrap();
        return;
    }
    let package = workspace.join("package");
    let javascript = package.join("javascript");
    let assets = package.join("assets");
    tokio::fs::create_dir_all(javascript.join("nested"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(&assets).await.unwrap();
    tokio::fs::write(javascript.join("index.js"), MANIFEST)
        .await
        .unwrap();
    tokio::fs::write(javascript.join("nested/helper"), b"#!/bin/sh\n")
        .await
        .unwrap();
    tokio::fs::write(assets.join("private"), b"asset\0bytes")
        .await
        .unwrap();
    tokio::fs::write(package.join("icon.png"), [])
        .await
        .unwrap();
    tokio::fs::write(package.join("LICENSE.md"), "MIT\n")
        .await
        .unwrap();
    tokio::fs::write(package.join("instructions.md"), "Test instructions\n")
        .await
        .unwrap();
    tokio::fs::set_permissions(&javascript, std::fs::Permissions::from_mode(0o770))
        .await
        .unwrap();
    tokio::fs::set_permissions(
        javascript.join("nested/helper"),
        std::fs::Permissions::from_mode(0o700),
    )
    .await
    .unwrap();
    assert_eq!(mode(&javascript), 0o770);
    assert_eq!(mode(javascript.join("index.js")), 0o600);
    assert_eq!(mode(&assets), 0o700);
    assert_eq!(mode(assets.join("private")), 0o600);
    let output = workspace.join("permissions.s9pk");
    pack(
        CliContext::init(ClientConfig::default()).unwrap(),
        PackParams {
            path: Some(package),
            output: Some(output.clone()),
            javascript: None,
            icon: None,
            license: None,
            instructions: None,
            assets: None,
            no_assets: false,
            arch: Vec::new(),
        },
    )
    .await
    .unwrap();
    let s9pk = S9pk::open(output, None).await.unwrap();
    let js_source = s9pk
        .as_archive()
        .contents()
        .get_path("javascript.squashfs")
        .unwrap()
        .expect_file()
        .unwrap();
    let extracted = extract(&**js_source, &workspace, "javascript-extracted").await;
    assert_eq!(mode(&extracted), 0o775);
    assert_eq!(mode(extracted.join("nested")), 0o755);
    assert_eq!(mode(extracted.join("index.js")), 0o644);
    assert_eq!(mode(extracted.join("nested/helper")), 0o744);
    let assets_source = s9pk
        .as_archive()
        .contents()
        .get_path("assets.squashfs")
        .unwrap()
        .expect_file()
        .unwrap();
    let extracted = extract(&**assets_source, &workspace, "assets-extracted").await;
    assert_eq!(mode(&extracted), 0o700);
    assert_eq!(mode(extracted.join("private")), 0o600);
    assert_eq!(
        tokio::fs::read(extracted.join("private")).await.unwrap(),
        b"asset\0bytes"
    );
    assert_eq!(mode(&javascript), 0o770);
    assert_eq!(mode(javascript.join("index.js")), 0o600);
    assert_eq!(mode(javascript.join("nested/helper")), 0o700);
    assert_eq!(mode(&assets), 0o700);
    assert_eq!(mode(assets.join("private")), 0o600);
    drop(s9pk);
    workspace.gc().await.unwrap();
}

fn asset_tar() -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_ustar();
    header.set_size(10);
    header.set_mode(0o640);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(123);
    builder
        .append_data(&mut header, "file", &b"tar\0bytes\n"[..])
        .unwrap();
    builder.into_inner().unwrap()
}

#[tokio::test]
async fn tar_asset_source_is_not_overwritten_and_creates_native_image() {
    let workspace = Arc::new(TmpDir::new().await.unwrap());
    let path = workspace.join("assets.tar");
    let original = asset_tar();
    tokio::fs::write(&path, &original).await.unwrap();
    let source = PackSource::Squashfs(Arc::new(
        SqfsDir::from_path(&path, workspace.clone()).await.unwrap(),
    ));
    let dest = extract(&source, &workspace, "extracted").await;
    assert_eq!(
        tokio::fs::read(dest.join("file")).await.unwrap(),
        b"tar\0bytes\n"
    );
    assert_eq!(mode(dest.join("file")), 0o640);
    assert_eq!(tokio::fs::read(path).await.unwrap(), original);
    drop(source);
    workspace.gc().await.unwrap();
}

#[tokio::test]
async fn export_validates_exit_status_and_drains_large_trailing_padding() {
    let workspace = TmpDir::new().await.unwrap();
    let archive = workspace.join("export.tar");
    let mut bytes = asset_tar();
    bytes.resize(bytes.len() + 2 * crate::CAP_1_MiB, 0);
    tokio::fs::write(&archive, bytes).await.unwrap();
    let dest = workspace.join("export.squashfs");
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        export_squashfs(Command::new("cat").arg(&archive), &dest),
    )
    .await
    .unwrap()
    .unwrap();
    let failed = export_squashfs(
        Command::new("sh")
            .arg("-c")
            .arg(r#"cat "$1"; echo 'export-failed-after-tar' >&2; exit 7"#)
            .arg("sh")
            .arg(&archive),
        &workspace.join("failed.squashfs"),
    )
    .await;
    assert!(format!("{:?}", failed.unwrap_err()).contains("export-failed-after-tar"));
    assert!(!workspace.join("failed.squashfs").exists());
    tokio::fs::write(&archive, b"truncated tar").await.unwrap();
    assert!(
        export_squashfs(
            Command::new("cat").arg(&archive),
            &workspace.join("bad.squashfs")
        )
        .await
        .is_err()
    );
    workspace.delete().await.unwrap();
}

#[tokio::test]
async fn both_manifest_callers_accept_apostrophes_in_bundle_paths() {
    let workspace = TmpDir::new().await.unwrap();
    let package = workspace.join("package's-path");
    let javascript = package.join("javascript");
    tokio::fs::create_dir_all(&javascript).await.unwrap();
    tokio::fs::create_dir(package.join("assets")).await.unwrap();
    tokio::fs::write(javascript.join("index.js"), MANIFEST)
        .await
        .unwrap();
    for name in ["icon.png", "LICENSE.md", "instructions.md"] {
        tokio::fs::write(package.join(name), []).await.unwrap();
    }
    let manifest: Manifest = serde_json::from_slice(
        &javascript_manifest(&javascript.join("index.js"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(&*manifest.id, "permissions");
    let ingredients = list_ingredients(
        CliContext::init(ClientConfig::default()).unwrap(),
        PackParams {
            path: Some(package.clone()),
            output: None,
            javascript: None,
            icon: None,
            license: None,
            instructions: None,
            assets: None,
            no_assets: false,
            arch: Vec::new(),
        },
    )
    .await
    .unwrap();
    assert!(ingredients.contains(&package.join("assets")));
    workspace.delete().await.unwrap();
}
