use std::io::Cursor;
use std::sync::Arc;

use serde_json::{Value, json};

use super::*;
use crate::util::io::TmpDir;
use crate::util::squashfs::test_support::append;

#[derive(Clone, Default)]
struct Fixture {
    blobs: BTreeMap<PathBuf, Vec<u8>>,
}

impl Fixture {
    fn blob(&mut self, bytes: Vec<u8>, media: &str) -> Value {
        let digest = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
        self.blobs.insert(
            Path::new("blobs/sha256").join(digest.strip_prefix("sha256:").unwrap()),
            bytes.clone(),
        );
        json!({"mediaType": media, "digest": digest, "size": bytes.len()})
    }

    fn image(&mut self, arch: &str, raw: &[u8], compressed: Vec<u8>, media: &str) -> Value {
        let layer = self.blob(compressed, media);
        let config = self.blob(serde_json::to_vec(&json!({
            "architecture": arch, "os": "linux", "config": {"Cmd": ["serve"], "Env": ["HELLO=world"]},
            "rootfs": {"type": "layers", "diff_ids": [format!("sha256:{}", hex::encode(Sha256::digest(raw)))]},
        })).unwrap(), MediaType::ImageConfig.as_ref());
        self.blob(
            serde_json::to_vec(&json!({
                "schemaVersion": 2, "mediaType": MediaType::ImageManifest,
                "config": config, "layers": [layer],
            }))
            .unwrap(),
            MediaType::ImageManifest.as_ref(),
        )
    }

    fn index(&mut self, manifests: Vec<Value>) -> Value {
        self.blob(serde_json::to_vec(&json!({"schemaVersion": 2, "mediaType": MediaType::ImageIndex, "manifests": manifests})).unwrap(), MediaType::ImageIndex.as_ref())
    }

    fn files(mut self, manifests: Vec<Value>) -> BTreeMap<PathBuf, Vec<u8>> {
        self.blobs.insert(
            "oci-layout".into(),
            br#"{"imageLayoutVersion":"1.0.0"}"#.to_vec(),
        );
        self.blobs.insert(
            "index.json".into(),
            serde_json::to_vec(&json!({"schemaVersion": 2, "manifests": manifests})).unwrap(),
        );
        self.blobs
    }

    fn archive(self, manifests: Vec<Value>) -> Vec<u8> {
        archive(self.files(manifests))
    }
}

fn archive(files: BTreeMap<PathBuf, Vec<u8>>) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    let mut tar = Vec::new();
    for (path, bytes) in files {
        append(&mut tar, path.as_os_str().as_bytes(), b'0', &bytes, None);
    }
    tar.extend_from_slice(&[0; 1024]);
    tar
}

pub(crate) fn simple_archive(architecture: &str) -> Vec<u8> {
    let raw = layer();
    let mut fixture = Fixture::default();
    let selected = fixture.image(
        architecture,
        &raw,
        raw.clone(),
        MediaType::ImageLayer.as_ref(),
    );
    fixture.archive(vec![selected])
}

pub(crate) fn legacy_docker_save_fixture(raw: &[u8]) -> Vec<u8> {
    let mut fixture = Fixture::default();
    let mut image = fixture.image("amd64", raw, raw.to_vec(), MediaType::ImageLayer.as_ref());
    let image_path = Path::new("blobs/sha256").join(
        image["digest"]
            .as_str()
            .unwrap()
            .strip_prefix("sha256:")
            .unwrap(),
    );
    let manifest: Value = serde_json::from_slice(&fixture.blobs[&image_path]).unwrap();
    image["annotations"] = json!({
        "io.containerd.image.name": "docker.io/start9/conversion-test/main:1.2.3",
        "org.opencontainers.image.ref.name": "1.2.3",
    });
    let mut files = fixture.files(vec![image]);
    files.insert("manifest.json".into(), serde_json::to_vec(&json!([{
        "Config": format!("blobs/sha256/{}", manifest["config"]["digest"].as_str().unwrap().strip_prefix("sha256:").unwrap()),
        "RepoTags": ["start9/conversion-test/main:1.2.3"],
        "Layers": [format!("blobs/sha256/{}", manifest["layers"][0]["digest"].as_str().unwrap().strip_prefix("sha256:").unwrap())],
    }])).unwrap());
    archive(files)
}

