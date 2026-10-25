use tokio::process::Command;

use super::*;
use crate::util::Invoke;
use crate::util::squashfs::test_support::append;

pub(crate) fn save_archive(layers: &[Vec<u8>], architecture: &str) -> (Vec<u8>, String) {
    archive_with_config(
        layers,
        &serde_json::json!({
            "architecture": architecture,
            "os": "linux",
            "config": {"Cmd": ["serve", "--arm64"], "Entrypoint": ["/app/start"],
                "Env": ["ARCH=arm64", "EMPTY="], "WorkingDir": "/arm64", "User": "1000:1000"},
            "rootfs": {"type": "layers", "diff_ids": layers.iter()
                .map(|bytes| format!("sha256:{}", hex::encode(Sha256::digest(bytes)))).collect::<Vec<_>>()},
        }),
    )
}

fn archive_with_config(layers: &[Vec<u8>], config: &serde_json::Value) -> (Vec<u8>, String) {
    let config = serde_json::to_vec(config).unwrap();
    let id = format!("sha256:{}", hex::encode(Sha256::digest(&config)));
    let mut archive = Vec::new();
    append(&mut archive, b"config.json", b'0', &config, None);
    let mut paths = Vec::new();
    for (index, layer) in layers.iter().enumerate() {
        let path = format!("layer-{index}.tar");
        append(&mut archive, path.as_bytes(), b'0', layer, None);
        paths.push(path);
    }
    append(
        &mut archive,
        b"manifest.json",
        b'0',
        &serde_json::to_vec(&serde_json::json!([
            {"Config": "config.json", "Layers": paths},
        ]))
        .unwrap(),
        None,
    );
    archive.extend_from_slice(&[0; 1024]);
    (archive, id)
}

fn image_layer() -> Vec<u8> {
    let mut layer = Vec::new();
    append(&mut layer, b".", b'5', b"", None);
    append(&mut layer, b"etc", b'5', b"", None);
    append(
        &mut layer,
        b"etc/hosts",
        b'0',
        b"image-provided hosts",
        None,
    );
    append(
        &mut layer,
        b"etc/hostname",
        b'0',
        b"image-provided hostname",
        None,
    );
    append(
        &mut layer,
        b"etc/resolv.conf",
        b'0',
        b"image-provided resolver",
        None,
    );
    append(&mut layer, b"etc/mtab", b'2', b"", Some(b"/proc/mounts"));
    append(
        &mut layer,
        b".dockerenv",
        b'0',
        b"image-provided marker",
        None,
    );
    append(
        &mut layer,
        b"environment-alias",
        b'1',
        b"",
        Some(b".dockerenv"),
    );
    append(&mut layer, b"dev", b'5', b"", None);
    append(&mut layer, b"proc", b'5', b"", None);
    append(&mut layer, b"sys", b'5', b"", None);
    layer.extend_from_slice(&[0; 1024]);
    layer
}

#[tokio::test]
async fn image_files_and_metadata_survive_without_docker_initialization() {
    let (archive, _) = save_archive(&[image_layer()], "amd64");
    let (image, _) = from_image_archive(archive.as_slice(), "--platform=linux/amd64")
        .await
        .unwrap();
    assert_eq!(image.root_metadata().modification_time, 42);
    for (path, expected) in [
        ("etc/hosts", "image-provided hosts"),
        ("etc/hostname", "image-provided hostname"),
        ("etc/resolv.conf", "image-provided resolver"),
        (".dockerenv", "image-provided marker"),
    ] {
        let node = image.contents().get_path(path).unwrap().as_node().unwrap();
        let crate::util::squashfs::NodeContents::File(source) = &node.contents else {
            panic!("not a file")
        };
        assert_eq!(source.to_vec(None).await.unwrap(), expected.as_bytes());
        assert_eq!(
            (
                node.metadata.mode,
                node.metadata.uid,
                node.metadata.gid,
                node.metadata.modification_time
            ),
            (0o640, 17, 23, 42)
        );
    }
    assert!(
        matches!(&image.contents().get_path("etc/mtab").unwrap().as_node().unwrap().contents,
        crate::util::squashfs::NodeContents::Symlink(target) if target == Path::new("/proc/mounts"))
    );
    for path in ["dev", "proc", "sys"] {
        assert!(
            image
                .contents()
                .get(path)
                .unwrap()
                .as_node()
                .unwrap()
                .contents
                .as_directory()
                .is_some()
        );
    }
}

