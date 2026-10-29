use std::os::unix::fs::PermissionsExt;

use super::*;

#[tokio::test]
async fn builder_and_local_layouts_avoid_daemon_round_trips() {
    const CHILD: &str = "STARTOS_OCI_SOURCE_TEST";
    const TEST: &str =
        "s9pk::v2::pack::oci_tests::builder_and_local_layouts_avoid_daemon_round_trips";
    let temp = Arc::new(TmpDir::new().await.unwrap());
    if std::env::var_os(CHILD).is_none() {
        tokio::fs::write(
            temp.join("image.tar"),
            super::super::image::oci_test_archive("arm64"),
        )
        .await
        .unwrap();
        let docker = temp.join("docker");
        tokio::fs::write(
            &docker,
            r#"#!/bin/sh
set -eu
case "$*" in
  'buildx build '*)
    case " $* " in *" --output=type=oci,dest=- "*) ;; *) exit 1 ;; esac
    touch "$STARTOS_OCI_SOURCE_TEST/built"
    cat "$STARTOS_OCI_SOURCE_TEST/image.tar" ;;
  'build '*--format=oci*)
    touch "$STARTOS_OCI_SOURCE_TEST/built" ;;
  'image save --format=oci-archive start9/package-build:'*)
    touch "$STARTOS_OCI_SOURCE_TEST/exported"
    cat "$STARTOS_OCI_SOURCE_TEST/image.tar" ;;
  'rmi start9/package-build:'*)
    touch "$STARTOS_OCI_SOURCE_TEST/removed" ;;
  *) echo "unexpected command: $*" >&2; exit 1 ;;
esac
"#,
        )
        .await
        .unwrap();
        tokio::fs::set_permissions(docker, std::fs::Permissions::from_mode(0o755))
            .await
            .unwrap();
        tokio::fs::copy(&temp.join("docker"), &temp.join("podman"))
            .await
            .unwrap();
        let mut paths = vec![temp.to_path_buf()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        for source in ["docker-builder", "podman-builder", "local"] {
            Command::new(std::env::current_exe().unwrap())
                .arg(TEST)
                .arg("--exact")
                .env(CHILD, &**temp)
                .env("STARTOS_OCI_SOURCE_KIND", source)
                .env("PATH", std::env::join_paths(&paths).unwrap())
                .invoke(ErrorKind::Filesystem)
                .await
                .unwrap();
            assert_eq!(temp.join("built").exists(), source != "local");
            assert_eq!(temp.join("exported").exists(), source == "podman-builder");
            assert_eq!(temp.join("removed").exists(), source == "podman-builder");
            for marker in ["built", "exported", "removed"] {
                if temp.join(marker).exists() {
                    tokio::fs::remove_file(temp.join(marker)).await.unwrap();
                }
            }
        }
        temp.gc().await.unwrap();
        return;
    }
    let kind = std::env::var("STARTOS_OCI_SOURCE_KIND").unwrap();
    PREFER_DOCKER.set(kind != "podman-builder").unwrap();
    let source = if kind != "local" {
        ImageSource::DockerBuild {
            workdir: None,
            dockerfile: None,
            build_args: None,
        }
    } else {
        ImageSource::OciLayout {
            path: PathBuf::from(std::env::var_os(CHILD).unwrap()).join("image.tar"),
            reference: None,
        }
    };
    let mut files = DirectoryContents::<TmpSource<PackSource>>::new();
    source
        .load(
            temp.clone(),
            &"main".parse().unwrap(),
            "aarch64",
            &mut files,
        )
        .await
        .unwrap();
    let metadata: ImageMetadata = serde_json::from_slice(
        &files
            .get_path("images/aarch64/main.json")
            .unwrap()
            .expect_file()
            .unwrap()
            .to_vec(None)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&metadata.env).unwrap(),
        serde_json::json!({"HELLO": "world"})
    );
    assert_eq!(metadata.cmd.unwrap(), ["serve"]);
    assert_eq!(
        files
            .get_path("images/aarch64/main.env")
            .unwrap()
            .expect_file()
            .unwrap()
            .to_vec(None)
            .await
            .unwrap(),
        b"HELLO=world"
    );
    let filesystem = Squashfs::deserialize(Arc::<[u8]>::from(
        files
            .get_path("images/aarch64/main.squashfs")
            .unwrap()
            .expect_file()
            .unwrap()
            .to_vec(None)
            .await
            .unwrap(),
    ))
    .await
    .unwrap();
    let crate::util::squashfs::NodeContents::File(file) = &filesystem
        .contents()
        .get("file")
        .unwrap()
        .as_node()
        .unwrap()
        .contents
    else {
        panic!("not a file")
    };
    assert_eq!(file.to_vec(None).await.unwrap(), b"image content");
    drop(filesystem);
    drop(files);
    temp.gc().await.unwrap();
}