fn layer() -> Vec<u8> {
    let mut tar = Vec::new();
    append(&mut tar, b"file", b'0', b"image content", None);
    tar.extend_from_slice(&[0; 1024]);
    tar
}

async fn packed(bytes: &[u8], reference: Option<&str>) -> Result<Vec<u8>, Error> {
    let saved = Squashfs::from_tar(bytes).await?;
    let (filesystem, config) = from_layout(&saved, "--platform=linux/amd64", reference).await?;
    let (metadata, environment) = config.into_parts();
    assert_eq!(metadata.cmd.unwrap(), ["serve"]);
    assert_eq!(environment, ["HELLO=world"]);
    let mut output = Cursor::new(Vec::new());
    filesystem.serialize(&mut output).await?;
    Ok(output.into_inner())
}

#[tokio::test]
async fn nested_multiplatform_layouts_and_compressed_layers_are_reproducible() {
    let raw = layer();
    let mut gzip = async_compression::tokio::write::GzipEncoder::new(Vec::new());
    tokio::io::AsyncWriteExt::write_all(&mut gzip, &raw)
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::shutdown(&mut gzip).await.unwrap();
    let mut expected = None;
    for (bytes, media) in [
        (raw.clone(), MediaType::ImageLayer),
        (gzip.into_inner(), MediaType::ImageLayerGzip),
        (
            zstd::encode_all(raw.as_slice(), 3).unwrap(),
            MediaType::ImageLayerZstd,
        ),
    ] {
        let mut fixture = Fixture::default();
        let selected = fixture.image("amd64", &raw, bytes, media.as_ref());
        let other = fixture.image("arm64", &raw, raw.clone(), MediaType::ImageLayer.as_ref());
        let nested = fixture.index(vec![other, selected]);
        let irrelevant = json!({"mediaType":"application/vnd.oci.image.manifest.v1+json", "digest":format!("sha256:{}", "0".repeat(64)), "size": 0, "platform":{"os":"unknown","architecture":"unknown"}});
        let actual = packed(&fixture.archive(vec![irrelevant, nested]), None)
            .await
            .unwrap();
        match &expected {
            Some(expected) => assert_eq!(&actual, expected),
            None => expected = Some(actual),
        }
    }
}

#[tokio::test]
async fn references_select_one_image_and_unqualified_ambiguity_is_rejected() {
    let raw = layer();
    let mut fixture = Fixture::default();
    let mut first = fixture.image("amd64", &raw, raw.clone(), MediaType::ImageLayer.as_ref());
    first["annotations"] = json!({
        "org.opencontainers.image.ref.name": "release",
        "io.containerd.image.name": "docker.io/start9/example/main:1.2.3",
    });
    let mut other_raw = raw.clone();
    other_raw.extend_from_slice(&[0; 512]);
    let second = fixture.image(
        "amd64",
        &other_raw,
        other_raw.clone(),
        MediaType::ImageLayer.as_ref(),
    );
    let digest = first["digest"].as_str().unwrap().to_owned();
    let bytes = fixture.archive(vec![first, second]);
    assert!(
        packed(&bytes, None)
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("several images")
    );
    assert_eq!(
        packed(&bytes, Some("release")).await.unwrap(),
        packed(&bytes, Some(&digest)).await.unwrap()
    );
    assert_eq!(
        packed(&bytes, Some("start9/example/main:1.2.3"))
            .await
            .unwrap(),
        packed(&bytes, Some(&digest)).await.unwrap()
    );
    assert!(
        packed(&bytes, Some("missing"))
            .await
            .err()
            .unwrap()
            .source
            .is::<MissingImagePlatform>()
    );
}

