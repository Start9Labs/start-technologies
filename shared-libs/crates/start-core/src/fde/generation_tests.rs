use super::*;
use crate::os_overlay::{OVERLAY_FILE, VerityImage};
use crate::rpc_continuations::Guid;
use crate::util::squashfs::{Entry, NodeContents};

#[tokio::test]
#[ignore = "requires exclusive managed guest, genuine FDE_SOURCE_IMAGE and verity/loop/overlay/namespace privileges"]
async fn authorized_configuration_uses_prospective_native_inputs_and_redacts_before_signing() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let rootfs = PathBuf::from(std::env::var_os("FDE_SOURCE_IMAGE").expect("genuine source image"));
    let tree = root.join("rootfs.verity");
    let verity = VerityImage::format(&rootfs, &tree).await.unwrap();
    let raw = root.join("raw");
    let data = root.join("data");
    let store = root.join("generations");
    for path in ["etc/ssh", "etc/ssl", "opt", "candidate/home", "custom"] {
        tokio::fs::create_dir_all(raw.join(path)).await.unwrap();
    }
    tokio::fs::create_dir_all(data.join("main")).await.unwrap();
    tokio::fs::create_dir(&store).await.unwrap();
    tokio::fs::write(raw.join("etc/shadow"), b"private-shadow-marker")
        .await
        .unwrap();
    tokio::fs::write(
        raw.join("etc/passwd"),
        b"root:x:0:0:root:/root:/bin/bash\nstart9:x:1001:1000::/candidate/home:/bin/bash\n",
    )
    .await
    .unwrap();
    tokio::fs::write(
        raw.join("etc/nsswitch.conf"),
        b"passwd: files\ngroup: files\nhosts: files dns\n",
    )
    .await
    .unwrap();
    tokio::fs::write(raw.join("etc/ssl/openssl.cnf"), b"openssl_conf = openssl_init\n[openssl_init]\nproviders = provider_sect\n[provider_sect]\ndefault = default_sect\n[default_sect]\nactivate = 1\n").await.unwrap();
    tokio::fs::write(
        raw.join("etc/ssh/sshd_config"),
        b"Include /opt/authorized-ssh.conf\n",
    )
    .await
    .unwrap();
    tokio::fs::write(
        raw.join("opt/authorized-ssh.conf"),
        b"HostKey ~start9/server-key\nHostKey \"/custom/key with space\"\n",
    )
    .await
    .unwrap();
    for (path, bytes) in [
        ("candidate/home/server-key", b"native-nss-key".as_slice()),
        ("custom/key with space", b"quoted-key".as_slice()),
    ] {
        tokio::fs::write(raw.join(path), bytes).await.unwrap();
    }
    let source = root.join("public");
    let data_view = root.join("data-view");
    let public = verity
        .mount(
            &rootfs,
            &tree,
            &format!("fde-authorized-{}", Guid::new()),
            &source,
        )
        .await
        .unwrap();
    let mounted_data = MountGuard::mount(&Bind::new(&data), &data_view, ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    let snapshot = Squashfs::from_directory(&raw).await.unwrap().into_dyn();
    let key = crate::os_overlay::generate_signing_key().unwrap();
    let (prepared, legacy) =
        prepare_configuration(snapshot, public, mounted_data, store.clone(), key)
            .await
            .unwrap();
    assert!(
        !crate::disk::mount::util::is_mountpoint(&source)
            .await
            .unwrap()
    );
    assert!(
        !crate::disk::mount::util::is_mountpoint(&data_view)
            .await
            .unwrap()
    );
    let bytes = tokio::fs::read(prepared.staging_path().join(OVERLAY_FILE))
        .await
        .unwrap();
    let image = Squashfs::deserialize(std::sync::Arc::<[u8]>::from(bytes))
        .await
        .unwrap();
    for path in [
        "etc/shadow",
        "candidate/home/server-key",
        "custom/key with space",
    ] {
        let Some(Entry::Node(node)) = image.contents().get_path(Path::new(path)) else {
            panic!("missing private reference");
        };
        let NodeContents::Symlink(target) = &node.contents else {
            panic!("private content published");
        };
        let backing = data
            .join("main")
            .join(super::super::private_config::DIRECTORY)
            .join(
                target
                    .strip_prefix(super::super::private_config::RUNTIME)
                    .unwrap(),
            );
        assert_eq!(
            tokio::fs::read(&backing).await.unwrap(),
            tokio::fs::read(raw.join(path)).await.unwrap()
        );
    }
    let published = prepared.publish().await.unwrap();
    assert!(published.join(OVERLAY_FILE).is_file());
    legacy.replace_legacy(&raw).await.unwrap();
    assert!(
        tokio::fs::symlink_metadata(raw.join("candidate/home/server-key"))
            .await
            .unwrap()
            .is_symlink()
    );

    tokio::fs::write(raw.join("etc/ssl/openssl.cnf"), b"config_diagnostics = 1\nopenssl_conf = init\n[init]\nalg_section = algorithms\n[algorithms]\ndefault_properties = fips=yes\n").await.unwrap();
    let snapshot = Squashfs::from_directory(&raw).await.unwrap().into_dyn();
    let public = verity
        .mount(
            &rootfs,
            &tree,
            &format!("fde-authorized-{}", Guid::new()),
            &source,
        )
        .await
        .unwrap();
    let mounted_data = MountGuard::mount(&Bind::new(&data), &data_view, ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    assert!(
        prepare_configuration(
            snapshot,
            public,
            mounted_data,
            store.clone(),
            crate::os_overlay::generate_signing_key().unwrap()
        )
        .await
        .is_err()
    );
    assert!(
        !crate::disk::mount::util::is_mountpoint(&source)
            .await
            .unwrap()
    );
    assert!(
        !crate::disk::mount::util::is_mountpoint(&data_view)
            .await
            .unwrap()
    );
    assert_eq!(std::fs::read_dir(&store).unwrap().count(), 1);
}