#[tokio::test]
async fn symlinked_resolver_becomes_a_regular_file() {
    let mut layer = Vec::new();
    append(&mut layer, b"etc", b'5', b"", None);
    append(
        &mut layer,
        b"etc/resolv.conf",
        b'2',
        b"",
        Some(b"/nix/store/image-root/etc/resolv.conf"),
    );
    append(
        &mut layer,
        b"etc/hosts",
        b'2',
        b"",
        Some(b"/nix/store/image-root/etc/hosts"),
    );
    layer.extend_from_slice(&[0; 1024]);
    let (archive, _) = save_archive(&[layer], "amd64");
    let (image, _) = from_image_archive(archive.as_slice(), "--platform=linux/amd64")
        .await
        .unwrap();
    let node = image
        .contents()
        .get_path("etc/resolv.conf")
        .unwrap()
        .as_node()
        .unwrap();
    let crate::util::squashfs::NodeContents::File(source) = &node.contents else {
        panic!("not a file")
    };
    assert!(source.to_vec(None).await.unwrap().is_empty());
    assert_eq!(
        (node.metadata.mode, node.metadata.uid, node.metadata.gid),
        (0o644, 0, 0)
    );
    assert!(matches!(
        &image
            .contents()
            .get_path("etc/hosts")
            .unwrap()
            .as_node()
            .unwrap()
            .contents,
        crate::util::squashfs::NodeContents::Symlink(_)
    ));
}

#[tokio::test]
async fn symlinked_resolver_replacement_preserves_other_hardlink_paths() {
    for alias in [b"etc/zzz".as_slice(), b"etc/aaa".as_slice()] {
        let mut layer = Vec::new();
        append(&mut layer, b"etc", b'5', b"", None);
        append(
            &mut layer,
            b"etc/resolv.conf",
            b'2',
            b"",
            Some(b"/nix/store/image-root/etc/resolv.conf"),
        );
        append(&mut layer, alias, b'1', b"", Some(b"etc/resolv.conf"));
        layer.extend_from_slice(&[0; 1024]);
        let (archive, _) = save_archive(&[layer], "amd64");
        let (image, _) = from_image_archive(archive.as_slice(), "--platform=linux/amd64")
            .await
            .unwrap();
        let node = image
            .contents()
            .get_path("etc/resolv.conf")
            .unwrap()
            .as_node()
            .expect("resolver must have its own inode");
        let crate::util::squashfs::NodeContents::File(source) = &node.contents else {
            panic!("resolver is not a file")
        };
        assert!(source.to_vec(None).await.unwrap().is_empty());
        let alias = std::str::from_utf8(alias).unwrap();
        let node = resolve_hardlink(Path::new(alias), |target| {
            image.contents().get_path(target).map(|entry| match entry {
                Entry::Node(node) => Ok(node),
                Entry::Hardlink(target) => Err(target.clone()),
            })
        })
        .unwrap();
        assert!(matches!(
            &node.contents,
            crate::util::squashfs::NodeContents::Symlink(target)
                if target == Path::new("/nix/store/image-root/etc/resolv.conf")
        ));
        assert_eq!(
            (node.metadata.mode, node.metadata.uid, node.metadata.gid),
            (0o640, 17, 23)
        );
    }
}