#[tokio::test]
async fn descriptors_and_uncompressed_diff_ids_are_both_enforced() {
    let raw = layer();
    let mut fixture = Fixture::default();
    let selected = fixture.image("amd64", &raw, raw.clone(), MediaType::ImageLayer.as_ref());
    let baseline = fixture.clone().files(vec![selected.clone()]);
    let mut bad_size = selected.clone();
    bad_size["size"] = json!(selected["size"].as_u64().unwrap() + 1);
    assert!(
        packed(&fixture.clone().archive(vec![bad_size]), None)
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("descriptor size")
    );
    let mut corrupted = baseline.clone();
    let path = Path::new("blobs/sha256").join(
        selected["digest"]
            .as_str()
            .unwrap()
            .strip_prefix("sha256:")
            .unwrap(),
    );
    corrupted.get_mut(&path).unwrap()[0] ^= 1;
    assert!(
        packed(&archive(corrupted), None)
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("descriptor digest")
    );
    let mut missing = baseline;
    missing.remove(&path);
    assert!(packed(&archive(missing), None).await.is_err());
    let mut wrong_diff = Fixture::default();
    let selected = wrong_diff.image(
        "amd64",
        b"not the uncompressed layer",
        raw,
        MediaType::ImageLayer.as_ref(),
    );
    assert!(
        packed(&wrong_diff.archive(vec![selected]), None)
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("configured digest")
    );
}

#[tokio::test]
async fn embedded_sha512_blobs_are_validated_without_blob_files() {
    let raw = layer();
    let mut fixture = Fixture::default();
    let selected = fixture.image("amd64", &raw, raw.clone(), MediaType::ImageLayer.as_ref());
    let path = Path::new("blobs/sha256").join(
        selected["digest"]
            .as_str()
            .unwrap()
            .strip_prefix("sha256:")
            .unwrap(),
    );
    let bytes = fixture.blobs.remove(&path).unwrap();
    let mut embedded = selected;
    embedded["digest"] = json!(format!("sha512:{}", hex::encode(Sha512::digest(&bytes))));
    embedded["data"] = json!(base64::prelude::BASE64_STANDARD.encode(bytes));
    let good = fixture.clone().archive(vec![embedded.clone()]);
    packed(&good, None).await.unwrap();
    embedded["data"] = json!(base64::prelude::BASE64_STANDARD.encode(b"corrupted"));
    assert!(
        packed(&fixture.archive(vec![embedded]), None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn local_directories_and_archives_share_the_same_importer() {
    let raw = layer();
    let mut fixture = Fixture::default();
    let selected = fixture.image("amd64", &raw, raw.clone(), MediaType::ImageLayer.as_ref());
    let files = fixture.files(vec![selected]);
    let expected = packed(&archive(files.clone()), None).await.unwrap();
    let temp = Arc::new(TmpDir::new().await.unwrap());
    for (path, bytes) in files {
        let path = temp.join(path);
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(path, bytes).await.unwrap();
    }
    let (filesystem, _) = super::super::from_path(&temp, "--platform=linux/amd64", None)
        .await
        .unwrap();
    let mut actual = Cursor::new(Vec::new());
    filesystem.serialize(&mut actual).await.unwrap();
    assert_eq!(actual.into_inner(), expected);
    drop(filesystem);
    temp.gc().await.unwrap();
}

#[test]
fn image_name_references_use_docker_qualification() {
    let digest = format!("sha256:{}", "0".repeat(64));
    for (reference, name) in [
        (
            "nginx".to_owned(),
            "docker.io/library/nginx:latest".to_owned(),
        ),
        (
            "docker.io/nginx".to_owned(),
            "docker.io/library/nginx:latest".to_owned(),
        ),
        (
            "index.docker.io/nginx".to_owned(),
            "docker.io/library/nginx:latest".to_owned(),
        ),
        (
            "start9/example/main:1.2.3".to_owned(),
            "docker.io/start9/example/main:1.2.3".to_owned(),
        ),
        (
            "ghcr.io/start9labs/example".to_owned(),
            "ghcr.io/start9labs/example:latest".to_owned(),
        ),
        (
            format!("localhost:5000/example@{digest}"),
            format!("localhost:5000/example@{digest}"),
        ),
    ] {
        let descriptor: Descriptor = serde_json::from_value(json!({
            "mediaType": MediaType::ImageManifest, "digest": digest, "size": 0,
            "annotations": {"io.containerd.image.name": name},
        }))
        .unwrap();
        assert!(reference_matches(&descriptor, &reference), "{reference}");
        assert!(!reference_matches(
            &descriptor,
            "other.invalid/unrelated:latest"
        ));
        assert!(!reference_matches(&descriptor, "invalid reference"));
    }
}
