use super::*;

#[tokio::test]
async fn permission_private_files_and_directory_aliases_stay_out_of_public_images() {
    let (_root, raw, data) = fixture().await;
    tokio::fs::create_dir_all(raw.join("etc/nut"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(raw.join("etc/restricted"))
        .await
        .unwrap();
    tokio::fs::set_permissions(
        raw.join("etc/restricted"),
        std::fs::Permissions::from_mode(0o710),
    )
    .await
    .unwrap();
    for path in ["etc/nut/upsd.users", "etc/nut/upsmon.conf"] {
        tokio::fs::write(raw.join(path), b"nut-password")
            .await
            .unwrap();
        tokio::fs::set_permissions(raw.join(path), std::fs::Permissions::from_mode(0o640))
            .await
            .unwrap();
    }
    tokio::fs::write(raw.join("etc/unknown"), b"unknown-private")
        .await
        .unwrap();
    tokio::fs::set_permissions(
        raw.join("etc/unknown"),
        std::fs::Permissions::from_mode(0o600),
    )
    .await
    .unwrap();
    tokio::fs::write(raw.join("etc/restricted/readable"), b"directory-private")
        .await
        .unwrap();
    tokio::fs::set_permissions(
        raw.join("etc/restricted/readable"),
        std::fs::Permissions::from_mode(0o644),
    )
    .await
    .unwrap();
    tokio::fs::hard_link(raw.join("etc/restricted/readable"), raw.join("etc/alias"))
        .await
        .unwrap();
    tokio::fs::write(raw.join("etc/public.note"), b"public-note")
        .await
        .unwrap();
    tokio::fs::set_permissions(
        raw.join("etc/public.note"),
        std::fs::Permissions::from_mode(0o644),
    )
    .await
    .unwrap();
    let projected = project(&raw, &data, &[]).await;
    for (path, bytes) in [
        ("etc/nut/upsd.users", b"nut-password".as_slice()),
        ("etc/nut/upsmon.conf", b"nut-password"),
        ("etc/unknown", b"unknown-private"),
        ("etc/restricted/readable", b"directory-private"),
        ("etc/alias", b"directory-private"),
    ] {
        let backing = backing_file(&data, target(&projected, path)).await.unwrap();
        assert_eq!(tokio::fs::read(&backing).await.unwrap(), bytes);
        assert_eq!(
            resolved(&projected.snapshot, Path::new(path))
                .unwrap()
                .1
                .metadata,
            Metadata::new(0o777)
        );
        let before = tokio::fs::metadata(raw.join(path)).await.unwrap();
        let after = tokio::fs::metadata(backing).await.unwrap();
        assert_eq!(
            (after.mode(), after.uid(), after.gid(), after.mtime()),
            (before.mode(), before.uid(), before.gid(), before.mtime())
        );
    }
    assert!(
        crate::util::io::same_file(
            &backing_file(&data, target(&projected, "etc/alias"))
                .await
                .unwrap(),
            &backing_file(&data, target(&projected, "etc/restricted/readable"))
                .await
                .unwrap()
        )
        .await
        .unwrap()
    );
    let mut encoded = std::io::Cursor::new(Vec::new());
    projected.snapshot.serialize(&mut encoded).await.unwrap();
    let public = Squashfs::deserialize(std::sync::Arc::<[u8]>::from(encoded.into_inner()))
        .await
        .unwrap()
        .into_dyn();
    let mut files = Vec::new();
    for (path, entry) in snapshot_entries(&public) {
        if let Entry::Node(Node {
            contents: NodeContents::File(source),
            ..
        }) = entry
        {
            files.push((path, source.to_vec(None).await.unwrap()));
        }
    }
    assert_eq!(
        files,
        [(PathBuf::from("etc/public.note"), b"public-note".to_vec())]
    );
}

#[tokio::test]
async fn unsupported_suffixless_profile_symlinks_reject_before_staging() {
    let (_root, raw, data) = fixture().await;
    let profiles = raw.join(crate::net::wifi::SAVED_PROFILE_DIRECTORY.trim_start_matches('/'));
    tokio::fs::create_dir_all(&profiles).await.unwrap();
    tokio::fs::symlink("/external/credential", profiles.join("legacy"))
        .await
        .unwrap();
    assert!(
        ProjectedConfig::stage(
            Squashfs::from_directory(&raw).await.unwrap().into_dyn(),
            &data,
            &[]
        )
        .await
        .is_err()
    );
    assert!(!data.join(DIRECTORY).exists());
}

#[tokio::test]
#[ignore = "requires an exclusively owned software TPM and tpm2-tools"]
async fn software_tpm_bootstrap_stays_public_through_private_projection() {
    let (_root, raw, data) = fixture().await;
    let authority = crate::util::crypto::gen_nistp256().unwrap();
    let credentials = crate::fde::credentials::test_credentials();
    let sealed = crate::fde::credentials::SealedCredentials::seal(
        &credentials,
        &authority,
        Path::new("/var/tmp"),
    )
    .await
    .unwrap();
    let mut projected = project(&raw, &data, &[]).await;
    projected.insert_bootstrap(sealed).unwrap();
    let path = Path::new(crate::fde::credentials::SEALED_CREDENTIALS_FILE);
    let (_, node) = resolved(&projected.snapshot, path).unwrap();
    assert_eq!(node.metadata, Metadata::new(0o600));
    let NodeContents::File(source) = &node.contents else {
        panic!("bootstrap needs locked data")
    };
    let bytes = source.to_vec(None).await.unwrap();
    assert!(
        !bytes
            .windows(b"test-secret".len())
            .any(|window| window == b"test-secret")
    );
    let sealed: crate::fde::credentials::SealedCredentials =
        serde_json::from_slice(&bytes).unwrap();
    let authorization = crate::fde::tpm::authorize(
        &crate::fde::tpm::read_measurements().await.unwrap(),
        &authority,
    )
    .await
    .unwrap();
    let public =
        openssl::pkey::PKey::public_key_from_der(&authority.public_key_to_der().unwrap()).unwrap();
    let restored = sealed.unseal(&authorization, &public).await.unwrap();
    assert_eq!(
        serde_json::to_value(&restored).unwrap(),
        serde_json::to_value(&credentials).unwrap()
    );
    assert!(
        backing_file(&data, target(&projected, "etc/shadow"))
            .await
            .unwrap()
            .is_file()
    );
}

#[tokio::test]
async fn fresh_bootstrap_ciphertext_stays_inline_after_private_projection() {
    let (_root, raw, data) = fixture().await;
    let path = crate::fde::credentials::SEALED_CREDENTIALS_FILE;
    tokio::fs::create_dir_all(raw.join(path).parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(raw.join(path), b"stale-private-input")
        .await
        .unwrap();
    let mut projected = project(&raw, &data, &[]).await;
    assert!(projected.snapshot.contents().get_path(path).is_none());
    let sealed: crate::fde::credentials::SealedCredentials = serde_json::from_value(
        serde_json::json!({"key":{"public":[1],"private":[2]},"envelope":[3,4]}),
    )
    .unwrap();
    let expected = serde_json::to_vec(&sealed).unwrap();
    projected.insert_bootstrap(sealed).unwrap();
    let (_, node) = resolved(&projected.snapshot, Path::new(path)).unwrap();
    assert_eq!(node.metadata, Metadata::new(0o600));
    let NodeContents::File(source) = &node.contents else {
        panic!("ciphertext was projected onto locked data")
    };
    assert_eq!(source.to_vec(None).await.unwrap(), expected);
    assert!(!projected.replacements.contains_key(Path::new(path)));
    let mut resumed = ProjectedConfig::stage(projected.snapshot.clone(), &data, &[])
        .await
        .unwrap();
    assert!(resumed.snapshot.contents().get_path(path).is_none());
    resumed
        .insert_bootstrap(serde_json::from_slice(&expected).unwrap())
        .unwrap();
    assert!(matches!(
        resolved(&resumed.snapshot, Path::new(path))
            .unwrap()
            .1
            .contents,
        NodeContents::File(_)
    ));
}
