use ed25519_dalek::SigningKey;
use rpc_toolkit::Server;

use super::*;
use crate::context::config::ClientConfig;
use crate::developer::write_signing_key;
use crate::s9pk::S9pk;
use crate::s9pk::merkle_archive::directory_contents::DirectoryContents;
use crate::s9pk::merkle_archive::{Entry, MerkleArchive};
use crate::s9pk::v2::pack::ImageSource;

const IMAGE: &[u8] = b"packed image sentinel\0bytes";

#[tokio::test]
async fn add_image_resigns_with_workspace_key_and_preserves_original_on_key_failure() {
    const CHILD_ENV: &str = "STARTOS_ADD_IMAGE_SIGNING_CHILD";
    const TEST: &str = "s9pk::rpc::tests::add_image_resigns_with_workspace_key_and_preserves_original_on_key_failure";
    let workspace = Arc::new(TmpDir::new().await.unwrap());
    if std::env::var_os(CHILD_ENV).is_none() {
        Command::new(std::env::current_exe().unwrap())
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

    let original_key = SigningKey::from_bytes(&[11; 32]);
    let build_key = SigningKey::from_bytes(&[23; 32]);
    let startos = std::env::current_dir().unwrap().join(".startos");
    tokio::fs::create_dir(&startos).await.unwrap();
    let key_path = startos.join("build.key.pem");
    write_signing_key(&build_key, &key_path).await.unwrap();

    let manifest: Manifest = serde_json::from_value(serde_json::json!({
        "id": "signing-test", "version": "1.0.0:0",
        "canMigrateTo": "*", "canMigrateFrom": "*",
        "title": "Signing test", "description": {"short": "Test", "long": "Test"},
        "releaseNotes": "Test", "license": "MIT",
        "packageRepo": "https://example.com/package", "upstreamRepo": "https://example.com/upstream",
        "osVersion": "0.4.0", "images": {}, "volumes": [], "dependencies": {},
        "hardwareRequirements": {"arch": ["x86_64"]}
    }))
    .unwrap();
    let mut contents = DirectoryContents::<Arc<[u8]>>::new();
    for (path, bytes) in [
        ("icon.png", b"icon".as_slice()),
        ("LICENSE.md", b"MIT".as_slice()),
        ("instructions.md", b"instructions".as_slice()),
        ("javascript.squashfs", b"javascript".as_slice()),
        ("images/x86_64/added.squashfs", IMAGE),
        ("images/x86_64/added.json", b"{}".as_slice()),
        ("images/x86_64/added.env", b"TEST=sentinel\n".as_slice()),
    ] {
        contents
            .insert_path(path, Entry::file(Arc::<[u8]>::from(bytes)))
            .unwrap();
    }
    let mut fixture = S9pk::new_with_manifest(
        MerkleArchive::new(contents, original_key.clone(), SIG_CONTEXT),
        None,
        manifest,
    );
    let original_path = workspace.join("original.s9pk");
    fixture
        .serialize(&mut create_file(&original_path).await.unwrap(), true)
        .await
        .unwrap();
    let original_bytes = tokio::fs::read(&original_path).await.unwrap();
    let original = S9pk::open(&original_path, None).await.unwrap();
    assert_eq!(original.as_archive().signer(), original_key.verifying_key());

    let params = imbl_value::json!({
        "s9pk": original_path, "id": "added",
        "config": {
            "source": "packed", "arch": ["x86_64"],
            "emulateMissing": true, "nvidiaContainer": false
        }
    });
    let ctx = CliContext::init(ClientConfig::default()).unwrap();
    let server = Server::new(
        move || {
            let ctx = ctx.clone();
            async move { Ok(ctx) }
        },
        s9pk(),
    );
    server
        .handle_command("edit.add-image", params.clone())
        .await
        .unwrap();
    let edited = S9pk::open(&original_path, None).await.unwrap();
    assert_eq!(edited.as_archive().signer(), build_key.verifying_key());
    assert_ne!(edited.as_archive().signer(), original_key.verifying_key());
    assert_eq!(&*edited.as_manifest().id, "signing-test");
    let image_id: ImageId = "added".parse().unwrap();
    let config = edited.as_manifest().images.get(&image_id).unwrap();
    assert!(matches!(config.source, ImageSource::Packed));
    assert!(config.arch.contains("x86_64"));
    for (path, expected) in [
        ("images/x86_64/added.squashfs", IMAGE),
        ("images/x86_64/added.json", b"{}".as_slice()),
        ("images/x86_64/added.env", b"TEST=sentinel\n".as_slice()),
    ] {
        assert_eq!(
            edited
                .as_archive()
                .contents()
                .get_path(path)
                .unwrap()
                .read_file_to_vec()
                .await
                .unwrap(),
            expected
        );
    }
    let manifest_bytes = edited
        .as_archive()
        .contents()
        .get_path("manifest.json")
        .unwrap()
        .read_file_to_vec()
        .await
        .unwrap();
    let stored: Manifest = serde_json::from_slice(&manifest_bytes).unwrap();
    assert!(stored.images.contains_key(&image_id));
    assert!(!original.as_manifest().images.contains_key(&image_id));

    let invalid_path = workspace.join("stale-signature.s9pk");
    let mut stale = super::super::load(
        MultiCursorFile::from(open_file(&original_path).await.unwrap()),
        || panic!("V2 loading must not request a signing key"),
        None,
    )
    .await
    .unwrap();
    stale.as_manifest_mut().metadata.title = "Changed without re-signing".into();
    stale
        .serialize(&mut create_file(&invalid_path).await.unwrap(), true)
        .await
        .unwrap();
    assert!(S9pk::open(&invalid_path, None).await.is_err());

    tokio::fs::write(&original_path, &original_bytes)
        .await
        .unwrap();
    tokio::fs::remove_file(&key_path).await.unwrap();
    assert!(
        server
            .handle_command("edit.add-image", params)
            .await
            .is_err()
    );
    assert_eq!(
        tokio::fs::read(&original_path).await.unwrap(),
        original_bytes
    );
    assert!(!original_path.with_extension("s9pk.tmp").exists());
    assert_eq!(
        S9pk::open(&original_path, None)
            .await
            .unwrap()
            .as_archive()
            .signer(),
        original_key.verifying_key()
    );
    workspace.gc().await.unwrap();
}