#[tokio::test]
async fn compressed_layers_have_identical_filesystem_bytes_and_validate_uncompressed_digests() {
    let layer = image_layer();
    let config = serde_json::json!({"architecture": "amd64", "os": "linux", "rootfs": {
        "type": "layers", "diff_ids": [format!("sha256:{}", hex::encode(Sha256::digest(&layer)))],
    }});
    let mut gzip = async_compression::tokio::write::GzipEncoder::new(Vec::new());
    tokio::io::AsyncWriteExt::write_all(&mut gzip, &layer)
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::shutdown(&mut gzip).await.unwrap();
    let zstd = zstd::encode_all(layer.as_slice(), 3).unwrap();
    let mut expected = None;
    for layer in [layer, gzip.into_inner(), zstd] {
        let (archive, _) = archive_with_config(&[layer], &config);
        let (image, _) = from_image_archive(archive.as_slice(), "--platform=linux/amd64")
            .await
            .unwrap();
        let mut bytes = Cursor::new(Vec::new());
        image.serialize(&mut bytes).await.unwrap();
        match &expected {
            Some(expected) => assert_eq!(&bytes.into_inner(), expected),
            None => expected = Some(bytes.into_inner()),
        }
    }
}

#[tokio::test]
async fn archive_paths_order_and_timestamps_do_not_affect_bytes_across_processes() {
    const INPUT: &str = "STARTOS_IMAGE_ARCHIVE_REPRO_INPUT";
    const OUTPUT: &str = "STARTOS_IMAGE_ARCHIVE_REPRO_OUTPUT";
    const TEST: &str = "s9pk::v2::image::tests::archive_paths_order_and_timestamps_do_not_affect_bytes_across_processes";
    if let Some(input) = std::env::var_os(INPUT) {
        let (image, _) = from_image_archive(
            crate::util::io::open_file(Path::new(&input)).await.unwrap(),
            "--platform=linux/amd64",
        )
        .await
        .unwrap();
        image
            .serialize(
                &mut crate::util::io::create_file(Path::new(&std::env::var_os(OUTPUT).unwrap()))
                    .await
                    .unwrap(),
            )
            .await
            .unwrap();
        return;
    }
    let workspace = crate::util::io::TmpDir::new().await.unwrap();
    let layer = image_layer();
    let (original, _) = save_archive(&[layer.clone()], "amd64");
    let config = {
        use std::io::Read;
        let mut archive = tar::Archive::new(original.as_slice());
        let mut config = Vec::new();
        for member in archive.entries().unwrap() {
            let mut member = member.unwrap();
            if member.path().unwrap() == Path::new("config.json") {
                member.read_to_end(&mut config).unwrap();
                break;
            }
        }
        config
    };
    const DECOY: &[u8] = br#"{"architecture":"arm64","os":"linux","rootfs":{"type":"layers"}}"#;
    let mut expected = None;
    for (index, (config_path, layer_path)) in [
        ("config.json", "legacy/layer.tar"),
        ("blobs/sha256/config", "blobs/sha256/layer"),
        ("config-digest.json", "layer-digest.tar"),
    ]
    .into_iter()
    .enumerate()
    {
        let manifest = serde_json::to_vec(&serde_json::json!([
            {"Config": "decoy.json", "Layers": []},
            {"Config": config_path, "Layers": [layer_path]},
        ]))
        .unwrap();
        let mut entries = vec![
            (config_path, config.as_slice()),
            (layer_path, layer.as_slice()),
            ("manifest.json", manifest.as_slice()),
            ("decoy.json", DECOY),
        ];
        if index % 2 == 1 {
            entries.reverse();
        }
        let mut archive = Vec::new();
        for (path, bytes) in entries {
            let offset = archive.len();
            append(&mut archive, path.as_bytes(), b'0', bytes, None);
            let mut header =
                tokio_tar::Header::from_byte_slice(&archive[offset..offset + 512]).clone();
            header.set_mtime(1000 + index as u64);
            header.set_uid(100 + index as u64);
            header.set_cksum();
            archive[offset..offset + 512].copy_from_slice(header.as_bytes());
        }
        archive.extend_from_slice(&[0; 1024]);
        let input = workspace.join(format!("save-{index}.tar"));
        let output = workspace.join(format!("image-{index}.squashfs"));
        tokio::fs::write(&input, archive).await.unwrap();
        Command::new(std::env::current_exe().unwrap())
            .arg(TEST)
            .arg("--exact")
            .arg("--nocapture")
            .env(INPUT, &input)
            .env(OUTPUT, &output)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        let actual = tokio::fs::read(output).await.unwrap();
        match &expected {
            Some(expected) => assert_eq!(&actual, expected),
            None => expected = Some(actual),
        }
    }
    workspace.delete().await.unwrap();
}

#[tokio::test]
async fn layer_digests_cover_transport_padding_and_gzip_trailers_are_checked() {
    let mut layer = image_layer();
    let (archive, id) = save_archive(&[layer.clone()], "amd64");
    let mut original = tar::Archive::new(archive.as_slice());
    use std::io::Read;
    let mut config = Vec::new();
    original
        .entries()
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .read_to_end(&mut config)
        .unwrap();
    let config: serde_json::Value = serde_json::from_slice(&config).unwrap();
    layer.extend_from_slice(&[0; 512]);
    let (archive, unchanged_id) = archive_with_config(&[layer], &config);
    assert_eq!(id, unchanged_id);
    assert!(
        from_image_archive(archive.as_slice(), "--platform=linux/amd64")
            .await
            .is_err()
    );

    let layer = image_layer();
    let mut gzip = async_compression::tokio::write::GzipEncoder::new(Vec::new());
    tokio::io::AsyncWriteExt::write_all(&mut gzip, &layer)
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::shutdown(&mut gzip).await.unwrap();
    let mut gzip = gzip.into_inner();
    let crc = gzip.len() - 8;
    gzip[crc] ^= 1;
    let (archive, _) = archive_with_config(&[gzip], &config);
    assert!(
        from_image_archive(archive.as_slice(), "--platform=linux/amd64")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn wrong_platform_and_layer_digests_are_rejected() {
    let layer = image_layer();
    let (archive, _) = save_archive(&[layer.clone()], "amd64");
    assert!(
        from_image_archive(archive.as_slice(), "--platform=linux/arm64")
            .await
            .is_err()
    );
    for rootfs in [
        serde_json::json!({"type": "layers", "diff_ids": [format!("sha256:{}", "0".repeat(64))]}),
        serde_json::json!({"type": "layers", "diff_ids": []}),
        serde_json::json!({"type": "unknown", "diff_ids": []}),
    ] {
        let config = serde_json::json!({"architecture": "amd64", "os": "linux", "rootfs": rootfs});
        let (archive, _) = archive_with_config(&[layer.clone()], &config);
        assert!(
            from_image_archive(archive.as_slice(), "--platform=linux/amd64")
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn scratch_images_do_not_require_a_container_command_or_any_layers() {
    let config =
        serde_json::json!({"architecture": "amd64", "os": "linux", "rootfs": {"type": "layers"}});
    let (archive, _) = archive_with_config(&[], &config);
    let (image, config) = from_image_archive(archive.as_slice(), "--platform=linux/amd64")
        .await
        .unwrap();
    assert!(image.contents().is_empty());
    let (metadata, env) = config.into_parts();
    assert!(metadata.cmd.is_none());
    assert!(metadata.entrypoint.is_none());
    assert!(env.is_empty());
}

#[tokio::test]
async fn archives_with_several_images_for_the_platform_are_rejected() {
    let config = serde_json::to_vec(
        &serde_json::json!({"architecture": "amd64", "os": "linux", "rootfs": {"type": "layers"}}),
    )
    .unwrap();
    let mut archive = Vec::new();
    append(&mut archive, b"a.json", b'0', &config, None);
    append(&mut archive, b"b.json", b'0', &config, None);
    append(
        &mut archive,
        b"manifest.json",
        b'0',
        br#"[{"Config":"a.json"},{"Config":"b.json"}]"#,
        None,
    );
    archive.extend_from_slice(&[0; 1024]);
    assert!(
        from_image_archive(archive.as_slice(), "--platform=linux/amd64")
            .await
            .is_err()
    );
    assert!(
        from_image_archive(archive.as_slice(), "--platform=linux/arm64")
            .await
            .is_err()
    );
}
